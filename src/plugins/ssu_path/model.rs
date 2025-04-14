use std::{collections::BTreeMap, sync::LazyLock};

use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use serde_yaml::Mapping;
use time::{
    OffsetDateTime, PrimitiveDateTime,
    macros::{format_description, offset},
};

use crate::{
    PluginError,
    plugins::ssu_path::{SsuPathPlugin, utils::ElementRefExt as _},
};

use super::SsuPathPluginError;

pub struct SsuPathEntry {
    pub id: String,
    pub thumbnail: String,
    pub title: String,
    pub description: String,
    pub label: String,
    pub major_types: Vec<String>,
    pub apply_duration: (OffsetDateTime, OffsetDateTime),
    pub course_duration: (OffsetDateTime, OffsetDateTime),
    pub target: String,
    pub user_type: String,
    pub competencies: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryParams {
    enc_sddpb_seq: String,
}

static THUMBNAIL_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".img_wrap img").unwrap());
static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".tit[data-params]").unwrap());
static DESC_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("p.desc").unwrap());
static LABEL_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".label_box").unwrap());
static MAJOR_TYPE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".major_type > li").unwrap());
static APPLY_DURATION_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".info_wrap > dl:first-child > dd").unwrap());
static COURSE_DURATION_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".info_wrap > dl:nth-child(2) > dd").unwrap());
static TARGET_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".info_wrap > dl:nth-child(3) > dd").unwrap());
static USER_TYPE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".info_wrap > dl:nth-child(4) > dd").unwrap());
static COMPETENCIES_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("li.cabil dd > span").unwrap());
const DATE_FORMAT: &[::time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year].[month].[day] [hour]:[minute]");
const UTC_OFFSET: time::UtcOffset = offset!(+9);

impl SsuPathEntry {
    pub fn from_element(element: scraper::ElementRef) -> Result<Self, SsuPathPluginError> {
        dbg!(element.text());
        let title_elem = element
            .select(&TITLE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse title of entry".to_string(),
            )))?;
        let id = serde_json::from_str::<EntryParams>(title_elem.attr("data-params").ok_or(
            SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse id of entry".to_string(),
            )),
        )?)?
        .enc_sddpb_seq;
        let title = title_elem.to_string("");
        let thumbnail = element
            .select(&THUMBNAIL_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse thumbnail of entry".to_string(),
            )))?
            .attr("src")
            .unwrap_or_default()
            .to_string();
        let description = element
            .select(&DESC_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse description of entry".to_string(),
            )))?
            .to_string("");
        let label = element
            .select(&LABEL_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse label of entry".to_string(),
            )))?
            .to_string("");
        let major_types = element
            .select(&MAJOR_TYPE_SELECTOR)
            .map(|e| e.to_string(""))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let apply_duration_str = element
            .select(&APPLY_DURATION_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse apply duration of entry".to_string(),
            )))?
            .to_string("");
        let apply_duration = Self::as_date_tuple(&apply_duration_str)?;
        let course_duration_str = element
            .select(&COURSE_DURATION_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse course duration of entry".to_string(),
            )))?
            .to_string("");
        let course_duration = Self::as_date_tuple(&course_duration_str)?;
        let target = element
            .select(&TARGET_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse target of entry".to_string(),
            )))?
            .to_string("");
        let user_type = element
            .select(&USER_TYPE_SELECTOR)
            .next()
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot parse user type of entry".to_string(),
            )))?
            .to_string("");
        let competencies = element
            .select(&COMPETENCIES_SELECTOR)
            .map(|e| e.to_string(""))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        Ok(Self {
            id,
            thumbnail,
            title,
            description,
            label,
            major_types,
            apply_duration,
            course_duration,
            target,
            user_type,
            competencies,
        })
    }

    fn as_date_tuple(str: &str) -> Result<(OffsetDateTime, OffsetDateTime), SsuPathPluginError> {
        let mut apply_durations = str.split("~").map(|s| {
            dbg!(&s);
            PrimitiveDateTime::parse(s, DATE_FORMAT)
                .unwrap()
                .assume_offset(UTC_OFFSET)
        });
        let apply_duration = apply_durations
            .next()
            .and_then(|d| apply_durations.next().map(|e| (d, e)));
        apply_duration.ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
            "Cannot parse apply duration of entry".to_string(),
        )))
    }
}

type WeekName = String;

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
            .ok_or(SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(
                "Cannot find first column".to_string(),
            )))?
            .to_string("");
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

pub fn construct_content(
    program_table: &SsuPathProgramTable,
    course_table: &SsuPathCourseTable,
) -> String {
    let mut frontmatters = String::new();
    frontmatters.push_str(&serde_yaml::to_string(&program_table.info).unwrap());
    frontmatters.push('\n');
    frontmatters.push_str(&serde_yaml::to_string(&course_table.overview).unwrap());
    for (week_name, week_table) in &course_table.weeks {
        let val = serde_yaml::Value::Mapping(Mapping::from_iter([(
            serde_yaml::to_value(week_name).unwrap(),
            serde_yaml::to_value(week_table).unwrap(),
        )]));
        frontmatters.push_str(&serde_yaml::to_string(&val).unwrap());
    }
    let mut content = String::new();
    content.push_str(&format!(
        "---\n{}\n---\n{}",
        frontmatters, program_table.content
    ));
    content
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
            let value = ve.to_string("");
            (key, value)
        });
    Ok(BTreeMap::from_iter(entry_iter))
}
