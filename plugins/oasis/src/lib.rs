use futures::{TryStreamExt, stream::FuturesOrdered};
use serde::Deserialize;
use ssufid::{
    PluginError,
    core::{Attachment, SsufidPlugin, SsufidPost},
};
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};

pub struct OasisPlugin;

impl OasisPlugin {
    const API_BASE_URL: &'static str = "https://oasis.ssu.ac.kr/pyxis-api";

    async fn list_posts(
        base_url: &str,
        posts_limit: u32,
    ) -> Result<Vec<OasisPostMeta>, PluginError> {
        let res = reqwest::get(format!(
            "{}/1/bulletin-boards/1/bulletins?nameOption=part&isSeq=false&onlyWriter=false&max={}",
            base_url, posts_limit
        ))
        .await
        .map_err(|e| PluginError::request::<Self>(e.to_string()))?
        .json::<OasisBoardResponse>()
        .await
        .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        if !res.success {
            return Err(PluginError::custom::<Self>(
                "Failed to fetch posts".to_string(),
                res.message,
            ));
        }
        Ok(res.data.list)
    }

    async fn request_posts(metas: Vec<OasisPostMeta>) -> Result<Vec<SsufidPost>, PluginError> {
        metas
            .into_iter()
            .map(async |meta| meta.to_ssufid_post().await)
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
    }
}

impl SsufidPlugin for OasisPlugin {
    const IDENTIFIER: &'static str = "oasis.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 도서관";
    const DESCRIPTION: &'static str = "숭실대학교 도서관 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "https://oasis.ssu.ac.kr/library-services/bulletin/notice";

    async fn crawl(
        &self,
        posts_limit: u32,
    ) -> Result<Vec<ssufid::core::SsufidPost>, ssufid::PluginError> {
        let metas = Self::list_posts(Self::API_BASE_URL, posts_limit).await?;

        Self::request_posts(metas).await.map_err(|e| {
            PluginError::custom::<Self>(
                e.to_string(),
                "Thread panicked while parsing posts to html".to_string(),
            )
        })
    }
}

#[derive(Deserialize, Debug)]

struct OasisBoardResponse {
    success: bool,
    data: OasisBoardData,
    #[allow(dead_code)]
    code: String,
    message: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct OasisPostResponse {
    success: bool,
    data: OasisPost,
    code: String,
    message: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct OasisBoardData {
    list: Vec<OasisPostMeta>,
    total_count: u32,
    offset: u32,
    max: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct OasisPostMeta {
    id: u32,
    seq_no: u32,
    title: String,
    writer: String,
    #[serde(deserialize_with = "deserialize_oasis_datetime")]
    date_created: OffsetDateTime,
    attachments: Vec<OasisAttachment>,
    #[serde(deserialize_with = "deserialize_oasis_datetime")]
    last_updated: OffsetDateTime,
}

impl OasisPostMeta {
    async fn to_ssufid_post(&self) -> Result<SsufidPost, PluginError> {
        let res = reqwest::get(format!(
            "{}/1/bulletins/1/{}?nameOption=part",
            OasisPlugin::API_BASE_URL,
            self.id
        ))
        .await
        .map_err(|e| {
            PluginError::request::<OasisPlugin>(format!("Failed to request to post api {e:?}"))
        })?
        .json::<OasisPostResponse>()
        .await
        .map_err(|e| {
            PluginError::parse::<OasisPlugin>(format!("Failed to parse post api body {e:?}"))
        })?;

        if !res.success {
            return Err(PluginError::custom::<OasisPlugin>(
                "Failed to fetch post".to_string(),
                res.message,
            ));
        }

        Ok(res.data.into())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct OasisAttachment {
    id: u32,
    physical_name: Option<String>,
    logical_name: String,
    original_image_url: String,
    file_type: String,
    file_size: u32,
}

impl From<OasisAttachment> for Attachment {
    fn from(attachment: OasisAttachment) -> Self {
        Attachment {
            name: Some(attachment.logical_name),
            url: format!(
                "{}{}",
                OasisPlugin::API_BASE_URL,
                attachment.original_image_url,
            ),
            mime_type: Some(attachment.file_type),
        }
    }
}

const DATETIME_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct OasisPost {
    id: u32,
    seq_no: u32,
    title: String,
    content: String,
    worker: OasisWorker,
    #[serde(deserialize_with = "deserialize_oasis_datetime")]
    date_created: OffsetDateTime,
    #[serde(deserialize_with = "deserialize_oasis_datetime")]
    last_updated: OffsetDateTime,
    attachments: Vec<OasisAttachment>,
}

#[derive(Deserialize, Debug)]
struct OasisWorker {
    name: String,
}

impl From<OasisPost> for SsufidPost {
    fn from(post: OasisPost) -> Self {
        SsufidPost {
            id: post.id.to_string(),
            title: post.title,
            url: format!("{}/{}", OasisPlugin::BASE_URL, post.id),
            author: Some(post.worker.name),
            description: None,
            category: vec![],
            created_at: post.date_created,
            updated_at: Some(post.last_updated),
            thumbnail: None,
            content: post.content,
            attachments: post.attachments.into_iter().map(Attachment::from).collect(),
            metadata: None,
        }
    }
}

fn deserialize_oasis_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PrimitiveDateTime::parse(&s, DATETIME_FORMAT)
        .map_err(serde::de::Error::custom)?
        .assume_offset(offset!(+9)))
}
