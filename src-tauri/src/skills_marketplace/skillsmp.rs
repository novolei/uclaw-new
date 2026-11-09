//! skillsmp.com client — search only (no list/detail/audit). Key optional:
//! anonymous 50/day, keyed 500/day. Install/preview go through the row's
//! GitHub URL (`super::github`).

use super::{MarketplaceError, SkillSummary};

const DEFAULT_BASE: &str = "https://skillsmp.com";

pub struct SkillsmpClient {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SkillsmpClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base(DEFAULT_BASE, api_key)
    }

    pub fn with_base(base: &str, api_key: Option<String>) -> Self {
        let api_key = api_key.filter(|k| !k.trim().is_empty());
        Self { base: base.trim_end_matches('/').to_string(), api_key, http: super::http_client() }
    }

    /// `GET /api/v1/skills/search?q=&limit=&sortBy=stars`. A rejected key
    /// (401/403) retries once anonymously.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError> {
        let limit = limit.clamp(1, 100);
        let with_key = self.api_key.is_some();
        match self.search_once(query, limit, with_key).await {
            Err(e) if with_key && super::is_auth_rejection(&e) => {
                tracing::warn!("skillsmp: API key rejected ({e}); retrying anonymously");
                self.search_once(query, limit, false).await
            }
            other => other,
        }
    }

    async fn search_once(&self, query: &str, limit: u32, with_key: bool) -> Result<Vec<SkillSummary>, MarketplaceError> {
        let q = urlencoding::encode(query);
        let mut req = self
            .http
            .get(format!("{}/api/v1/skills/search?q={q}&limit={limit}&sortBy=stars", self.base));
        if with_key {
            if let Some(key) = self.api_key.as_deref() {
                req = req.bearer_auth(key);
            }
        }
        let resp = req.send().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(super::status_error(resp.status()));
        }
        let body: SearchResponse = resp.json().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
        Ok(body.data.skills.into_iter().map(SkillsmpRow::into_summary).collect())
    }
}

#[derive(serde::Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    data: SearchData,
}

#[derive(serde::Deserialize, Default)]
struct SearchData {
    #[serde(default)]
    skills: Vec<SkillsmpRow>,
}

#[derive(serde::Deserialize)]
struct SkillsmpRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "githubUrl", default)]
    github_url: String,
    #[serde(rename = "skillUrl", default)]
    skill_url: String,
    #[serde(default)]
    stars: u64,
}

impl SkillsmpRow {
    fn into_summary(self) -> SkillSummary {
        SkillSummary {
            slug: self.id.clone(),
            id: self.id,
            name: self.name,
            source: self.author,
            installs: self.stars,
            source_type: "github".to_string(),
            install_url: self.github_url,
            url: self.skill_url,
            description: self.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_normalizes_skillsmp_rows() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .with_status(200)
            .with_body(r#"{"success":true,"data":{"skills":[{"id":"a-pdf-skill-md","name":"pdf","author":"TuYv","description":"PDF stuff","githubUrl":"https://github.com/TuYv/ccpm/tree/main/x","skillUrl":"https://skillsmp.com/skills/a","stars":7}]}}"#)
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(&server.url(), None);
        let out = c.search("pdf", 5).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "pdf");
        assert_eq!(out[0].source, "TuYv");
        assert_eq!(out[0].installs, 7);
        assert_eq!(out[0].install_url, "https://github.com/TuYv/ccpm/tree/main/x");
        assert_eq!(out[0].url, "https://skillsmp.com/skills/a");
        assert_eq!(out[0].source_type, "github");
        assert_eq!(out[0].description, "PDF stuff");
        assert_eq!(out[0].slug, "a-pdf-skill-md");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn rejected_key_falls_back_to_anonymous() {
        let mut server = mockito::Server::new_async().await;
        let m401 = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .match_header("authorization", "Bearer sk_bad")
            .with_status(401)
            .create_async()
            .await;
        let m200 = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"data":{"skills":[{"id":"x-skill-md","name":"pdf","githubUrl":"https://github.com/o/r/tree/main/p"}]}}"#)
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(&server.url(), Some("sk_bad".into()));
        let out = c.search("pdf", 5).await.unwrap();
        assert_eq!(out.len(), 1, "anonymous retry returns results despite the bad key");
        m401.assert_async().await;
        m200.assert_async().await;
    }

    #[tokio::test]
    async fn search_anonymous_sends_no_auth_header() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/skills/search?q=x&limit=1&sortBy=stars")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"data":{"skills":[]}}"#)
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(&server.url(), None);
        assert!(c.search("x", 1).await.unwrap().is_empty());
        m.assert_async().await;
    }
}
