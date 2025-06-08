use std::process::Command;

use futures::{TryStreamExt, stream::FuturesOrdered};
use serde::Deserialize;
use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};

pub struct MediambaPlugin;

impl MediambaPlugin {
    const API_BASE_URL: &'static str = "https://api.mediamba.ssu.ac.kr";

    async fn list_posts(
        base_url: &str,
        posts_limit: u32,
    ) -> Result<Vec<MediambaPost>, PluginError> {
        let res = reqwest::get(format!(
            "{}/v1/board/?page=0&size={}&menuId=89&content=",
            base_url, posts_limit
        ))
        .await
        .map_err(|e| PluginError::request::<Self>(e.to_string()))?
        .json::<MediambaBoardResponse>()
        .await
        .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        if !res.success {
            return Err(PluginError::custom::<Self>(
                "Failed to fetch posts".to_string(),
                res.message,
            ));
        }
        Ok(res.data.boards)
    }

    async fn parse_posts(posts: Vec<MediambaPost>) -> Result<Vec<SsufidPost>, PluginError> {
        posts
            .into_iter()
            .map(async |post| post.to_ssufid_post("http://localhost:8000").await)
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
    }
}

impl SsufidPlugin for MediambaPlugin {
    const IDENTIFIER: &'static str = "mediamba.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 미디어경영학부";
    const DESCRIPTION: &'static str = "숭실대학교 미디어경영학부 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://mediamba.ssu.ac.kr/board/notice";

    async fn crawl(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
        let mut runtime = Command::new("deno")
            .args([
                "run",
                "--allow-read",
                "--allow-write",
                "--allow-env",
                "--allow-net",
                "--allow-import",
                "./lexical-parser/src/main.ts",
            ])
            .spawn()
            .map_err(|e| {
                PluginError::custom::<MediambaPlugin>(
                    e.to_string(),
                    "Failed to spawn lexical parser".to_string(),
                )
            })?;
        let posts = Self::list_posts(Self::API_BASE_URL, posts_limit).await?;
        let result = Self::parse_posts(posts).await.map_err(|e| {
            PluginError::custom::<Self>(
                e.to_string(),
                "Thread panicked while parsing posts to html".to_string(),
            )
        });
        runtime.kill().map_err(|e| {
            PluginError::custom::<MediambaPlugin>(
                e.to_string(),
                "Failed to kill lexical parser".to_string(),
            )
        })?;
        result
    }
}

#[derive(Deserialize, Debug)]

struct MediambaBoardResponse {
    success: bool,
    data: MediambaBoardData,
    #[allow(dead_code)]
    code: String,
    message: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct MediambaBoardData {
    boards: Vec<MediambaPost>,
    page: u32,
    size: u32,
    total_page: u32,
}

const DATETIME_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct MediambaPost {
    id: u32,
    title: String,
    is_pinned: bool,
    hits: u32,
    like_summary: Option<String>,
    content: String,
    access_role: String,
    attachments: Option<serde_json::Value>,
    has_attachment: bool,
    user_id: u32,
    user_name: String,
    menu_id: u32,
    board_navigation: Option<serde_json::Value>,
    #[serde(deserialize_with = "deserialize_mediamba_datetime")]
    created_at: OffsetDateTime,
    #[serde(deserialize_with = "deserialize_mediamba_datetime")]
    updated_at: OffsetDateTime,
}
impl MediambaPost {
    async fn to_ssufid_post(
        &self,
        parser_host: &str,
    ) -> Result<ssufid::core::SsufidPost, PluginError> {
        let client = reqwest::Client::new();
        let res = client
            .post(parser_host)
            .body(self.content.clone())
            .send()
            .await
            .map_err(|e| PluginError::request::<MediambaPlugin>(e.to_string()))?;
        if !res.status().is_success() {
            return Err(PluginError::parse::<MediambaPlugin>(format!(
                "Failed to receive content: {}",
                res.status(),
            )));
        }

        let content_html = res
            .text()
            .await
            .map_err(|e| PluginError::parse::<MediambaPlugin>(e.to_string()))?;

        Ok(SsufidPost {
            id: self.id.to_string(),
            url: format!("{}/{}", MediambaPlugin::BASE_URL, self.id),
            author: Some(self.user_name.clone()),
            title: self.title.clone(),
            description: Some(content_html.clone()),
            category: vec![],
            created_at: self.created_at,
            updated_at: Some(self.updated_at),
            thumbnail: None,
            content: content_html,
            attachments: vec![],
            metadata: None,
        })
    }
}

fn deserialize_mediamba_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PrimitiveDateTime::parse(&s, DATETIME_FORMAT)
        .map_err(serde::de::Error::custom)?
        .assume_offset(offset!(+9)))
}
