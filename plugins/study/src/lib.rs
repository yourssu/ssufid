mod model;

use std::{collections::HashSet, sync::LazyLock};

use base64::{Engine as _, prelude::BASE64_STANDARD};
use futures::{TryStreamExt, stream::FuturesOrdered};
use reqwest::header::{CONTENT_TYPE, REFERER};
use scraper::Selector;
use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};
use url::Url;

use crate::model::{StudyBoardRequest, StudyPost, StudyPostListResponse, StudyPostMeta};

pub struct StudyPlugin;

static MODEL_TEXTAREA_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("textarea#model").expect("Failed to parse selector for model textarea")
});

fn decompress_string(input: &str) -> Result<String, PluginError> {
    let decompressed =
        lz_str::decompress_from_utf16(input).ok_or(PluginError::custom::<StudyPlugin>(
            "Failed to decompress string".to_string(),
            "The input string may be corrupted or the compression format has changed".to_string(),
        ))?;

    String::from_utf16(&decompressed).map_err(|e| {
        PluginError::parse::<StudyPlugin>(format!("Failed to parse decompressed data: {e}"))
    })
}

const POST_URL: &str = "https://study.ssu.ac.kr/community/notice_view.do";

fn construct_post_url(sb_seq: u32) -> String {
    let mut url = Url::parse(POST_URL).unwrap();
    url.query_pairs_mut()
        .append_pair("sbSeq", &BASE64_STANDARD.encode(sb_seq.to_string()));
    url.to_string()
}

impl StudyPlugin {
    const API_BASE_URL: &'static str = "https://study.ssu.ac.kr/xhr16";

    async fn compressed_request(url: &str, body: &str) -> Result<String, PluginError> {
        let client = reqwest::Client::new();
        let req = lz_str::compress_to_utf16(body);
        let res = client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(REFERER, Self::BASE_URL)
            .body(req)
            .send()
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?;
        let text = res
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        decompress_string(&text)
    }

    async fn initial_response() -> Result<StudyPostListResponse, PluginError> {
        let client = reqwest::Client::new();
        let initial_res = client.get(Self::BASE_URL).send().await.map_err(|e| {
            PluginError::request::<Self>(format!("Failed to request to initial page {e:?}"))
        })?;

        let text = initial_res.text().await.map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse initial page body {e:?}"))
        })?;
        let document = scraper::Html::parse_document(&text);
        let model_textarea = document
            .select(&MODEL_TEXTAREA_SELECTOR)
            .next()
            .ok_or_else(|| {
                PluginError::custom::<Self>(
                    "Failed to find model textarea".to_string(),
                    "The page structure may have changed".to_string(),
                )
            })?;
        let decompressed_str = decompress_string(&model_textarea.text().collect::<String>())?;
        let res: StudyPostListResponse = serde_json::from_str(&decompressed_str).map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse JSON data of post list: {e}"))
        })?;

        Ok(res)
    }

    async fn post_meta(posts_limit: u32) -> Result<Vec<StudyPostMeta>, PluginError> {
        tracing::info!("Fetching post metadata with limit: {}", posts_limit);
        tracing::info!("Fetching initial response from {}", Self::BASE_URL);
        let initial_res = Self::initial_response().await?;
        let total_page_count = initial_res.pagination_info.total_page_count;
        let mut metas = HashSet::<StudyPostMeta>::from_iter(initial_res.list.iter().cloned());
        let mut req: StudyBoardRequest = initial_res.into();
        while metas.len() < posts_limit as usize && req.page < total_page_count {
            tracing::info!(
                "Fetching page {} of {} for post metadata",
                req.page + 1,
                total_page_count
            );
            req.set_page(req.page + 1);
            let req_body = serde_json::to_string(&req).map_err(|e| {
                PluginError::parse::<Self>(format!("Failed to serialize request: {e}"))
            })?;
            let res = Self::compressed_request(
                &format!("{}/board/boardList.do", Self::API_BASE_URL),
                &req_body,
            )
            .await?;
            let res: StudyPostListResponse = serde_json::from_str(&res)
                .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
            metas.extend(res.list);
        }

        let mut metas = metas.into_iter().collect::<Vec<_>>();
        metas.truncate(posts_limit as usize);
        metas.sort_by_key(|meta| meta.sb_seq);

        Ok(metas)
    }

    async fn post(sb_seq: u32) -> Result<StudyPost, PluginError> {
        let client = reqwest::Client::new();
        let res = client
            .get(construct_post_url(sb_seq))
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!("Failed to get post {sb_seq}: {e:?}"))
            })?;

        let text = res.text().await.map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse initial page body {e:?}"))
        })?;
        let document = scraper::Html::parse_document(&text);
        let model_textarea = document
            .select(&MODEL_TEXTAREA_SELECTOR)
            .next()
            .ok_or_else(|| {
                PluginError::custom::<Self>(
                    "Failed to find model textarea".to_string(),
                    "The page structure may have changed".to_string(),
                )
            })?;
        let decompressed_str = decompress_string(&model_textarea.text().collect::<String>())?;
        let post: StudyPost = serde_json::from_str(&decompressed_str).map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse JSON data of post: {e}"))
        })?;

        Ok(post)
    }
}

impl SsufidPlugin for StudyPlugin {
    const IDENTIFIER: &'static str = "study.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 국제처";
    const DESCRIPTION: &'static str = "숭실대학교 국제처 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://study.ssu.ac.kr/community/notice_list.do";

    async fn crawl(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
        tracing::info!("Crawling {} posts from {}", posts_limit, Self::IDENTIFIER);
        Self::post_meta(posts_limit)
            .await?
            .into_iter()
            .map(|meta| Self::post(meta.sb_seq))
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<StudyPost>>()
            .await
            .map_err(|e| {
                PluginError::custom::<Self>(e.to_string(), "Failed to crawl posts".to_string())
            })
            .map(|posts| posts.into_iter().map(SsufidPost::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_integration() {
        let plugin = StudyPlugin;
        let posts = plugin.crawl(5).await.unwrap();

        assert!(!posts.is_empty());
        assert!(posts.len() <= 5);

        for post in posts {
            assert!(!post.id.is_empty());
            assert!(!post.title.is_empty());
            assert!(!post.url.is_empty());
        }
    }

    #[tokio::test]
    async fn test_initial_response_integration() {
        let response = StudyPlugin::initial_response().await.unwrap();

        assert!(!response.list.is_empty());
        assert!(response.pagination_info.total_page_count > 0);
    }
}
