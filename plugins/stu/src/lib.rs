use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};

pub struct StuPlugin;

impl StuPlugin {
    const API_BASE_URL: &'static str = "https://backend.sssupport.shop";

    async fn list_posts(base_url: &str, posts_limit: u32) -> Result<Vec<StuPost>, PluginError> {
        let res = reqwest::Client::new()
            .get(format!(
                "{base_url}/board/공지사항게시판/posts/search?page=0&take={posts_limit}&q="
            ))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| {
                tracing::error!(?e);
                PluginError::request::<Self>(e.to_string())
            })?
            .json::<StuBoardResponse>()
            .await
            .map_err(|e| {
                tracing::error!(?e);
                PluginError::parse::<Self>(e.to_string())
            })?;
        if !res.is_success {
            return Err(PluginError::custom::<Self>(
                "Failed to fetch posts".to_string(),
                res.message.unwrap_or("Unknown error".to_string()),
            ));
        }
        Ok(res.data.post_list_res_dto)
    }
}

impl SsufidPlugin for StuPlugin {
    const IDENTIFIER: &'static str = "stu.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 총학생회";
    const DESCRIPTION: &'static str = "숭실대학교 총학생회 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://stu.ssu.ac.kr/notice";

    async fn crawl(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
        Self::list_posts(Self::API_BASE_URL, posts_limit)
            .await
            .map(|posts| posts.into_iter().map(SsufidPost::from).collect())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StuBoardResponse {
    data: StuBoardData,
    is_success: bool,
    code: String,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StuBoardData {
    post_list_res_dto: Vec<StuPost>,
    page_info: StuBoardPageInfo,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StuBoardPageInfo {
    page_num: u32,
    page_size: u32,
    total_elements: u32,
    total_pages: u32,
}

const DATETIME_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]/[month]/[day] [hour]:[minute]:[second]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StuPost {
    post_id: u32,
    title: String,
    content: String,
    #[serde(deserialize_with = "deserialize_stu_datetime")]
    date: OffsetDateTime,
    category: Option<String>,
    thumb_nail: Option<String>,
    author: String,
}

impl From<StuPost> for SsufidPost {
    fn from(post: StuPost) -> Self {
        SsufidPost {
            id: post.post_id.to_string(),
            title: post.title,
            url: format!("{}/{}", StuPlugin::BASE_URL, post.post_id),
            author: Some(post.author),
            description: None,
            category: post.category.into_iter().collect::<Vec<_>>(),
            created_at: post.date,
            updated_at: None,
            thumbnail: post.thumb_nail,
            content: post.content,
            attachments: vec![],
            metadata: None,
        }
    }
}

fn deserialize_stu_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PrimitiveDateTime::parse(&s, DATETIME_FORMAT)
        .map_err(serde::de::Error::custom)?
        .assume_offset(offset!(+9)))
}
