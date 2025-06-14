use std::sync::LazyLock;

use futures::{TryStreamExt as _, stream::FuturesOrdered};
use scraper::Selector;
use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};
use time::{
    Date,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

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

#[allow(dead_code)]
struct WordpressMetadata<T: SsufidPlugin> {
    is_announcement: bool,
    title: String,
    url: String,
    created_at: time::OffsetDateTime,
    _marker: std::marker::PhantomData<T>,
}

impl<T: SsufidPlugin> TryFrom<scraper::ElementRef<'_>> for WordpressMetadata<T> {
    type Error = PluginError;

    fn try_from(element: scraper::ElementRef<'_>) -> Result<Self, Self::Error> {
        let mut childrens = element.child_elements();

        let number_element = childrens.next().ok_or_else(|| {
            PluginError::parse::<T>("Failed to find number element in the board item".into())
        })?;

        let title_element = childrens
            .next()
            .and_then(|el| el.child_elements().next())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find title element".into()))?;

        let date_element = childrens
            .skip(1)
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date element".into()))?;

        let is_announcement = number_element
            .text()
            .next()
            .map_or(false, |text| text.contains("공지"));

        let title = title_element.text().collect::<String>();

        let url = title_element
            .value()
            .attr("href")
            .ok_or_else(|| {
                PluginError::parse::<T>("Failed to find URL in the title element".into())
            })?
            .to_string();

        let date_text = date_element
            .text()
            .next()
            .ok_or_else(|| PluginError::parse::<T>("Failed to find date text".into()))?
            .trim();
        let created_at = Date::parse(&date_text, DATE_FORMAT)
            .map_err(|e| PluginError::parse::<T>(format!("Failed to parse date: {e:?}")))?
            .midnight()
            .assume_offset(offset!(+09:00));

        Ok(Self {
            is_announcement,
            title,
            url,
            created_at,
            _marker: std::marker::PhantomData,
        })
    }
}

#[repr(transparent)]
pub(crate) struct WordpressCrawler<T: SsufidPlugin> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> WordpressCrawler<T>
where
    T: SsufidPlugin,
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
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;
        let document = scraper::Html::parse_document(&html);

        document
            .select(&BOARD_TABLE_ITEM_SELECTOR)
            .map(|el| WordpressMetadata::<T>::try_from(el))
            .collect()
    }

    async fn fetch_post(&self, metadata: &WordpressMetadata<T>) -> Result<SsufidPost, PluginError> {
        let post_url = format!("{}", metadata.url);
        let html = reqwest::get(post_url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;
        let document = scraper::Html::parse_document(&html);

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
        let created_at = Date::parse(&date_text, DATE_FORMAT)
            .map_err(|e| PluginError::parse::<T>(format!("Failed to parse date: {e:?}")))?
            .midnight()
            .assume_offset(offset!(+09:00));

        let content = document
            .select(&CONTENT_SELECTOR)
            .next()
            .map(|el| el.inner_html())
            .ok_or_else(|| PluginError::parse::<T>("Failed to find content in the post".into()))?;

        // Here you would typically save the post to your database or process it further.
        tracing::info!(
            "Fetched post: {} ({}), created at: {}",
            title,
            metadata.url,
            created_at
        );

        Ok(SsufidPost {
            id: "".to_string(),
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
