pub(crate) mod metadata;

use std::sync::LazyLock;

use futures::{TryStreamExt as _, stream::FuturesOrdered};
use scraper::Selector;
use ssufid::{
    PluginError, PluginErrorKind,
    core::{SsufidPlugin, SsufidPost},
};
use time::{
    Date,
    macros::{format_description, offset},
};
use url::Url;

use crate::common::wordpress::metadata::{
    DefaultWordpressMetadataResolver, WordpressMetadata, WordpressMetadataResolver,
};

// Hmm
static BOARD_TABLE_ITEM_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("div.baord_table tbody > tr").expect("Failed to parse board table selector")
});

static TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("table.t_view p.title").expect("Failed to parse title selector")
});

static DATE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("table.t_view ul.date_w > li > dl:first-child > dd")
        .expect("Failed to parse date selector")
});

static CONTENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("table.t_view div.td_box").expect("Failed to parse content selector")
});

#[repr(transparent)]
pub(crate) struct WordpressCrawler<
    T: SsufidPlugin,
    M: WordpressMetadataResolver = DefaultWordpressMetadataResolver,
    P: WordpressPostResolver = DefaultWordpressPostResolver,
> {
    _marker: std::marker::PhantomData<(T, M, P)>,
}

impl<T, R, P> WordpressCrawler<T, R, P>
where
    T: SsufidPlugin,
    R: WordpressMetadataResolver,
    P: WordpressPostResolver,
{
    pub(crate) fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let metadata = self.fetch_metadata_list(posts_limit).await?;
        tracing::info!("fetch {} posts", metadata.len());
        metadata
            .iter()
            .map(|m| self.fetch_post(m))
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))
    }

    async fn fetch_metadata_list(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<WordpressMetadata<T>>, PluginError> {
        // Simulate fetching metadata from a WordPress site
        let mut metadata_list = Vec::with_capacity(posts_limit as usize);
        let mut page = 1;
        let mut announcements = 0;
        while metadata_list.len() < posts_limit as usize + announcements as usize {
            let metadata = self.fetch_page(page).await?;
            announcements += metadata.iter().filter(|m| m.is_announcement).count() as u32;
            let empty = metadata.is_empty();
            metadata_list.extend(metadata);
            if empty {
                break; // No more pages to fetch
            }

            page += 1; // Simulate pagination
        }
        // Make sure announcements are sorted correctly
        metadata_list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        metadata_list.truncate(100);

        Ok(metadata_list)
    }

    async fn fetch_page(&self, page: u32) -> Result<Vec<WordpressMetadata<T>>, PluginError> {
        let page_url = format!("{}/page/{}", T::BASE_URL, page);

        let html = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<T>(format!("Failed to request list page: {e:?}")))?
            .text()
            .await
            .map_err(|e| {
                PluginError::parse::<T>(format!("Failed to parse list html body: {e:?}"))
            })?;
        let document = scraper::Html::parse_document(&html);

        document
            .select(&BOARD_TABLE_ITEM_SELECTOR)
            .map(R::resolve)
            .collect::<Result<Vec<_>, _>>()
            .or_else(|e| {
                if matches!(e.kind(), PluginErrorKind::Custom(name) if name.as_ref() == "NO_ENTRY")
                {
                    Ok(vec![])
                } else {
                    Err(e)
                }
            })
    }

    async fn fetch_post(&self, metadata: &WordpressMetadata<T>) -> Result<SsufidPost, PluginError> {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await; // Rate limiting
        let post_url = metadata.url.clone();
        let html = reqwest::get(post_url)
            .await
            .map_err(|e| PluginError::request::<T>(format!("Failed to request post page: {e:?}")))?
            .text()
            .await
            .map_err(|e| {
                PluginError::parse::<T>(format!("Failed to parse post html body: {e:?}"))
            })?;
        let document = scraper::Html::parse_document(&html);
        let post = P::resolve_post::<T>(metadata, document)?;

        // Here you would typically save the post to your database or process it further.
        tracing::info!(
            "Fetched post: {} ({}), created at: {}",
            &post.title,
            metadata.url,
            &post.created_at
        );

        Ok(post)
    }
}

pub(crate) trait WordpressPostResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>];
    fn resolve_post<T: SsufidPlugin>(
        metadata: &WordpressMetadata<T>,
        document: scraper::Html,
    ) -> Result<SsufidPost, PluginError> {
        let id = Url::parse(&metadata.url)
            .map_err(|e| PluginError::parse::<T>(format!("Failed to parse URL: {e:?}")))?
            .query_pairs()
            .find(|(k, _)| k == "slug")
            .ok_or_else(|| {
                PluginError::parse::<T>("Failed to find 'slug' query parameter in the URL".into())
            })?
            .1
            .to_string();
        let title = document
            .select(&TITLE_SELECTOR)
            .next()
            .and_then(|el| el.text().next())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find title in the post".into()))?
            .to_string();

        let date_text = document
            .select(&DATE_SELECTOR)
            .next()
            .and_then(|el| el.text().next())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date in the post".into()))?
            .trim();
        let created_at = Date::parse(date_text, Self::DATE_FORMAT)
            .map_err(|e| PluginError::parse::<T>(format!("Failed to parse date: {e:?}")))?
            .midnight()
            .assume_offset(offset!(+09:00));

        let content = document
            .select(&CONTENT_SELECTOR)
            .next()
            .map(|el| el.inner_html())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find content in the post".into()))?;
        Ok(SsufidPost {
            id,
            title,
            url: metadata.url.clone(),
            content,
            created_at,
            author: None,
            description: None,
            category: if metadata.is_announcement {
                vec!["공지".to_string()]
            } else {
                vec![]
            },
            updated_at: None,
            thumbnail: None,
            attachments: vec![],
            metadata: None,
        })
    }
}

pub(crate) struct DefaultWordpressPostResolver;

impl WordpressPostResolver for DefaultWordpressPostResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]-[month]-[day]");
}

pub(crate) struct DotDateWordpressPostResolver;

impl WordpressPostResolver for DotDateWordpressPostResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year].[month].[day]");
}

pub(crate) struct KorDateWordpressPostResolver;

impl WordpressPostResolver for KorDateWordpressPostResolver {
    const DATE_FORMAT: &'static [time::format_description::FormatItem<'static>] =
        format_description!("[year]년 [month padding:none]월 [day padding:none]일");
}
