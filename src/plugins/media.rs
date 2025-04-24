use std::sync::LazyLock;

use futures::{TryStreamExt as _, stream::FuturesUnordered};
use scraper::{ElementRef, Html, Selector, html::Select};
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};

use crate::{
    PluginError,
    core::{Attachment, SsufidPlugin, SsufidPost},
};

trait SelectExt {
    fn elem_first(&mut self) -> Result<ElementRef, PluginError>;

    fn text_first(&mut self) -> Result<String, PluginError>;

    fn html_first(&mut self) -> Result<String, PluginError>;
}

impl SelectExt for Select<'_, '_> {
    fn elem_first(&mut self) -> Result<ElementRef, PluginError> {
        self.next().ok_or(PluginError::parse::<MediaPlugin>(
            "Failed to get element".to_string(),
        ))
    }
    fn text_first(&mut self) -> Result<String, PluginError> {
        Ok(self.elem_first()?.text().collect::<String>())
    }

    fn html_first(&mut self) -> Result<String, PluginError> {
        Ok(self.elem_first()?.html())
    }
}

pub struct MediaPlugin;

impl SsufidPlugin for MediaPlugin {
    const IDENTIFIER: &'static str = "media.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 글로벌벌미디어학부";
    const DESCRIPTION: &'static str =
        "숭실대학교 글로벌미디어학부 홈페이지의 공지사항을 제공합니다.";
    const BASE_URL: &'static str = "http://media.ssu.ac.kr/sub.php?code=XxH00AXY&category=1";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let post_ids = Self::list_posts(Self::BASE_URL, posts_limit).await?;
        post_ids
            .iter()
            .map(|post_id| Self::get_post(post_id))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<Vec<_>>()
            .await
    }
}

static LIST_ITEM_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("table tbody tr a").unwrap());

impl MediaPlugin {
    async fn list_posts(base_url: &str, posts_limit: u32) -> Result<Vec<String>, PluginError> {
        let res = reqwest::get(format!(
            "{base_url}&orderType=idx&orderBy=desc&mode=list&page=1&limit={posts_limit}"
        ))
        .await
        .map_err(|e| PluginError::request::<Self>(e.to_string()))?
        .text()
        .await
        .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        let document = Html::parse_document(&res);
        let links = document.select(&LIST_ITEM_SELECTOR);
        links.map(as_post_id).collect()
    }

    async fn get_post(post_id: &str) -> Result<SsufidPost, PluginError> {
        let res = reqwest::get(format!("{}&mode=view&board_num={post_id}", Self::BASE_URL))
            .await
            .map_err(|e| PluginError::request::<Self>(e.to_string()))?
            .text()
            .await
            .map_err(|e| PluginError::parse::<Self>(e.to_string()))?;
        let document = Html::parse_document(&res);
        let media_post = MediaPost::from_document(Self::BASE_URL, post_id, document)?;
        Ok(media_post.into())
    }
}

fn as_post_id(element: scraper::ElementRef) -> Result<String, PluginError> {
    element
        .value()
        .attr("onclick")
        .ok_or(PluginError::parse::<MediaPlugin>(
            "Failed to parse post id, cannot find onclick attr".to_string(),
        ))?
        .split('\'')
        .nth(1)
        .map(str::to_string)
        .ok_or(PluginError::parse::<MediaPlugin>(
            "Failed to parse post id, invalid id format".to_string(),
        ))
}

pub struct MediaPost {
    pub id: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub created_at: OffsetDateTime,
    pub attachments: Vec<Attachment>,
    pub content: String,
}

static TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("#fn").unwrap());
static AUTHOR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".s_default_view_body_1 > table > tbody > tr:first-child > td:first-child > span:last-child").unwrap()
});
static CREATED_AT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".s_default_view_body_1 > table > tbody > tr:first-child > td:nth-child(2) > span:last-child").unwrap()
});
static ATTACHMENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".s_default_view_body_1 > table > tbody > tr:last-child a").unwrap()
});
static CONTENT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".s_default_view_body_2 > table td").unwrap());

const DATE_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

impl MediaPost {
    fn from_document(base_url: &str, id: &str, document: Html) -> Result<Self, PluginError> {
        let title = document.select(&TITLE_SELECTOR).text_first()?;
        let author = document.select(&AUTHOR_SELECTOR).text_first()?;
        let created_at = document.select(&CREATED_AT_SELECTOR).text_first()?;
        let created_at = PrimitiveDateTime::parse(created_at.trim(), DATE_FORMAT)
            .map(|dt| dt.assume_offset(offset!(+9)))
            .map_err(|e| {
                PluginError::parse::<MediaPlugin>(format!("Failed to parse date: {}", e))
            })?;
        let attachments = document
            .select(&ATTACHMENT_SELECTOR)
            .filter_map(|element| {
                element.value().attr("href").map(|href| {
                    Attachment::from_guess(element.text().collect::<String>(), href.to_string())
                })
            })
            .collect::<Vec<_>>();
        let content = document.select(&CONTENT_SELECTOR).html_first()?;

        Ok(Self {
            id: id.to_string(),
            url: format!("{base_url}&mode=view&board_num={id}"),
            title,
            author,
            created_at,
            attachments,
            content,
        })
    }
}

impl From<MediaPost> for SsufidPost {
    fn from(post: MediaPost) -> Self {
        Self {
            id: post.id,
            url: post.url,
            title: post.title,
            description: None,
            thumbnail: None,
            author: Some(post.author),
            category: vec![],
            created_at: post.created_at,
            updated_at: None,
            attachments: post.attachments,
            content: post.content,
            metadata: None,
        }
    }
}
