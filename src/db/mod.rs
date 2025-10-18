use std::{collections::HashMap, fmt::Debug, path::Path, sync::LazyLock};

use anyhow::Result;

use chrono::Utc;
use regex::Regex;

use crate::config::Config;

#[cfg(test)]
pub mod test;

mod sqlite_db;
pub use sqlite_db::DbSqlite;

fn now() -> i64 {
    Utc::now().timestamp_millis()
}

pub type EntryId = i64;
pub type Mime = String;
pub type RawContent = Vec<u8>;
pub type MimeDataMap = HashMap<Mime, RawContent>;

pub enum Content<'a> {
    Text(&'a str),
    Image(&'a [u8]),
    UriList(Vec<&'a str>),
}

impl<'a> Content<'a> {
    fn try_new(mime: &str, content: &'a [u8]) -> Result<Option<Self>> {
        if mime == "text/uri-list" {
            let text = core::str::from_utf8(content)?;

            let uris = text
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();

            return Ok(Some(Content::UriList(uris)));
        }

        if mime.starts_with("text/") {
            return Ok(Some(Content::Text(core::str::from_utf8(content)?)));
        }

        if mime.starts_with("image/") {
            return Ok(Some(Content::Image(content)));
        }

        Ok(None)
    }
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

/// More we have mime types here, Less we spend time in the [`EntryTrait::preferred_content`] function.
const PRIV_MIME_TYPES_SIMPLE: &[&str] = &[
    "image/png",
    "image/jpg",
    "image/jpeg",
    "image/bmp",
    "text/plain;charset=utf-8",
    "text/plain",
    "STRING",
    "UTF8_STRING",
    "TEXT",
];
const PRIV_MIME_TYPES_REGEX_STR: &[&str] = &["text/plain*", "text/*", "image/*"];

static PRIV_MIME_TYPES_REGEX: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    PRIV_MIME_TYPES_REGEX_STR
        .iter()
        .map(|r| Regex::new(r).unwrap())
        .collect()
});

pub trait EntryTrait: Debug + Clone + Send {
    fn is_favorite(&self) -> bool;

    fn raw_content(&self) -> &MimeDataMap;

    #[allow(dead_code)]
    fn into_raw_content(self) -> MimeDataMap;

    fn id(&self) -> EntryId;

    // note: hot fn, do not log
    fn preferred_content(
        &self,
        preferred_mime_types: &[Regex],
    ) -> Option<((&str, &RawContent), Content<'_>)> {
        for pref_mime_regex in preferred_mime_types {
            for (mime, raw_content) in self.raw_content() {
                if !raw_content.is_empty() && pref_mime_regex.is_match(mime) {
                    match Content::try_new(mime, raw_content) {
                        Ok(Some(content)) => return Some(((mime, raw_content), content)),
                        Ok(None) => {
                            // unsupported mime type
                        }
                        Err(_e) => {}
                    }
                }
            }
        }

        for pref_mime in PRIV_MIME_TYPES_SIMPLE {
            if let Some(raw_content) = self.raw_content().get(*pref_mime)
                && !raw_content.is_empty()
            {
                match Content::try_new(pref_mime, raw_content) {
                    Ok(Some(content)) => return Some(((pref_mime, raw_content), content)),
                    Ok(None) => {}
                    Err(_e) => {}
                }
            }
        }

        for pref_mime_regex in PRIV_MIME_TYPES_REGEX.iter() {
            for (mime, raw_content) in self.raw_content() {
                if !raw_content.is_empty() && pref_mime_regex.is_match(mime) {
                    match Content::try_new(mime, raw_content) {
                        Ok(Some(content)) => return Some(((mime, raw_content), content)),
                        Ok(None) => {}
                        Err(_e) => {}
                    }
                }
            }
        }

        None
    }

    fn searchable_content(&self) -> impl Iterator<Item = &str> {
        self.raw_content().iter().filter_map(|(mime, content)| {
            if mime.starts_with("text/") {
                let text = core::str::from_utf8(content).ok()?;

                if mime == "text/html"
                    && let Some(alt) = find_alt(text)
                {
                    return Some(alt);
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

    async fn insert(&mut self, data: MimeDataMap) -> Result<()>;

    async fn insert_with_time(&mut self, data: MimeDataMap, time: i64) -> Result<()>;

    async fn delete(&mut self, data: EntryId) -> Result<()>;

    async fn clear(&mut self) -> Result<()>;

    async fn add_favorite(&mut self, entry: EntryId, index: Option<usize>) -> Result<()>;

    async fn remove_favorite(&mut self, entry: EntryId) -> Result<()>;

    fn search(&mut self);

    fn set_query_and_search(&mut self, query: String);

    fn get_query(&self) -> &str;

    fn get(&self, index: usize) -> Option<&Self::Entry>;

    fn get_from_id(&self, id: EntryId) -> Option<&Self::Entry>;

    fn iter(&self) -> impl Iterator<Item = &'_ Self::Entry>;

    fn search_iter(&self) -> impl Iterator<Item = &'_ Self::Entry>;

    fn either_iter(
        &self,
    ) -> itertools::Either<
        impl Iterator<Item = &'_ Self::Entry>,
        impl Iterator<Item = &'_ Self::Entry>,
    >;

    fn len(&self) -> usize;

    async fn handle_message(&mut self, message: DbMessage) -> Result<()>;

    fn is_search_active(&self) -> bool {
        !self.get_query().is_empty()
    }

    fn non_favorite_count(&self) -> usize;
}

#[derive(Clone, Debug)]
pub enum DbMessage {
    CheckUpdate,
}
// currently best effort
fn find_alt(html: &str) -> Option<&str> {
    const DEB: &str = "alt=\"";

    if let Some(pos) = html.find(DEB) {
        const OFFSET: usize = DEB.len();

        if let Some(pos_end) = html[pos + OFFSET..].find('"') {
            return Some(&html[pos + OFFSET..pos + pos_end + OFFSET]);
        }
    }

    None
}
