use serde::{Deserialize, Serialize};
use ssufid::core::{Attachment, SsufidPost};
use time::{
    Date, OffsetDateTime,
    macros::{format_description, offset},
};

use crate::construct_post_url;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(super) struct StudyBoardRequest {
    search_type: String,
    site_cd: String,
    section_cd: String,
    notice_cd: String,
    #[serde(rename = "contentYN")]
    content_yn: String,
    search_val: String,
    board_cd: String,
    gb_cd: String,
    target_cd: String,
    cate_seq: String,
    pub current_page_no: u32,
    first_page_no: u32,
    first_page_no_on_page_list: u32,
    first_record_index: u32,
    last_page_no: u32,
    last_page_no_on_page_list: u32,
    last_record_index: u32,
    page_size: u32,
    record_count_per_page: u32,
    total_page_count: u32,
    total_record_count: u32,
    view_record_count: u32,
    pub page: u32,
}

impl StudyBoardRequest {
    pub fn set_page(&mut self, page: u32) {
        self.current_page_no = page;
        self.page = page;
    }
}

impl From<StudyPostListResponse> for StudyBoardRequest {
    fn from(response: StudyPostListResponse) -> Self {
        Self {
            site_cd: response.site_cd,
            section_cd: response.section_cd,
            board_cd: response.board_cd,
            current_page_no: response.pagination_info.current_page_no,
            first_page_no: response.pagination_info.first_page_no,
            first_page_no_on_page_list: response.pagination_info.first_page_no_on_page_list,
            first_record_index: response.pagination_info.first_record_index,
            last_page_no: response.pagination_info.last_page_no,
            last_page_no_on_page_list: response.pagination_info.last_page_no_on_page_list,
            last_record_index: response.pagination_info.last_record_index,
            page_size: response.pagination_info.page_size,
            record_count_per_page: response.pagination_info.record_count_per_page,
            total_page_count: response.pagination_info.total_page_count,
            total_record_count: response.pagination_info.total_record_count,
            view_record_count: response.pagination_info.view_record_count,
            page: response.pagination_info.current_page_no,
            ..Default::default()
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyPaginationInfo {
    current_page_no: u32,
    first_page_no: u32,
    first_page_no_on_page_list: u32,
    first_record_index: u32,
    last_page_no: u32,
    last_page_no_on_page_list: u32,
    last_record_index: u32,
    page_size: u32,
    record_count_per_page: u32,
    pub total_page_count: u32,
    total_record_count: u32,
    view_record_count: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyPostListResponse {
    uri: String,
    site_cd: String,
    board_cd: String,
    section_cd: String,
    record_count_per_page: u32,
    pub pagination_info: StudyPaginationInfo,
    pub list: Vec<StudyPostMeta>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyPostMeta {
    pub(crate) sb_seq: u32,
    title: String,
    file_no: u32,
    board_cd: String,
    user_nm: String,
    #[serde(deserialize_with = "deserialize_study_datetime")]
    reg_dt: OffsetDateTime,
    file_cnt: u32,
    top_yn: String,
    file_list: Vec<StudyFile>,
}

impl PartialEq for StudyPostMeta {
    fn eq(&self, other: &Self) -> bool {
        self.sb_seq == other.sb_seq
    }
}

impl Eq for StudyPostMeta {}

impl std::hash::Hash for StudyPostMeta {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.sb_seq.hash(state);
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyFile {
    file_no: u32,
    file_order: u32,
    file_nm: String,
    file_ext: String,
    save_path: String,
    reg_dt: String,
    status: u32,
}

impl StudyFile {
    pub fn to_attachment(&self, post_url: String) -> Attachment {
        Attachment {
            name: Some(self.file_nm.clone()),
            url: post_url,
            mime_type: None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyPost {
    pub sb_seq: String,
    pub info: StudyPostInfo,
    pub file_list: Vec<StudyFile>,
}

impl From<StudyPost> for SsufidPost {
    fn from(post: StudyPost) -> Self {
        let post_url = construct_post_url(post.info.sb_seq);
        SsufidPost {
            id: post.sb_seq,
            title: post.info.title,
            url: post_url.clone(),
            author: Some(post.info.user_nm),
            description: None,
            category: vec![],
            created_at: post.info.reg_dt,
            updated_at: Some(post.info.mod_dt),
            thumbnail: None,
            content: post.info.content,
            attachments: post
                .file_list
                .into_iter()
                .map(|f| f.to_attachment(post_url.clone()))
                .collect(),
            metadata: None,
        }
    }
}

const DATE_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year].[month].[day]");

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct StudyPostInfo {
    sb_seq: u32,
    site_cd: String,
    title: String,
    content: String,
    top_yn: String,
    #[serde(deserialize_with = "deserialize_study_datetime")]
    reg_dt: OffsetDateTime,
    #[serde(deserialize_with = "deserialize_study_datetime")]
    mod_dt: OffsetDateTime,
    user_nm: String,
}

fn deserialize_study_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Date::parse(&s, DATE_FORMAT)
        .map_err(serde::de::Error::custom)?
        .midnight()
        .assume_offset(offset!(+9)))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_construct_post_url() {
        let sb_seq = 2945;
        let url = construct_post_url(sb_seq);
        assert_eq!(
            url,
            "https://study.ssu.ac.kr/community/notice_view.do?sbSeq=Mjk0NQ%3D%3D"
        );
    }
}
