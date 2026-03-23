use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidCalendar {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ends_at: Option<time::OffsetDateTime>,
    pub location: Option<String>,
    pub url: Option<String>,
}

impl PartialOrd for SsufidCalendar {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(
            self.starts_at
                .cmp(&other.starts_at)
                .then_with(|| self.id.cmp(&other.id)),
        )
    }
}

impl SsufidCalendar {
    pub fn contents_eq(&self, other: &SsufidCalendar) -> bool {
        self.id.trim() == other.id.trim()
            && self.title.trim() == other.title.trim()
            && self.description.as_deref().map(str::trim)
                == other.description.as_deref().map(str::trim)
            && self.starts_at == other.starts_at
            && self.ends_at == other.ends_at
            && self.location.as_deref().map(str::trim) == other.location.as_deref().map(str::trim)
            && self.url.as_deref().map(str::trim) == other.url.as_deref().map(str::trim)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SsufidCalendarSiteData {
    pub(crate) title: String,
    pub(crate) source: String,
    pub(crate) description: String,
    pub(crate) items: Vec<SsufidCalendar>,
}

#[cfg(feature = "ics")]
impl SsufidCalendarSiteData {
    pub fn to_ics(&self) -> String {
        super::ics::to_ics(self)
    }
}
