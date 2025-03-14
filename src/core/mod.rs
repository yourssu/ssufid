use std::collections::HashMap;

use time;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SsufidPost {
    id: String,
    title: String,
    category: String,
    url: String,
    created_at: time::OffsetDateTime,
    updated_at: Option<time::OffsetDateTime>,
    content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SsufidResult {
    title: String,
    source: String,
    description: String,
    items: Vec<SsufidPost>,
}

impl SsufidResult {
    pub fn to_rss(&self) -> String {
        // convert to RSS format file
        todo!()
    }
}

pub struct Core {
    cache: HashMap<String, Vec<SsufidPost>>,
}

impl Core {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new()
        }
    }

    pub async fn run<T: SsufidPlugin>(&mut self, plugin: T) -> Result<SsufidResult, SsufidError> {
        let new_entries = plugin.crawl().await?;
        let old_entries = match self.cache.get(T::IDENTIFIER) {
            Some(entries) => entries,
            None => todo!("retrieve cache from file")
        };
        self.cache.insert(T::IDENTIFIER.to_string(), new_entries);
        
        // Compare with new and old: `updated_at` 설정
        // and return the result
        todo!()
    }

    pub async fn dispose_cache(&self) -> Result<(), std::io::Error> {
        // Save all caches into files
        todo!()
    }
}

pub trait SsufidPlugin {
    const IDENTIFIER: &'static str;
    async fn crawl(&self) -> Result<Vec<SsufidPost>, SsufidError>;
}

pub enum SsufidError {
    CrawlError,
}