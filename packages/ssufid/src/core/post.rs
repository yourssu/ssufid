use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Attachment {
    pub url: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

impl Attachment {
    pub fn from_guess(name: String, url: String) -> Self {
        let mime = mime_guess::from_path(&name).first().map(|m| m.to_string());
        Self {
            url,
            name: Some(name),
            mime_type: mime,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidPost {
    pub id: String,
    pub url: String,
    pub author: Option<String>,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<time::OffsetDateTime>,
    pub thumbnail: Option<String>,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub metadata: Option<BTreeMap<String, String>>,
}

impl PartialOrd for SsufidPost {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.created_at.cmp(&other.created_at))
    }
}

impl SsufidPost {
    pub fn contents_eq(&self, other: &SsufidPost) -> bool {
        self.id.trim() == other.id.trim()
            && self.title.trim() == other.title.trim()
            && self.category == other.category
            && self.content.trim() == other.content.trim()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidSiteData {
    pub(crate) title: String,
    pub(crate) source: String,
    pub(crate) description: String,
    pub(crate) items: Vec<SsufidPost>,
}

#[cfg(feature = "rss")]
impl SsufidSiteData {
    pub fn to_rss(self) -> ::rss::Channel {
        self.into()
    }
}
