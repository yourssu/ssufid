use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use indexmap::IndexMap;
use tokio::sync::RwLock;
use tokio::{io::AsyncWriteExt, time::Instant};

use crate::error::{Error, PluginError};

mod calendar;
pub mod post;

pub use calendar::{SsufidCalendar, SsufidCalendarSiteData};
pub use post::{Attachment, SsufidPost, SsufidSiteData};

pub struct SsufidCore {
    cache: Arc<RwLock<HashMap<String, Vec<SsufidPost>>>>,
    calendar_cache: Arc<RwLock<HashMap<String, Vec<SsufidCalendar>>>>,
    cache_dir: String,
}

impl SsufidCore {
    pub const POST_COUNT_LIMIT: u32 = 100;
    pub const CALENDAR_DAY_LIMIT: u32 = 30;
    pub const RETRY_COUNT: u32 = 3;

    pub fn new(cache_dir: &str) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            calendar_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_dir: cache_dir.to_string(),
        }
    }

    pub async fn run_with_retry<T: SsufidPostPlugin>(
        &self,
        plugin: &T,
        posts_limit: u32,
        retry_count: u32,
    ) -> Result<SsufidSiteData, Error> {
        for attempt in 1..=retry_count {
            let start = Instant::now();

            let result = self.run(plugin, posts_limit).await;

            if let Ok(data) = &result {
                let elapsed = start.elapsed();

                tracing::info!(
                    target: "content_update",
                    type = "crawl_success",
                    id = T::IDENTIFIER,
                    title = T::TITLE,
                    url = T::BASE_URL,
                    posts_limit,
                    posts = data.items.len(),
                    retry_count,
                    attempt,
                    elapsed = ?elapsed,
                    "Successfully crawled {} posts in {:.2}s",
                    data.items.len(),
                    elapsed.as_secs_f32()
                );

                return result;
            }
        }
        tracing::error!(
            target: "content_update",
            type = "crawl_failed",
            id = T::IDENTIFIER,
            title = T::TITLE,
            url = T::BASE_URL,
            posts_limit,
            retry_count,
            "All {} crawl attempts failed with error",
            retry_count
        );
        Err(Error::AttemptsExceeded(T::IDENTIFIER))
    }

    #[tracing::instrument(
        name = "run_plugin",
        target = "content_update",
        skip(self, plugin),
        fields(plugin = T::IDENTIFIER, posts_limit)
    )]
    pub async fn run<T: SsufidPostPlugin>(
        &self,
        plugin: &T,
        posts_limit: u32,
    ) -> Result<SsufidSiteData, Error> {
        let new_entries = plugin.crawl(posts_limit).await.inspect_err(|e| {
            tracing::error!(
                type = "crawl_attempt_failed",
                id = T::IDENTIFIER,
                title = T::TITLE,
                posts_limit,
                error = ?e,
                "Crawl attempt failed"
            )
        })?;
        tracing::info!(
            type = "crawl_attempt_success",
            id = T::IDENTIFIER,
            title = T::TITLE,
            posts_limit
        );
        let cache = Arc::clone(&self.cache);
        let updated_entries = {
            let cache = cache.read().await;
            #[allow(unused_variables)]
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries.clone(),
                None => self.read_cache(T::IDENTIFIER).await?,
            };
            merge_entries(old_entries, new_entries)
        };
        {
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), updated_entries.clone());
        }
        Ok(SsufidSiteData {
            title: T::TITLE.to_string(),
            source: T::BASE_URL.to_string(),
            description: T::DESCRIPTION.to_string(),
            items: updated_entries
                .into_iter()
                .rev()
                .take(Self::POST_COUNT_LIMIT as usize)
                .collect(),
        })
    }

    pub async fn run_calendar_with_retry<T: SsufidCalendarPlugin>(
        &self,
        plugin: &T,
        calendar_limit_days: u32,
        retry_count: u32,
    ) -> Result<SsufidCalendarSiteData, Error> {
        for attempt in 1..=retry_count {
            let start = Instant::now();

            let result = self.run_calendar(plugin, calendar_limit_days).await;

            if let Ok(data) = &result {
                let elapsed = start.elapsed();

                tracing::info!(
                    target: "content_update",
                    type = "calendar_crawl_success",
                    id = T::IDENTIFIER,
                    title = T::TITLE,
                    url = T::BASE_URL,
                    calendar_limit_days,
                    events = data.items.len(),
                    retry_count,
                    attempt,
                    elapsed = ?elapsed,
                    "Successfully crawled {} calendar entries in {:.2}s",
                    data.items.len(),
                    elapsed.as_secs_f32()
                );

                return result;
            }
        }
        tracing::error!(
            target: "content_update",
            type = "calendar_crawl_failed",
            id = T::IDENTIFIER,
            title = T::TITLE,
            url = T::BASE_URL,
            calendar_limit_days,
            retry_count,
            "All {} calendar crawl attempts failed with error",
            retry_count
        );
        Err(Error::AttemptsExceeded(T::IDENTIFIER))
    }

    #[tracing::instrument(
        name = "run_calendar_plugin",
        target = "content_update",
        skip(self, plugin),
        fields(plugin = T::IDENTIFIER, calendar_limit_days)
    )]
    pub async fn run_calendar<T: SsufidCalendarPlugin>(
        &self,
        plugin: &T,
        calendar_limit_days: u32,
    ) -> Result<SsufidCalendarSiteData, Error> {
        let new_entries = plugin.crawl(calendar_limit_days).await.inspect_err(|e| {
            tracing::error!(
                type = "calendar_crawl_attempt_failed",
                id = T::IDENTIFIER,
                title = T::TITLE,
                calendar_limit_days,
                error = ?e,
                "Calendar crawl attempt failed"
            )
        })?;
        tracing::info!(
            type = "calendar_crawl_attempt_success",
            id = T::IDENTIFIER,
            title = T::TITLE,
            calendar_limit_days
        );
        let cache = Arc::clone(&self.calendar_cache);
        let updated_entries = {
            let cache = cache.read().await;
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries.clone(),
                None => self.read_calendar_cache(T::IDENTIFIER).await?,
            };
            merge_calendar_entries(old_entries, new_entries)
        };
        {
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), updated_entries.clone());
        }
        Ok(SsufidCalendarSiteData {
            title: T::TITLE.to_string(),
            source: T::BASE_URL.to_string(),
            description: T::DESCRIPTION.to_string(),
            items: filter_calendar_entries_by_days(updated_entries, calendar_limit_days)
                .into_iter()
                .rev()
                .collect(),
        })
    }

    pub async fn save_cache(&self) -> Result<(), Error> {
        tokio::fs::create_dir_all(Path::new(&self.cache_dir)).await?;

        {
            let cache = Arc::clone(&self.cache);
            let cache = cache.read().await;
            for (id, posts) in &*cache {
                let json = serde_json::to_string_pretty(&posts)?;
                let path = self.post_cache_path(id);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let mut file = tokio::fs::File::create(path).await?;
                file.write_all(json.as_bytes()).await?;
            }
        }

        {
            let calendar_cache = Arc::clone(&self.calendar_cache);
            let calendar_cache = calendar_cache.read().await;
            for (id, items) in &*calendar_cache {
                let json = serde_json::to_string_pretty(&items)?;
                let path = self.calendar_cache_path(id);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let mut file = tokio::fs::File::create(path).await?;
                file.write_all(json.as_bytes()).await?;
            }
        }

        Ok(())
    }

    fn post_cache_path(&self, id: &str) -> PathBuf {
        Path::new(&self.cache_dir).join(format!("{id}.json"))
    }

    fn calendar_cache_path(&self, id: &str) -> PathBuf {
        Path::new(&self.cache_dir)
            .join("calendar")
            .join(format!("{id}.json"))
    }

    async fn read_cache(&self, id: &str) -> Result<Vec<SsufidPost>, Error> {
        let path = self.post_cache_path(id);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::Io(e)),
        };
        let items: Vec<SsufidPost> = serde_json::from_str(&content)?;
        Ok(items)
    }

    async fn read_calendar_cache(&self, id: &str) -> Result<Vec<SsufidCalendar>, Error> {
        let path = self.calendar_cache_path(id);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::Io(e)),
        };
        let items: Vec<SsufidCalendar> = serde_json::from_str(&content)?;
        Ok(items)
    }
}

fn merge_entries(
    old_entries: Vec<SsufidPost>,
    mut new_entries: Vec<SsufidPost>,
) -> Vec<SsufidPost> {
    let mut old_entries_map = old_entries
        .into_iter()
        .map(|post: SsufidPost| (post.id.clone(), post))
        .collect::<IndexMap<String, SsufidPost>>();
    old_entries_map
        .sort_by(|_k, v, _k2, v2| v.partial_cmp(v2).unwrap_or(std::cmp::Ordering::Equal));
    let current_time = time::OffsetDateTime::now_utc();
    new_entries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let new_entries = new_entries;
    for post in new_entries {
        let Some(old) = old_entries_map.get(&post.id) else {
            tracing::info!(
                target: "content_update",
                type = "post_created",
                id = %post.id,
                title = %post.title,
                url = %post.url,
            );
            old_entries_map.insert(post.id.clone(), post);
            continue;
        };
        if old.contents_eq(&post) {
            continue;
        }
        tracing::info!(
            target: "content_update",
            type = "post_updated",
            id = %post.id,
            title = %post.title,
            url = %post.url,
        );
        if post.updated_at.is_some() {
            old_entries_map.insert(post.id.clone(), post);
        } else {
            old_entries_map.insert(
                post.id.clone(),
                SsufidPost {
                    created_at: old.created_at,
                    updated_at: Some(current_time),
                    ..post
                },
            );
        }
    }
    old_entries_map.into_values().collect()
}

fn filter_calendar_entries_by_days(
    entries: Vec<SsufidCalendar>,
    calendar_limit_days: u32,
) -> Vec<SsufidCalendar> {
    if calendar_limit_days == 0 {
        return entries;
    }

    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(calendar_limit_days as i64);

    entries
        .into_iter()
        .filter(|item| item.starts_at >= cutoff)
        .collect()
}

fn merge_calendar_entries(
    old_entries: Vec<SsufidCalendar>,
    mut new_entries: Vec<SsufidCalendar>,
) -> Vec<SsufidCalendar> {
    let mut old_entries_map = old_entries
        .into_iter()
        .map(|item: SsufidCalendar| (item.id.clone(), item))
        .collect::<IndexMap<String, SsufidCalendar>>();
    old_entries_map
        .sort_by(|_k, v, _k2, v2| v.partial_cmp(v2).unwrap_or(std::cmp::Ordering::Equal));
    new_entries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    for item in new_entries {
        let Some(old) = old_entries_map.get(&item.id) else {
            tracing::info!(
                target: "content_update",
                type = "calendar_created",
                id = %item.id,
                title = %item.title,
                url = ?item.url,
            );
            old_entries_map.insert(item.id.clone(), item);
            continue;
        };
        if old.contents_eq(&item) {
            continue;
        }
        tracing::info!(
            target: "content_update",
            type = "calendar_updated",
            id = %item.id,
            title = %item.title,
            url = ?item.url,
        );
        old_entries_map.insert(item.id.clone(), item);
    }

    old_entries_map.into_values().collect()
}

pub trait SsufidPlugin {
    const TITLE: &'static str;
    const IDENTIFIER: &'static str;
    const DESCRIPTION: &'static str;
    const BASE_URL: &'static str;
}

pub trait SsufidPostPlugin: SsufidPlugin {
    fn crawl(
        &self,
        posts_limit: u32,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidPost>, PluginError>> + Send;
}

pub trait SsufidCalendarPlugin: SsufidPlugin {
    fn crawl(
        &self,
        calendar_limit_days: u32,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidCalendar>, PluginError>> + Send;
}

#[cfg(test)]
mod tests {
    use std::{time::Duration, vec};

    use time::OffsetDateTime;
    use time::macros::datetime;
    use tokio::io::AsyncWriteExt;

    use super::{
        Attachment, SsufidCalendar, SsufidCalendarPlugin, SsufidCore, SsufidPlugin, SsufidPost,
        filter_calendar_entries_by_days, merge_calendar_entries, merge_entries,
    };
    use crate::error::PluginError;

    struct MockCalendarPlugin {
        items: Vec<SsufidCalendar>,
    }

    impl SsufidPlugin for MockCalendarPlugin {
        const TITLE: &'static str = "Mock Calendar";
        const IDENTIFIER: &'static str = "mock.calendar";
        const DESCRIPTION: &'static str = "Mock calendar plugin for tests";
        const BASE_URL: &'static str = "https://example.com/calendar";
    }

    impl SsufidCalendarPlugin for MockCalendarPlugin {
        async fn crawl(
            &self,
            _calendar_limit_days: u32,
        ) -> Result<Vec<SsufidCalendar>, PluginError> {
            Ok(self.items.clone())
        }
    }

    #[tokio::test]
    async fn test_read_cache() {
        let mock = vec![
            SsufidPost {
                id: "test-id-1".to_string(),
                url: "https://example.com/test1".to_string(),
                author: Some("Author One".to_string()),
                title: "Test Title 1".to_string(),
                description: Some("This is a description for test 1.".to_string()),
                category: vec!["Category A".to_string()],
                created_at: datetime!(2024-03-22 12:00:00 UTC),
                updated_at: None,
                thumbnail: Some("https://example.com/thumb1.jpg".to_string()),
                content: "Test Content 1".to_string(),
                attachments: vec![Attachment {
                    url: "https://example.com/attachment1.pdf".to_string(),
                    name: Some("Attachment 1".to_string()),
                    mime_type: Some("application/pdf".to_string()),
                }],
                metadata: Some(
                    [("key1".to_string(), "value1".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
            SsufidPost {
                id: "test-id-2".to_string(),
                url: "https://example.com/test2".to_string(),
                author: None,
                title: "Test Title 2".to_string(),
                description: None,
                category: vec!["Category B".to_string(), "Category C".to_string()],
                created_at: datetime!(2024-03-23 10:00:00 UTC),
                updated_at: Some(datetime!(2024-03-23 11:00:00 UTC)),
                thumbnail: None,
                content: "Test Content 2".to_string(),
                attachments: vec![],
                metadata: None,
            },
        ];

        let mock_json = serde_json::to_string_pretty(&mock).unwrap();
        let dir = std::path::Path::new("./cache_test");
        let test_file_path = dir.join("test.json");
        tokio::fs::create_dir_all(dir).await.unwrap();
        let mut test_file = tokio::fs::File::create(&test_file_path).await.unwrap();
        test_file.write_all(mock_json.as_bytes()).await.unwrap();
        test_file.flush().await.unwrap();

        let core = SsufidCore::new("./cache_test");
        let read_data = core.read_cache("test").await.unwrap();
        assert_eq!(mock, read_data);

        tokio::fs::remove_file(&test_file_path).await.unwrap();
        tokio::fs::remove_dir_all("./cache_test").await.unwrap();
    }

    #[tokio::test]
    async fn test_read_cache_file_not_found() {
        let core = SsufidCore::new("./unknown");
        let read_data = core.read_cache("not_found").await.unwrap();
        assert!(read_data == vec![]);
    }

    #[tokio::test]
    async fn test_read_calendar_cache() {
        let mock = vec![SsufidCalendar {
            id: "calendar-1".to_string(),
            title: "Calendar Title 1".to_string(),
            description: Some("Calendar Description 1".to_string()),
            starts_at: datetime!(2024-03-22 12:00:00 UTC),
            ends_at: Some(datetime!(2024-03-22 13:00:00 UTC)),
            location: Some("Seoul".to_string()),
            url: Some("https://example.com/calendar/1".to_string()),
        }];

        let mock_json = serde_json::to_string_pretty(&mock).unwrap();
        let dir = std::path::Path::new("./calendar_cache_test/calendar");
        let test_file_path = dir.join("test.json");
        tokio::fs::create_dir_all(dir).await.unwrap();
        let mut test_file = tokio::fs::File::create(&test_file_path).await.unwrap();
        test_file.write_all(mock_json.as_bytes()).await.unwrap();
        test_file.flush().await.unwrap();

        let core = SsufidCore::new("./calendar_cache_test");
        let read_data = core.read_calendar_cache("test").await.unwrap();
        assert_eq!(mock, read_data);

        tokio::fs::remove_file(&test_file_path).await.unwrap();
        tokio::fs::remove_dir_all("./calendar_cache_test")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_read_calendar_cache_file_not_found() {
        let core = SsufidCore::new("./unknown-calendar");
        let read_data = core.read_calendar_cache("not_found").await.unwrap();
        assert!(read_data == vec![]);
    }

    #[test]
    fn test_merge_entries() {
        let now = OffsetDateTime::now_utc();
        let old_entries = vec![
            SsufidPost {
                id: "1".to_string(),
                url: "http://example.com/1".to_string(),
                author: Some("Author 1".to_string()),
                title: "Old Title 1".to_string(),
                description: Some("Description for 1".to_string()),
                category: vec!["Category 1".to_string()],
                created_at: now - Duration::from_secs(1),
                updated_at: None,
                thumbnail: Some("http://example.com/thumb1.jpg".to_string()),
                content: "Old Content 1".to_string(),
                attachments: vec![Attachment {
                    url: "http://example.com/attach1.doc".to_string(),
                    name: None,
                    mime_type: None,
                }],
                metadata: Some(
                    [("meta_key_1".to_string(), "meta_value_1".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
            SsufidPost {
                id: "2".to_string(),
                url: "http://example.com/2".to_string(),
                author: Some("Author 2".to_string()),
                title: "Old Title 2".to_string(),
                description: Some("Description for 2".to_string()),
                category: vec!["Category 2".to_string()],
                created_at: now,
                updated_at: Some(now),
                thumbnail: Some("http://example.com/thumb2.jpg".to_string()),
                content: "Old Content 2".to_string(),
                attachments: vec![],
                metadata: None,
            },
            SsufidPost {
                id: "0".to_string(),
                url: "http://example.com/1".to_string(),
                author: Some("Author 1".to_string()),
                title: "Old Title 1".to_string(),
                description: Some("Description for 1".to_string()),
                category: vec!["Category 1".to_string()],
                created_at: now - Duration::from_secs(2),
                updated_at: None,
                thumbnail: Some("http://example.com/thumb1.jpg".to_string()),
                content: "Old Content 1".to_string(),
                attachments: vec![Attachment {
                    url: "http://example.com/attach1.doc".to_string(),
                    name: None,
                    mime_type: None,
                }],
                metadata: Some(
                    [("meta_key_1".to_string(), "meta_value_1".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
        ];

        let new_entries = vec![
            SsufidPost {
                id: "1".to_string(),
                url: "http://example.com/1".to_string(),
                author: Some("Author 1".to_string()),
                title: "Old Title 1".to_string(),
                description: Some("Description for 1".to_string()),
                category: vec!["Category 1".to_string()],
                created_at: now,
                updated_at: None,
                thumbnail: Some("http://example.com/thumb1.jpg".to_string()),
                content: "Old Content 1".to_string(),
                attachments: vec![Attachment {
                    url: "http://example.com/attach1.doc".to_string(),
                    name: None,
                    mime_type: None,
                }],
                metadata: Some(
                    [("meta_key_1".to_string(), "meta_value_1".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
            SsufidPost {
                id: "2".to_string(),
                url: "http://example.com/2_new".to_string(),
                author: Some("Author 2 Updated".to_string()),
                title: "Updated Title 2".to_string(),
                description: Some("Description for 2 Updated".to_string()),
                category: vec!["Category 2".to_string()],
                created_at: now + Duration::from_secs(1),
                updated_at: None,
                thumbnail: Some("http://example.com/thumb2_new.jpg".to_string()),
                content: "Old Content 2".to_string(),
                attachments: vec![Attachment {
                    url: "http://example.com/attach2.png".to_string(),
                    name: Some("New Attachment".to_string()),
                    mime_type: Some("image/png".to_string()),
                }],
                metadata: Some(
                    [("meta_key_2".to_string(), "meta_value_2".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
            SsufidPost {
                id: "3".to_string(),
                url: "http://example.com/3".to_string(),
                author: Some("New Author 3".to_string()),
                title: "New Title 3".to_string(),
                description: Some("Description for 3".to_string()),
                category: vec!["Category 3".to_string()],
                created_at: now + Duration::from_secs(2),
                updated_at: None,
                thumbnail: None,
                content: "New Content 3".to_string(),
                attachments: vec![],
                metadata: None,
            },
            SsufidPost {
                id: "4".to_string(),
                url: "http://example.com/4".to_string(),
                author: Some("Author 4".to_string()),
                title: "Title 4".to_string(),
                description: Some("Description for 4".to_string()),
                category: vec!["Category 4".to_string()],
                created_at: now + Duration::from_secs(3),
                updated_at: Some(now + Duration::from_secs(3)),
                thumbnail: Some("http://example.com/thumb4.jpg".to_string()),
                content: "Content 4".to_string(),
                attachments: vec![],
                metadata: None,
            },
        ];

        let result = merge_entries(old_entries, new_entries);

        assert_eq!(result[0].id, "0");
        assert!(result[1].updated_at.is_none());
        assert_eq!(result[1].title, "Old Title 1");
        assert!(result[2].updated_at.is_some());
        assert_eq!(result[2].title, "Updated Title 2");
        assert!(result[3].updated_at.is_none());
        assert_eq!(result[3].title, "New Title 3");
        assert_eq!(result[4].updated_at, Some(now + Duration::from_secs(3)));
        assert_eq!(result[4].title, "Title 4");
    }

    #[test]
    fn test_filter_calendar_entries_by_days() {
        let now = OffsetDateTime::now_utc();
        let entries = vec![
            SsufidCalendar {
                id: "old".to_string(),
                title: "Old Event".to_string(),
                description: None,
                starts_at: now - Duration::from_secs(31 * 24 * 3600),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "recent".to_string(),
                title: "Recent Event".to_string(),
                description: None,
                starts_at: now - Duration::from_secs(5 * 24 * 3600),
                ends_at: None,
                location: None,
                url: None,
            },
        ];

        let filtered = filter_calendar_entries_by_days(entries.clone(), 10);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "recent");

        let unfiltered = filter_calendar_entries_by_days(entries, 0);
        assert_eq!(unfiltered.len(), 2);
    }

    #[test]
    fn test_merge_calendar_entries() {
        let now = OffsetDateTime::now_utc();
        let old_entries = vec![
            SsufidCalendar {
                id: "1".to_string(),
                title: "Old Event 1".to_string(),
                description: Some("Old Description 1".to_string()),
                starts_at: now,
                ends_at: Some(now + Duration::from_secs(3600)),
                location: Some("Room A".to_string()),
                url: Some("https://example.com/events/1".to_string()),
            },
            SsufidCalendar {
                id: "0".to_string(),
                title: "Older Event".to_string(),
                description: None,
                starts_at: now - Duration::from_secs(3600),
                ends_at: None,
                location: None,
                url: None,
            },
        ];

        let new_entries = vec![
            SsufidCalendar {
                id: "1".to_string(),
                title: "Old Event 1".to_string(),
                description: Some("Old Description 1".to_string()),
                starts_at: now,
                ends_at: Some(now + Duration::from_secs(3600)),
                location: Some("Room A".to_string()),
                url: Some("https://example.com/events/1".to_string()),
            },
            SsufidCalendar {
                id: "2".to_string(),
                title: "Updated Event 2".to_string(),
                description: Some("Updated Description 2".to_string()),
                starts_at: now + Duration::from_secs(1800),
                ends_at: Some(now + Duration::from_secs(5400)),
                location: Some("Room B".to_string()),
                url: Some("https://example.com/events/2".to_string()),
            },
        ];

        let result = merge_calendar_entries(old_entries, new_entries);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, "0");
        assert_eq!(result[1].id, "1");
        assert_eq!(result[2].id, "2");
        assert_eq!(result[2].title, "Updated Event 2");
    }

    #[tokio::test]
    async fn test_run_calendar() {
        let cache_dir = "./run_calendar_cache_test";
        let core = SsufidCore::new(cache_dir);
        let plugin = MockCalendarPlugin {
            items: vec![
                SsufidCalendar {
                    id: "1".to_string(),
                    title: "Event 1".to_string(),
                    description: Some("Description 1".to_string()),
                    starts_at: datetime!(2024-03-22 12:00:00 UTC),
                    ends_at: Some(datetime!(2024-03-22 13:00:00 UTC)),
                    location: Some("Seoul".to_string()),
                    url: Some("https://example.com/calendar/1".to_string()),
                },
                SsufidCalendar {
                    id: "2".to_string(),
                    title: "Event 2".to_string(),
                    description: None,
                    starts_at: datetime!(2024-03-23 12:00:00 UTC),
                    ends_at: None,
                    location: None,
                    url: None,
                },
            ],
        };

        let result = core.run_calendar(&plugin, 1000).await.unwrap();

        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].id, "2");
        assert_eq!(result.items[1].id, "1");
    }

    #[tokio::test]
    async fn test_run_calendar_filters_by_days() {
        let cache_dir = "./run_calendar_cache_filter_test";
        let core = SsufidCore::new(cache_dir);
        let now = OffsetDateTime::now_utc();
        let plugin = MockCalendarPlugin {
            items: vec![
                SsufidCalendar {
                    id: "old".to_string(),
                    title: "Old Event".to_string(),
                    description: None,
                    starts_at: now - Duration::from_secs(40 * 24 * 3600),
                    ends_at: None,
                    location: None,
                    url: None,
                },
                SsufidCalendar {
                    id: "recent".to_string(),
                    title: "Recent Event".to_string(),
                    description: None,
                    starts_at: now - Duration::from_secs(5 * 24 * 3600),
                    ends_at: None,
                    location: None,
                    url: None,
                },
            ],
        };

        let filtered = core.run_calendar(&plugin, 10).await.unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].id, "recent");

        let unfiltered = core.run_calendar(&plugin, 0).await.unwrap();
        assert_eq!(unfiltered.items.len(), 2);
        assert_eq!(unfiltered.items[0].id, "recent");
        assert_eq!(unfiltered.items[1].id, "old");

        if tokio::fs::try_exists(cache_dir).await.unwrap() {
            tokio::fs::remove_dir_all(cache_dir).await.unwrap();
        }

        if tokio::fs::try_exists(cache_dir).await.unwrap() {
            tokio::fs::remove_dir_all(cache_dir).await.unwrap();
        }
    }
}

#[cfg(feature = "rss")]
pub mod rss;

#[cfg(feature = "ics")]
pub mod ics;
