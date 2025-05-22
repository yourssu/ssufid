use std::sync::Arc;

use reqwest::{Client, cookie::Jar};
use thiserror::Error;

use super::utils::{DEFAULT_USER_AGENT, default_header};

const SMARTID_LOGIN_URL: &str = "https://smartid.ssu.ac.kr/Symtra_sso/smln.asp";
const SMARTID_LOGIN_FORM_REQUEST_URL: &str = "https://smartid.ssu.ac.kr/Symtra_sso/smln_pcs.asp";

// TODO: Use rusaint for sso login, after "sso" feature is added to rusaint

#[derive(Error, Debug)]
pub enum SsuSsoError {
    /// 웹 요청, 응답 오류
    #[error("Request error: {0}")]
    RequestError(#[from] reqwest::Error),
    /// 페이지 로그인 폼을 찾을 수 없음
    #[error("Can't load form data from page, is page changed?")]
    CantLoadForm,
    /// 페이지 로그인이 실패하여 토큰이 응답에 포함되지 않음
    #[error("Token is not included in response: {0}")]
    CantFindToken(String),
}

pub(super) async fn obtain_ssu_sso_token(id: &str, password: &str) -> Result<String, SsuSsoError> {
    let jar: Arc<Jar> = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_provider(jar)
        .cookie_store(true)
        .user_agent(DEFAULT_USER_AGENT)
        .build()?;
    let body = client
        .get(SMARTID_LOGIN_URL)
        .headers(default_header())
        .send()
        .await?
        .text()
        .await?;
    let (in_tp_bit, rqst_caus_cd) = parse_login_form(&body)?;
    let params = [
        ("in_tp_bit", in_tp_bit.as_str()),
        ("rqst_caus_cd", rqst_caus_cd.as_str()),
        ("userid", id),
        ("pwd", password),
    ];
    let res = client
        .post(SMARTID_LOGIN_FORM_REQUEST_URL)
        .headers(default_header())
        .form(&params)
        .send()
        .await?;
    let cookie_token = {
        res.cookies()
            .find(|cookie| cookie.name() == "sToken" && !cookie.value().is_empty())
            .map(|cookie| cookie.value().to_string())
    };
    let message = if cookie_token.is_none() {
        let mut content = res.text().await?;
        let start = content.find("alert(\"").unwrap_or(0);
        let end = content.find("\");").unwrap_or(content.len());
        content.truncate(end);
        let message = content.split_off(start + 7);
        Some(message)
    } else {
        None
    };
    cookie_token.ok_or(SsuSsoError::CantFindToken(
        message.unwrap_or("Internal Error".to_string()),
    ))
}

fn parse_login_form(body: &str) -> Result<(String, String), SsuSsoError> {
    let document = scraper::Html::parse_document(body);
    let in_tp_bit_selector = scraper::Selector::parse(r#"input[name="in_tp_bit"]"#).unwrap();
    let rqst_caus_cd_selector = scraper::Selector::parse(r#"input[name="rqst_caus_cd"]"#).unwrap();
    let in_tp_bit = document
        .select(&in_tp_bit_selector)
        .next()
        .ok_or(SsuSsoError::CantLoadForm)?
        .value()
        .attr("value")
        .ok_or(SsuSsoError::CantLoadForm)?;
    let rqst_caus_cd = document
        .select(&rqst_caus_cd_selector)
        .next()
        .ok_or(SsuSsoError::CantLoadForm)?
        .value()
        .attr("value")
        .ok_or(SsuSsoError::CantLoadForm)?;
    Ok((in_tp_bit.to_owned(), rqst_caus_cd.to_owned()))
}
