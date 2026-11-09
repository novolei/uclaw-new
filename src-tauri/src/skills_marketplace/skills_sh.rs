//! skills.sh client: anonymous search; a key unlocks the `/api/v1` endpoints.

use super::{MarketplaceError, SkillAudit, SkillDetail, SkillSummary};

const DEFAULT_BASE: &str = "https://skills.sh";

pub struct SkillsShClient {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SkillsShClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base(DEFAULT_BASE, api_key)
    }

    pub fn with_base(base: &str, api_key: Option<String>) -> Self {
        let api_key = api_key.filter(|k| !k.trim().is_empty());
        Self { base: base.trim_end_matches('/').to_string(), api_key, http: super::http_client() }
    }

    fn key(&self) -> Result<&str, MarketplaceError> {
        self.api_key.as_deref().ok_or(MarketplaceError::MissingApiKey)
    }

    async fn get_keyed<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, MarketplaceError> {
        let key = self.key()?;
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(super::status_error(resp.status()));
        }
        resp.json::<T>().await.map_err(|e| MarketplaceError::Http(e.to_string()))
    }

    /// Keyed `/api/v1/skills/search` when a key is set (a 401/403 retries once
    /// anonymously, so a bad key never breaks search); else anonymous
    /// `/api/search` (rate-limited, ~60/hour).
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError> {
        let limit = limit.clamp(1, 200);
        if self.api_key.is_some() {
            match self.search_keyed(query, limit).await {
                Err(e) if super::is_auth_rejection(&e) => {
                    tracing::warn!("skills.sh: API key rejected ({e}); retrying anonymously");
                    self.search_anonymous(query, limit).await
                }
                other => other,
            }
        } else {
            self.search_anonymous(query, limit).await
        }
    }

    async fn search_keyed(&self, query: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(default)]
            data: Vec<SkillSummary>,
        }
        let q = urlencoding::encode(query);
        let w: Wrap = self.get_keyed(&format!("/api/v1/skills/search?q={q}&limit={limit}")).await?;
        Ok(w.data)
    }

    /// `{skills:[{id,skillId,name,installs,source}]}`; `id` (`owner/repo/path`)
    /// becomes `install_url` so a keyless install can fetch from GitHub.
    async fn search_anonymous(&self, query: &str, limit: u32) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(default)]
            skills: Vec<AnonRow>,
        }
        #[derive(serde::Deserialize)]
        struct AnonRow {
            #[serde(default)]
            id: String,
            #[serde(rename = "skillId", default)]
            skill_id: String,
            #[serde(default)]
            name: String,
            #[serde(default)]
            installs: u64,
            #[serde(default)]
            source: String,
        }
        let q = urlencoding::encode(query);
        let resp = self
            .http
            .get(format!("{}/api/search?q={q}&limit={limit}", self.base))
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(super::status_error(resp.status()));
        }
        let w: Wrap = resp.json().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
        Ok(w
            .skills
            .into_iter()
            .map(|r| SkillSummary {
                slug: if r.skill_id.is_empty() { r.id.clone() } else { r.skill_id },
                name: r.name,
                source: r.source,
                installs: r.installs,
                source_type: "github".to_string(),
                install_url: r.id.clone(),
                url: String::new(),
                description: String::new(),
                id: r.id,
            })
            .collect())
    }

    /// `GET /api/v1/skills?view=&page=&per_page=` (key required).
    pub async fn list(&self, view: &str, page: u32, per_page: u32) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(default)]
            data: Vec<SkillSummary>,
        }
        let view = match view {
            "trending" | "hot" | "all-time" => view,
            _ => "trending",
        };
        let per_page = per_page.clamp(1, 200);
        let w: Wrap = self
            .get_keyed(&format!("/api/v1/skills?view={view}&page={page}&per_page={per_page}"))
            .await?;
        Ok(w.data)
    }

    /// `GET /api/v1/skills/{id}` with inline files (key required).
    pub async fn detail(&self, id: &str) -> Result<SkillDetail, MarketplaceError> {
        self.get_keyed(&format!("/api/v1/skills/{id}")).await
    }

    /// `GET /api/v1/skills/audit/{id}` (key required).
    pub async fn audit(&self, id: &str) -> Result<SkillAudit, MarketplaceError> {
        self.get_keyed(&format!("/api/v1/skills/audit/{id}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_parses_data_array() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/search?q=react&limit=5")
            .match_header("authorization", "Bearer sk_test")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"expo/skills/react-native","slug":"react-native","name":"React Native","source":"expo/skills","installs":3842}],"count":1}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), Some("sk_test".into()));
        let out = c.search("react", 5).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "react-native");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn no_key_searches_anonymous_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/search?q=excel&limit=3")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"query":"excel","skills":[{"id":"claude-office-skills/skills/excel-automation","skillId":"excel-automation","name":"excel-automation","installs":9529,"source":"claude-office-skills/skills"}],"count":1}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), None);
        let out = c.search("excel", 3).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "excel-automation");
        assert_eq!(out[0].source, "claude-office-skills/skills");
        assert_eq!(out[0].installs, 9529);
        assert_eq!(out[0].install_url, "claude-office-skills/skills/excel-automation");
        assert_eq!(out[0].source_type, "github");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn rejected_key_falls_back_to_anonymous_search() {
        let mut server = mockito::Server::new_async().await;
        let m401 = server.mock("GET", "/api/v1/skills/search?q=excel&limit=3")
            .match_header("authorization", "Bearer sk_bad")
            .with_status(401)
            .create_async().await;
        let m200 = server.mock("GET", "/api/search?q=excel&limit=3")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"skills":[{"id":"o/r/excel","skillId":"excel","name":"excel","installs":7,"source":"o/r"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), Some("sk_bad".into()));
        let out = c.search("excel", 3).await.unwrap();
        assert_eq!(out.len(), 1, "bad key falls back to anonymous results");
        m401.assert_async().await;
        m200.assert_async().await;
    }

    #[tokio::test]
    async fn rate_limit_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _m = server.mock("GET", "/api/search?q=x&limit=1")
            .with_status(429)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), None);
        let err = c.search("x", 1).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::RateLimited), "got {err:?}");
    }

    #[tokio::test]
    async fn detail_requires_key() {
        let c = SkillsShClient::with_base("http://127.0.0.1:9", None);
        let err = c.detail("o/r/s").await.unwrap_err();
        assert!(matches!(err, MarketplaceError::MissingApiKey), "got {err:?}");
    }

    #[tokio::test]
    async fn detail_parses_files() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/expo/skills/react-native")
            .with_status(200)
            .with_body(r#"{"id":"expo/skills/react-native","source":"expo/skills","slug":"react-native","hash":"abc","files":[{"path":"SKILL.md","contents":"---\nname: rn\n---\nbody"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), Some("sk_test".into()));
        let d = c.detail("expo/skills/react-native").await.unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "SKILL.md");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn audit_parses_risk_level() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/audit/expo/skills/react-native")
            .with_status(200)
            .with_body(r#"{"audits":[{"status":"pass","riskLevel":"LOW","summary":"ok"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(&server.url(), Some("sk_test".into()));
        let a = c.audit("expo/skills/react-native").await.unwrap();
        assert_eq!(a.audits[0].risk_level, "LOW");
        m.assert_async().await;
    }
}
