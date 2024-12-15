use derivative::Derivative;
use futures::{future::BoxFuture, FutureExt};
use sqlx::{migrate::MigrateDatabase, prelude::*, sqlite::SqliteRow, Sqlite, SqliteConnection};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use anyhow::{anyhow, bail, Result};
use nucleo::{
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
    Matcher, Utf32Str,
};

use chrono::Utc;

use crate::{
    app::{APP, APPID, ORG, QUALIFIER},
    config::Config,
    utils::{self},
};

#[cfg(test)]
pub mod test;

type TimeId = i64;

const DB_VERSION: &str = "5";
const DB_PATH: &str = constcat::concat!(APPID, "-db-", DB_VERSION, ".sqlite");

#[derive(Clone, Eq, Derivative)]
pub struct Entry {
    pub creation: TimeId,
    // todo: lazelly load image in memory, since we can't search them anyways
    /// (Mime, Content)
    pub content: HashMap<String, Vec<u8>>,
    pub is_favorite: bool,
}

impl Hash for Entry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for e in self.content.values() {
            e.hash(state);
        }
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
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
        content: HashMap<String, Vec<u8>>,
        is_favorite: bool,
    ) -> Self {
        Self {
            creation,
            content,
            is_favorite,
        }
    }

    pub fn new_now(
        content: HashMap<String, Vec<u8>>,
        is_favorite: bool,
    ) -> Self {
        Self::new(
            Utc::now().timestamp_millis(),
            content,
            is_favorite,
        )
    }

    /// SELECT creation, mime, content, metadataMime, metadata
    fn from_row(row: &SqliteRow, favorites: &Favorites) -> Result<Self> {
        let id = row.get("creation");
        let is_fav = favorites.contains(&id);

        Ok(Entry::new(
            id,
            row.get("mime"),
            row.get("content"),
            row.try_get("metadataMime")
                .ok()
                .map(|metadata_mime| EntryMetadata {
                    mime: metadata_mime,
                    value: row.get("metadata"),
                }),
            is_fav,
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
    data_version: i64,
    favorites: Favorites,
}

#[derive(Default)]
struct Favorites {
    favorites: Vec<TimeId>,
    favorites_hash_set: HashSet<TimeId>,
}

impl Favorites {
    fn contains(&self, id: &TimeId) -> bool {
        self.favorites_hash_set.contains(id)
    }
    fn clear(&mut self) {
        self.favorites.clear();
        self.favorites_hash_set.clear();
    }

    fn insert_at(&mut self, id: TimeId, pos: Option<usize>) {
        match pos {
            Some(pos) => self.favorites.insert(pos, id),
            None => self.favorites.push(id),
        }
        self.favorites_hash_set.insert(id);
    }

    fn remove(&mut self, id: &TimeId) -> Option<usize> {
        self.favorites_hash_set.remove(id);
        self.favorites.iter().position(|e| e == id).inspect(|i| {
            self.favorites.remove(*i);
        })
    }

    fn fav(&self) -> &Vec<TimeId> {
        &self.favorites
    }

    fn change(&mut self, prev: &TimeId, new: TimeId) {
        let pos = self.favorites.iter().position(|e| e == prev).unwrap();
        self.favorites[pos] = new;
        self.favorites_hash_set.remove(prev);
        self.favorites_hash_set.insert(new);
    }
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

        let migration_path = db_dir.join("migrations");
        std::fs::create_dir_all(&migration_path)?;
        include_dir::include_dir!("migrations")
            .extract(&migration_path)
            .unwrap();

        match sqlx::migrate::Migrator::new(migration_path).await {
            Ok(migrator) => migrator,
            Err(e) => {
                warn!("migrator error {e}, fall back to relative path");
                sqlx::migrate::Migrator::new(Path::new("./migrations")).await?
            }
        }
        .run(&mut conn)
        .await?;

        if let Some(max_duration) = config.maximum_entries_lifetime() {
            let now_millis = utils::now_millis();
            let max_millis = max_duration.as_millis().try_into().unwrap_or(u64::MAX);

            let query_delete_old_one = r#"
                DELETE FROM ClipboardEntries
                WHERE (? - creation) >= ? AND creation NOT IN(
                    SELECT id
                    FROM FavoriteClipboardEntries
                );
            "#;

            sqlx::query(query_delete_old_one)
                .bind(now_millis)
                .bind(max_millis as i64)
                .execute(&mut conn)
                .await
                .unwrap();
        }

        if let Some(max_number_of_entries) = &config.maximum_entries_number {
            let query_delete_old_one = r#"
                WITH MostRecentEntries AS (
                    SELECT creation
                    FROM ClipboardEntries
                    ORDER BY creation DESC
                    LIMIT ?
                )
                DELETE FROM ClipboardEntries
                WHERE creation NOT IN (
                    SELECT creation
                    FROM MostRecentEntries
                    FULL JOIN FavoriteClipboardEntries ON (creation = id));
            "#;

            sqlx::query(query_delete_old_one)
                .bind(max_number_of_entries)
                .execute(&mut conn)
                .await
                .unwrap();
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
            favorites: Favorites::default(),
        };

        db.reload().await?;

        Ok(db)
    }

    async fn reload(&mut self) -> Result<()> {
        self.hashs.clear();
        self.state.clear();
        self.favorites.clear();

        {
            let query_load_favs = r#"
                SELECT id, position
                FROM FavoriteClipboardEntries
            "#;

            let rows = sqlx::query(query_load_favs)
                .fetch_all(&mut self.conn)
                .await?;

            let mut rows = rows
                .iter()
                .map(|row| {
                    let id: i64 = row.get("id");
                    let index: i32 = row.get("position");
                    (id, index as usize)
                })
                .collect::<Vec<_>>();

            rows.sort_by(|e1, e2| e1.1.cmp(&e2.1));

            debug_assert_eq!(rows.last().map(|e| e.1 + 1).unwrap_or(0), rows.len());

            for (id, pos) in rows {
                self.favorites.insert_at(id, Some(pos));
            }
        }

        {
            let query_load_table = r#"
                SELECT creation, mime, content, metadataMime, metadata
                FROM ClipboardEntries
                JOIN ClipboardContents ON (id = creation)
                GROUP BY
            "#;

            let rows = sqlx::query(query_load_table)
                .fetch_all(&mut self.conn)
                .await?;

                sqlx::query(query_load_table)
                    .fetch(executor)

            for row in &rows {
                let data = Entry::from_row(row, &self.favorites)?;

                self.hashs.insert(data.get_hash(), data.creation);
                self.state.insert(data.creation, data);
            }
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
            .map(|e| Entry::from_row(&e, &self.favorites).unwrap());

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

            let new_id = last_row.creation;

            let data_hash = data.get_hash();

            if let Some(old_id) = self.hashs.remove(&data_hash) {
                self.state.remove(&old_id);

                if self.favorites.contains(&old_id) {
                    data.is_favorite = true;
                    let query_delete_old_id = r#"
                        UPDATE FavoriteClipboardEntries
                        SET id = $1
                        WHERE id = $2;
                    "#;

                    sqlx::query(query_delete_old_id)
                        .bind(new_id)
                        .bind(old_id)
                        .execute(&mut self.conn)
                        .await?;

                    self.favorites.change(&old_id, new_id);
                } else {
                    data.is_favorite = false;
                }

                // in case 2 same data were inserted in a short period
                // we don't want to remove the old_id
                if new_id != old_id {
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

            data.creation = new_id;

            self.hashs.insert(data_hash, data.creation);
            self.state.insert(data.creation, data);

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

        if data.is_favorite {
            self.favorites.remove(&data.creation);
        }

        self.search();
        Ok(())
    }

    pub async fn clear(&mut self) -> Result<()> {
        let query_delete = r#"
            DELETE FROM ClipboardEntries
            WHERE creation NOT IN(
                SELECT id
                FROM FavoriteClipboardEntries
            );
        "#;

        sqlx::query(query_delete).execute(&mut self.conn).await?;

        self.reload().await?;

        Ok(())
    }

    pub async fn add_favorite(&mut self, entry: &Entry, index: Option<usize>) -> Result<()> {
        debug_assert!(!self.favorites.fav().contains(&entry.creation));

        self.favorites.insert_at(entry.creation, index);

        if let Some(pos) = index {
            let query = r#"
                UPDATE FavoriteClipboardEntries
                SET position = position + 1
                WHERE position >= ?;
            "#;
            sqlx::query(query)
                .bind(pos as i32)
                .execute(&mut self.conn)
                .await
                .unwrap();
        }

        let index = index.unwrap_or(self.favorite_len() - 1);

        {
            let query = r#"
                INSERT INTO FavoriteClipboardEntries (id, position)
                VALUES ($1, $2);
            "#;

            sqlx::query(query)
                .bind(entry.creation)
                .bind(index as i32)
                .execute(&mut self.conn)
                .await?;
        }

        if let Some(e) = self.state.get_mut(&entry.creation) {
            e.is_favorite = true;
        }

        Ok(())
    }

    pub async fn remove_favorite(&mut self, entry: &Entry) -> Result<()> {
        debug_assert!(self.favorites.fav().contains(&entry.creation));

        {
            let query = r#"
                DELETE FROM FavoriteClipboardEntries
                WHERE id = ?;
            "#;

            sqlx::query(query)
                .bind(entry.creation)
                .execute(&mut self.conn)
                .await?;
        }

        if let Some(pos) = self.favorites.remove(&entry.creation) {
            let query = r#"
                UPDATE FavoriteClipboardEntries
                SET position = position - 1
                WHERE position >= ?;
            "#;
            sqlx::query(query)
                .bind(pos as i32)
                .execute(&mut self.conn)
                .await?;
        }

        if let Some(e) = self.state.get_mut(&entry.creation) {
            e.is_favorite = false;
        }
        Ok(())
    }

    pub fn favorite_len(&self) -> usize {
        self.favorites.favorites.len()
    }

    pub fn search(&mut self) {
        if self.query.is_empty() {
            self.filtered.clear();
        } else if let Some(atom) = &self.needle {
            self.filtered = Self::iter_inner(&self.state, &self.favorites)
                .filter_map(|data| {
                    data.get_searchable_text().and_then(|text| {
                        let mut buf = Vec::new();

                        let haystack = Utf32Str::new(text, &mut buf);

                        let mut indices = Vec::new();

                        let _res = atom.indices(haystack, &mut self.matcher, &mut indices);

                        if !indices.is_empty() {
                            Some((data.creation, indices))
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
            self.iter().nth(index)
        } else {
            self.filtered
                .get(index)
                .map(|(id, _indices)| &self.state[id])
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ Entry> {
        debug_assert!(self.query.is_empty());
        Self::iter_inner(&self.state, &self.favorites)
    }

    fn iter_inner<'a>(
        state: &'a BTreeMap<TimeId, Entry>,
        favorites: &'a Favorites,
    ) -> impl Iterator<Item = &'a Entry> + 'a {
        favorites
            .fav()
            .iter()
            .filter_map(|id| state.get(id))
            .chain(state.values().filter(|e| !e.is_favorite).rev())
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
