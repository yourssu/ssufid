// IT대학의 컴퓨터학부, 소프트웨어학부, 정보보호학과에
// 해당하는 플러그인에서 사용되는 공통 모듈입니다.

use futures::{TryStreamExt, stream::FuturesUnordered};
use log::{info, warn};
use scraper::{Html, Selector};
use thiserror::Error;
use time::{
    Date,
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};
use url::Url;

use crate::{
    PluginError,
    core::{Attachment, SsufidPlugin, SsufidPost},
};
use scraper::Element;

#[derive(Debug)]
struct ItMetadata {
    category: Option<String>,
    id: String,
    url: String,
    author: Option<String>,
    created_at: time::OffsetDateTime,
}

struct ItSelectors {
    // in the notice list page
    table: Selector,
    tr: Selector,
    url: Selector,
    author: Selector,
    created_at: Selector,
    category: Selector,

    // in the content page
    title: Selector,
    thumbnail: Selector,
    content: Selector,
    attachments: Selector,
}

impl ItSelectors {
    fn new() -> Self {
        Self {
            table: Selector::parse("#bo_list > div.notice_list > table > tbody").unwrap(),
            tr: Selector::parse("tr").unwrap(),
            url: Selector::parse("td.td_subject > div > a").unwrap(),
            author: Selector::parse("td.td_name.sv_use > span").unwrap(),
            created_at: Selector::parse("td.td_datetime").unwrap(),
            category: Selector::parse("td.td_num2 > p").unwrap(),
            title: Selector::parse("#bo_v_title > span").unwrap(),
            thumbnail: Selector::parse("#bo_v_con img").unwrap(),
            content: Selector::parse("#bo_v_con").unwrap(),
            attachments: Selector::parse("#bo_v_file > ul > li > a").unwrap(),
        }
    }
}

#[derive(Error, Debug)]
enum ItMetadataError {
    #[error("URL not found error")]
    UrlNotFound,
    #[error("URL parse failed for {0}")]
    UrlParseError(String),
    #[error("ID is empty for URL: {0}")]
    IdEmpty(String),
    #[error("Date element not found for ID: {0}")]
    DateNotFound(String),
    #[error("Date parse failed for {0}")]
    DateParseError(String),
}

const DATE_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

pub(crate) struct ItCrawler<T: SsufidPlugin> {
    selectors: ItSelectors,
    _marker: std::marker::PhantomData<T>,
}

impl<T> ItCrawler<T>
where
    T: SsufidPlugin,
{
    pub(crate) fn new() -> Self {
        Self {
            selectors: ItSelectors::new(),
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let metadata_list = self.fetch_metadata_list(posts_limit).await?;
        info!(
            "[{}] fetch {} post contents",
            T::IDENTIFIER,
            metadata_list.len()
        );
        metadata_list
            .iter()
            .map(|metadata| self.fetch_post(metadata))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<Vec<_>>()
            .await
    }

    /// 1 페이지부터 순서대로 최대 `posts_limit`개의 메타데이터를 반환합니다.
    async fn fetch_metadata_list(&self, posts_limit: u32) -> Result<Vec<ItMetadata>, PluginError> {
        let mut remain = posts_limit as usize;
        let mut page = 1;
        let mut metadata_list: Vec<ItMetadata> = vec![];

        while remain > 0 {
            info!("[{}] page: {}", T::IDENTIFIER, page);
            let mut metadata = self
                .fetch_metadata(page)
                .await?
                .into_iter()
                .take(remain)
                .collect::<Vec<ItMetadata>>();

            if metadata.is_empty() {
                break;
            }

            remain -= metadata.len();
            metadata_list.append(&mut metadata);
            page += 1;
        }

        Ok(metadata_list)
    }

    /// `page` 페이지의 메타데이터 리스트를 반환합니다.
    async fn fetch_metadata(&self, page: u32) -> Result<Vec<ItMetadata>, PluginError> {
        let page_url = format!("{}/&page={}", T::BASE_URL, page);

        let html = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list = document
            .select(&self.selectors.table)
            .next()
            .ok_or(PluginError::parse::<T>(
                "Table element not found".to_string(),
            ))?
            .select(&self.selectors.tr);

        let posts_metadata = notice_list
            .map(|tr| {
                let category = tr
                    .select(&self.selectors.category)
                    .next()
                    .map(|p| p.text().collect::<String>().trim().to_string());

                let url = tr
                    .select(&self.selectors.url)
                    .next()
                    .and_then(|a| a.value().attr("href"))
                    .ok_or(ItMetadataError::UrlNotFound)?
                    .to_string();

                let id = Url::parse(&url)
                    .map_err(|_| ItMetadataError::UrlParseError(url.clone()))?
                    .query_pairs()
                    .find(|(key, value)| key == "wr_id" && !value.is_empty())
                    .map(|(_, value)| value.to_string())
                    .ok_or(ItMetadataError::IdEmpty(url.clone()))?;

                let author = tr
                    .select(&self.selectors.author)
                    .next()
                    .map(|span| span.text().collect::<String>().trim().to_string());

                let created_at = {
                    let date = tr
                        .select(&self.selectors.created_at)
                        .next()
                        .map(|element| element.text().collect::<String>().trim().to_string())
                        .ok_or(ItMetadataError::DateNotFound(id.clone()))?;
                    Date::parse(&date, DATE_FORMAT)
                        .map_err(|_| ItMetadataError::DateParseError(date.clone()))?
                        .midnight()
                        .assume_offset(offset!(+09:00))
                };
                Ok(ItMetadata {
                    category,
                    id,
                    url,
                    author,
                    created_at,
                })
            })
            .filter_map(|result: Result<ItMetadata, ItMetadataError>| {
                // 경고 메시지 모아서 출력
                // 메타데이터 크롤링 실패 시 크롤링 대상에서 제외
                result
                    .inspect_err(|e| warn!("[{}] {:?}", T::IDENTIFIER, e))
                    .ok()
            })
            .collect::<Vec<ItMetadata>>();

        Ok(posts_metadata)
    }

    /// `metadata`에 해당하는 게시글의 내용을 크롤링하여 반환합니다.
    async fn fetch_post(&self, metadata: &ItMetadata) -> Result<SsufidPost, PluginError> {
        let html = reqwest::get(&metadata.url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let title = document
            .select(&self.selectors.title)
            .next()
            .map(|span| span.text().collect::<String>().trim().to_string())
            .ok_or(PluginError::parse::<T>(
                "Title element not found".to_string(),
            ))?;

        let thumbnail = document
            .select(&self.selectors.thumbnail)
            .next()
            .and_then(|img| img.value().attr("src"));

        let content = document
            .select(&self.selectors.content)
            .next()
            .ok_or(PluginError::parse::<T>(
                "Content element not found".to_string(),
            ))?
            .child_elements()
            .map(|p| p.html())
            .collect::<Vec<String>>()
            .join("\n");

        let attachments = document
            .select(&self.selectors.attachments)
            .map(|a| Attachment {
                url: a.value().attr("href").unwrap_or_default().to_string(),
                name: a
                    .first_element_child()
                    .map(|strong| strong.text().collect::<String>()),
                mime_type: None,
            })
            .collect();

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            author: metadata.author.clone(),
            title,
            description: None,
            category: metadata.category.clone().map_or(vec![], |c| vec![c]),
            created_at: metadata.created_at,
            updated_at: None,
            thumbnail: thumbnail.map(String::from),
            content,
            attachments,
            metadata: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::plugins::cse::bachelor::CseBachelorPlugin;

    use super::*;

    #[tokio::test]
    async fn test_crawler_fetch_metadata() {
        let crawler: ItCrawler<CseBachelorPlugin> = ItCrawler::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = crawler.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        for metadata in &metadata_list {
            println!("{:?}", metadata);
        }

        let first_metadata = &metadata_list[0];
        assert!(!first_metadata.id.is_empty());
        assert!(first_metadata.url.trim().starts_with("https"));
        assert!(
            first_metadata.created_at.year() >= 2025,
            "Created date should be recent"
        );

        // 학사 공지사항의 첫 게시글은 공지 카테고리 존재
        assert_eq!(first_metadata.category, Some("공지".to_string()));
    }

    #[tokio::test]
    async fn test_crawler_fetch_post() {
        let crawler: ItCrawler<CseBachelorPlugin> = ItCrawler::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = crawler.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        let first_metadata = &metadata_list[0];

        let post = crawler.fetch_post(first_metadata).await.unwrap();
        assert!(!post.title.is_empty());
    }

    #[tokio::test]
    async fn test_crawler_fetch_metadata_list() {
        let posts_limit = 100;
        let crawler: ItCrawler<CseBachelorPlugin> = ItCrawler::new();

        let metadata_list = crawler.fetch_metadata_list(posts_limit).await.unwrap();
        assert_eq!(metadata_list.len(), posts_limit as usize);
    }
}
