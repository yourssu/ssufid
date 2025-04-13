use std::sync::{Arc, LazyLock};

use futures::{TryStreamExt, stream::FuturesUnordered};
use model::{SsuPathCourseTable, SsuPathEntry, SsuPathProgramTable, construct_content};
use scraper::{Html, Selector};
use sso::SsuSsoError;

use crate::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub mod model;
pub mod sso;
mod utils;

pub enum SsuPathCredential {
    Token(String),
    Password(String, String),
}

pub struct SsuPathPlugin {
    credential: SsuPathCredential,
}

#[derive(Debug)]
pub struct SsuPathPluginError(PluginError);

impl From<SsuPathPluginError> for PluginError {
    fn from(err: SsuPathPluginError) -> Self {
        err.0
    }
}

impl From<reqwest::Error> for SsuPathPluginError {
    fn from(err: reqwest::Error) -> Self {
        SsuPathPluginError(PluginError::request::<SsuPathPlugin>(format!(
            "Request error: {}",
            err
        )))
    }
}

impl From<serde_json::Error> for SsuPathPluginError {
    fn from(value: serde_json::Error) -> Self {
        SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(format!(
            "Parse error: {}",
            value
        )))
    }
}

impl From<SsuSsoError> for SsuPathPluginError {
    fn from(err: SsuSsoError) -> Self {
        SsuPathPluginError(PluginError::request::<SsuPathPlugin>(format!(
            "SSU SSO error: {}",
            err
        )))
    }
}

impl SsuPathPlugin {
    pub fn new(credential: SsuPathCredential) -> Self {
        SsuPathPlugin { credential }
    }

    async fn client(&self) -> Result<reqwest::Client, SsuPathPluginError> {
        Ok(match &self.credential {
            SsuPathCredential::Token(token) => self.client_with_token(token).await,
            SsuPathCredential::Password(id, password) => {
                self.client_with_password(id, password).await
            }
        }?)
    }

    async fn client_with_password(
        &self,
        id: &str,
        password: &str,
    ) -> Result<reqwest::Client, SsuSsoError> {
        let token = sso::obtain_ssu_sso_token(id, password).await?;
        self.client_with_token(&token).await
    }

    async fn client_with_token(&self, token: &str) -> Result<reqwest::Client, SsuSsoError> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .cookie_store(true)
            .user_agent(utils::DEFAULT_USER_AGENT)
            .build()?;
        client.get(format!("https://path.ssu.ac.kr/comm/login/user/loginProc.do?rtnUrl=/index.do?paramStart=paramStart?sToken={token}")).send().await?;
        Ok(client)
    }
}

const ENTRIES_PER_PAGE: usize = 10;

impl SsufidPlugin for SsuPathPlugin {
    const IDENTIFIER: &'static str = "path.ssu.ac.kr";
    const TITLE: &'static str = "숭실대학교 SSU-PATH";
    const DESCRIPTION: &'static str =
        "숭실대학교 비교과 시스템 SSU-PATH의 비교과 프로그램 정보를 제공합니다.";
    const BASE_URL: &'static str =
        "https://path.ssu.ac.kr/ptfol/imng/icmpNsbjtPgm/findIcmpNsbjtPgmList.do";

    async fn crawl(&self, posts_limit: u32) -> Result<Vec<SsufidPost>, PluginError> {
        let pages = (posts_limit as usize).div_ceil(ENTRIES_PER_PAGE);
        let client = self.client().await?;
        let entries = (1..=pages)
            .map(|page| entries(&client, page))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .flatten()
            .take(posts_limit as usize)
            .collect::<Vec<_>>();
        Ok(entries
            .iter()
            .map(|entry| post(&client, entry))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<Vec<_>>()
            .await?)
    }
}

const PATH_LIST_URL: &str = "https://path.ssu.ac.kr/ptfol/imng/icmpNsbjtPgm/findIcmpNsbjtPgmList.do?paginationInfo.currentPageNo=";

static ENTRIES_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div.lica_wrap > ul > li").unwrap());

async fn entries(
    client: &reqwest::Client,
    page: usize,
) -> Result<Vec<SsuPathEntry>, SsuPathPluginError> {
    let url = format!("{PATH_LIST_URL}{page}");
    let response = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&response);
    document
        .select(&ENTRIES_SELECTOR)
        .map(SsuPathEntry::from_element)
        .collect::<Result<Vec<SsuPathEntry>, SsuPathPluginError>>()
}

const PATH_ENTRY_URL: &str =
    "https://path.ssu.ac.kr/ptfol/imng/icmpNsbjtPgm/findIcmpNsbjtPgmInfo.do?encSddpbSeq=";

async fn post(
    client: &reqwest::Client,
    entry: &SsuPathEntry,
) -> Result<SsufidPost, SsuPathPluginError> {
    let url = format!("{PATH_ENTRY_URL}{}", entry.id);
    let response = client.get(&url).send().await?.text().await?;
    let document = Html::parse_document(&response);
    let program_table = SsuPathProgramTable::from_document(&document)?;
    let course_table = SsuPathCourseTable::from_document(&document)?;
    let content = construct_content(&program_table, &course_table);
    Ok(SsufidPost {
        id: entry.id.clone(),
        title: program_table.title,
        category: entry.label.clone(),
        url,
        created_at: entry.apply_duration.0,
        content,
        updated_at: None,
    })
}
