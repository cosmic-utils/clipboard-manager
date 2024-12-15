use futures::future::BoxFuture;
use std::{collections::HashMap, fmt::Debug, path::Path};

use anyhow::{bail, Result};

use chrono::Utc;

use crate::config::Config;

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

pub trait EntryTrait: Debug + Clone + Send {
    fn is_favorite(&self) -> bool;

    fn raw_content(&self) -> &MimeDataMap;

    fn into_raw_content(self) -> MimeDataMap;

    fn id(&self) -> EntryId;

    // todo: prioritize certain mime types
    fn qr_code_content(&self) -> &[u8] {
        self.raw_content().iter().next().unwrap().1
    }

    // todo: prioritize certain mime types
    fn viewable_content(&self) -> Result<Content<'_>> {
        for (mime, content) in self.raw_content() {
            if mime == "text/uri-list" {
                let text = core::str::from_utf8(content)?;

                let uris = text
                    .lines()
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect();

                return Ok(Content::UriList(uris));
            }

            if mime.starts_with("text/") {
                return Ok(Content::Text(core::str::from_utf8(content)?));
            }

            if mime.starts_with("image/") {
                return Ok(Content::Image(content));
            }
        }

        bail!(
            "unsupported mime types {:#?}",
            self.raw_content().keys().collect::<Vec<_>>()
        )
    }

    fn searchable_content(&self) -> impl Iterator<Item = &str> {
        self.raw_content().iter().filter_map(|(mime, content)| {
            if mime.starts_with("text/") {
                let text = core::str::from_utf8(content).ok()?;

                if mime == "text/html" {
                    if let Some(alt) = find_alt(text) {
                        return Some(alt);
                    }
                }

                return Some(text);
            }

            None
        })
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

    fn get_query(&self) -> &str;

    fn get(&self, index: usize) -> Option<&Self::Entry>;

    fn get_from_id(&self, id: EntryId) -> Option<&Self::Entry>;

    fn iter(&self) -> Box<dyn Iterator<Item = &'_ Self::Entry> + '_>;

    fn len(&self) -> usize;

    async fn handle_message(&mut self, message: DbMessage) -> Result<()>;

    fn is_search_active(&self) -> bool {
        !self.get_query().is_empty()
    }
}

#[derive(Clone, Debug)]
pub enum DbMessage {
    CheckUpdate,
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
