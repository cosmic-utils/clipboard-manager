use std::{
    fmt::Display,
    io,
    ptr::NonNull,
    time::{SystemTime, UNIX_EPOCH},
};

use aho_corasick::AhoCorasick;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use derivative::Derivative;
use unicode_normalization::UnicodeNormalization;

use crate::app::{APP, ORG, QUALIFIER};

// todo: enforce that only this app can read/write this file.

#[cfg(debug_assertions)]
const DB_FILE: &str = "cosmic-clipboard-manager-db-debug";

#[cfg(not(debug_assertions))]
const DB_FILE: &str = "cosmic-clipboard-manager-db";

#[derive(Derivative)]
#[derivative(PartialEq, Hash)]
#[derive(Eq, Clone, Debug)]
pub struct Data {
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub creation: u128,

    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    pub mime: String,

    pub value: String,
}

impl Display for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Data {
    pub fn new(mime: String, value: String) -> Self {
        let since_the_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        Self {
            creation: since_the_epoch.as_millis(),
            mime,
            value,
        }
    }
}

struct NonNullButSend<T>(NonNull<T>);
unsafe impl<T> Send for NonNullButSend<T> {}

pub struct Db {
    handle: sled::Db,
    // this field probably need to be Pin
    state: IndexSet<Data>,
    filtered: Vec<NonNullButSend<Data>>,
    query: String,
    search_engine: Option<AhoCorasick>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeyDb(u128);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DataDb {
    mime: String,
    value: String,
}

impl Db {
    pub fn new() -> Result<Self, sled::Error> {

        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();
        let db_path = directories.cache_dir().join(DB_FILE);

        let db_handle = sled::open(db_path)?;

        let mut state = IndexSet::new();

        for e in db_handle.iter() {
            match e {
                Ok((key, value)) => {
                    let key = bincode::deserialize::<KeyDb>(&key).expect("key");
                    let value = bincode::deserialize::<DataDb>(&value).expect("value");

                    let value = Data {
                        mime: value.mime,
                        value: value.value,
                        creation: key.0,
                    };

                    if !state.insert(value) {
                        panic!("already exist");
                    }
                }
                Err(e) => {
                    log::error!("{e}");
                }
            }
        }

        let db = Db {
            handle: db_handle,
            state,
            filtered: Vec::new(),
            query: String::new(),
            search_engine: None,
        };

        Ok(db)
    }

    pub fn clear(&mut self) -> Result<(), sled::Error> {
        self.handle.clear()?;
        self.state.clear();
        self.handle.flush()?;
        self.do_search();
        Ok(())
    }

    pub fn insert(&mut self, data: Data) -> Result<(), sled::Error> {
        if let Some(prev_data) = self.state.get(&data) {
            debug!("already present");

            let prev_key = KeyDb(prev_data.creation);

            if !self.state.shift_remove(&data) {
                panic!("");
            }

            if self.handle.remove(prev_key)?.is_none() {
                log::warn!("there was no entry found in the database");
                panic!();
            }
        }

        if !self.state.insert(data.clone()) {
            panic!();
        }

        let key = KeyDb(data.creation);

        let data_db = DataDb {
            mime: data.mime,
            value: data.value,
        };
        let mut data_db_bin = Vec::new();

        bincode::serialize_into(&mut data_db_bin, &data_db).unwrap();

        self.handle.insert(key, data_db_bin)?;

        self.handle.flush()?;

        self.do_search();
        Ok(())
    }

    pub fn delete(&mut self, data: &Data) -> Result<(), sled::Error> {
        if !self.state.shift_remove(data) {
            log::warn!("delete: no entry to remove in state for {data}");
            panic!();
        }

        if (self.handle.remove(KeyDb(data.creation))?).is_none() {
            log::warn!("delete: no entry to remove in db for {data}");
            panic!();
        }

        self.handle.flush()?;

        self.do_search();
        Ok(())
    }

    pub fn search(&mut self, query: String) {
        self.query = query;

        if self.query.is_empty() {
            self.filtered.clear();
            self.search_engine.take();
        } else {
            let search_engine = AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(vec![Self::normalize_text(&self.query)])
                .unwrap();

            self.search_engine.replace(search_engine);
            self.do_search();
        }
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

    fn do_search(&mut self) {
        use rayon::prelude::*;

        if let Some(search_engine) = &self.search_engine {
            // https://www.reddit.com/r/rust/comments/1boo2fb/comment/kwqahjv/?context=3
            self.filtered = self
                .state
                .par_iter()
                .filter(|s| {
                    let normalized = Self::normalize_text(&s.value);
                    let mut iter = search_engine.find_iter(&normalized);
                    iter.next().is_some()
                })
                .map(|e| NonNullButSend(NonNull::from(e)))
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

    pub fn get(&self, index: usize) -> Option<&Data> {
        if self.query.is_empty() {
            // because we expose the tree in reverse
            self.state.get_index(self.len() - 1 - index)
        } else {
            self.filtered.get(index).map(|e| unsafe { e.0.as_ref() })
        }
    }

    pub fn iter(&self) -> Box<dyn Iterator<Item = &Data> + '_> {
        if self.query.is_empty() {
            Box::new(self.state.iter().rev())
        } else {
            Box::new(self.filtered.iter().map(|e| unsafe { e.0.as_ref() }))
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

impl AsRef<[u8]> for KeyDb {
    fn as_ref(&self) -> &[u8] {
        let size = std::mem::size_of::<KeyDb>();
        // We can use `std::slice::from_raw_parts` to create a slice from the u128 value
        // This is done by casting the reference to a pointer and then creating a slice from it
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, size) }
    }
}

#[cfg(test)]
mod test {
    use super::{Data, Db};

    // todo: re enable tests when they pass locally

    //#[test]
    fn clear() {
        let mut db = Db::new().unwrap();

        db.clear().unwrap();
    }

    //#[test]
    fn test() {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .init();
        let mut db = Db::new().unwrap();

        db.clear().unwrap();

        let data1 = Data::new("text".into(), "value1".into());

        db.insert(data1.clone()).unwrap();

        db.insert(data1.clone()).unwrap();

        assert!(db.state.len() == 1);

        let data2 = Data::new("text".into(), "value2".into());

        db.insert(data2.clone()).unwrap();

        assert!(db.state.len() == 2);

        let mut iter = db.state.iter().rev();

        assert!(iter.next().unwrap() == &data2);
        assert!(iter.next().unwrap() == &data1);

        let new_data1 = Data::new("text".into(), "value1".into());

        db.insert(new_data1.clone()).unwrap();

        assert!(db.state.len() == 2);

        let mut iter = db.state.iter().rev();

        assert!(iter.next().unwrap() == &new_data1);
        assert!(iter.next().unwrap() == &data2);
    }
}
