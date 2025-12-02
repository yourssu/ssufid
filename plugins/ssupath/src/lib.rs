use std::sync::{Arc, LazyLock};

use futures::{TryStreamExt, stream::FuturesUnordered};
use model::{
    SsuPathProgram, SsuPathProgramKind, construct_content, construct_frontmatters,
    table::{SsuPathCourseTable, SsuPathDivisionTable, SsuPathProgramTable},
};
use scraper::{Html, Selector};
use sso::SsuSsoError;
use url::Url;
use utils::default_header;

use ssufid::{
    PluginError,
    core::{SsufidPlugin, SsufidPost},
};

pub mod model;
pub mod sso;
mod utils;

pub enum SsuPathCredential {
    Token(String, String),
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
            "Request error: {err}"
        )))
    }
}

impl From<serde_json::Error> for SsuPathPluginError {
    fn from(value: serde_json::Error) -> Self {
        SsuPathPluginError(PluginError::parse::<SsuPathPlugin>(format!(
            "Parse error: {value}"
        )))
    }
}

impl From<SsuSsoError> for SsuPathPluginError {
    fn from(err: SsuSsoError) -> Self {
        SsuPathPluginError(PluginError::request::<SsuPathPlugin>(format!(
            "SSU SSO error: {err}"
        )))
    }
}

impl SsuPathPlugin {
    pub fn new(credential: SsuPathCredential) -> Self {
        SsuPathPlugin { credential }
    }

    async fn client(&self) -> Result<reqwest::Client, SsuPathPluginError> {
        Ok(match &self.credential {
            SsuPathCredential::Token(id, token) => self.client_with_token(id, token).await,
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
        self.client_with_token(id, &token).await
    }

    async fn client_with_token(
        &self,
        id: &str,
        token: &str,
    ) -> Result<reqwest::Client, SsuSsoError> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .cookie_provider(jar.clone())
            .user_agent(utils::DEFAULT_USER_AGENT)
            .default_headers(default_header())
            .build()?;
        let res = client.get("https://path.ssu.ac.kr/").send().await?;
        let Some((_, rtn_url)) = res.url().query_pairs().find(|(k, _)| k == "rtnUrl") else {
            return Err(SsuSsoError::CantLoadForm);
        };
        let res = client
            .get(format!(
                "https://path.ssu.ac.kr/comm/login/user/loginChk.do?rtnUrl={rtn_url}"
            ))
            .send()
            .await?;
        let Some((_, api_return_url)) = res.url().query_pairs().find(|(k, _)| k == "apiReturnUrl")
        else {
            return Err(SsuSsoError::CantLoadForm);
        };
        jar.add_cookie_str(
            &format!("sToken={token}; Domain=.ssu.ac.kr; Path=/; secure"),
            &"https://path.ssu.ac.kr".parse::<Url>().unwrap(),
        );
        tracing::info!("{api_return_url}?sToken={token}&sIdno={id}");
        let res = client
            .get(format!("{api_return_url}?sToken={token}&sIdno={id}"))
            .header("Referer", "https://smartid.ssu.ac.kr/")
            .send()
            .await?;
        if res.status() != reqwest::StatusCode::OK {
            return Err(SsuSsoError::CantFindToken(
                "Authorization failed".to_string(),
            ));
        }
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
        tracing::info!("Crawling with {} posts limit", posts_limit);
        let pages = (posts_limit as usize).div_ceil(ENTRIES_PER_PAGE);
        tracing::info!("Crawling {pages} pages");
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
) -> Result<Vec<SsuPathProgram>, SsuPathPluginError> {
    let url = format!("{PATH_LIST_URL}{page}");
    tracing::info!("Crawling entries from {url}");
    let response = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&response);
    document
        .select(&ENTRIES_SELECTOR)
        .map(SsuPathProgram::from_element)
        .collect::<Result<Vec<SsuPathProgram>, SsuPathPluginError>>()
}

const PATH_ENTRY_URL: &str =
    "https://path.ssu.ac.kr/ptfol/imng/icmpNsbjtPgm/findIcmpNsbjtPgmInfo.do?encSddpbSeq=";

#[tracing::instrument(level=tracing::Level::DEBUG, skip(client, program), fields(program_id = %program.id, program_title = %program.title))]
async fn post(
    client: &reqwest::Client,
    program: &SsuPathProgram,
) -> Result<SsufidPost, SsuPathPluginError> {
    tracing::info!("Crawling program {}", program.id);
    let url = format!("{PATH_ENTRY_URL}{}", program.id);
    let response = client.get(&url).send().await?.text().await?;
    let document = Html::parse_document(&response);
    let program_table = SsuPathProgramTable::from_document(&document)?;
    let course_table = match program.kind {
        SsuPathProgramKind::Single { .. } => Some(SsuPathCourseTable::from_document(&document)?),
        _ => None,
    };
    let division_table = match program.kind {
        SsuPathProgramKind::Division(_) => Some(SsuPathDivisionTable::from_document(&document)?),
        _ => None,
    };
    let content = construct_content(&program_table, &course_table, &division_table);
    let frontmatters = construct_frontmatters(&program_table, &course_table, &division_table);
    Ok(SsufidPost {
        id: program.id.clone(),
        title: program_table.title,
        description: Some(program.description.clone()),
        category: vec![program.label.clone()],
        url,
        created_at: program.create_at(),
        content,
        updated_at: None,
        author: program.major_types.first().cloned(),
        thumbnail: Some(program.thumbnail.clone()),
        attachments: Vec::default(),
        metadata: Some(frontmatters),
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires valid credentials"]
    async fn test_authorization() {
        dotenvy::dotenv().ok();
        let plugin = SsuPathPlugin::new(SsuPathCredential::Password(
            std::env::var("SSU_ID").unwrap(),
            std::env::var("SSU_PASSWORD").unwrap(),
        ));
        let client = plugin.client().await.unwrap();
        let response = client.get("https://path.ssu.ac.kr/").send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
}
