use std::collections::VecDeque;

use chrono::{DateTime, Utc};

pub struct SelectionEntry {
    pub id: u64,
    pub text: String,
    pub timestamp: DateTime<Utc>,
}

pub struct SelectionBuffer {
    entries: VecDeque<SelectionEntry>,
    max_entries: usize,
    next_id: u64,
    query: String,
    filtered: Vec<usize>,
}

impl SelectionBuffer {
    pub fn new(max_entries: u32) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: max_entries as usize,
            next_id: 1,
            query: String::new(),
            filtered: Vec::new(),
        }
    }

    pub fn push(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        let entry = SelectionEntry {
            id: self.next_id,
            text,
            timestamp: Utc::now(),
        };
        self.next_id += 1;
        self.entries.push_front(entry);
        if self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
        // Re-run search if active
        if !self.query.is_empty() {
            self.run_search();
        }
    }

    pub fn search(&mut self, query: String) {
        self.query = query;
        self.run_search();
    }

    fn run_search(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.text.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
    }

    pub fn get_query(&self) -> &str {
        &self.query
    }

    pub fn is_search_active(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SelectionEntry> {
        self.entries.iter()
    }

    pub fn search_iter(&self) -> impl Iterator<Item = &SelectionEntry> {
        let entries = &self.entries;
        self.filtered.iter().filter_map(move |&i| entries.get(i))
    }

    pub fn get_by_id(&self, id: u64) -> Option<&SelectionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.filtered.clear();
        self.query.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn set_max(&mut self, n: u32) {
        self.max_entries = n as usize;
        while self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }
}
