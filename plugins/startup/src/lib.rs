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

pub struct StartupPlugin;

impl StartupPlugin {
    const API_BASE_URL: &'static str = "https://startup.ssu.ac.kr/api";

    async fn list_posts(base_url: &str, posts_limit: u32) -> Result<Vec<StartupPost>, PluginError> {
        let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/board/content/list?boardEnName=notice&categoryCodeId&pageNum=1&pageSize={posts_limit}&searchMonth="
        )).header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| {tracing::error!(?e); PluginError::request::<Self>(e.to_string())})?
        .json::<StartupBoardResponse>()
        .await
        .map_err(|e| {tracing::error!(?e); PluginError::parse::<Self>(e.to_string())})?;
        if res.code != 200 {
            return Err(PluginError::custom::<Self>(
                "Failed to fetch posts".to_string(),
                res.message.unwrap_or("Unknown error".to_string()),
            ));
        }
        Ok(res.data.content.list)
    }
}

impl SsufidPlugin for StartupPlugin {
    const IDENTIFIER: &'static str = "startup.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 창업포털";
    const DESCRIPTION: &'static str = "숭실대학교 창업포털 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://startup.ssu.ac.kr/board/notice";

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

struct StartupBoardResponse {
    data: StartupBoardData,
    code: u32,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartupBoardData {
    content: StartupBoardContent,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartupBoardContent {
    list: Vec<StartupPost>,
    total: u32,
    page_num: u32,
    page_size: u32,
    size: u32,
    start_row: u32,
    end_row: u32,
    pages: u32,
    pre_page: u32,
    next_page: u32,
    is_first_page: bool,
    is_last_page: bool,
    has_previous_page: bool,
    has_next_page: bool,
    navigate_pages: u32,
    navigatepage_nums: Vec<u32>,
    navigate_first_page: u32,
    navigate_last_page: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartupBoardCategory {
    category_code_id: u32,
    board_en_name: String,
    category_name: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartupFile {
    file_id: u32,
    content_id: u32,
    status: String,
    file_size: u32,
    division: String,
    file_path: String,
    file_name: String,
    file_origin_name: String,
    file_extension: String,
    work: String,
}

impl From<StartupFile> for Attachment {
    fn from(file: StartupFile) -> Self {
        Attachment {
            name: Some(file.file_origin_name),
            url: format!(
                "{}/resource/download/{}",
                StartupPlugin::API_BASE_URL,
                file.file_id,
            ),
            mime_type: None,
        }
    }
}

const DATETIME_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartupPost {
    board_content_id: u32,
    board_en_name: String,
    board_title: String,
    board_content: String,
    user_id: String,
    category_code_id: u32,
    status: String,
    #[serde(deserialize_with = "deserialize_yn")]
    notice_yn: bool,
    #[serde(deserialize_with = "deserialize_yn")]
    with_notice_yn: bool,
    view_cnt: u32,
    #[serde(deserialize_with = "deserialize_startup_datetime")]
    reg_date: OffsetDateTime,
    #[serde(deserialize_with = "deserialize_startup_datetime")]
    update_date: OffsetDateTime,
    board_category: StartupBoardCategory,
    file_list: Vec<StartupFile>,
}

impl From<StartupPost> for SsufidPost {
    fn from(post: StartupPost) -> Self {
        SsufidPost {
            id: post.board_content_id.to_string(),
            title: post.board_title,
            url: format!(
                "{}/{}?boardEnName=notice",
                StartupPlugin::BASE_URL,
                post.board_content_id
            ),
            author: Some(post.user_id),
            description: None,
            category: vec![post.board_category.category_name],
            created_at: post.reg_date,
            updated_at: Some(post.update_date),
            thumbnail: None,
            content: post.board_content,
            attachments: post.file_list.into_iter().map(Attachment::from).collect(),
            metadata: None,
        }
    }
}

fn deserialize_yn<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "Y" | "y" => Ok(true),
        "N" | "n" => Ok(false),
        _ => Err(serde::de::Error::custom("Invalid value for Y/N")),
    }
}

fn deserialize_startup_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PrimitiveDateTime::parse(&s, DATETIME_FORMAT)
        .map_err(serde::de::Error::custom)?
        .assume_offset(offset!(+9)))
}
