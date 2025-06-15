use std::str::FromStr;

use reqwest::header::{
    HeaderMap, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION,
};
use scraper::ElementRef;
use serde::ser::Error as _;
use serde::Serializer;
use time::{
    format_description::well_known::Rfc3339,
    macros::{format_description, offset},
    OffsetDateTime, PrimitiveDateTime,
};

use crate::PluginError;

use super::{SsuPathPlugin, SsuPathPluginError};

pub(super) const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36";

pub(super) fn default_header() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8"
            .parse()
            .unwrap(),
    );
    headers.insert(ACCEPT_ENCODING, "deflate, br".parse().unwrap());
    headers.insert(ACCEPT_LANGUAGE, "ko,en;q=0.9,en-US;q=0.8".parse().unwrap());
    headers.insert(CACHE_CONTROL, "max-age=0".parse().unwrap());
    headers.insert(CONNECTION, "keep-alive".parse().unwrap());
    headers
}

pub(super) trait ElementRefExt {
    fn to_string(&self, delimiter: &str) -> String;
}

impl ElementRefExt for ElementRef<'_> {
    fn to_string(&self, delimiter: &str) -> String {
        self.text()
            .collect::<Vec<_>>()
            .join(delimiter)
            .trim()
            .to_string()
    }
}

pub(super) trait OptionExt<T> {
    fn ok_or_parse_err(self, msg: String) -> Result<T, SsuPathPluginError>;

    fn ok_and_parse<F: FromStr>(self, msg: String) -> Result<F, SsuPathPluginError>;

    fn ok_and_parse_u32(self, msg: String) -> Result<u32, SsuPathPluginError>;
}

impl OptionExt<String> for Option<String> {
    fn ok_or_parse_err(self, msg: String) -> Result<String, SsuPathPluginError> {
        self.ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(msg)))
    }

    fn ok_and_parse<F: FromStr>(self, msg: String) -> Result<F, SsuPathPluginError> {
        self.ok_or_else(|| SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(msg.clone())))?
            .parse::<F>()
            .map_err(|_| SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(msg)))
    }

    fn ok_and_parse_u32(self, msg: String) -> Result<u32, SsuPathPluginError> {
        self.map(|str| str.replace(",", ""))
            .ok_and_parse::<u32>(msg)
    }
}

impl OptionExt<String> for Option<ElementRef<'_>> {
    fn ok_or_parse_err(self, msg: String) -> Result<String, SsuPathPluginError> {
        Ok(self
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(msg)))?
            .to_string(""))
    }

    fn ok_and_parse<F: FromStr>(self, msg: String) -> Result<F, SsuPathPluginError> {
        self.ok_or_parse_err(msg.clone())?
            .parse::<F>()
            .map_err(|_| SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(msg)))
    }

    fn ok_and_parse_u32(self, msg: String) -> Result<u32, SsuPathPluginError> {
        self.ok_and_parse::<u32>(msg)
    }
}

const DATE_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year].[month].[day] [hour]:[minute]");
// Alterative format for old dates
const DATE_FORMAT_ALT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
const UTC_OFFSET: time::UtcOffset = offset!(+9);

pub(super) trait ParseDateRange {
    fn parse_date_range(&self) -> Result<(OffsetDateTime, OffsetDateTime), SsuPathPluginError>;
}

impl ParseDateRange for String {
    fn parse_date_range(&self) -> Result<(OffsetDateTime, OffsetDateTime), SsuPathPluginError> {
        let mut apply_durations = self.split("~").map(|s| {
            PrimitiveDateTime::parse(s.trim(), DATE_FORMAT)
                .or_else(|_| PrimitiveDateTime::parse(s.trim(), DATE_FORMAT_ALT))
                .map(|dt| dt.assume_offset(UTC_OFFSET))
                .map_err(|e| {
                    SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                        format!("Cannot parse date: {e}").to_string(),
                    ))
                })
        });
        let apply_duration =
            apply_durations
                .next()
                .and_then(|d: Result<OffsetDateTime, SsuPathPluginError>| {
                    apply_durations.next().map(|e| Ok((d?, e?)))
                });
        apply_duration.ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
            "Cannot parse apply duration of entry".to_string(),
        )))?
    }
}

pub fn serialize_date_range<S>(
    date_range: &(OffsetDateTime, OffsetDateTime),
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let date_range = [
        date_range.0.format(&Rfc3339).map_err(S::Error::custom)?,
        date_range.1.format(&Rfc3339).map_err(S::Error::custom)?,
    ];
    serializer.collect_seq(&date_range)
}
