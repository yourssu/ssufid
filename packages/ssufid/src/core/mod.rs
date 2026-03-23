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

pub use calendar::{CalendarCrawlRange, SsufidCalendar, SsufidCalendarSiteData};
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
        let mut last_error = None;

        for attempt in 1..=retry_count {
            let start = Instant::now();

            match self.run(plugin, posts_limit).await {
                Ok(data) => {
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

                    return Ok(data);
                }
                Err(error) => {
                    last_error = Some(error);
                }
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
            error = ?last_error,
            "All {} crawl attempts failed with error",
            retry_count
        );
        Err(Error::AttemptsExceeded {
            plugin: T::IDENTIFIER,
            attempts: retry_count,
            source: last_error.map(Box::new),
        })
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
                target: "content_update",
                type = "crawl_attempt_failed",
                id = T::IDENTIFIER,
                title = T::TITLE,
                posts_limit,
                error = ?e,
                "Crawl attempt failed"
            )
        })?;
        tracing::info!(
            target: "content_update",
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
        calendar_range: &CalendarCrawlRange,
        retry_count: u32,
    ) -> Result<SsufidCalendarSiteData, Error> {
        let mut last_error = None;

        for attempt in 1..=retry_count {
            let start = Instant::now();

            match self.run_calendar(plugin, calendar_range).await {
                Ok(data) => {
                    let elapsed = start.elapsed();

                    tracing::info!(
                        target: "content_update",
                        type = "calendar_crawl_success",
                        id = T::IDENTIFIER,
                        title = T::TITLE,
                        url = T::BASE_URL,
                        calendar_range_start = %calendar_range.start(),
                        calendar_range_end = %calendar_range.end(),
                        events = data.items.len(),
                        retry_count,
                        attempt,
                        elapsed = ?elapsed,
                        "Successfully crawled {} calendar entries in {:.2}s",
                        data.items.len(),
                        elapsed.as_secs_f32()
                    );

                    return Ok(data);
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }
        tracing::error!(
            target: "content_update",
            type = "calendar_crawl_failed",
            id = T::IDENTIFIER,
            title = T::TITLE,
            url = T::BASE_URL,
            calendar_range_start = %calendar_range.start(),
            calendar_range_end = %calendar_range.end(),
            retry_count,
            error = ?last_error,
            "All {} calendar crawl attempts failed with error",
            retry_count
        );
        Err(Error::AttemptsExceeded {
            plugin: T::IDENTIFIER,
            attempts: retry_count,
            source: last_error.map(Box::new),
        })
    }

    #[tracing::instrument(
        name = "run_calendar_plugin",
        target = "content_update",
        skip(self, plugin, calendar_range),
        fields(
            plugin = T::IDENTIFIER,
            calendar_range_start = %calendar_range.start(),
            calendar_range_end = %calendar_range.end()
        )
    )]
    pub async fn run_calendar<T: SsufidCalendarPlugin>(
        &self,
        plugin: &T,
        calendar_range: &CalendarCrawlRange,
    ) -> Result<SsufidCalendarSiteData, Error> {
        let new_entries = plugin.crawl(calendar_range).await.inspect_err(|e| {
            tracing::error!(
                target: "content_update",
                type = "calendar_crawl_attempt_failed",
                id = T::IDENTIFIER,
                title = T::TITLE,
                calendar_range_start = %calendar_range.start(),
                calendar_range_end = %calendar_range.end(),
                error = ?e,
                "Calendar crawl attempt failed"
            )
        })?;
        let new_entries = filter_calendar_entries_by_range(new_entries, calendar_range);
        tracing::info!(
            target: "content_update",
            type = "calendar_crawl_attempt_success",
            id = T::IDENTIFIER,
            title = T::TITLE,
            calendar_range_start = %calendar_range.start(),
            calendar_range_end = %calendar_range.end()
        );
        let cache = Arc::clone(&self.calendar_cache);
        let updated_entries = {
            let cache = cache.read().await;
            let old_entries = match cache.get(T::IDENTIFIER) {
                Some(entries) => entries.clone(),
                None => self.read_calendar_cache(T::IDENTIFIER).await?,
            };
            merge_calendar_entries(old_entries, new_entries, calendar_range)
        };
        {
            let mut cache = cache.write().await;
            cache.insert(T::IDENTIFIER.to_string(), updated_entries.clone());
        }
        Ok(SsufidCalendarSiteData {
            title: T::TITLE.to_string(),
            source: T::BASE_URL.to_string(),
            description: T::DESCRIPTION.to_string(),
            items: filter_calendar_entries_by_range(updated_entries, calendar_range)
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

fn filter_calendar_entries_by_range(
    entries: Vec<SsufidCalendar>,
    calendar_range: &CalendarCrawlRange,
) -> Vec<SsufidCalendar> {
    entries
        .into_iter()
        .filter(|item| calendar_range.contains_start(item.starts_at))
        .collect()
}

fn merge_calendar_entries(
    old_entries: Vec<SsufidCalendar>,
    mut new_entries: Vec<SsufidCalendar>,
    calendar_range: &CalendarCrawlRange,
) -> Vec<SsufidCalendar> {
    let mut old_entries_map = old_entries
        .into_iter()
        .filter(|item| !calendar_range.contains_start(item.starts_at))
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
        calendar_range: &CalendarCrawlRange,
    ) -> impl std::future::Future<Output = Result<Vec<SsufidCalendar>, PluginError>> + Send;
}

#[cfg(test)]
mod tests {
    use std::{time::Duration, vec};

    use time::OffsetDateTime;
    use time::macros::datetime;
    use tokio::io::AsyncWriteExt;

    use super::{
        Attachment, CalendarCrawlRange, SsufidCalendar, SsufidCalendarPlugin, SsufidCore,
        SsufidPlugin, SsufidPost, SsufidPostPlugin, filter_calendar_entries_by_range,
        merge_calendar_entries, merge_entries,
    };
    use crate::error::{Error, PluginError};

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
            _calendar_range: &CalendarCrawlRange,
        ) -> Result<Vec<SsufidCalendar>, PluginError> {
            Ok(self.items.clone())
        }
    }

    struct MockPostPlugin {
        error_name: String,
        error_message: String,
    }

    impl SsufidPlugin for MockPostPlugin {
        const TITLE: &'static str = "Mock Post";
        const IDENTIFIER: &'static str = "mock.post";
        const DESCRIPTION: &'static str = "Mock post plugin for tests";
        const BASE_URL: &'static str = "https://example.com/post";
    }

    impl SsufidPostPlugin for MockPostPlugin {
        async fn crawl(&self, _posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
            Err(PluginError::custom::<MockPostPlugin>(
                self.error_name.clone(),
                self.error_message.clone(),
            ))
        }
    }

    struct MockFailingCalendarPlugin {
        error_message: String,
    }

    impl SsufidPlugin for MockFailingCalendarPlugin {
        const TITLE: &'static str = "Mock Failing Calendar";
        const IDENTIFIER: &'static str = "mock.failing.calendar";
        const DESCRIPTION: &'static str = "Mock failing calendar plugin for tests";
        const BASE_URL: &'static str = "https://example.com/failing-calendar";
    }

    impl SsufidCalendarPlugin for MockFailingCalendarPlugin {
        async fn crawl(
            &self,
            _calendar_range: &CalendarCrawlRange,
        ) -> Result<Vec<SsufidCalendar>, PluginError> {
            Err(PluginError::parse::<MockFailingCalendarPlugin>(
                self.error_message.clone(),
            ))
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
    fn test_filter_calendar_entries_by_range() {
        let entries = vec![
            SsufidCalendar {
                id: "old".to_string(),
                title: "Old Event".to_string(),
                description: None,
                starts_at: datetime!(2024-02-10 00:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "recent".to_string(),
                title: "Recent Event".to_string(),
                description: None,
                starts_at: datetime!(2024-03-20 00:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
        ];
        let range = CalendarCrawlRange::new(
            datetime!(2024-03-01 00:00:00 UTC),
            datetime!(2024-03-31 23:59:59 UTC),
        )
        .unwrap();

        let filtered = filter_calendar_entries_by_range(entries, &range);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "recent");
    }

    #[test]
    fn test_merge_calendar_entries_overwrites_only_inside_range() {
        let old_entries = vec![
            SsufidCalendar {
                id: "outside".to_string(),
                title: "Outside Range".to_string(),
                description: None,
                starts_at: datetime!(2024-02-20 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "inside-stale".to_string(),
                title: "Stale Inside Range".to_string(),
                description: None,
                starts_at: datetime!(2024-03-15 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "inside-updated".to_string(),
                title: "Old Title".to_string(),
                description: None,
                starts_at: datetime!(2024-03-16 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
        ];

        let new_entries = vec![
            SsufidCalendar {
                id: "inside-updated".to_string(),
                title: "New Title".to_string(),
                description: None,
                starts_at: datetime!(2024-03-16 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "inside-new".to_string(),
                title: "Brand New Inside Range".to_string(),
                description: None,
                starts_at: datetime!(2024-03-18 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
        ];
        let range = CalendarCrawlRange::new(
            datetime!(2024-03-01 00:00:00 UTC),
            datetime!(2024-03-31 23:59:59 UTC),
        )
        .unwrap();

        let result = merge_calendar_entries(old_entries, new_entries, &range);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, "outside");
        assert_eq!(result[1].id, "inside-updated");
        assert_eq!(result[1].title, "New Title");
        assert_eq!(result[2].id, "inside-new");
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
        let range = CalendarCrawlRange::new(
            datetime!(2024-03-01 00:00:00 UTC),
            datetime!(2024-03-31 23:59:59 UTC),
        )
        .unwrap();

        let result = core.run_calendar(&plugin, &range).await.unwrap();

        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].id, "2");
        assert_eq!(result.items[1].id, "1");
    }

    #[tokio::test]
    async fn test_run_calendar_filters_and_replaces_range_cache() {
        let cache_dir = "./run_calendar_cache_filter_test";
        let core = SsufidCore::new(cache_dir);
        let cache_dir_path = std::path::Path::new(cache_dir).join("calendar");
        tokio::fs::create_dir_all(&cache_dir_path).await.unwrap();

        let cached_entries = vec![
            SsufidCalendar {
                id: "outside".to_string(),
                title: "Outside Range".to_string(),
                description: None,
                starts_at: datetime!(2024-02-10 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
            SsufidCalendar {
                id: "stale".to_string(),
                title: "Stale Event".to_string(),
                description: None,
                starts_at: datetime!(2024-03-05 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            },
        ];
        let cache_json = serde_json::to_string_pretty(&cached_entries).unwrap();
        let cache_file_path = cache_dir_path.join("mock.calendar.json");
        let mut cache_file = tokio::fs::File::create(&cache_file_path).await.unwrap();
        cache_file.write_all(cache_json.as_bytes()).await.unwrap();
        cache_file.flush().await.unwrap();

        let plugin = MockCalendarPlugin {
            items: vec![SsufidCalendar {
                id: "fresh".to_string(),
                title: "Fresh Event".to_string(),
                description: None,
                starts_at: datetime!(2024-03-20 12:00:00 UTC),
                ends_at: None,
                location: None,
                url: None,
            }],
        };
        let range = CalendarCrawlRange::new(
            datetime!(2024-03-01 00:00:00 UTC),
            datetime!(2024-03-31 23:59:59 UTC),
        )
        .unwrap();

        let result = core.run_calendar(&plugin, &range).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "fresh");

        let cache = core.calendar_cache.read().await;
        let merged_cache = cache.get("mock.calendar").unwrap();
        assert_eq!(merged_cache.len(), 2);
        assert_eq!(merged_cache[0].id, "outside");
        assert_eq!(merged_cache[1].id, "fresh");

        if tokio::fs::try_exists(cache_dir).await.unwrap() {
            tokio::fs::remove_dir_all(cache_dir).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_run_with_retry_preserves_last_error() {
        let core = SsufidCore::new("./retry_post_test");
        let plugin = MockPostPlugin {
            error_name: "network".to_string(),
            error_message: "last post failure".to_string(),
        };

        let error = core.run_with_retry(&plugin, 10, 2).await.unwrap_err();
        match error {
            Error::AttemptsExceeded {
                plugin,
                attempts,
                source,
            } => {
                assert_eq!(plugin, MockPostPlugin::IDENTIFIER);
                assert_eq!(attempts, 2);
                let source = source.expect("missing preserved source error");
                match *source {
                    Error::Plugin(plugin_error) => {
                        assert_eq!(plugin_error.plugin(), MockPostPlugin::IDENTIFIER);
                        assert_eq!(plugin_error.message(), "last post failure");
                    }
                    other => panic!("unexpected source error: {other:?}"),
                }
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_run_calendar_with_retry_preserves_last_error() {
        let core = SsufidCore::new("./retry_calendar_test");
        let plugin = MockFailingCalendarPlugin {
            error_message: "last calendar failure".to_string(),
        };
        let range = CalendarCrawlRange::new(
            datetime!(2024-03-01 00:00:00 UTC),
            datetime!(2024-03-31 23:59:59 UTC),
        )
        .unwrap();

        let error = core
            .run_calendar_with_retry(&plugin, &range, 3)
            .await
            .unwrap_err();
        match error {
            Error::AttemptsExceeded {
                plugin,
                attempts,
                source,
            } => {
                assert_eq!(plugin, MockFailingCalendarPlugin::IDENTIFIER);
                assert_eq!(attempts, 3);
                let source = source.expect("missing preserved source error");
                match *source {
                    Error::Plugin(plugin_error) => {
                        assert_eq!(plugin_error.plugin(), MockFailingCalendarPlugin::IDENTIFIER);
                        assert_eq!(plugin_error.message(), "last calendar failure");
                    }
                    other => panic!("unexpected source error: {other:?}"),
                }
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[cfg(feature = "rss")]
pub mod rss;

#[cfg(feature = "ics")]
pub mod ics;
