use std::{collections::BTreeMap, sync::LazyLock};

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use ssufid::PluginError;

use crate::{
    SsuPathPlugin, SsuPathPluginError,
    utils::{ElementRefExt, OptionExt, ParseDateRange as _, serialize_date_range},
};
pub struct SsuPathProgramTable {
    pub title: String,
    pub content: String,
    pub info: BTreeMap<String, String>,
}

static PROGRAM_TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#tilesContent > div.table_top:nth-child(2) > h4").unwrap());

static PROGRAM_TABLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#tilesContent > div.table_top:nth-child(2) + .table_wrap > table > tbody")
        .unwrap()
});

impl SsuPathProgramTable {
    #[tracing::instrument(level=tracing::Level::DEBUG, name = "parse_program_table", skip(document))]
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        let title = document
            .select(&PROGRAM_TITLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse title of content".to_string(),
            )))?
            .to_string("");

        let table = document
            .select(&PROGRAM_TABLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot find program table of content".to_string(),
            )))?;
        let mut info = parse_table(table)?;
        let content = info.remove("프로그램 주요내용").unwrap().to_string();
        Ok(Self {
            title,
            content,
            info,
        })
    }
}

type WeekName = String;

pub struct SsuPathCourseTable {
    pub overview: BTreeMap<String, String>,
    pub weeks: Vec<(WeekName, BTreeMap<String, String>)>,
}

static COURSE_TABLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#tilesContent > div.table_top:nth-child(4) + .table_wrap > table > tbody")
        .unwrap()
});

static WEEK_TABLES_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        "#tilesContent > div.table_top:nth-child(4) + .table_wrap ~ .table_wrap > table > tbody",
    )
    .unwrap()
});

impl SsuPathCourseTable {
    #[tracing::instrument(level=tracing::Level::DEBUG, name = "parse_course_table", skip(document))]
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        let overview_elem =
            document
                .select(&COURSE_TABLE_SELECTOR)
                .next()
                .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                    "Cannot find course table of content".to_string(),
                )))?;
        let overview = parse_table(overview_elem)?;
        let weeks = document
            .select(&WEEK_TABLES_SELECTOR)
            .map(Self::parse_week_table)
            .collect::<Result<Vec<_>, SsuPathPluginError>>()?;
        Ok(Self { overview, weeks })
    }

    #[tracing::instrument(level=tracing::Level::DEBUG, name = "parse_week_table", skip(table))]
    fn parse_week_table(
        table: ElementRef,
    ) -> Result<(WeekName, BTreeMap<String, String>), SsuPathPluginError> {
        let week_row_elem =
            table
                .child_elements()
                .next()
                .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                    "Cannot find first row".to_string(),
                )))?;
        let week_name = week_row_elem
            .child_elements()
            .next()
            .ok_or_parse_err("Cannot parse week name".to_string())?;
        let entry_iter = table
            .child_elements()
            .flat_map(|tr| {
                if tr.attr("class").unwrap_or("") == "first" {
                    tr.child_elements()
                        .skip(1)
                        .step_by(2)
                        .zip(tr.child_elements().skip(2).step_by(2))
                        .collect::<Vec<(ElementRef, ElementRef)>>()
                } else {
                    tr.child_elements()
                        .step_by(2)
                        .zip(tr.child_elements().skip(1).step_by(2))
                        .collect::<Vec<(ElementRef, ElementRef)>>()
                }
            })
            .map(|(ke, ve)| {
                let key = ke.to_string("");
                let value = ve.to_string("");
                (key, value)
            });
        Ok((week_name, BTreeMap::from_iter(entry_iter)))
    }
}

fn parse_table(table: ElementRef) -> Result<BTreeMap<String, String>, SsuPathPluginError> {
    let entry_iter = table
        .child_elements()
        .flat_map(|tr| {
            tr.child_elements()
                .step_by(2)
                .zip(tr.child_elements().skip(1))
        })
        .map(|(ke, ve)| {
            let key = ke.to_string("");
            let value = ve.to_string("").replace("\t", "");
            (key, value)
        });
    Ok(BTreeMap::from_iter(entry_iter))
}

static DIVISION_TABLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("form[name='viewForm'] .table_wrap table").unwrap());

static DIVISION_TABLE_HEADER_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("thead > tr > th").unwrap());

static DIVISION_TABLE_ROWS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("tbody > tr").unwrap());

pub struct SsuPathDivisionTable {
    pub headers: Vec<String>,
    pub rows: Vec<SsuPathDivisionTableRow>,
}

impl SsuPathDivisionTable {
    #[tracing::instrument(level=tracing::Level::DEBUG, name = "parse_division_table", skip(document))]
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        let table = document
            .select(&DIVISION_TABLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot find division table of content".to_string(),
            )))?;
        let headers = table
            .select(&DIVISION_TABLE_HEADER_SELECTOR)
            .map(|e| e.to_string(""))
            .collect::<Vec<_>>();
        let rows = table
            .select(&DIVISION_TABLE_ROWS_SELECTOR)
            .map(|elem| SsuPathDivisionTableRow::from_elem(headers.clone(), elem))
            .collect::<Result<Vec<_>, SsuPathPluginError>>()?;
        Ok(Self { headers, rows })
    }
}

#[derive(Serialize, Deserialize)]
pub struct SsuPathDivisionTableRow {
    #[serde(rename = "번호", deserialize_with = "deserialize_string_to_u32")]
    pub order: u32,
    #[serde(rename = "분반명")]
    pub name: String,
    #[serde(
        rename = "신청기간",
        serialize_with = "serialize_date_range",
        deserialize_with = "deserialize_date_range"
    )]
    pub apply_duration: (OffsetDateTime, OffsetDateTime),
    #[serde(
        rename = "운영기간",
        serialize_with = "serialize_date_range",
        deserialize_with = "deserialize_date_range"
    )]
    pub operate_duration: (OffsetDateTime, OffsetDateTime),
    #[serde(rename = "모집정원", deserialize_with = "deserialize_string_to_u32")]
    pub total: u32,
    #[serde(rename = "대기정원", deserialize_with = "deserialize_string_to_u32")]
    pub awaiter: u32,
    #[serde(rename = "신청인원", deserialize_with = "deserialize_string_to_u32")]
    pub applier: u32,
    #[serde(
        rename = "대기신청인원",
        deserialize_with = "deserialize_string_to_u32"
    )]
    pub await_applier: u32,
}

fn deserialize_string_to_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.trim().parse::<u32>().map_err(serde::de::Error::custom)
}

fn deserialize_date_range<'de, D>(
    deserializer: D,
) -> Result<(OffsetDateTime, OffsetDateTime), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse_date_range()
        .map_err(|e| serde::de::Error::custom(format!("Cannot parse date range: {:?}", e)))
}

impl SsuPathDivisionTableRow {
    #[tracing::instrument(level=tracing::Level::DEBUG, name = "parse_division_table_row", skip(elem))]
    pub fn from_elem(headers: Vec<String>, elem: ElementRef) -> Result<Self, SsuPathPluginError> {
        tracing::debug!("Parsing division table row: {}", elem.inner_html());
        let columns = elem
            .child_elements()
            .map(|e| e.to_string(""))
            .collect::<Vec<_>>();

        if columns.len() != headers.len() {
            return Err(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                format!(
                    "Cannot parse division table row, incorrect number of columns: expected {}, got {}",
                    headers.len(),
                    columns.len()
                ),
            )));
        }

        let map: BTreeMap<String, String> = headers.into_iter().zip(columns.into_iter()).collect();

        // BTreeMap을 serde_json::Value로 변환 후 deserialize
        let value = serde_json::to_value(&map).map_err(|e| {
            SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(format!(
                "Cannot serialize map to value: {}",
                e
            )))
        })?;

        serde_json::from_value(value).map_err(|e| {
            SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(format!(
                "Cannot deserialize value to SsuPathDivisionTableRow: {}",
                e
            )))
        })
    }
}
