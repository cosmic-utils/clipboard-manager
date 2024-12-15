use alive_lock_file::LockResult;
use derivative::Derivative;
use futures::StreamExt;
use sqlx::{migrate::MigrateDatabase, prelude::*, Sqlite, SqliteConnection};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use anyhow::{anyhow, Result};
use nucleo::{
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
    Matcher, Utf32Str,
};

use crate::{
    app::{APP, APPID, ORG, QUALIFIER},
    config::Config,
    utils::{self},
};

use super::{now, DbMessage, DbTrait, EntryId, EntryTrait, MimeDataMap};

type Time = i64;

const DB_VERSION: &str = "5";
const DB_PATH: &str = constcat::concat!(APPID, "-db-", DB_VERSION, ".sqlite");

const LOCK_FILE: &str = constcat::concat!(APPID, "-db", ".lock");

pub struct DbSqlite {
    conn: SqliteConnection,
    /// Hash -> Id
    hashs: HashMap<u64, EntryId>,
    /// time -> Id
    times: BTreeMap<Time, EntryId>,
    /// Id -> Entry
    entries: HashMap<EntryId, Entry>,
    filtered: Vec<EntryId>,
    query: String,
    needle: Option<Atom>,
    matcher: RefCell<Matcher>,
    data_version: i64,
    pub(super) favorites: Favorites,
}

#[derive(Clone, Eq, Derivative)]
pub struct Entry {
    pub id: EntryId,
    pub creation: Time,
    // todo: lazelly load image in memory, since we can't search them anyways
    /// (Mime, Content)
    pub raw_content: MimeDataMap,
    pub is_favorite: bool,
}

#[derive(Default)]
pub(super) struct Favorites {
    favorites: Vec<EntryId>,
    favorites_hash_set: HashSet<EntryId>,
}

impl Favorites {
    fn contains(&self, id: &EntryId) -> bool {
        self.favorites_hash_set.contains(id)
    }
    fn clear(&mut self) {
        self.favorites.clear();
        self.favorites_hash_set.clear();
    }

    fn insert_at(&mut self, id: EntryId, pos: Option<usize>) {
        match pos {
            Some(pos) => self.favorites.insert(pos, id),
            None => self.favorites.push(id),
        }
        self.favorites_hash_set.insert(id);
    }

    fn remove(&mut self, id: &EntryId) -> Option<usize> {
        self.favorites_hash_set.remove(id);
        self.favorites.iter().position(|e| e == id).inspect(|i| {
            self.favorites.remove(*i);
        })
    }

    pub(super) fn fav(&self) -> &Vec<EntryId> {
        &self.favorites
    }

    #[allow(dead_code)]
    fn change(&mut self, prev: &EntryId, new: EntryId) {
        let pos = self.favorites.iter().position(|e| e == prev).unwrap();
        self.favorites[pos] = new;
        self.favorites_hash_set.remove(prev);
        self.favorites_hash_set.insert(new);
    }

    pub(super) fn len(&self) -> usize {
        self.favorites.len()
    }
}

fn hash_entry_content<H: Hasher>(data: &MimeDataMap, state: &mut H) {
    for e in data.values() {
        e.hash(state);
    }
}

fn get_hash_entry_content(data: &MimeDataMap) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_entry_content(data, &mut hasher);
    hasher.finish()
}

impl Hash for Entry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_entry_content(&self.raw_content, state);
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl EntryTrait for Entry {
    fn is_favorite(&self) -> bool {
        self.is_favorite
    }

    fn raw_content(&self) -> &MimeDataMap {
        &self.raw_content
    }

    fn id(&self) -> EntryId {
        self.id
    }

    fn into_raw_content(self) -> MimeDataMap {
        self.raw_content
    }
}

impl Entry {
    fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Data")
            .field("id", &self.id)
            .field("creation", &self.creation)
            .field("content", &self.viewable_content())
            .finish()
    }
}

impl DbTrait for DbSqlite {
    type Entry = Entry;

    async fn new(config: &Config) -> Result<Self> {
        let directories = directories::ProjectDirs::from(QUALIFIER, ORG, APP).unwrap();

        std::fs::create_dir_all(directories.cache_dir())?;

        Self::with_path(config, directories.cache_dir()).await
    }

    async fn with_path(config: &Config, db_dir: &Path) -> Result<Self> {
        if let Err(e) = alive_lock_file::remove_lock(LOCK_FILE) {
            error!("can't remove lock {e}");
        }

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
                WHERE (? - creation) >= ? AND id NOT IN(
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
            let query_get_most_older = r#"
                SELECT creation
                FROM ClipboardEntries
                ORDER BY creation DESC
                LIMIT 1 OFFSET ?
            "#;

            match sqlx::query(query_get_most_older)
                .bind(max_number_of_entries)
                .fetch_optional(&mut conn)
                .await
                .unwrap()
            {
                Some(r) => {
                    let creation: Time = r.get("creation");

                    let query_delete_old_one = r#"
                
                            DELETE FROM ClipboardEntries
                            WHERE creation < ? AND id NOT IN (
                                SELECT id
                                FROM FavoriteClipboardEntries);
                            "#;

                    sqlx::query(query_delete_old_one)
                        .bind(creation)
                        .execute(&mut conn)
                        .await
                        .unwrap();
                }
                None => {
                    // nothing to do
                }
            }
        }

        let mut db = DbSqlite {
            data_version: fetch_data_version(&mut conn).await?,
            conn,
            hashs: HashMap::default(),
            times: BTreeMap::default(),
            entries: HashMap::default(),
            filtered: Vec::default(),
            query: String::default(),
            needle: None,
            matcher: Matcher::new(nucleo::Config::DEFAULT).into(),
            favorites: Favorites::default(),
        };

        db.reload().await?;

        Ok(db)
    }

    async fn reload(&mut self) -> Result<()> {
        self.hashs.clear();
        self.entries.clear();
        self.times.clear();
        self.favorites.clear();

        // init favorite
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

        // init entries and times
        {
            let query_load_table = r#"
                SELECT id, creation
                FROM ClipboardEntries
            "#;

            let mut stream = sqlx::query(query_load_table).fetch(&mut self.conn);

            while let Some(res) = stream.next().await {
                let row = res?;

                let id = row.get("id");
                let creation = row.get("creation");

                let entry = Entry {
                    id,
                    creation,
                    raw_content: MimeDataMap::default(),
                    is_favorite: self.favorites.contains(&id),
                };

                self.entries.insert(id, entry);

                self.times.insert(creation, id);
            }
        }

        // init contents
        {
            // todo: we can probably optimize by sorting with id

            let query_load_table = r#"
                SELECT id, mime, content
                FROM ClipboardContents
            "#;

            let mut stream = sqlx::query(query_load_table).fetch(&mut self.conn);

            while let Some(res) = stream.next().await {
                let row = res?;

                let id = row.get("id");
                let mime: String = row.get("mime");
                let content: Vec<u8> = row.get("content");

                let entry = self.entries.get_mut(&id).expect("entry should exist");
                entry.raw_content.insert(mime, content);
            }
        }

        // init hashs
        {
            for entry in self.entries.values() {
                self.hashs.insert(entry.get_hash(), entry.id());
            }
        }

        self.search();

        Ok(())
    }

    fn get_from_id(&self, id: EntryId) -> Option<&Self::Entry> {
        self.entries.get(&id)
    }

    async fn insert(&mut self, data: MimeDataMap) -> Result<()> {
        self.insert_with_time(data, now()).await
    }
    async fn insert_with_time(&mut self, data: MimeDataMap, now: i64) -> Result<()> {
        match alive_lock_file::try_lock(LOCK_FILE)? {
            LockResult::Success => {}
            LockResult::AlreadyLocked => {
                info!("db already locked");
                return Ok(());
            }
        }

        let hash = get_hash_entry_content(&data);

        if let Some(id) = self.hashs.get(&hash) {
            let entry = self.entries.get_mut(id).unwrap();
            entry.creation = now;
            self.times.remove(&entry.creation);
            self.times.insert(now, *id);

            let query_update_creation = r#"
                UPDATE ClipboardEntries
                SET creation = $1
                WHERE id = $2;
            "#;

            sqlx::query(query_update_creation)
                .bind(now)
                .bind(id)
                .execute(&mut self.conn)
                .await?;
        } else {
            let id = now;

            let query_insert_new_entry = r#"
                INSERT INTO ClipboardEntries (id, creation)
                SELECT $1, $2
            "#;

            sqlx::query(query_insert_new_entry)
                .bind(id)
                .bind(now)
                .execute(&mut self.conn)
                .await?;

            for (mime, content) in &data {
                let query_insert_content = r#"
                    INSERT INTO ClipboardContents (id, mime, content)
                    SELECT $1, $2, $3
                "#;

                sqlx::query(query_insert_content)
                    .bind(id)
                    .bind(mime)
                    .bind(content)
                    .execute(&mut self.conn)
                    .await?;
            }

            let entry = Entry {
                id,
                creation: now,
                raw_content: data,
                is_favorite: false,
            };

            self.times.insert(now, id);
            self.hashs.insert(hash, id);
            self.entries.insert(id, entry);
        }

        self.search();
        Ok(())
    }

    async fn delete(&mut self, id: EntryId) -> Result<()> {
        let query = r#"
            DELETE FROM ClipboardEntries
            WHERE id = ?;
        "#;

        sqlx::query(query).bind(id).execute(&mut self.conn).await?;

        match self.entries.remove(&id) {
            Some(entry) => {
                self.hashs.remove(&entry.get_hash());
                self.times.remove(&entry.creation);

                if entry.is_favorite() {
                    self.favorites.remove(&entry.creation);
                }
            }
            None => {
                warn!("no entry to remove")
            }
        }

        self.search();
        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        let query_delete = r#"
            DELETE FROM ClipboardEntries
            WHERE id NOT IN(
                SELECT id
                FROM FavoriteClipboardEntries
            );
        "#;

        sqlx::query(query_delete).execute(&mut self.conn).await?;

        self.reload().await?;

        Ok(())
    }

    async fn add_favorite(&mut self, id: EntryId, index: Option<usize>) -> Result<()> {
        debug_assert!(!self.favorites.fav().contains(&id));

        self.favorites.insert_at(id, index);

        if let Some(pos) = index {
            let query_bump_positions = r#"
                UPDATE FavoriteClipboardEntries
                SET position = position + 1
                WHERE position >= ?;
            "#;
            sqlx::query(query_bump_positions)
                .bind(pos as i32)
                .execute(&mut self.conn)
                .await
                .unwrap();
        }

        let index = index.unwrap_or(self.favorites.len() - 1);

        {
            let query = r#"
                INSERT INTO FavoriteClipboardEntries (id, position)
                VALUES ($1, $2);
            "#;

            sqlx::query(query)
                .bind(id)
                .bind(index as i32)
                .execute(&mut self.conn)
                .await?;
        }

        if let Some(e) = self.entries.get_mut(&id) {
            e.is_favorite = true;
        }

        Ok(())
    }

    async fn remove_favorite(&mut self, id: EntryId) -> Result<()> {
        debug_assert!(self.favorites.fav().contains(&id));

        {
            let query = r#"
                DELETE FROM FavoriteClipboardEntries
                WHERE id = ?;
            "#;

            sqlx::query(query).bind(id).execute(&mut self.conn).await?;
        }

        if let Some(pos) = self.favorites.remove(&id) {
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

        if let Some(e) = self.entries.get_mut(&id) {
            e.is_favorite = false;
        }

        Ok(())
    }

    fn search(&mut self) {
        if self.query.is_empty() {
            self.filtered.clear();
        } else if let Some(atom) = &self.needle {
            self.filtered = self
                .iter()
                .filter_map(|entry| {
                    if entry.searchable_content().any(|text| {
                        let mut buf = Vec::new();

                        let haystack = Utf32Str::new(text, &mut buf);

                        let mut indices = Vec::new();

                        let _res =
                            atom.indices(haystack, &mut self.matcher.borrow_mut(), &mut indices);

                        !indices.is_empty()
                    }) {
                        Some(entry.id)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
        }
    }

    fn set_query_and_search(&mut self, query: String) {
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

    fn get_query(&self) -> &str {
        &self.query
    }

    fn get(&self, index: usize) -> Option<&Self::Entry> {
        self.iter().nth(index)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &'_ Self::Entry> + '_> {
        if self.is_search_active() {
            Box::new(self.filtered.iter().filter_map(|id| self.entries.get(id)))
        } else {
            Box::new(
                self.favorites
                    .fav()
                    .iter()
                    .filter_map(|id| self.entries.get(id))
                    .chain(
                        self.times
                            .values()
                            .filter_map(|id| self.entries.get(id))
                            .filter(|e| !e.is_favorite)
                            .rev(),
                    ),
            )
        }
    }

    fn len(&self) -> usize {
        if self.query.is_empty() {
            self.entries.len()
        } else {
            self.filtered.len()
        }
    }

    async fn handle_message(&mut self, _message: DbMessage) -> Result<()> {
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
