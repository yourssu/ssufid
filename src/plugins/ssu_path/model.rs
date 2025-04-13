use std::{collections::BTreeMap, sync::LazyLock};

use scraper::{Html, Selector};
use time::OffsetDateTime;

use crate::{PluginError, plugins::ssu_path::SsuPathPlugin};

use super::SsuPathPluginError;

pub struct SsuPathEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    pub compentencies: Vec<String>,
    pub apply_duration: (OffsetDateTime, OffsetDateTime),
    pub course_duration: (OffsetDateTime, OffsetDateTime),
}

impl SsuPathEntry {
    pub fn from_element(element: scraper::ElementRef) -> Result<Self, SsuPathPluginError> {
        todo!();
    }
}

static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#tilesContent > div.table_top:nth-child(2) > h4").unwrap());

static PROGRAM_TABLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#tilesContent > div.table_top:nth-child(2) + .table_wrap > table").unwrap()
});

type WeekName = String;

pub struct SsuPathProgramTable {
    pub title: String,
    pub content: String,
    pub info: BTreeMap<String, String>,
}

impl SsuPathProgramTable {
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        let title = document
            .select(&TITLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse title of content".to_string(),
            )))?
            .text()
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        let table = document
            .select(&PROGRAM_TABLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot find program table of content".to_string(),
            )))?;
        // TODO: parse the table into a BTreeMap<String, String>. and extract the content and info.

        todo!();
    }
}

static COURSE_TABLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#tilesContent > div.table_top:nth-child(4) + .table_wrap > table").unwrap()
});

static WEEK_TABLES_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        "#tilesContent > div.table_top:nth-child(4) + .table_wrap ~ .table_wrap > table",
    )
    .unwrap()
});

pub struct SsuPathCourseTable {
    pub overview: BTreeMap<String, String>,
    pub weeks: Vec<(WeekName, BTreeMap<String, String>)>,
}

impl SsuPathCourseTable {
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        todo!();
    }
}

pub fn construct_content(
    program_table: &SsuPathProgramTable,
    course_table: &SsuPathCourseTable,
) -> String {
    let frontmatters = "";
    let mut content = String::new();
    content.push_str(&format!(
        "---\n{}\n---\n{}",
        frontmatters, program_table.content
    ));
    content
}
