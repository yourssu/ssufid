pub mod table;

use std::{collections::BTreeMap, sync::LazyLock};

use scraper::{ElementRef, Selector};
use serde::Deserialize;
use serde_yaml::Mapping;
use table::{SsuPathCourseTable, SsuPathProgramTable};
use time::OffsetDateTime;

use ssufid::PluginError;

use crate::{SsuPathPlugin, utils::ElementRefExt as _};

use super::{
    SsuPathPluginError,
    utils::{OptionExt, ParseDateRange},
};

pub struct SsuPathProgramDivision {
    pub title: String,
    pub apply_duration: (OffsetDateTime, OffsetDateTime),
    pub course_duration: (OffsetDateTime, OffsetDateTime),
    pub applier: u32,
    pub awaiter: u32,
    pub total: u32,
    pub location: String,
}

pub enum SsuPathProgramKind {
    Single {
        apply_duration: (OffsetDateTime, OffsetDateTime),
        course_duration: (OffsetDateTime, OffsetDateTime),
        miles: u32,
        applier: u32,
        awaiter: u32,
        total: u32,
    },
    Division(Vec<SsuPathProgramDivision>),
}

pub struct SsuPathProgram {
    pub id: String,
    pub thumbnail: String,
    pub title: String,
    pub description: String,
    pub label: String,
    pub major_types: Vec<String>,
    pub target: String,
    pub user_type: String,
    pub competencies: Vec<String>,
    pub kind: SsuPathProgramKind,
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
static INFOS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".info_wrap dl").unwrap());
static DESC_INFOS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".etc_cont dl").unwrap());
static COMPETENCIES_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("li.cabil dd > span").unwrap());
static CLASSES_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".class_list > .class_cont").unwrap());
static CLASSES_TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".tit").unwrap());
static CLASSES_DESCS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("dl").unwrap());

impl SsuPathProgram {
    pub fn from_element(element: scraper::ElementRef) -> Result<Self, SsuPathPluginError> {
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
            .ok_or_parse_err("Cannot parse description of entry".to_string())?;
        let label = element
            .select(&LABEL_SELECTOR)
            .next()
            .ok_or_parse_err("Cannot parse label of entry".to_string())?;
        let major_types = element
            .select(&MAJOR_TYPE_SELECTOR)
            .map(|e| e.to_string(""))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let info_map = element
            .select(&INFOS_SELECTOR)
            .filter_map(Self::dl_to_pair)
            .collect::<BTreeMap<String, String>>();
        let target = info_map
            .get("신청대상")
            .cloned()
            .ok_or_parse_err("Cannot parse target of entry".to_string())?;
        let user_type = info_map
            .get("신청신분")
            .cloned()
            .ok_or_parse_err("Cannot parse user type of entry".to_string())?;
        let competencies = element
            .select(&COMPETENCIES_SELECTOR)
            .map(|e| e.to_string(""))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let mut classes = element.select(&CLASSES_SELECTOR).peekable();
        if classes.peek().is_none() {
            let apply_duration = info_map
                .get("신청기간")
                .cloned()
                .ok_or_parse_err("Cannot parse apply duration of entry".to_string())?
                .parse_date_range()?;
            let course_duration = info_map
                .get("교육기간")
                .cloned()
                .ok_or_parse_err("Cannot parse course duration of entry".to_string())?
                .parse_date_range()?;
            let desc_info_map = element
                .select(&DESC_INFOS_SELECTOR)
                .filter_map(Self::dl_to_pair)
                .collect::<BTreeMap<String, String>>();
            let miles = desc_info_map
                .get("마일리지")
                .cloned()
                .ok_and_parse_u32("Cannot parse miles of entry".to_string())
                .inspect_err(|e| {
                    log::warn!("Failed to parse miles of entry: {e:?}");
                })
                .unwrap_or(0);
            let applier = desc_info_map
                .get("신청자")
                .cloned()
                .ok_and_parse_u32("Cannot parse applier of entry".to_string())?;
            let awaiter = desc_info_map
                .get("대기자")
                .cloned()
                .ok_and_parse_u32("Cannot parse awaiter of entry".to_string())?;
            let total = desc_info_map
                .get("모집정원")
                .cloned()
                .ok_and_parse_u32("Cannot parse total of entry".to_string())?;
            Ok(Self {
                id,
                thumbnail,
                title,
                description,
                label,
                major_types,
                target,
                user_type,
                competencies,
                kind: SsuPathProgramKind::Single {
                    apply_duration,
                    course_duration,
                    miles,
                    applier,
                    awaiter,
                    total,
                },
            })
        } else {
            let classes = classes
                .map(Self::parse_division)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Self {
                id,
                thumbnail,
                title,
                description,
                label,
                major_types,
                target,
                user_type,
                competencies,
                kind: SsuPathProgramKind::Division(classes),
            })
        }
    }

    fn parse_division(elem: ElementRef) -> Result<SsuPathProgramDivision, SsuPathPluginError> {
        let title = elem
            .select(&CLASSES_TITLE_SELECTOR)
            .next()
            .ok_or_parse_err("Cannot parse title of entry".to_string())?;
        let desc_map = elem
            .select(&CLASSES_DESCS_SELECTOR)
            .filter_map(Self::dl_to_pair)
            .collect::<BTreeMap<String, String>>();
        let apply_duration = desc_map
            .get("신청기간")
            .cloned()
            .ok_or_parse_err("Cannot parse apply duration of entry".to_string())?
            .parse_date_range()?;
        let course_duration = desc_map
            .get("운영기간")
            .cloned()
            .ok_or_parse_err("Cannot parse course duration of entry".to_string())?
            .parse_date_range()?;
        let applier = desc_map
            .get("신청자")
            .cloned()
            .ok_and_parse_u32("Cannot parse applier of entry".to_string())?;
        let awaiter = desc_map
            .get("대기자")
            .cloned()
            .ok_and_parse_u32("Cannot parse awaiter of entry".to_string())?;
        let total = desc_map
            .get("모집정원")
            .cloned()
            .ok_and_parse_u32("Cannot parse total of entry".to_string())?;
        let location = desc_map
            .get("교육장소")
            .cloned()
            .ok_or_parse_err("Cannot parse location of entry".to_string())?;
        Ok(SsuPathProgramDivision {
            title,
            apply_duration,
            course_duration,
            applier,
            awaiter,
            total,
            location,
        })
    }

    fn dl_to_pair(elem: ElementRef) -> Option<(String, String)> {
        let mut iter = elem.child_elements();
        let key = iter.next().map(|elem| elem.to_string(""))?;
        let value = iter.next().map(|elem| elem.to_string(""))?;
        Some((key, value))
    }

    pub(super) fn create_at(&self) -> OffsetDateTime {
        match &self.kind {
            SsuPathProgramKind::Single { apply_duration, .. } => apply_duration.0,
            SsuPathProgramKind::Division(divisions) => {
                divisions.first().map(|d| d.apply_duration.0).unwrap()
            }
        }
    }
}

pub fn construct_content(
    program_table: &SsuPathProgramTable,
    course_table: &Option<SsuPathCourseTable>,
    division_table: &Option<table::SsuPathDivisionTable>,
) -> String {
    let mut frontmatters = String::new();
    frontmatters.push_str(&serde_yaml::to_string(&program_table.info).unwrap());
    frontmatters.push('\n');
    if let Some(course_table) = course_table {
        frontmatters.push_str(&serde_yaml::to_string(&course_table.overview).unwrap());
        for (week_name, week_table) in &course_table.weeks {
            let val = serde_yaml::Value::Mapping(Mapping::from_iter([(
                serde_yaml::to_value(week_name).unwrap(),
                serde_yaml::to_value(week_table).unwrap(),
            )]));
            frontmatters.push_str(&serde_yaml::to_string(&val).unwrap());
        }
    }
    if let Some(division_table) = division_table {
        let val = serde_yaml::Value::Mapping(Mapping::from_iter([(
            serde_yaml::to_value("분반").unwrap(),
            serde_yaml::to_value(&division_table.rows).unwrap(),
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
