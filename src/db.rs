use std::{
    fmt::Display,
    time::{SystemTime, UNIX_EPOCH},
};

use aho_corasick::AhoCorasick;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use derivative::Derivative;

// todo: enforce that only this app can read/write this file.
const DB_PATH: &str = "/tmp/welcome-to-sled";

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

pub struct Db {
    handle: sled::Db,
    state: IndexSet<Data>,
}

impl Db {
    pub fn iter(&self) -> indexmap::set::Iter<'_, Data> {
        self.state.iter()
    }
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
        let db_handle = sled::open(DB_PATH)?;

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

                    // todo: remove this clone
                    if !state.insert(value.clone()) {
                        panic!("already exist: {}", value);
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
        };

        Ok(db)
    }

    pub fn clear(&mut self) -> Result<(), sled::Error> {
        self.handle.clear()?;
        self.state.clear();
        self.handle.flush()?;
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
        Ok(())
    }

    pub fn search(&self, query: &str) -> Vec<&Data> {
        use rayon::prelude::*;

        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(vec![query])
            .unwrap();

        self.state
            .par_iter()
            .filter(|s| {
                let mut iter = ac.find_iter(&s.value);
                return iter.next().is_some()
            })
            .collect()
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

mod test {
    use super::{Data, Db};

    #[test]
    fn clear() {
        let mut db = Db::new().unwrap();

        db.clear().unwrap();
    }

    #[test]
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
