use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;

use super::types::{RegistryEntry, RegistryIndex, RegistrySource};

const USER_AGENT: &str = "uClaw-Marketplace/1.0";
const FETCH_TIMEOUT_SECS: u64 = 30;

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| anyhow!("build reqwest client: {}", e))
}

/// Fetch the registry index.
///
/// Halo protocol: `GET {source.url}/index.json` → [`RegistryIndex`].
/// Mirrors `HaloAdapter.fetchIndex()` from hello-halo.
pub async fn fetch_index(source: &RegistrySource) -> Result<RegistryIndex> {
    let url = format!("{}/index.json", source.url.trim_end_matches('/'));
    let client = http_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {} failed", url))?;
    if !resp.status().is_success() {
        return Err(anyhow!("registry index HTTP {}: {}", resp.status(), url));
    }
    let body = resp
        .text()
        .await
        .with_context(|| format!("body read failed: {}", url))?;
    let index: RegistryIndex = serde_json::from_str(&body)
        .with_context(|| format!("registry index JSON parse failed: {}", url))?;

    // Duplicate-slug check (mirrors hello-halo halo.adapter.ts behaviour).
    let mut seen: HashSet<&str> = HashSet::new();
    for app in &index.apps {
        if !seen.insert(app.slug.as_str()) {
            return Err(anyhow!("duplicate slug in registry: {}", app.slug));
        }
    }

    Ok(index)
}

/// Fetch a single app's `spec.yaml`.
///
/// Halo protocol: `GET {source.url}/{entry.path}/spec.yaml` → raw YAML string.
/// `entry.download_url` takes precedence when present.
pub async fn fetch_spec_yaml(source: &RegistrySource, entry: &RegistryEntry) -> Result<String> {
    let url = entry.download_url.clone().unwrap_or_else(|| {
        format!(
            "{}/{}/spec.yaml",
            source.url.trim_end_matches('/'),
            entry.path
        )
    });
    let client = http_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {} failed", url))?;
    if !resp.status().is_success() {
        return Err(anyhow!("spec.yaml HTTP {}: {}", resp.status(), url));
    }
    resp.text()
        .await
        .with_context(|| format!("body read failed: {}", url))
}
