use derivative::Derivative;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    fs::{self, DirBuilder, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    path::Path,
    thread::sleep,
    time::Duration,
};

use aho_corasick::AhoCorasick;
use anyhow::{anyhow, bail, Result};

use chrono::Utc;
use mime::Mime;
use rusqlite::{named_params, Connection, OpenFlags};
use unicode_normalization::UnicodeNormalization;

use crate::app::{APP, ORG, QUALIFIER};

const DB_FILE: &str = "clipboard-manager-db-1.sqlite";

type TimeId = i64; // maybe add some randomness at the end

#[derive(Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Debug, Clone, Eq)]
pub struct Data {
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub creation: TimeId,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub mime: String,

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
    pub fn new(remove_old_entries: &Option<Duration>) -> Result<Self> {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();

        std::fs::create_dir_all(directories.cache_dir())?;

        let db_path = directories.cache_dir().join(DB_FILE);

        if !db_path.exists() {
            let conn = Connection::open_with_flags(
                db_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            )?;

            let query = r#"
                CREATE TABLE data (
                    creation INTEGER PRIMARY KEY,
                    mime TEXT NOT NULL,
                    content BLOB NOT NULL
                )
            "#;

            conn.execute(query, ())?;

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
                    let delta = Utc::now().timestamp_millis() - data.creation;
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

    pub fn insert(&mut self, data: Data) -> Result<()> {
        if let Some(id) = self.hashs.remove(&data.get_hash()) {
            self.state.remove(&id);
            let query = r#"
                DELETE FROM data
                WHERE creation = ?1;
            "#;

            self.conn.execute(query, [id])?;
        }

        let query = r#"
            INSERT INTO data (creation, mime, content)
            VALUES(?1, ?2, ?3);
        "#;

        self.conn
            .execute(query, (&data.creation, &data.mime, &data.content))?;

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
        let query = r#"
            DROP TABLE data
        "#;
        self.conn.execute(query, [])?;

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
        fs::File,
        io::{Read, Write},
    };

    use super::Data;

    #[test]
    fn t() {
        // 9754003402134539932

        let data = Data::new("mime".into(), "hello".as_bytes().into());

        let hash = data.get_hash();

        println!("{}", hash);
    }
}
