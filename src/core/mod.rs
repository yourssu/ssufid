use std::{collections::HashMap, sync::Arc, vec};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidPost {
    id: String,
    title: String,
    category: String,
    url: String,
    created_at: time::OffsetDateTime,
    updated_at: Option<time::OffsetDateTime>,
    content: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidSiteData {
    title: String,
    source: String,
    description: String,
    items: Vec<SsufidPost>,
}

#[cfg(feature = "rss")]
impl SsufidSiteData {
    pub fn to_rss(self) -> ::rss::Channel {
        self.into()
    }
}

pub struct SsufidCore {
    cache: Arc<RwLock<HashMap<String, Vec<SsufidPost>>>>,
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
        let updated_entries = {
            // read lock scope
            let cache = cache.read().await;
            #[allow(unused_variables)]
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries,
                None => &self.read_cache(T::IDENTIFIER).await?,
            };

            // Compare with new and old: `updated_at` 설정
            // and return the result
            vec![]
        };
        {
            // write lock scope
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), new_entries);
        }
        Ok(SsufidSiteData {
            title: "TODO".to_string(), // T::TITLE
            source: T::IDENTIFIER.to_string(),
            description: "TODO".to_string(), // T::DESC
            items: updated_entries,
        })
    }

    pub async fn save_cache(&self) -> Result<(), SsufidError> {
        // Save all caches into files
        let cache = Arc::clone(&self.cache);
        let cache = cache.read().await;
        let dir = std::path::Path::new(&self.cache_dir);
        tokio::fs::create_dir_all(dir).await.unwrap();

        for (id, posts) in &*cache {
            let json = serde_json::to_string_pretty(&posts).unwrap();
            let mut file = tokio::fs::File::create(dir.join(format!("{id}.json"))).await?;
            file.write_all(json.as_bytes()).await.unwrap();
        }
        Ok(())
    }

    async fn read_cache(&self, id: &str) -> Result<Vec<SsufidPost>, SsufidError> {
        let path = std::path::Path::new(&self.cache_dir).join(format!("{id}.json"));
        let content = tokio::fs::read_to_string(&path).await?;
        let items: Vec<SsufidPost> = serde_json::from_str(&content)?;
        Ok(items)
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
    #[error("Crawl error: {0}")]
    CrawlError(String),

    #[error("File I/O error: {0}")]
    FileIOError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

// 임시 테스트
#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::{SsufidCore, SsufidPost};

    #[tokio::test]
    async fn core_read_cache_test() {
        let mock = vec![
            SsufidPost {
                id: "asdf".to_string(),
                title: "asdf".to_string(),
                category: "asdf".to_string(),
                url: "asdf".to_string(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: None,
                content: "asdf".to_string(),
            },
            SsufidPost {
                id: "asdf".to_string(),
                title: "asdf".to_string(),
                category: "asdf".to_string(),
                url: "asdf".to_string(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: Some(time::OffsetDateTime::now_utc()),
                content: "asdf".to_string(),
            },
        ];

        // write mock data
        let test_data_str = serde_json::to_string_pretty(&mock).unwrap();
        let dir = std::path::Path::new("./.ssufid/cache_test");
        tokio::fs::create_dir_all(dir).await.unwrap();
        let mut test_file = tokio::fs::File::create(dir.join("test.json"))
            .await
            .unwrap();
        test_file.write_all(test_data_str.as_bytes()).await.unwrap();

        // read data and compare
        let core = SsufidCore::new("./.ssufid/cache_test");
        let read_data = core.read_cache("test").await.unwrap();
        assert_eq!(mock, read_data);
    }
}

#[cfg(feature = "rss")]
pub mod rss;
