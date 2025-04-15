use std::{collections::BTreeMap, sync::LazyLock};

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    PluginError,
    plugins::ssu_path::{
        SsuPathPlugin, SsuPathPluginError,
        utils::{ElementRefExt, OptionExt, ParseDateRange as _, serialize_date_range},
    },
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

pub struct SsuPathDivisionTable {
    pub headers: Vec<String>,
    pub rows: Vec<SsuPathDivisionTableRow>,
}

impl SsuPathDivisionTable {
    pub fn from_document(document: &Html) -> Result<Self, SsuPathPluginError> {
        let mut table_childs = document
            .select(&DIVISION_TABLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot find division table of content".to_string(),
            )))?
            .child_elements()
            .skip(1);
        let mut headers = table_childs
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse title of content".to_string(),
            )))?
            .child_elements()
            .map(|e| e.to_string(""))
            .collect::<Vec<_>>();
        headers.pop();
        let headers = headers;
        let rows = table_childs
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse title of content".to_string(),
            )))?
            .child_elements()
            .map(SsuPathDivisionTableRow::from_elem)
            .collect::<Result<Vec<_>, SsuPathPluginError>>()?;
        Ok(Self { headers, rows })
    }
}

#[derive(Serialize)]
pub struct SsuPathDivisionTableRow {
    #[serde(rename = "번호")]
    pub order: u32,
    #[serde(rename = "분반명")]
    pub name: String,
    #[serde(rename = "신청기간", serialize_with = "serialize_date_range")]
    pub apply_duration: (OffsetDateTime, OffsetDateTime),
    #[serde(rename = "모집정원")]
    pub total: u32,
    #[serde(rename = "대기정원")]
    pub awaiter: u32,
    #[serde(rename = "신청인원")]
    pub applier: u32,
    #[serde(rename = "대기신청인원")]
    pub await_applier: u32,
}

impl SsuPathDivisionTableRow {
    pub fn from_elem(elem: ElementRef) -> Result<Self, SsuPathPluginError> {
        let columns = elem
            .child_elements()
            .map(|e| e.to_string(""))
            .collect::<Vec<_>>();
        if columns.len() != 8 {
            return Err(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse division table row".to_string(),
            )));
        }
        let mut columns = columns.into_iter();
        let order = columns
            .next()
            .ok_and_parse_u32("Cannot parse order".to_string())?;
        let name = columns
            .next()
            .ok_or_parse_err("Cannot parse name".to_string())?;
        let apply_duration = columns
            .next()
            .ok_or_parse_err("Cannot parse apply duration".to_string())?
            .parse_date_range()?;
        let total = columns
            .next()
            .ok_and_parse_u32("Cannot parse total".to_string())?;
        let awaiter = columns
            .next()
            .ok_and_parse_u32("Cannot parse awaiter".to_string())?;
        let applier = columns
            .next()
            .ok_and_parse_u32("Cannot parse applier".to_string())?;
        let await_applier = columns
            .next()
            .ok_and_parse_u32("Cannot parse await_applier".to_string())?;
        Ok(Self {
            order,
            name,
            apply_duration,
            total,
            awaiter,
            applier,
            await_applier,
        })
    }
}
