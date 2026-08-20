//! GitHub REST client: starred list + README fetch with ETag caching.

use anyhow::{anyhow, bail, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::db::Repo;

const API: &str = "https://api.github.com";
const PER_PAGE: usize = 100;
const MAX_PAGES: usize = 200; // 20k stars guard

pub struct GithubClient {
    http: reqwest::Client,
    token: String,
}

#[derive(Deserialize)]
struct StarredItem {
    starred_at: String,
    repo: ApiRepo,
}

#[derive(Deserialize)]
struct ApiRepo {
    id: i64,
    full_name: String,
    description: Option<String>,
    language: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
    stargazers_count: i64,
    html_url: String,
    homepage: Option<String>,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    fork: bool,
    pushed_at: Option<String>,
}

pub enum ReadmeResult {
    /// New content + its ETag.
    Fresh(String, Option<String>),
    /// 304 — cached copy still valid.
    NotModified,
    /// Repo has no README (404).
    Missing,
}

impl GithubClient {
    pub fn new(token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("star-recall")
            .build()?;
        Ok(Self {
            http,
            token: token.trim().to_string(),
        })
    }

    fn auth_headers(&self, accept: &str) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|_| anyhow!("token contains invalid characters"))?,
        );
        h.insert(ACCEPT, HeaderValue::from_str(accept)?);
        h.insert("X-GitHub-Api-Version", HeaderValue::from_static("2022-11-28"));
        Ok(h)
    }

    /// Validate token; returns the login name.
    pub async fn viewer_login(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        let resp = self
            .http
            .get(format!("{API}/user"))
            .headers(self.auth_headers("application/vnd.github+json")?)
            .send()
            .await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            bail!("token rejected by GitHub (401)");
        }
        let resp = check_rate_limit(resp)?;
        Ok(resp.json::<User>().await?.login)
    }

    /// All starred repos of the authenticated user, newest star first.
    pub async fn list_starred(
        &self,
        mut on_page: impl FnMut(usize, usize),
    ) -> Result<Vec<(String, Repo)>> {
        let mut out = Vec::new();
        for page in 1..=MAX_PAGES {
            let resp = self
                .http
                .get(format!("{API}/user/starred?per_page={PER_PAGE}&page={page}"))
                .headers(self.auth_headers("application/vnd.github.star+json")?)
                .send()
                .await?;
            let resp = check_rate_limit(resp)?;
            let items: Vec<StarredItem> = resp.json().await?;
            let n = items.len();
            for item in items {
                let r = item.repo;
                out.push((
                    item.starred_at.clone(),
                    Repo {
                        id: r.id,
                        full_name: r.full_name,
                        description: r.description,
                        language: r.language,
                        topics: r.topics,
                        stars: r.stargazers_count,
                        html_url: r.html_url,
                        homepage: r.homepage.filter(|h| !h.is_empty()),
                        archived: r.archived,
                        fork: r.fork,
                        starred_at: Some(item.starred_at),
                        pushed_at: r.pushed_at,
                    },
                ));
            }
            on_page(page, out.len());
            if n < PER_PAGE {
                break;
            }
        }
        Ok(out)
    }

    /// Raw README with conditional request. 304 does not count against the rate limit.
    pub async fn fetch_readme(&self, full_name: &str, etag: Option<&str>) -> Result<ReadmeResult> {
        let mut headers = self.auth_headers("application/vnd.github.raw+json")?;
        if let Some(etag) = etag {
            if let Ok(v) = HeaderValue::from_str(etag) {
                headers.insert(IF_NONE_MATCH, v);
            }
        }
        let resp = self
            .http
            .get(format!("{API}/repos/{full_name}/readme"))
            .headers(headers)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NOT_MODIFIED => Ok(ReadmeResult::NotModified),
            StatusCode::NOT_FOUND => Ok(ReadmeResult::Missing),
            _ => {
                let resp = check_rate_limit(resp)?;
                let etag = resp
                    .headers()
                    .get(ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                Ok(ReadmeResult::Fresh(resp.text().await?, etag))
            }
        }
    }
}

/// Surface rate-limit exhaustion with the reset time instead of a generic error.
fn check_rate_limit(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        let remaining = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        if remaining == "0" {
            let reset = resp
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?");
            bail!("GitHub rate limit exhausted; resets at unix {reset}");
        }
        bail!("GitHub returned {status}");
    }
    if !status.is_success() {
        bail!("GitHub returned {status}");
    }
    Ok(resp)
}
