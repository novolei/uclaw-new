//! Fetch a skill's files from a GitHub directory (contents API + raw
//! download). Shared by every provider and by the agent install tool.
//! Text files only, top-level only, ≤ 32 files, ≤ 512 KB each, SKILL.md
//! required.

use super::{MarketplaceError, SkillDetail};

const GITHUB_API: &str = "https://api.github.com";
pub(crate) const MAX_FILES: usize = 32;
pub(crate) const MAX_FILE_BYTES: usize = 512 * 1024;

/// A skill directory inside a GitHub repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSkillRef {
    pub owner: String,
    pub repo: String,
    pub dir_path: String,
    pub git_ref: String,
}

impl GithubSkillRef {
    /// Install slug: `owner__repo__path` with `/` → `__`.
    pub fn slug(&self) -> String {
        format!("{}__{}__{}", self.owner, self.repo, self.dir_path.replace('/', "__"))
    }
}

/// `https://github.com/{o}/{r}/(tree|blob)/{ref}/{path}` → ref. `None` otherwise.
pub fn parse_github_url(url: &str) -> Option<GithubSkillRef> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 5 || (parts[2] != "tree" && parts[2] != "blob") {
        return None;
    }
    Some(GithubSkillRef {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
        git_ref: parts[3].to_string(),
        dir_path: parts[4..].join("/"),
    })
}

/// `owner/repo[/path]` → `(owner, repo, Some(path) | None)`. `None` when
/// fewer than two segments or any segment is empty.
pub fn parse_source(source: &str) -> Option<(String, String, Option<String>)> {
    let parts: Vec<&str> = source.trim().split('/').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let path = (parts.len() > 2).then(|| parts[2..].join("/"));
    Some((parts[0].to_string(), parts[1].to_string(), path))
}

/// Resolve an install source to a skill directory:
/// - a GitHub URL → parsed directly;
/// - `owner/repo` or `owner/repo/<id>` with `skill_id` → git-trees lookup of
///   `**/<skill_id>/SKILL.md` (skills.sh ids are not always the real path);
/// - `owner/repo/<path>` without `skill_id` → that path on `main`.
pub async fn resolve_source(source: &str, skill_id: Option<&str>) -> Result<GithubSkillRef, MarketplaceError> {
    if source.trim().starts_with("http") {
        return parse_github_url(source)
            .ok_or_else(|| MarketplaceError::Invalid(format!("不是有效的 GitHub 技能目录链接：{source}")));
    }
    let (owner, repo, path) = parse_source(source)
        .ok_or_else(|| MarketplaceError::Invalid(format!("来源应为 owner/repo[/路径]：{source}")))?;
    if let Some(sid) = skill_id.map(str::trim).filter(|s| !s.is_empty()) {
        let resolved = crate::agent::tools::builtin::skill_marketplace_resolve::resolve_skill_path(&owner, &repo, sid)
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        return Ok(GithubSkillRef { owner, repo, dir_path: resolved.dir_path, git_ref: resolved.branch });
    }
    let dir_path = path.ok_or_else(|| {
        MarketplaceError::Invalid(format!("来源 {source} 只有 owner/repo，需要技能 id 才能定位目录"))
    })?;
    Ok(GithubSkillRef { owner, repo, dir_path, git_ref: "main".to_string() })
}

/// Download a skill directory → `SkillDetail` (`hash` = SKILL.md blob sha).
pub async fn fetch_skill_dir(r: &GithubSkillRef) -> Result<SkillDetail, MarketplaceError> {
    fetch_skill_dir_with_base(GITHUB_API, r).await
}

/// [`fetch_skill_dir`] against another API base (tests).
pub async fn fetch_skill_dir_with_base(api_base: &str, r: &GithubSkillRef) -> Result<SkillDetail, MarketplaceError> {
    #[derive(serde::Deserialize)]
    struct Entry {
        #[serde(default)]
        name: String,
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        sha: String,
        #[serde(default)]
        download_url: Option<String>,
    }
    let http = super::http_client();
    let list_url = format!(
        "{}/repos/{}/{}/contents/{}?ref={}",
        api_base.trim_end_matches('/'),
        r.owner,
        r.repo,
        urlencoding::encode(&r.dir_path).replace("%2F", "/"),
        urlencoding::encode(&r.git_ref),
    );
    let resp = http
        .get(&list_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| MarketplaceError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(super::status_error(resp.status()));
    }
    let listing: serde_json::Value = resp.json().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
    let entries: Vec<Entry> = match listing {
        serde_json::Value::Array(_) => {
            serde_json::from_value(listing).map_err(|e| MarketplaceError::Http(e.to_string()))?
        }
        _ => return Err(MarketplaceError::Invalid(format!("{} 不是一个目录", r.dir_path))),
    };
    if entries.len() > MAX_FILES {
        return Err(MarketplaceError::Invalid(format!(
            "技能目录有 {} 个条目，超过上限 {MAX_FILES}",
            entries.len()
        )));
    }
    let hash = entries
        .iter()
        .find(|e| e.name == "SKILL.md" && e.kind == "file")
        .map(|e| e.sha.clone())
        .ok_or_else(|| MarketplaceError::Invalid(format!("{} 里没有 SKILL.md", r.dir_path)))?;

    let mut files = Vec::new();
    for e in entries.iter().filter(|e| e.kind == "file") {
        let Some(url) = e.download_url.as_deref().filter(|u| !u.is_empty()) else {
            continue;
        };
        let resp = http.get(url).send().await.map_err(|err| MarketplaceError::Http(err.to_string()))?;
        if !resp.status().is_success() {
            return Err(super::status_error(resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|err| MarketplaceError::Http(err.to_string()))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(MarketplaceError::Install(format!(
                "{} 有 {} 字节，超过单文件上限 {MAX_FILE_BYTES}",
                e.name,
                bytes.len()
            )));
        }
        // Skill files are text; skip binary assets instead of failing the install.
        let Ok(contents) = String::from_utf8(bytes.to_vec()) else {
            tracing::warn!(file = %e.name, "skills_marketplace: skipping non-text file");
            continue;
        };
        files.push(super::SkillFile { path: e.name.clone(), contents });
    }
    let slug = r.slug();
    Ok(SkillDetail {
        id: format!("{}/{}/{}", r.owner, r.repo, r.dir_path),
        source: format!("{}/{}", r.owner, r.repo),
        slug,
        hash,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(path: &str) -> GithubSkillRef {
        GithubSkillRef { owner: "o".into(), repo: "r".into(), dir_path: path.into(), git_ref: "main".into() }
    }

    #[test]
    fn parses_tree_url() {
        let r = parse_github_url("https://github.com/TuYv/ccpm/tree/main/skills/pdf").unwrap();
        assert_eq!(r, GithubSkillRef { owner: "TuYv".into(), repo: "ccpm".into(), dir_path: "skills/pdf".into(), git_ref: "main".into() });
        assert_eq!(r.slug(), "TuYv__ccpm__skills__pdf");
        assert!(parse_github_url("https://github.com/o/r/blob/dev/x").is_some());
    }

    #[test]
    fn rejects_non_github_and_too_shallow() {
        assert!(parse_github_url("https://gitlab.com/o/r/tree/main/x").is_none());
        assert!(parse_github_url("https://github.com/o/r").is_none());
        assert!(parse_github_url("https://github.com/o/r/tree/main").is_none());
        assert!(parse_github_url("https://github.com/o/r/pulls/1/x").is_none());
    }

    #[test]
    fn parse_source_splits_owner_repo_path() {
        assert_eq!(parse_source("o/r"), Some(("o".into(), "r".into(), None)));
        assert_eq!(parse_source("o/r/a/b"), Some(("o".into(), "r".into(), Some("a/b".into()))));
        assert_eq!(parse_source("o"), None);
        assert_eq!(parse_source("o//x"), None);
        assert_eq!(parse_source("o/r/"), None);
    }

    async fn listing(server: &mut mockito::ServerGuard, entries: serde_json::Value) -> mockito::Mock {
        server
            .mock("GET", "/repos/o/r/contents/s?ref=main")
            .with_status(200)
            .with_body(entries.to_string())
            .create_async()
            .await
    }

    #[tokio::test]
    async fn fetch_skill_dir_reads_files_and_hash() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let _l = listing(&mut server, serde_json::json!([
            {"name": "SKILL.md", "type": "file", "sha": "abc123", "download_url": format!("{base}/raw/SKILL.md")},
            {"name": "notes.md", "type": "file", "sha": "def", "download_url": format!("{base}/raw/notes.md")},
            {"name": "sub", "type": "dir", "sha": "x", "download_url": null},
        ])).await;
        let _a = server.mock("GET", "/raw/SKILL.md").with_body("# skill").create_async().await;
        let _b = server.mock("GET", "/raw/notes.md").with_body("notes").create_async().await;
        let d = fetch_skill_dir_with_base(&base, &gh("s")).await.unwrap();
        assert_eq!(d.hash, "abc123");
        assert_eq!(d.slug, "o__r__s");
        assert_eq!(d.source, "o/r");
        let names: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, vec!["SKILL.md", "notes.md"], "dirs skipped");
        assert_eq!(d.files[0].contents, "# skill");
    }

    #[tokio::test]
    async fn fetch_skill_dir_requires_skill_md() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let _l = listing(&mut server, serde_json::json!([
            {"name": "README.md", "type": "file", "sha": "a", "download_url": format!("{base}/raw/README.md")},
        ])).await;
        let err = fetch_skill_dir_with_base(&base, &gh("s")).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Invalid(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn fetch_skill_dir_rejects_oversize_file() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let _l = listing(&mut server, serde_json::json!([
            {"name": "SKILL.md", "type": "file", "sha": "a", "download_url": format!("{base}/raw/SKILL.md")},
        ])).await;
        let _a = server.mock("GET", "/raw/SKILL.md").with_body("x".repeat(MAX_FILE_BYTES + 1)).create_async().await;
        let err = fetch_skill_dir_with_base(&base, &gh("s")).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Install(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn fetch_skill_dir_skips_binary_files() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let _l = listing(&mut server, serde_json::json!([
            {"name": "SKILL.md", "type": "file", "sha": "a", "download_url": format!("{base}/raw/SKILL.md")},
            {"name": "logo.png", "type": "file", "sha": "b", "download_url": format!("{base}/raw/logo.png")},
        ])).await;
        let _a = server.mock("GET", "/raw/SKILL.md").with_body("# s").create_async().await;
        let _b = server.mock("GET", "/raw/logo.png").with_body(vec![0xff_u8, 0xfe, 0x00]).create_async().await;
        let d = fetch_skill_dir_with_base(&base, &gh("s")).await.unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "SKILL.md");
    }

    #[tokio::test]
    async fn fetch_skill_dir_rejects_too_many_files() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let many: Vec<serde_json::Value> = (0..=MAX_FILES)
            .map(|i| serde_json::json!({"name": format!("f{i}.md"), "type": "file", "sha": "a", "download_url": format!("{base}/raw/f")}))
            .collect();
        let _l = listing(&mut server, serde_json::Value::Array(many)).await;
        let err = fetch_skill_dir_with_base(&base, &gh("s")).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::Invalid(_)), "got {err:?}");
    }
}
