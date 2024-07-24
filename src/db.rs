use cosmic::{
    cosmic_theme::palette::cast::ComponentsInto,
    iced::{subscription, Subscription},
    iced_sctk::util,
};
use derivative::Derivative;
use futures::{future::BoxFuture, FutureExt, SinkExt};
use sqlx::{
    migrate::MigrateDatabase, prelude::*, sqlite::SqliteExecutor, sqlite::SqliteRow, Sqlite,
    SqliteConnection, SqlitePool,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    fs::{self, DirBuilder, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};
use tokio::sync::mpsc;

use anyhow::{anyhow, bail, Result};
use nucleo::{
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
    Matcher, Utf32Str,
};

use chrono::Utc;
use mime::Mime;

use crate::{
    app::{APP, APPID, ORG, QUALIFIER},
    config::Config,
    message::AppMessage,
    utils::{self, now_millis, remove_dir_contents},
};

type TimeId = i64;

const DB_VERSION: &str = "4";
const DB_PATH: &str = constcat::concat!(APPID, "-db-", DB_VERSION, ".sqlite");

// warning: if you change somethings in here, change the db version
#[derive(Clone, Eq, Derivative)]
#[derivative(PartialEq, Hash)]
pub struct Entry {
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub creation: TimeId,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub mime: String,

    // todo: lazelly load image in memory, since we can't search them anyways
    pub content: Vec<u8>,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    /// (Mime, Content)
    pub metadata: Option<EntryMetadata>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EntryMetadata {
    pub mime: String,
    pub value: String,
}

impl Entry {
    fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn new(
        creation: i64,
        mime: String,
        content: Vec<u8>,
        metadata: Option<EntryMetadata>,
    ) -> Self {
        Self {
            creation,
            mime,
            content,
            metadata,
        }
    }

    pub fn new_now(mime: String, content: Vec<u8>, metadata: Option<EntryMetadata>) -> Self {
        Self::new(Utc::now().timestamp_millis(), mime, content, metadata)
    }

    /// SELECT creation, mime, content, metadataMime, metadata
    fn from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Entry::new(
            row.get("creation"),
            row.get("mime"),
            row.get("content"),
            row.try_get("metadataMime")
                .ok()
                .map(|metadata_mime| EntryMetadata {
                    mime: metadata_mime,
                    value: row.get("metadata"),
                }),
        ))
    }
}

impl Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Data")
            .field("creation", &self.creation)
            .field("mime", &self.mime)
            .field("content", &self.get_content())
            .field("metadata", &self.metadata)
            .finish()
    }
}

pub enum Content<'a> {
    Text(&'a str),
    Image(&'a Vec<u8>),
    UriList(Vec<&'a str>),
}

impl Debug for Content<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(arg0) => f.debug_tuple("Text").field(arg0).finish(),
            Self::UriList(arg0) => f.debug_tuple("UriList").field(arg0).finish(),
            Self::Image(_) => f.debug_tuple("Image").finish(),
        }
    }
}

impl Entry {
    pub fn get_content(&self) -> Result<Content<'_>> {
        if self.mime == "text/uri-list" {
            let text = if let Some(metadata) = &self.metadata {
                &metadata.value
            } else {
                core::str::from_utf8(&self.content)?
            };

            let uris = text
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();

            return Ok(Content::UriList(uris));
        }
        if self.mime.starts_with("text/") {
            return Ok(Content::Text(core::str::from_utf8(&self.content)?));
        }

        if self.mime.starts_with("image/") {
            return Ok(Content::Image(&self.content));
        }

        bail!("unsupported mime type {}", self.mime)
    }

    fn get_searchable_text(&self) -> Option<&str> {
        if self.mime.starts_with("text/") {
            return core::str::from_utf8(&self.content).ok();
        }

        if let Some(metadata) = &self.metadata {
            #[allow(clippy::assigning_clones)]
            if metadata.mime == "text/html" {
                if let Some(alt) = find_alt(&metadata.value) {
                    return Some(alt);
                }
            }
            return Some(&metadata.value);
        }

        None
    }
}

// currently best effort
fn find_alt(html: &str) -> Option<&str> {
    const DEB: &str = "alt=\"";

    if let Some(pos) = html.find(DEB) {
        const OFFSET: usize = DEB.as_bytes().len();

        if let Some(pos_end) = html[pos + OFFSET..].find('"') {
            return Some(&html[pos + OFFSET..pos + pos_end + OFFSET]);
        }
    }

    None
}

pub struct Db {
    conn: SqliteConnection,
    hashs: HashMap<u64, TimeId>,
    state: BTreeMap<TimeId, Entry>,
    filtered: Vec<(TimeId, Vec<u32>)>,
    query: String,
    needle: Option<Atom>,
    matcher: Matcher,
    // time
    last_update: i64,
    data_version: i64,
}

impl Db {
    pub async fn new(config: &Config) -> Result<Self> {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();

        std::fs::create_dir_all(directories.cache_dir())?;

        Self::inner_new(config, directories.cache_dir()).await
    }

    async fn inner_new(config: &Config, db_dir: &Path) -> Result<Self> {
        let db_path = db_dir.join(DB_PATH);

        let db_path = db_path
            .to_str()
            .ok_or(anyhow!("can't convert path to str"))?;

        if !Sqlite::database_exists(db_path).await? {
            info!("Creating database {}", db_path);
            Sqlite::create_database(db_path).await?;
        }

        let mut conn = SqliteConnection::connect(db_path).await?;

        let migration_path = Path::new(constcat::concat!("/usr/share/", APP, "/migrations"));

        match sqlx::migrate::Migrator::new(migration_path).await {
            Ok(migrator) => migrator,
            Err(e) => {
                warn!("migrator error {e}, fall back to relative path");
                sqlx::migrate::Migrator::new(Path::new("./migrations")).await?
            }
        }
        .run(&mut conn)
        .await?;

        if let Some(max_duration) = &config.maximum_entries_lifetime {
            let now_millis = utils::now_millis();
            let max_millis = max_duration.as_millis().try_into().unwrap_or(u64::MAX);

            let query_delete_old_one = r#"
                DELETE FROM ClipboardEntries
                WHERE (? - creation) >= ?;
            "#;

            sqlx::query(query_delete_old_one)
                .bind(now_millis)
                .bind(max_millis as i64)
                .execute(&mut conn)
                .await?;
        }

        if let Some(max_number_of_entries) = &config.maximum_entries_number {
            let query_delete_old_one = r#"
                DELETE FROM ClipboardEntries
                WHERE creation NOT IN
                    (SELECT creation FROM ClipboardEntries
                    ORDER BY creation DESC
                    LIMIT ?);
            "#;

            sqlx::query(query_delete_old_one)
                .bind(max_number_of_entries)
                .execute(&mut conn)
                .await?;
        }

        let mut db = Db {
            data_version: fetch_data_version(&mut conn).await?,
            conn,
            hashs: HashMap::default(),
            state: BTreeMap::default(),
            filtered: Vec::default(),
            query: String::default(),
            needle: None,
            matcher: Matcher::new(nucleo::Config::DEFAULT),
            last_update: 0,
        };

        db.reload().await?;

        Ok(db)
    }

    async fn reload(&mut self) -> Result<()> {
        self.hashs.clear();
        self.state.clear();

        let query_load_table = r#"
            SELECT creation, mime, content, metadataMime, metadata
            FROM ClipboardEntries
        "#;

        let rows = sqlx::query(query_load_table)
            .fetch_all(&mut self.conn)
            .await?;

        for row in &rows {
            let data = Entry::from_row(row)?;

            self.hashs.insert(data.get_hash(), data.creation);
            self.state.insert(data.creation, data);
        }

        self.search();

        Ok(())
    }

    async fn get_last_row(&mut self) -> Result<Option<Entry>> {
        let query_get_last_row = r#"
            SELECT creation, mime, content, metadataMime, metadata
            FROM ClipboardEntries
            ORDER BY creation DESC
            LIMIT 1
        "#;

        let entry = sqlx::query(query_get_last_row)
            .fetch_optional(&mut self.conn)
            .await?
            .map(|e| Entry::from_row(&e).unwrap());

        Ok(entry)
    }

    // the <= 200 condition, is to unsure we reuse the same timestamp
    // of the first process that inserted the data.
    pub fn insert<'a: 'b, 'b>(&'a mut self, mut data: Entry) -> BoxFuture<'b, Result<()>> {
        async move {
            // insert a new data, only if the last row is not the same AND was not created recently
            let query_insert_if_not_exist = r#"
                WITH last_row AS (
                    SELECT creation, mime, content, metadataMime, metadata
                    FROM ClipboardEntries
                    ORDER BY creation DESC
                    LIMIT 1
                )
                INSERT INTO ClipboardEntries (creation, mime, content, metadataMime, metadata)
                SELECT $1, $2, $3, $4, $5
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM last_row AS lr
                    WHERE lr.content = $3 AND ($6 - lr.creation) <= 1000
                );
            "#;

            if let Err(e) = sqlx::query(query_insert_if_not_exist)
                .bind(data.creation)
                .bind(&data.mime)
                .bind(&data.content)
                .bind(data.metadata.as_ref().map(|m| &m.mime))
                .bind(data.metadata.as_ref().map(|m| &m.value))
                .bind(utils::now_millis())
                .execute(&mut self.conn)
                .await
            {
                if let sqlx::Error::Database(e) = &e {
                    if e.is_unique_violation() {
                        warn!("a different value with the same id was already inserted");
                        data.creation += 1;
                        return self.insert(data).await;
                    }
                }

                return Err(e.into());
            }

            // safe to unwrap since we insert before
            let last_row = self.get_last_row().await?.unwrap();

            let data_hash = data.get_hash();

            if let Some(old_id) = self.hashs.remove(&data_hash) {
                self.state.remove(&old_id);

                // in case 2 same data were inserted in a short period
                // we don't want to remove the old_id
                if last_row.creation != old_id {
                    let query_delete_old_id = r#"
                    DELETE FROM ClipboardEntries
                    WHERE creation = ?;
                "#;

                    sqlx::query(query_delete_old_id)
                        .bind(old_id)
                        .execute(&mut self.conn)
                        .await?;
                }
            }

            data.creation = last_row.creation;

            self.hashs.insert(data_hash, data.creation);
            self.state.insert(data.creation, data);
            self.last_update = now_millis();

            self.search();
            Ok(())
        }
        .boxed()
    }

    pub async fn delete(&mut self, data: &Entry) -> Result<()> {
        let query = r#"
            DELETE FROM ClipboardEntries
            WHERE creation = ?;
        "#;

        sqlx::query(query)
            .bind(data.creation)
            .execute(&mut self.conn)
            .await?;

        self.hashs.remove(&data.get_hash());
        self.state.remove(&data.creation);
        self.last_update = now_millis();

        self.search();
        Ok(())
    }

    pub async fn clear(&mut self) -> Result<()> {
        let query_delete = r#"
            DELETE FROM ClipboardEntries
        "#;

        sqlx::query(query_delete).execute(&mut self.conn).await?;

        self.state.clear();
        self.filtered.clear();
        self.hashs.clear();
        self.last_update = now_millis();

        Ok(())
    }

    pub fn search(&mut self) {
        if self.query.is_empty() {
            self.filtered.clear();
        } else if let Some(atom) = &self.needle {
            self.filtered = self
                .state
                .iter()
                .rev()
                .filter_map(|(id, data)| {
                    data.get_searchable_text().and_then(|text| {
                        let mut buf = Vec::new();

                        let haystack = Utf32Str::new(text, &mut buf);

                        let mut indices = Vec::new();

                        let _res = atom.indices(haystack, &mut self.matcher, &mut indices);

                        if !indices.is_empty() {
                            Some((*id, indices))
                        } else {
                            None
                        }
                    })
                })
                .collect::<Vec<_>>();
        }
    }

    pub fn set_query_and_search(&mut self, query: String) {
        if query.is_empty() {
            self.needle.take();
        } else {
            let atom = Atom::new(
                &query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Substring,
                true,
            );

            self.needle.replace(atom);
        }

        self.query = query;

        self.search();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        if self.query.is_empty() {
            // because we expose the tree in reverse
            self.state.iter().rev().nth(index).map(|e| e.1)
        } else {
            self.filtered
                .get(index)
                .map(|(id, _indices)| &self.state[id])
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ Entry> {
        debug_assert!(self.query.is_empty());
        self.state.values().rev()
    }

    pub fn search_iter(&self) -> impl Iterator<Item = (&'_ Entry, &'_ Vec<u32>)> {
        debug_assert!(!self.query.is_empty());

        self.filtered
            .iter()
            .map(|(id, indices)| (&self.state[id], indices))
    }

    pub fn len(&self) -> usize {
        if self.query.is_empty() {
            self.state.len()
        } else {
            self.filtered.len()
        }
    }

    pub async fn handle_message(&mut self, _message: DbMessage) -> Result<()> {
        let data_version = fetch_data_version(&mut self.conn).await?;

        if self.data_version != data_version {
            self.reload().await?;
        }

        self.data_version = data_version;

        Ok(())
    }
}

/// https://www.sqlite.org/pragma.html#pragma_data_version
async fn fetch_data_version(conn: &mut SqliteConnection) -> Result<i64> {
    let data_version: i64 = sqlx::query("PRAGMA data_version")
        .fetch_one(conn)
        .await?
        .get("data_version");

    Ok(data_version)
}

#[derive(Clone, Debug)]
pub enum DbMessage {
    CheckUpdate,
}

pub fn sub() -> Subscription<DbMessage> {
    cosmic::iced::time::every(Duration::from_millis(1000)).map(|_| DbMessage::CheckUpdate)
}

#[cfg(test)]
mod test {
    use std::{
        fs::{self, File},
        io::{Read, Write},
        path::PathBuf,
        thread::sleep,
        time::Duration,
    };

    use serial_test::serial;

    use anyhow::Result;
    use cosmic::{iced_sctk::util, widget::canvas::Path};

    use crate::{
        config::Config,
        utils::{self, remove_dir_contents},
    };

    use super::{Db, Entry};

    fn prepare_db_dir() -> PathBuf {
        let db_dir = PathBuf::from("tests");
        let _ = std::fs::create_dir_all(&db_dir);
        remove_dir_contents(&db_dir);
        db_dir
    }

    #[tokio::test]
    #[serial]
    async fn test() -> Result<()> {
        let db_dir = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_dir).await?;

        test_db(&mut db).await.unwrap();

        db.clear().await?;

        test_db(&mut db).await.unwrap();

        Ok(())
    }

    async fn test_db(db: &mut Db) -> Result<()> {
        assert!(db.len() == 0);

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into(), None);

        db.insert(data).await.unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into(), None);

        db.insert(data).await.unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Entry::new_now("text/plain".into(), "content2".as_bytes().into(), None);

        db.insert(data.clone()).await.unwrap();

        assert!(db.len() == 2);

        let next = db.iter().next().unwrap();

        assert!(next.creation == data.creation);
        assert!(next.content == data.content);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_old_one() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into(), None);
        db.insert(data).await.unwrap();

        sleep(Duration::from_millis(100));

        let data = Entry::new_now("text/plain".into(), "content2".as_bytes().into(), None);
        db.insert(data).await.unwrap();

        assert!(db.len() == 2);

        let db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

        assert!(db.len() == 2);

        let config = Config {
            maximum_entries_lifetime: Some(Duration::ZERO),
            ..Default::default()
        };
        let db = Db::inner_new(&config, &db_path).await.unwrap();

        assert!(db.len() == 0);
    }

    #[tokio::test]
    #[serial]
    async fn same() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

        let now = utils::now_millis();

        let data = Entry::new(now, "text/plain".into(), "content".as_bytes().into(), None);

        db.insert(data).await.unwrap();

        let data = Entry::new(now, "text/plain".into(), "content".as_bytes().into(), None);

        db.insert(data).await.unwrap();
        assert!(db.len() == 1);
    }

    #[tokio::test]
    #[serial]
    async fn different_content_same_time() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

        let now = utils::now_millis();

        let data = Entry::new(now, "text/plain".into(), "content".as_bytes().into(), None);

        db.insert(data).await.unwrap();

        let data = Entry::new(now, "text/plain".into(), "content2".as_bytes().into(), None);

        db.insert(data).await.unwrap();
        assert!(db.len() == 2);
    }
}
