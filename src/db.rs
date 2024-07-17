use cosmic::iced_sctk::util;
use derivative::Derivative;
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

use anyhow::{anyhow, bail, Result};
use nucleo::{
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
    Matcher, Utf32Str,
};

use chrono::Utc;
use mime::Mime;
use rusqlite::{named_params, params, Connection, ErrorCode, OpenFlags, OptionalExtension};

use crate::{
    app::{APP, APPID, ORG, QUALIFIER},
    config::Config,
    utils::{self, remove_dir_contents},
};

type TimeId = i64;

const DB_VERSION: &str = "1";
const DB_PATH: &str = constcat::concat!(APPID, "-db-", DB_VERSION, ".sqlite");

// warning: if you change somethings in here, change the db version
#[derive(Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Clone, Eq)]
pub struct Entry {
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub creation: TimeId,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub mime: String,

    // todo: lazelly load image in memory, since we can't search them anyways
    pub content: Vec<u8>,
}

impl Entry {
    fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn new(creation: i64, mime: String, content: Vec<u8>) -> Self {
        Self {
            creation,
            mime,
            content,
        }
    }

    pub fn new_now(mime: String, content: Vec<u8>) -> Self {
        Self::new(Utc::now().timestamp_millis(), mime, content)
    }
}

impl Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Data")
            .field("creation", &self.creation)
            .field("mime", &self.mime)
            .field("content", &self.get_content())
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
            let text = core::str::from_utf8(&self.content)?;

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

    pub fn get_text(&self) -> Option<&str> {
        self.get_content().ok().and_then(|c| match c {
            Content::Text(txt) => Some(txt),
            _ => None,
        })
    }
}

pub struct Db {
    conn: Connection,
    hashs: HashMap<u64, TimeId>,
    state: BTreeMap<TimeId, Entry>,
    filtered: Vec<(TimeId, Vec<u32>)>,
    query: String,
    needle: Option<Atom>,
    matcher: Matcher,
}

impl Db {
    pub fn new(config: &Config) -> Result<Self> {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();

        std::fs::create_dir_all(directories.cache_dir())?;

        Self::inner_new(config, directories.cache_dir())
    }

    fn inner_new(config: &Config, db_dir: &Path) -> Result<Self> {
        let db_path = db_dir.join(DB_PATH);

        if !db_path.exists() {
            remove_dir_contents(db_dir);

            let conn = Connection::open_with_flags(
                db_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            )?;

            let query_create_table = r#"
                CREATE TABLE data (
                    creation INTEGER PRIMARY KEY,
                    mime TEXT NOT NULL,
                    content BLOB NOT NULL
                )
            "#;

            conn.execute(query_create_table, ())?;

            return Ok(Db {
                conn,
                hashs: HashMap::default(),
                state: BTreeMap::default(),
                filtered: Vec::default(),
                query: String::default(),
                needle: None,
                matcher: Matcher::new(nucleo::Config::DEFAULT),
            });
        }

        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;

        if let Some(max_duration) = &config.maximum_entries_lifetime {
            let query_delete_old_one = r#"
                DELETE FROM data
                WHERE (:now - creation) >= :max;
            "#;

            conn.execute(
                query_delete_old_one,
                named_params! {
                    ":now": utils::now_millis(),
                    ":max": max_duration.as_millis().try_into().unwrap_or(u64::MAX),
                },
            )?;
        }

        if let Some(max_number_of_entries) = &config.maximum_entries_number {
            let query_delete_old_one = r#"
                DELETE FROM data 
                WHERE creation NOT IN
                    (SELECT creation FROM data
                    ORDER BY creation DESC
                    LIMIT :max_number_of_entries);
            "#;

            conn.execute(
                query_delete_old_one,
                named_params! {
                    ":max_number_of_entries": max_number_of_entries,
                },
            )?;
        }

        let mut hashs = HashMap::default();
        let mut state = BTreeMap::default();

        let query_load_table = r#"
            SELECT creation, mime, content
            FROM data
        "#;

        {
            let mut stmt = conn.prepare(query_load_table)?;

            let mut rows = stmt.query(())?;

            while let Some(row) = rows.next()? {
                let data = Entry::new(row.get(0)?, row.get(1)?, row.get(2)?);
                hashs.insert(data.get_hash(), data.creation);
                state.insert(data.creation, data);
            }
        }

        let db = Db {
            conn,
            hashs,
            state,
            filtered: Vec::default(),
            query: String::default(),
            needle: None,
            matcher: Matcher::new(nucleo::Config::DEFAULT),
        };

        Ok(db)
    }

    fn get_last_row(&mut self) -> Result<Option<Entry>> {
        let query_get_last_row = r#"
            SELECT creation, mime, content
            FROM data
            ORDER BY creation DESC
            LIMIT 1
        "#;

        let data = self
            .conn
            .query_row(query_get_last_row, (), |row| {
                Ok(Entry::new(row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;

        Ok(data)
    }

    // the <= 200 condition, is to unsure we reuse the same timestamp
    // of the first process that inserted the data.
    pub fn insert(&mut self, mut data: Entry) -> Result<()> {
        // insert a new data, only if the last row is not the same AND was not created recently
        let query_insert_if_not_exist = r#"
            WITH last_row AS (
                SELECT creation, mime, content
                FROM data
                ORDER BY creation DESC
                LIMIT 1
            )
            INSERT INTO data (creation, mime, content)
            SELECT :new_id, :new_mime, :new_content
            WHERE NOT EXISTS (
                SELECT 1
                FROM last_row AS lr
                WHERE lr.content = :new_content AND (:now - lr.creation) <= 1000
            );
        "#;

        if let Err(e) = self.conn.execute(
            query_insert_if_not_exist,
            named_params! {
                ":new_id": data.creation,
                ":new_mime": data.mime,
                ":new_content": data.content,
                ":now": utils::now_millis(),
            },
        ) {
            if let rusqlite::Error::SqliteFailure(rusqlite::ffi::Error { code, .. }, ..) = e {
                if code == ErrorCode::ConstraintViolation {
                    warn!("a different value with the same id was already inserted");
                    data.creation += 1;
                    return self.insert(data);
                }
            }
            return Err(e.into());
        }

        // safe to unwrap since we insert before
        let last_row = self.get_last_row()?.unwrap();

        if let Some(old_id) = self.hashs.remove(&data.get_hash()) {
            self.state.remove(&old_id);

            // in case 2 same data were inserted in a short period
            // we don't want to remove the old_id
            if last_row.creation != old_id {
                let query_delete_old_id = r#"
                    DELETE FROM data
                    WHERE creation = ?1;
                "#;

                self.conn.execute(query_delete_old_id, [old_id])?;
            }
        }

        data.creation = last_row.creation;

        self.hashs.insert(data.get_hash(), data.creation);
        self.state.insert(data.creation, data);

        self.search();
        Ok(())
    }

    pub fn delete(&mut self, data: &Entry) -> Result<()> {
        let query = r#"
            DELETE FROM data
            WHERE creation = ?1;
        "#;

        self.conn.execute(query, [data.creation])?;

        self.hashs.remove(&data.get_hash());
        self.state.remove(&data.creation);

        self.search();
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let query_delete = r#"
            DELETE FROM data
        "#;
        self.conn.execute(query_delete, [])?;

        self.state.clear();
        self.filtered.clear();
        self.hashs.clear();

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
                    data.get_text().and_then(|text| {
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

    #[test]
    #[serial]
    fn test() -> Result<()> {
        let db_dir = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_dir)?;

        test_db(&mut db).unwrap();

        db.clear()?;

        test_db(&mut db).unwrap();

        Ok(())
    }

    fn test_db(db: &mut Db) -> Result<()> {
        assert!(db.len() == 0);

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into());

        db.insert(data).unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into());

        db.insert(data).unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Entry::new_now("text/plain".into(), "content2".as_bytes().into());

        db.insert(data.clone()).unwrap();

        assert!(db.len() == 2);

        let next = db.iter().next().unwrap();

        assert!(next.creation == data.creation);
        assert!(next.content == data.content);

        Ok(())
    }

    #[test]
    #[serial]
    fn test_delete_old_one() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).unwrap();

        let data = Entry::new_now("text/plain".into(), "content".as_bytes().into());
        db.insert(data).unwrap();

        sleep(Duration::from_millis(100));

        let data = Entry::new_now("text/plain".into(), "content2".as_bytes().into());
        db.insert(data).unwrap();

        assert!(db.len() == 2);

        let db = Db::inner_new(&Config::default(), &db_path).unwrap();

        assert!(db.len() == 2);

        let config = Config {
            maximum_entries_lifetime: Some(Duration::ZERO),
            ..Default::default()
        };
        let db = Db::inner_new(&config, &db_path).unwrap();

        assert!(db.len() == 0);
    }

    #[test]
    #[serial]
    fn same() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).unwrap();

        let now = utils::now_millis();

        let data = Entry {
            creation: now,
            mime: "text/plain".into(),
            content: "content".as_bytes().into(),
        };

        db.insert(data).unwrap();

        let data = Entry {
            creation: now,
            mime: "text/plain".into(),
            content: "content".as_bytes().into(),
        };

        db.insert(data).unwrap();
        assert!(db.len() == 1);
    }

    #[test]
    #[serial]
    fn different_content_same_time() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(&Config::default(), &db_path).unwrap();

        let now = utils::now_millis();

        let data = Entry {
            creation: now,
            mime: "text/plain".into(),
            content: "content".as_bytes().into(),
        };

        db.insert(data).unwrap();

        let data = Entry {
            creation: now,
            mime: "text/plain".into(),
            content: "content2".as_bytes().into(),
        };

        db.insert(data).unwrap();
        assert!(db.len() == 2);
    }
}
