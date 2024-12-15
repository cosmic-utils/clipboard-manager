use alive_lock_file::LockResult;
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

// #[cfg(test)]
// pub mod test;

mod sqlite_db;
pub use sqlite_db::DbSqlite;


fn now() -> i64 {
    Utc::now().timestamp_millis()
}

pub type EntryId = i64;
pub type MimeDataMap = HashMap<String, Vec<u8>>;

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



pub trait EntryTrait: Debug + Clone + Send {

    fn is_favorite(&self) -> bool;

    fn content(&self) -> MimeDataMap;

    fn id(&self) -> EntryId;

    fn qr_code_content(&self) -> &[u8];


     fn get_content(&self) -> Result<Content<'_>> {
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

pub trait DbTrait: Sized {

    type Entry: EntryTrait;

    async fn new(config: &Config) -> Result<Self>;

    async fn with_path(config: &Config, db_dir: &Path) -> Result<Self>;

    async fn reload(&mut self) -> Result<()>;

    fn insert<'a: 'b, 'b>(&'a mut self, data: MimeDataMap) -> BoxFuture<'b, Result<()>>;

    async fn delete(&mut self, data: EntryId) -> Result<()>;

    async fn clear(&mut self) -> Result<()>;

    async fn add_favorite(&mut self, entry: EntryId, index: Option<usize>) -> Result<()>;

    async fn remove_favorite(&mut self, entry: EntryId) -> Result<()>;

    fn favorite_len(&self) -> usize;

    fn search(&mut self);

    fn set_query_and_search(&mut self, query: String);

    fn query(&self) -> &str;

    fn get(&self, index: usize) -> Option<&Self::Entry>;

    fn get_from_id(&self, id: EntryId) -> Option<&Self::Entry>;

    fn iter(&self) -> impl Iterator<Item = &'_ Self::Entry>;

    fn search_iter(&self) -> impl Iterator<Item = (&'_ Self::Entry, &'_ Vec<u32>)>;

    fn len(&self) -> usize;

    async fn handle_message(&mut self, message: DbMessage) -> Result<()>;
}

#[derive(Clone, Debug)]
pub enum DbMessage {
    CheckUpdate,
}
