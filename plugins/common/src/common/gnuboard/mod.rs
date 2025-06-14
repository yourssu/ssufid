//! IT대학의 컴퓨터학부, 소프트웨어학부, 정보보호학과에
//! 해당하는 플러그인에서 사용되는 공통 모듈입니다.
pub(crate) mod metadata;

use futures::{TryStreamExt, stream::FuturesOrdered};
use scraper::{Html, Selector};
use thiserror::Error;
use time::{
    format_description::BorrowedFormatItem,
    macros::{format_description, offset},
};

use scraper::Element;
use ssufid::{
    PluginError,
    core::{Attachment, SsufidPlugin, SsufidPost},
};

use crate::common::gnuboard::metadata::{GnuboardMetadata, GnuboardMetadataResolver};

struct GnuboardSelectors {
    // in the notice list page
    table: Selector,
    // in the content page
    title: Selector,
    thumbnail: Selector,
    content: Selector,
    attachments: Selector,
    created_at: Selector,
}

impl GnuboardSelectors {
    fn new() -> Self {
        Self {
            table: Selector::parse("#bo_list table > tbody").unwrap(),
            title: Selector::parse("#bo_v_title > span.bo_v_tit").unwrap(),
            thumbnail: Selector::parse("#bo_v_con img").unwrap(),
            content: Selector::parse("#bo_v_con").unwrap(),
            attachments: Selector::parse("#bo_v_file > ul > li > a").unwrap(),
            created_at: Selector::parse("#bo_v_info .if_date").unwrap(),
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum GnuboardMetadataError {
    #[error("URL not found error")]
    UrlNotFound,
    #[error("URL parse failed for {0}")]
    UrlParseError(String),
    #[error("ID is empty for URL: {0}")]
    IdEmpty(String),
}

pub(crate) struct GnuboardCrawler<T: SsufidPlugin, R: GnuboardMetadataResolver> {
    selectors: GnuboardSelectors,
    _marker: std::marker::PhantomData<(T, R)>,
}

impl<T, R> GnuboardCrawler<T, R>
where
    T: SsufidPlugin,
    R: GnuboardMetadataResolver,
{
    pub(crate) fn new() -> Self {
        Self {
            selectors: GnuboardSelectors::new(),
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let metadata_list = self.fetch_metadata_list(posts_limit).await?;
        tracing::info!("fetch {} post contents", metadata_list.len());
        metadata_list
            .iter()
            .map(|metadata| self.fetch_post(metadata))
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<_>>()
            .await
    }

    /// 1 페이지부터 순서대로 최대 `posts_limit`개의 메타데이터를 반환합니다.
    async fn fetch_metadata_list(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<GnuboardMetadata>, PluginError> {
        let mut remain = posts_limit as usize;
        let mut page = 1;
        let mut metadata_list: Vec<GnuboardMetadata> = vec![];

        while remain > 0 {
            tracing::info!(page);
            let mut metadata = self
                .fetch_metadata(page)
                .await?
                .into_iter()
                .take(remain)
                .collect::<Vec<GnuboardMetadata>>();

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
    async fn fetch_metadata(&self, page: u32) -> Result<Vec<GnuboardMetadata>, PluginError> {
        let page_url = format!("{}&page={}", T::BASE_URL, page);

        let html = reqwest::get(page_url)
            .await
            .map_err(|e| PluginError::request::<T>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<T>(e.to_string()))?;

        let document = Html::parse_document(&html);

        let notice_list =
            document
                .select(&self.selectors.table)
                .next()
                .ok_or(PluginError::parse::<T>(
                    "Table element not found".to_string(),
                ))?;

        let posts_metadata = notice_list
            .child_elements()
            .map(R::resolve)
            .filter_map(|result: Result<GnuboardMetadata, GnuboardMetadataError>| {
                // 경고 메시지 모아서 출력
                // 메타데이터 크롤링 실패 시 크롤링 대상에서 제외
                result
                    .inspect_err(|e| tracing::warn!(error = ?e, "Failed to parse Metadata"))
                    .ok()
            })
            .collect::<Vec<GnuboardMetadata>>();

        Ok(posts_metadata)
    }

    /// `metadata`에 해당하는 게시글의 내용을 크롤링하여 반환합니다.
    async fn fetch_post(&self, metadata: &GnuboardMetadata) -> Result<SsufidPost, PluginError> {
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
            .ok_or(PluginError::parse::<T>(format!(
                "Title element not found: URL {}",
                metadata.url
            )))?;

        let thumbnail = document
            .select(&self.selectors.thumbnail)
            .next()
            .and_then(|img| img.value().attr("src"));

        let content = document
            .select(&self.selectors.content)
            .next()
            .ok_or(PluginError::parse::<T>(format!(
                "Content element not found: URL {}",
                metadata.url
            )))?
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

        let created_at_str = document
            .select(&self.selectors.created_at)
            .next()
            .and_then(|el| el.text().last())
            .ok_or(PluginError::parse::<T>(format!(
                "Created date element not found: URL {}",
                metadata.url
            )))?
            .trim();
        const DATE_FORMAT: &[BorrowedFormatItem<'_>] =
            format_description!("[year]-[month]-[day] [hour]:[minute]");

        let created_at =
            time::PrimitiveDateTime::parse(&format!("20{}", created_at_str), DATE_FORMAT)
                .map_err(|_| {
                    PluginError::parse::<T>(format!(
                        "Failed to parse created date: {}",
                        created_at_str
                    ))
                })?
                .assume_offset(offset!(+9));

        Ok(SsufidPost {
            id: metadata.id.clone(),
            url: metadata.url.clone(),
            author: metadata.author.clone(),
            title,
            description: None,
            category: metadata.category.clone().map_or(vec![], |c| vec![c]),
            created_at,
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

    use crate::{common::gnuboard::metadata::ItGnuboardMetadataResolver, sites::CseBachelorPlugin};

    use super::*;

    #[tokio::test]
    async fn test_crawler_fetch_metadata() {
        let crawler: GnuboardCrawler<CseBachelorPlugin, ItGnuboardMetadataResolver> =
            GnuboardCrawler::new();

        // 1 페이지의 게시글 메타데이터 목록 가져오기
        let metadata_list = crawler.fetch_metadata(1).await.unwrap();
        assert!(!metadata_list.is_empty());

        for metadata in &metadata_list {
            tracing::info!("{:?}", metadata);
        }

        let first_metadata = &metadata_list[0];
        assert!(!first_metadata.id.is_empty());
        assert!(first_metadata.url.trim().starts_with("https"));

        // 학사 공지사항의 첫 게시글은 공지 카테고리 존재
        assert_eq!(first_metadata.category, Some("공지".to_string()));
    }

    #[tokio::test]
    async fn test_crawler_fetch_post() {
        let crawler: GnuboardCrawler<CseBachelorPlugin, ItGnuboardMetadataResolver> =
            GnuboardCrawler::new();

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
        let crawler: GnuboardCrawler<CseBachelorPlugin, ItGnuboardMetadataResolver> =
            GnuboardCrawler::new();

        let metadata_list = crawler.fetch_metadata_list(posts_limit).await.unwrap();
        assert_eq!(metadata_list.len(), posts_limit as usize);
    }
}
