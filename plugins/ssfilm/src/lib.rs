use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use ssufid::{
    PluginError,
    core::{Attachment, SsufidPlugin, SsufidPost},
};
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};
use url::Url;

pub struct SsfilmPlugin;

impl SsfilmPlugin {
    const API_BASE_URL: &'static str = "http://ssfilm.ssu.ac.kr/notice/notice_list";

    async fn list_posts(base_url: &str, posts_limit: u32) -> Result<Vec<SsfilmPost>, PluginError> {
        let mut posts = Vec::new();
        let mut last_notice_index: Option<u32> = None;
        while posts.len() < posts_limit as usize {
            let url = if let Some(index) = last_notice_index {
                &format!("{}?LastNoticeIndex={}", base_url, index)
            } else {
                base_url
            };

            let response = reqwest::Client::new()
                .get(url)
                .header(CONTENT_TYPE, "application/json")
                .send()
                .await
                .map_err(|e| PluginError::request::<Self>(format!("Failed to request: {e:?}")))?;

            if !response.status().is_success() {
                return Err(PluginError::request::<Self>(format!(
                    "Failed to request with status code: {}",
                    response.status()
                )));
            }

            let board_response: SsfilmBoardResponse = response.json().await.map_err(|e| {
                PluginError::parse::<Self>(format!("Failed to parse response json: {e:?}"))
            })?;

            if board_response.data_list.is_empty() {
                break;
            }

            if let Some(last) = board_response.data_list.last() {
                last_notice_index = last.notice_index.parse().ok().or(last_notice_index);
            }

            posts.extend(board_response.data_list);
        }

        posts.truncate(posts_limit as usize);
        Ok(posts)
    }
}

impl SsufidPlugin for SsfilmPlugin {
    const IDENTIFIER: &'static str = "ssfilm.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 예술창작학부 영화예술전공";
    const DESCRIPTION: &'static str = "숭실대학교 영화예술전공 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://ssfilm.ssu.ac.kr/notice/index";

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
#[allow(dead_code)]
struct SsfilmBoardResponse {
    data_list: Vec<SsfilmPost>,
    #[serde(rename = "restCount")]
    rest_count: u32,
    #[serde(rename = "LastNoticeIndex")]
    last_notice_index: LastNoticeIndex,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
#[allow(dead_code)]
enum LastNoticeIndex {
    False(bool),
    Index(String),
}

const DATETIME_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct SsfilmPost {
    notice_index: String,
    category: String,
    title: String,
    content: String,
    #[serde(rename = "RegID")]
    reg_id: String,
    #[serde(deserialize_with = "deserialize_ssfilm_datetime")]
    reg_date: OffsetDateTime,
    file_data: String,
    org_file: String,
    image: String,
}

impl From<SsfilmPost> for SsufidPost {
    fn from(post: SsfilmPost) -> Self {
        SsufidPost {
            id: post.notice_index,
            title: post.title,
            url: SsfilmPlugin::BASE_URL.to_string(),
            author: Some(post.reg_id),
            description: None,
            category: vec![post.category],
            created_at: post.reg_date,
            updated_at: None,
            thumbnail: None,
            content: post.content,
            attachments: (!post.file_data.is_empty())
                .then_some(Attachment {
                    url: construct_file_url(&post.file_data, &post.org_file),
                    name: Some(post.org_file),
                    mime_type: None,
                })
                .into_iter()
                .collect(),
            metadata: None,
        }
    }
}

fn construct_file_url(file_data: &str, org_file: &str) -> String {
    let mut download_url = Url::parse("http://ssfilm.ssu.ac.kr/download_file").unwrap();
    download_url
        .query_pairs_mut()
        .append_pair("filename", file_data)
        .append_pair(
            "filepath",
            format!("/resource/upload/notice/{}", org_file).as_str(),
        );
    download_url.to_string()
}

fn deserialize_ssfilm_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PrimitiveDateTime::parse(&s, DATETIME_FORMAT)
        .map_err(serde::de::Error::custom)?
        .assume_offset(offset!(+9)))
}
