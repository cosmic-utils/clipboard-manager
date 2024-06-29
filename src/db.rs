use cosmic::iced_sctk::util;
use derivative::Derivative;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    fs::{self, DirBuilder, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use aho_corasick::AhoCorasick;
use anyhow::{anyhow, bail, Result};

use chrono::Utc;
use mime::Mime;
use rusqlite::{named_params, params, Connection, ErrorCode, OpenFlags, OptionalExtension};
use unicode_normalization::UnicodeNormalization;

use crate::{
    app::{APP, APPID, ORG, QUALIFIER},
    utils::{self, remove_dir_contents},
};

type TimeId = i64;

const DB_VERSION: &str = "1";
const DB_PATH: &str = constcat::concat!(APPID, "-db-", DB_VERSION, ".sqlite");

// warning: if you change somethings in here, change the db version
#[derive(Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Clone, Eq)]
pub struct Data {
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub creation: TimeId,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub mime: String,

    // todo: lazelly load image in memory, since we can't search them anyways
    pub content: Vec<u8>,
}

impl Data {
    fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn new(mime: String, content: Vec<u8>) -> Self {
        Self {
            creation: Utc::now().timestamp_millis(),
            mime,
            content,
        }
    }
}

impl Debug for Data {
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
}

impl Debug for Content<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(arg0) => f.debug_tuple("Text").field(arg0).finish(),
        }
    }
}

impl Data {
    pub fn get_content(&self) -> Result<Content<'_>> {
        let mime = self.mime.parse::<Mime>()?;

        let content = match mime.type_() {
            mime::TEXT => {
                let text = unsafe {
                    match core::str::from_utf8(&self.content) {
                        Ok(txt) => txt,
                        Err(e) => {
                            core::str::from_utf8_unchecked(self.content.split_at(e.valid_up_to()).0)
                        }
                    }
                };
                Content::Text(text)
            }
            _ => bail!("unsuported mime type"),
        };

        Ok(content)
    }

    pub fn get_text(&self) -> Option<&str> {
        self.get_content().ok().map(|c| match c {
            Content::Text(txt) => txt,
        })
    }
}

pub struct Db {
    conn: Connection,
    hashs: HashMap<u64, TimeId>,
    state: BTreeMap<TimeId, Data>,
    filtered: Vec<TimeId>,
    query: String,
    search_engine: Option<AhoCorasick>,
}

impl Db {
    pub fn new(remove_old_entries: Option<Duration>) -> Result<Self> {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();

        std::fs::create_dir_all(directories.cache_dir())?;

        Self::inner_new(remove_old_entries, directories.cache_dir())
    }

    fn inner_new(remove_old_entries: Option<Duration>, db_dir: &Path) -> Result<Self> {
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
                search_engine: None,
            });
        }

        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;

        if let Some(max_duration) = &remove_old_entries {
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
                let data = Data {
                    creation: row.get(0)?,
                    mime: row.get(1)?,
                    content: row.get(2)?,
                };

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
            search_engine: None,
        };

        Ok(db)
    }

    fn get_last_row(&mut self) -> Result<Option<Data>> {
        let query_get_last_row = r#"
            SELECT creation, mime, content
            FROM data
            ORDER BY creation DESC
            LIMIT 1
        "#;

        let data = self
            .conn
            .query_row(query_get_last_row, (), |row| {
                Ok(Data {
                    creation: row.get(0)?,
                    mime: row.get(1)?,
                    content: row.get(2)?,
                })
            })
            .optional()?;

        Ok(data)
    }

    // the <= 200 condition, is to unsure we reuse the same timestamp
    // of the first process that inserted the data.
    pub fn insert(&mut self, mut data: Data) -> Result<()> {
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

    pub fn delete(&mut self, data: &Data) -> Result<()> {
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
        use rayon::prelude::*;

        if self.query.is_empty() {
            self.filtered.clear();
        } else if let Some(search_engine) = &self.search_engine {
            // https://www.reddit.com/r/rust/comments/1boo2fb/comment/kwqahjv/?context=3
            self.filtered = self
                .state
                .par_iter()
                .filter_map(|(id, data)| {
                    data.get_text().and_then(|text| {
                        let normalized = Self::normalize_text(text);
                        let mut iter = search_engine.find_iter(&normalized);
                        iter.next().map(|_| *id)
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                // we can't call rev on par_iter and par_bridge
                // doesn't preserve order + it's slower
                // maybe droping completelly rayon could be better
                // https://github.com/rayon-rs/rayon/issues/551
                .rev()
                .collect();
        }
    }

    pub fn set_query_and_search(&mut self, query: String) {
        if query.is_empty() {
            self.search_engine.take();
        } else {
            let search_engine = AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(vec![Self::normalize_text(&query)])
                .unwrap();

            self.search_engine.replace(search_engine);
        }

        self.query = query;

        self.search();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    fn normalize_text(value: &str) -> String {
        // collect<Cow<str>> always have the type Owned("...")
        // in my test, and it don't seems to be an acceptable
        // format for other api
        // https://github.com/unicode-rs/unicode-normalization/issues/99
        value.nfkd().collect()
    }

    pub fn get(&self, index: usize) -> Option<&Data> {
        if self.query.is_empty() {
            // because we expose the tree in reverse
            self.state.iter().rev().nth(index).map(|e| e.1)
        } else {
            self.filtered.get(index).map(|id| &self.state[id])
        }
    }

    pub fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Data> + 'a> {
        if self.query.is_empty() {
            Box::new(self.state.values().rev())
        } else {
            Box::new(self.filtered.iter().map(|id| &self.state[id]))
        }
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

    use crate::utils::{self, remove_dir_contents};

    use super::{Data, Db};

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

        let mut db = Db::inner_new(None, &db_dir)?;

        test_db(&mut db).unwrap();

        db.clear()?;

        test_db(&mut db).unwrap();

        Ok(())
    }

    fn test_db(db: &mut Db) -> Result<()> {
        assert!(db.len() == 0);

        let data = Data::new("text/plain".into(), "content".as_bytes().into());

        db.insert(data).unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Data::new("text/plain".into(), "content".as_bytes().into());

        db.insert(data).unwrap();

        assert!(db.len() == 1);

        sleep(Duration::from_millis(1000));

        let data = Data::new("text/plain".into(), "content2".as_bytes().into());

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

        let mut db = Db::inner_new(None, &db_path).unwrap();

        let data = Data::new("text/plain".into(), "content".as_bytes().into());
        db.insert(data).unwrap();

        sleep(Duration::from_millis(100));

        let data = Data::new("text/plain".into(), "content2".as_bytes().into());
        db.insert(data).unwrap();

        assert!(db.len() == 2);

        let db = Db::inner_new(Some(Duration::from_secs(10)), &db_path).unwrap();

        assert!(db.len() == 2);

        let db = Db::inner_new(Some(Duration::ZERO), &db_path).unwrap();

        assert!(db.len() == 0);
    }

    #[test]
    #[serial]
    fn same() {
        let db_path = prepare_db_dir();

        let mut db = Db::inner_new(None, &db_path).unwrap();

        let now = utils::now_millis();

        let data = Data {
            creation: now,
            mime: "text/plain".into(),
            content: "content".as_bytes().into(),
        };

        db.insert(data).unwrap();

        let data = Data {
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

        let mut db = Db::inner_new(None, &db_path).unwrap();

        let now = utils::now_millis();

        let data = Data {
            creation: now,
            mime: "text/plain".into(),
            content: "content".as_bytes().into(),
        };

        db.insert(data).unwrap();

        let data = Data {
            creation: now,
            mime: "text/plain".into(),
            content: "content2".as_bytes().into(),
        };

        db.insert(data).unwrap();
        assert!(db.len() == 2);
    }
}
