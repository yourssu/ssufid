use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidPost {
    pub id: String,
    pub title: String,
    pub category: String,
    pub url: String,
    pub created_at: time::OffsetDateTime,
    pub updated_at: Option<time::OffsetDateTime>,
    pub content: String,
}

impl SsufidPost {
    pub fn contents_eq(&self, other: &SsufidPost) -> bool {
        self.id.trim() == other.id.trim()
            && self.title.trim() == other.title.trim()
            && self.category.trim() == other.category.trim()
            && self.content.trim() == other.content.trim()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
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
    const POST_COUNT_LIMIT: u32 = 100;

    pub fn new(cache_dir: &str) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_dir: cache_dir.to_string(),
        }
    }

    pub async fn run<T: SsufidPlugin>(&self, plugin: T) -> Result<SsufidSiteData, SsufidError> {
        let new_entries = plugin.crawl(Self::POST_COUNT_LIMIT).await?;
        let cache = Arc::clone(&self.cache);
        let updated_entries = {
            // read lock scope
            let cache = cache.read().await;
            #[allow(unused_variables)]
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries,
                None => &self.read_cache(T::IDENTIFIER).await?,
            };

            inject_update_date(old_entries, new_entries)
        };
        {
            // write lock scope
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), updated_entries.clone());
        }
        Ok(SsufidSiteData {
            title: T::TITLE.to_string(),
            source: T::IDENTIFIER.to_string(),
            description: T::DESCRIPTION.to_string(),
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

fn inject_update_date(
    old_entries: &[SsufidPost],
    new_entries: impl IntoIterator<Item = SsufidPost>,
) -> Vec<SsufidPost> {
    let old_entries_map = old_entries
        .iter()
        .map(|post: &SsufidPost| (post.id.clone(), post))
        .collect::<HashMap<String, &SsufidPost>>();
    let current_time = time::OffsetDateTime::now_utc();
    new_entries
        .into_iter()
        .map(|post| {
            // 업데이트 정보를 플러그인이 제공했다면 자체 계산 제외
            if post.updated_at.is_some() {
                return post;
            }
            if let Some(old) = old_entries_map.get(&post.id) {
                let old = *old;
                if old.contents_eq(&post) {
                    return post;
                }
                SsufidPost {
                    updated_at: Some(current_time),
                    ..post
                }
            } else {
                post
            }
        })
        .collect()
}

pub trait SsufidPlugin {
    const TITLE: &'static str;
    const IDENTIFIER: &'static str;
    const DESCRIPTION: &'static str;

    fn crawl(
        &self,
        posts_limit: u32,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, SsufidError>> + Send;
}

#[derive(Debug, Error)]
pub enum SsufidError {
    #[error("Plugin error: {0}")]
    PluginError(String),

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
