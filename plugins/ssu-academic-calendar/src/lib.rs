use std::{collections::BTreeMap, sync::LazyLock};

use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use ssufid::{
    PluginError,
    core::{SsufidCalendar, SsufidCalendarPlugin, SsufidPlugin},
};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset, macros::offset};
use url::Url;

pub struct SsuAcademicCalendarPlugin;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
const KST: UtcOffset = offset!(+9);

static CALENDAR_SECTION_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#calendar").expect("valid calendar selector"));
static MONTH_BLOCK_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#calendar > div[id^='calendar']").expect("valid month block selector")
});
static MONTH_LABEL_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("span.font-size-50").expect("valid month label selector"));
static EVENT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("ul.tb > li").expect("valid event selector"));
static EVENT_COLUMN_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".row > div").expect("valid event column selector"));
static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid link selector"));

impl SsufidPlugin for SsuAcademicCalendarPlugin {
    const IDENTIFIER: &'static str = "ssu-academic-calendar";
    const TITLE: &'static str = "숭실대학교 학사일정";
    const DESCRIPTION: &'static str =
        "숭실대학교 공식 학사일정 페이지의 학사 일정을 캘린더 형식으로 제공합니다.";
    const BASE_URL: &'static str =
        "https://ssu.ac.kr/%ED%95%99%EC%82%AC/%ED%95%99%EC%82%AC%EC%9D%BC%EC%A0%95/";
}

impl SsufidCalendarPlugin for SsuAcademicCalendarPlugin {
    async fn crawl(&self, calendar_limit_days: u32) -> Result<Vec<SsufidCalendar>, PluginError> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| PluginError::request::<Self>(format!("Failed to build client: {e}")))?;
        let now = OffsetDateTime::now_utc().to_offset(KST);
        let target_years = Self::target_years(calendar_limit_days, now);
        let mut events = BTreeMap::new();

        for year in target_years {
            let page_url = Self::year_page_url(year)?;
            tracing::info!(year, url = %page_url, "Fetching academic calendar page");
            let html = Self::fetch_year_page(&client, &page_url).await?;
            for item in Self::parse_year_page(&html, year, &page_url)? {
                events.insert(item.id.clone(), item);
            }
        }

        let mut items = events.into_values().collect::<Vec<_>>();
        items.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Ok(items)
    }
}

impl SsuAcademicCalendarPlugin {
    fn target_years(calendar_limit_days: u32, now: OffsetDateTime) -> Vec<i32> {
        let current_year = now.year();
        let start_year = if calendar_limit_days == 0 {
            current_year - 1
        } else {
            (now - time::Duration::days(calendar_limit_days as i64)).year()
        };
        let end_year = if calendar_limit_days == 0 || now.month() == Month::December {
            current_year + 1
        } else {
            current_year
        };

        (start_year..=end_year).collect()
    }

    fn year_page_url(year: i32) -> Result<Url, PluginError> {
        let mut url = Url::parse(Self::BASE_URL)
            .map_err(|e| PluginError::parse::<Self>(format!("Invalid base URL: {e}")))?;
        url.query_pairs_mut()
            .append_pair("years", &year.to_string());
        Ok(url)
    }

    async fn fetch_year_page(client: &Client, page_url: &Url) -> Result<String, PluginError> {
        client
            .get(page_url.clone())
            .send()
            .await
            .map_err(|e| {
                PluginError::request::<Self>(format!("Failed to request {page_url}: {e}"))
            })?
            .error_for_status()
            .map_err(|e| {
                PluginError::request::<Self>(format!("Unexpected status from {page_url}: {e}"))
            })?
            .text()
            .await
            .map_err(|e| {
                PluginError::parse::<Self>(format!(
                    "Failed to read response body from {page_url}: {e}"
                ))
            })
    }

    fn parse_year_page(
        html: &str,
        year: i32,
        page_url: &Url,
    ) -> Result<Vec<SsufidCalendar>, PluginError> {
        let document = Html::parse_document(html);
        if document.select(&CALENDAR_SECTION_SELECTOR).next().is_none() {
            return Err(PluginError::parse::<Self>(format!(
                "Calendar section not found for year {year}"
            )));
        }

        let month_blocks = document.select(&MONTH_BLOCK_SELECTOR).collect::<Vec<_>>();
        if month_blocks.is_empty() {
            return Err(PluginError::parse::<Self>(format!(
                "Month blocks not found for year {year}"
            )));
        }

        let mut items = Vec::new();
        for month_block in month_blocks {
            let month = Self::parse_month(month_block)?;
            for event in month_block.select(&EVENT_SELECTOR) {
                match Self::parse_event(event, year, month, page_url) {
                    Ok(Some(item)) => items.push(item),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(year, month, error = %error, "Skipping malformed calendar item");
                    }
                }
            }
        }

        if items.is_empty() {
            return Err(PluginError::parse::<Self>(format!(
                "No calendar items were parsed for year {year}"
            )));
        }

        Ok(items)
    }

    fn parse_month(month_block: ElementRef<'_>) -> Result<u8, PluginError> {
        let month_text = month_block
            .select(&MONTH_LABEL_SELECTOR)
            .next()
            .map(Self::element_text)
            .ok_or_else(|| PluginError::parse::<Self>("Month label not found".to_string()))?;

        month_text.parse::<u8>().map_err(|e| {
            PluginError::parse::<Self>(format!("Failed to parse month '{month_text}': {e}"))
        })
    }

    fn parse_event(
        event: ElementRef<'_>,
        year: i32,
        month: u8,
        page_url: &Url,
    ) -> Result<Option<SsufidCalendar>, PluginError> {
        let columns = event.select(&EVENT_COLUMN_SELECTOR).collect::<Vec<_>>();
        let [date_column, title_column, ..] = columns.as_slice() else {
            return Err(PluginError::parse::<Self>(
                "Calendar item does not contain two columns".to_string(),
            ));
        };

        let date_text = Self::element_text(*date_column);
        let title = Self::normalize_whitespace(&Self::element_text(*title_column));
        if date_text.is_empty() || title.is_empty() {
            return Ok(None);
        }

        let (starts_at, ends_at) = Self::parse_event_dates(&date_text, year, month)?;
        let url = title_column
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|link| link.value().attr("href"))
            .and_then(|href| page_url.join(href).ok())
            .map(|url| url.to_string())
            .or_else(|| Some(page_url.to_string()));

        Ok(Some(SsufidCalendar {
            id: Self::build_event_id(&title, starts_at, ends_at),
            title,
            description: None,
            starts_at,
            ends_at,
            location: None,
            url,
        }))
    }

    fn parse_event_dates(
        raw_date_text: &str,
        year: i32,
        fallback_month: u8,
    ) -> Result<(OffsetDateTime, Option<OffsetDateTime>), PluginError> {
        let normalized = Self::normalize_date_text(raw_date_text);
        let mut parts = normalized
            .split('~')
            .map(str::trim)
            .filter(|part| !part.is_empty());
        let start_token = parts.next().ok_or_else(|| {
            PluginError::parse::<Self>(format!("Missing start date in '{raw_date_text}'"))
        })?;
        let start_partial = Self::parse_partial_date(start_token)?;
        let start_month = start_partial.month.unwrap_or(fallback_month);
        let start_date = Self::date_from_parts(year, start_month, start_partial.day)?;
        let starts_at = PrimitiveDateTime::new(start_date, Time::MIDNIGHT).assume_offset(KST);

        let Some(end_token) = parts.next() else {
            let ends_at = PrimitiveDateTime::new(start_date, Self::end_of_day()).assume_offset(KST);
            return Ok((starts_at, Some(ends_at)));
        };

        if parts.next().is_some() {
            return Err(PluginError::parse::<Self>(format!(
                "Too many range separators in '{raw_date_text}'"
            )));
        }

        let end_partial = Self::parse_partial_date(end_token)?;
        let (end_year, end_month) = match end_partial.month {
            Some(end_month) => {
                let end_year = if end_month < start_month {
                    year + 1
                } else {
                    year
                };
                (end_year, end_month)
            }
            None => {
                if end_partial.day < start_partial.day {
                    Self::next_month(year, start_month)
                } else {
                    (year, start_month)
                }
            }
        };
        let end_date = Self::date_from_parts(end_year, end_month, end_partial.day)?;
        let ends_at = PrimitiveDateTime::new(end_date, Self::end_of_day()).assume_offset(KST);

        Ok((starts_at, Some(ends_at)))
    }

    fn parse_partial_date(token: &str) -> Result<PartialDate, PluginError> {
        let token = token.trim().trim_end_matches('.');
        if let Some((month, day)) = token.split_once('.') {
            return Ok(PartialDate {
                month: Some(month.trim().parse::<u8>().map_err(|e| {
                    PluginError::parse::<Self>(format!(
                        "Failed to parse month token '{token}': {e}"
                    ))
                })?),
                day: day.trim().parse::<u8>().map_err(|e| {
                    PluginError::parse::<Self>(format!("Failed to parse day token '{token}': {e}"))
                })?,
            });
        }

        Ok(PartialDate {
            month: None,
            day: token.parse::<u8>().map_err(|e| {
                PluginError::parse::<Self>(format!("Failed to parse day token '{token}': {e}"))
            })?,
        })
    }

    fn date_from_parts(year: i32, month: u8, day: u8) -> Result<Date, PluginError> {
        Date::from_calendar_date(
            year,
            Month::try_from(month).map_err(|_| {
                PluginError::parse::<Self>(format!("Invalid month value '{month}'"))
            })?,
            day,
        )
        .map_err(|e| {
            PluginError::parse::<Self>(format!(
                "Invalid calendar date year={year} month={month} day={day}: {e}"
            ))
        })
    }

    fn next_month(year: i32, month: u8) -> (i32, u8) {
        if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        }
    }

    fn end_of_day() -> Time {
        Time::from_hms(23, 59, 59).expect("valid end of day time")
    }

    fn normalize_date_text(text: &str) -> String {
        Self::normalize_whitespace(
            &Self::strip_parenthesized(text)
                .replace(['–', '—', '－'], "~")
                .replace('〜', "~"),
        )
    }

    fn strip_parenthesized(text: &str) -> String {
        let mut depth: u32 = 0;
        let mut result = String::new();

        for ch in text.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ if depth == 0 => result.push(ch),
                _ => {}
            }
        }

        result
    }

    fn element_text(element: ElementRef<'_>) -> String {
        Self::normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
    }

    fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn build_event_id(
        title: &str,
        starts_at: OffsetDateTime,
        ends_at: Option<OffsetDateTime>,
    ) -> String {
        let end = ends_at
            .map(|date| date.date().to_string())
            .unwrap_or_else(|| "_".to_string());
        format!(
            "{}:{}:{}:{}",
            Self::IDENTIFIER,
            starts_at.date(),
            end,
            Self::slugify(title)
        )
    }

    fn slugify(text: &str) -> String {
        let mut slug = String::new();
        let mut last_was_sep = false;

        for ch in Self::normalize_whitespace(text).chars() {
            if ch.is_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                last_was_sep = false;
            } else if !last_was_sep {
                slug.push('-');
                last_was_sep = true;
            }
        }

        let trimmed = slug.trim_matches('-');
        if trimmed.is_empty() {
            "event".to_string()
        } else {
            trimmed.to_string()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PartialDate {
    month: Option<u8>,
    day: u8,
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    const SAMPLE_HTML: &str = r#"
    <div id="calendar">
        <h4>2026년</h4>
        <div id="calendar202601" class="row align-items-start">
            <div class="col-12 col-md-4 col-xl-2 m-hide">
                <div class="grid text-center pt-4">
                    <span class="d-inline-block mr-1">2026</span>
                    <span class="h1 font-weight-bold font-size-50">01 </span>
                </div>
            </div>
            <div class="col-12 col-md-8 col-xl-10">
                <div class="grid">
                    <ul class="tb">
                        <li>
                            <div class="row">
                                <div class="col-12 col-lg-4 col-xl-3 font-weight-normal text-primary">
                                    01.05 (월) ~ 01.28 (수)
                                </div>
                                <div class="col-12 col-lg-8 col-xl-9">
                                    2026학년도 1학기 휴학/복학 신청기간(1차)
                                </div>
                            </div>
                        </li>
                        <li>
                            <div class="row">
                                <div class="col-12 col-lg-4 col-xl-3 font-weight-normal text-primary">
                                    01.08 (목)
                                </div>
                                <div class="col-12 col-lg-8 col-xl-9">
                                    <a href="/academic-event">2025학년도 2학기 성적증명서 발급개시</a>
                                </div>
                            </div>
                        </li>
                    </ul>
                </div>
            </div>
        </div>
        <div id="calendar202612" class="row align-items-start">
            <div class="col-12 col-md-4 col-xl-2 m-hide">
                <div class="grid text-center pt-4">
                    <span class="d-inline-block mr-1">2026</span>
                    <span class="h1 font-weight-bold font-size-50">12 </span>
                </div>
            </div>
            <div class="col-12 col-md-8 col-xl-10">
                <div class="grid">
                    <ul class="tb">
                        <li>
                            <div class="row">
                                <div class="col-12 col-lg-4 col-xl-3 font-weight-normal text-primary">
                                    12.28 (월) ~ 01.03 (일)
                                </div>
                                <div class="col-12 col-lg-8 col-xl-9">
                                    겨울방학
                                </div>
                            </div>
                        </li>
                    </ul>
                </div>
            </div>
        </div>
    </div>
    "#;

    #[test]
    fn test_parse_single_day_event() {
        let (starts_at, ends_at) =
            SsuAcademicCalendarPlugin::parse_event_dates("01.08 (목)", 2026, 1).unwrap();

        assert_eq!(starts_at, datetime!(2026-01-08 00:00:00 +09:00));
        assert_eq!(ends_at, Some(datetime!(2026-01-08 23:59:59 +09:00)));
    }

    #[test]
    fn test_parse_range_event() {
        let (starts_at, ends_at) =
            SsuAcademicCalendarPlugin::parse_event_dates("02.23 (월) ~ 02.27 (금)", 2026, 2)
                .unwrap();

        assert_eq!(starts_at, datetime!(2026-02-23 00:00:00 +09:00));
        assert_eq!(ends_at, Some(datetime!(2026-02-27 23:59:59 +09:00)));
    }

    #[test]
    fn test_parse_cross_year_range_event() {
        let (starts_at, ends_at) =
            SsuAcademicCalendarPlugin::parse_event_dates("12.28 (월) ~ 01.03 (일)", 2026, 12)
                .unwrap();

        assert_eq!(starts_at, datetime!(2026-12-28 00:00:00 +09:00));
        assert_eq!(ends_at, Some(datetime!(2027-01-03 23:59:59 +09:00)));
    }

    #[test]
    fn test_build_event_id_is_stable_for_whitespace() {
        let starts_at = datetime!(2026-03-03 00:00:00 +09:00);
        let left =
            SsuAcademicCalendarPlugin::build_event_id("2026학년도   1학기 개강", starts_at, None);
        let right =
            SsuAcademicCalendarPlugin::build_event_id("2026학년도 1학기 개강", starts_at, None);

        assert_eq!(left, right);
    }

    #[test]
    fn test_parse_year_page() {
        let page_url = SsuAcademicCalendarPlugin::year_page_url(2026).unwrap();
        let items =
            SsuAcademicCalendarPlugin::parse_year_page(SAMPLE_HTML, 2026, &page_url).unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "2026학년도 1학기 휴학/복학 신청기간(1차)");
        assert_eq!(items[0].starts_at, datetime!(2026-01-05 00:00:00 +09:00));
        assert_eq!(
            items[0].ends_at,
            Some(datetime!(2026-01-28 23:59:59 +09:00))
        );
        assert_eq!(
            items[1].ends_at,
            Some(datetime!(2026-01-08 23:59:59 +09:00))
        );
        assert_eq!(
            items[1].url.as_deref(),
            Some("https://ssu.ac.kr/academic-event")
        );
        assert_eq!(items[2].starts_at, datetime!(2026-12-28 00:00:00 +09:00));
        assert_eq!(
            items[2].ends_at,
            Some(datetime!(2027-01-03 23:59:59 +09:00))
        );
    }

    #[test]
    fn test_target_years_with_limit_zero() {
        let now = datetime!(2026-03-23 12:00:00 +09:00);
        assert_eq!(
            SsuAcademicCalendarPlugin::target_years(0, now),
            vec![2025, 2026, 2027]
        );
    }

    #[test]
    fn test_target_years_with_limit_crosses_previous_year() {
        let now = datetime!(2026-01-03 12:00:00 +09:00);
        assert_eq!(
            SsuAcademicCalendarPlugin::target_years(30, now),
            vec![2025, 2026]
        );
    }

    #[test]
    fn test_target_years_in_december_includes_next_year() {
        let now = datetime!(2026-12-15 12:00:00 +09:00);
        assert_eq!(
            SsuAcademicCalendarPlugin::target_years(30, now),
            vec![2026, 2027]
        );
    }

    #[tokio::test]
    #[ignore = "Requires network access to ssu.ac.kr"]
    async fn test_live_crawl() {
        let plugin = SsuAcademicCalendarPlugin;
        let items = plugin.crawl(30).await.unwrap();

        assert!(!items.is_empty());
        assert!(items.iter().all(|item| !item.id.is_empty()));
        assert!(items.iter().all(|item| !item.title.is_empty()));
    }
}
