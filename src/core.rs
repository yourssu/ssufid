use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time;
use tokio::sync::RwLock;

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
pub struct SsufidSiteData {
    title: String,
    source: String,
    description: String,
    items: Vec<SsufidPost>,
}

impl SsufidSiteData {
    pub fn to_rss(&self) -> String {
        // convert to RSS format file
        todo!()
    }
}

pub struct SsufidCore {
    cache: Arc<RwLock<HashMap<String, Vec<SsufidPost>>>>,
    #[allow(dead_code)]
    cache_dir: String,
}

impl SsufidCore {
    pub fn new(cache_dir: &str) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_dir: cache_dir.to_string(),
        }
    }

    pub async fn run<T: SsufidPlugin>(&self, plugin: T) -> Result<SsufidSiteData, SsufidError> {
        let new_entries = plugin.crawl().await?;
        let cache = Arc::clone(&self.cache);
        #[allow(unused_variables, clippy::let_unit_value)]
        let ret = {
            let cache = cache.read().await;
            #[allow(unused_variables)]
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries,
                None => todo!("retrieve cache from file"),
            };
    
            // Compare with new and old: `updated_at` 설정
            // and return the result
        };
        {
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), new_entries);
        }
        todo!()
        // Ok(ret)
    }

    pub async fn save_cache(&self) -> Result<(), std::io::Error> {
        // Save all caches into files
        todo!()
    }
}

pub trait SsufidPlugin {
    const IDENTIFIER: &'static str;
    fn crawl(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, SsufidError>> + Send;
}

#[derive(Debug, Error)]
pub enum SsufidError {
    #[error("crawl error")]
    CrawlError,
    // TODO: 다양한 에러 타입 정의
}
