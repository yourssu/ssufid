use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use time;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

use crate::error::{Error, PluginError};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidPost {
    pub id: String,
    pub title: String,
    pub category: String,
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
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
    pub const POST_COUNT_LIMIT: u32 = 100;

    pub fn new(cache_dir: &str) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_dir: cache_dir.to_string(),
        }
    }

    pub async fn run<T: SsufidPlugin>(
        &self,
        plugin: T,
        posts_limit: u32,
    ) -> Result<SsufidSiteData, Error> {
        let new_entries = plugin.crawl(posts_limit).await?;
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

    pub async fn save_cache(&self) -> Result<(), Error> {
        // Save all caches into files
        let cache = Arc::clone(&self.cache);
        let cache = cache.read().await;
        let dir = std::path::Path::new(&self.cache_dir);
        tokio::fs::create_dir_all(dir).await?;

        for (id, posts) in &*cache {
            let json = serde_json::to_string_pretty(&posts)?;
            let mut file = tokio::fs::File::create(dir.join(format!("{id}.json"))).await?;
            file.write_all(json.as_bytes()).await?;
        }
        Ok(())
    }

    async fn read_cache(&self, id: &str) -> Result<Vec<SsufidPost>, Error> {
        let path = std::path::Path::new(&self.cache_dir).join(format!("{id}.json"));
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::Io(e)),
        };
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
                    return SsufidPost {
                        updated_at: old.updated_at,
                        ..post
                    };
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
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, PluginError>> + Send;
}

// 임시 테스트
#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::macros::datetime;
    use tokio::io::AsyncWriteExt;

    use super::{SsufidCore, SsufidPost, inject_update_date};

    #[tokio::test]
    async fn test_read_cache() {
        let mock = vec![
            SsufidPost {
                id: "test-id".to_string(),
                title: "Test Title".to_string(),
                category: "Test Category".to_string(),
                url: "https://example.com/test".to_string(),
                created_at: datetime!(2024-03-22 12:00:00 UTC),
                updated_at: None,
                content: "Test Content".to_string(),
            },
            SsufidPost {
                id: "test-id".to_string(),
                title: "Test Title".to_string(),
                category: "Test Category".to_string(),
                url: "https://example.com/test".to_string(),
                created_at: datetime!(2024-03-22 12:00:00 UTC),
                updated_at: Some(datetime!(2024-03-22 12:00:00 UTC)),
                content: "Test Content".to_string(),
            },
        ];

        // write file
        let mock_json = serde_json::to_string_pretty(&mock).unwrap();
        let dir = std::path::Path::new("./cache_test");
        let test_file_path = dir.join("test.json");
        tokio::fs::create_dir_all(dir).await.unwrap();
        let mut test_file = tokio::fs::File::create(&test_file_path).await.unwrap();
        test_file.write_all(mock_json.as_bytes()).await.unwrap();

        // read file
        let core = SsufidCore::new("./cache_test");
        let read_data = core.read_cache("test").await.unwrap();
        assert_eq!(mock, read_data);

        // delete test file
        tokio::fs::remove_file(&test_file_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_read_cache_file_not_found() {
        let core = SsufidCore::new("./unknown");
        let read_data = core.read_cache("not_found").await.unwrap();
        assert!(read_data == vec![]);
    }

    #[test]
    fn test_inject_update_date() {
        let now = OffsetDateTime::now_utc();
        let old_entries = vec![
            SsufidPost {
                id: "1".to_string(),
                title: "Old Title 1".to_string(),
                category: "Category 1".to_string(),
                url: "http://example.com/1".to_string(),
                created_at: now,
                updated_at: None,
                content: "Old Content 1".to_string(),
            },
            SsufidPost {
                id: "2".to_string(),
                title: "Old Title 2".to_string(),
                category: "Category 2".to_string(),
                url: "http://example.com/2".to_string(),
                created_at: now,
                updated_at: Some(now),
                content: "Old Content 2".to_string(),
            },
        ];

        let new_entries = vec![
            // Case 1: 기존 포스트와 내용이 같은 경우
            SsufidPost {
                id: "1".to_string(),
                title: "Old Title 1".to_string(),
                category: "Category 1".to_string(),
                url: "http://example.com/1".to_string(),
                created_at: now,
                updated_at: None,
                content: "Old Content 1".to_string(),
            },
            // Case 2: 기존 포스트와 내용이 다른 경우
            SsufidPost {
                id: "2".to_string(),
                title: "Updated Title 2".to_string(), // 제목 변경
                category: "Category 2".to_string(),
                url: "http://example.com/2".to_string(),
                created_at: now,
                updated_at: None,
                content: "Old Content 2".to_string(),
            },
            // Case 3: 새로운 포스트인 경우
            SsufidPost {
                id: "3".to_string(),
                title: "New Title 3".to_string(),
                category: "Category 3".to_string(),
                url: "http://example.com/3".to_string(),
                created_at: now,
                updated_at: None,
                content: "New Content 3".to_string(),
            },
            // Case 4: 이미 updated_at이 설정된 경우
            SsufidPost {
                id: "4".to_string(),
                title: "Title 4".to_string(),
                category: "Category 4".to_string(),
                url: "http://example.com/4".to_string(),
                created_at: now,
                updated_at: Some(now),
                content: "Content 4".to_string(),
            },
        ];

        let result = inject_update_date(&old_entries, new_entries);

        // Case 1: 내용이 같은 경우 updated_at이 None이어야 함
        assert!(result[0].updated_at.is_none());
        assert_eq!(result[0].title, "Old Title 1");

        // Case 2: 내용이 다른 경우 updated_at이 설정되어야 함
        assert!(result[1].updated_at.is_some());
        assert_eq!(result[1].title, "Updated Title 2");

        // Case 3: 새로운 포스트는 updated_at이 None이어야 함
        assert!(result[2].updated_at.is_none());
        assert_eq!(result[2].title, "New Title 3");

        // Case 4: 이미 updated_at이 설정된 경우 그대로 유지되어야 함
        assert_eq!(result[3].updated_at, Some(now));
        assert_eq!(result[3].title, "Title 4");
    }
}

#[cfg(feature = "rss")]
pub mod rss;
