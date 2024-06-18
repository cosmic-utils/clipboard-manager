use cosmic::iced_sctk::util;
use derivative::Derivative;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    fs::{self, DirBuilder, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use aho_corasick::AhoCorasick;
use anyhow::{anyhow, bail, Result};

use chrono::Utc;
use mime::Mime;
use rusqlite::{named_params, params, Connection, OpenFlags, OptionalExtension};
use unicode_normalization::UnicodeNormalization;

use crate::{
    app::{APP, ORG, QUALIFIER},
    utils,
};

fn db_path() -> Result<PathBuf> {
    if cfg!(test) {
        Ok(PathBuf::from("clipboard-manager-db-test.sqlite"))
    } else {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();
        std::fs::create_dir_all(directories.cache_dir())?;
        Ok(directories
            .cache_dir()
            .join("clipboard-manager-db-1.sqlite"))
    }
}

type TimeId = i64; // maybe add some randomness at the end

// warning: if you change somethings in here, change the number in the db path
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
                let text = unsafe { core::str::from_utf8_unchecked(&self.content) };
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

        let db_path = db_path()?;

        if !db_path.exists() {
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

        let mut hashs = HashMap::default();
        let mut state = BTreeMap::default();

        let query = r#"
            SELECT creation, mime, content
            FROM data
        "#;

        {
            let mut stmt = conn.prepare(query)?;

            let mut rows = stmt.query(())?;

            while let Some(row) = rows.next()? {
                let data = Data {
                    creation: row.get(0)?,
                    mime: row.get(1)?,
                    content: row.get(2)?,
                };

                if let Some(max_duration) = &remove_old_entries {
                    let delta = utils::now_millis() - data.creation;
                    let delta: u64 = delta.try_into().unwrap_or(u64::MAX);

                    if Duration::from_millis(delta) > *max_duration {
                        let query = r#"
                            DELETE FROM data
                            WHERE creation = ?1;
                        "#;

                        conn.execute(query, [data.creation])?;

                        continue;
                    }
                }

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

        self.conn.execute(
            query_insert_if_not_exist,
            named_params! {
                ":new_id": data.creation,
                ":new_mime": data.mime,
                ":new_content": data.content,
                ":now": utils::now_millis(),
            },
        )?;

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
        time::Duration,
    };

    use serial_test::serial;

    use anyhow::Result;
    use cosmic::iced_sctk::util;

    use crate::utils;

    use super::{db_path, Data, Db};

    #[test]
    #[serial]
    fn test() -> Result<()> {
        let _ = fs::remove_file(db_path()?);

        let mut db = Db::new(None)?;

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

        let data = Data::new("text/plain".into(), "content".as_bytes().into());

        db.insert(data).unwrap();

        assert!(db.len() == 1);

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
        let _ = fs::remove_file(db_path().unwrap());

        let mut db = Db::new(None).unwrap();

        let data = Data::new("text/plain".into(), "content".as_bytes().into());
        db.insert(data).unwrap();

        let data = Data::new("text/plain".into(), "content2".as_bytes().into());
        db.insert(data).unwrap();

        assert!(db.len() == 2);

        let db = Db::new(Some(Duration::from_secs(10))).unwrap();

        assert!(db.len() == 2);

        let db = Db::new(Some(Duration::from_secs(0))).unwrap();

        assert!(db.len() == 0);
    }

    #[test]
    #[serial]
    fn same() {
        let _ = fs::remove_file(db_path().unwrap());

        let mut db = Db::new(None).unwrap();

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
}
