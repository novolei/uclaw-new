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

/// Try a list of base URLs in order. Returns `(body, base_that_worked)` on
/// first success. Collected errors from earlier attempts are joined in the
/// final Err message so we don't drop diagnostic signal.
///
/// Takes owned `String` bases to keep the async future Send-clean
/// (borrowed `&str` lifetimes interact poorly with reqwest's future
/// auto-traits across await points in some toolchain versions).
async fn fetch_with_fallback(
    client: &reqwest::Client,
    bases: Vec<String>,
    relative_path: &str,
) -> Result<(String, String)> {
    let mut errors: Vec<String> = Vec::new();
    for base in bases {
        let url = format!(
            "{}/{}",
            base.trim_end_matches('/'),
            relative_path.trim_start_matches('/'),
        );
        match client.get(&url).send().await {
            Err(e) => {
                errors.push(format!("{}: send failed: {}", base, e));
                continue;
            }
            Ok(resp) => {
                if !resp.status().is_success() {
                    errors.push(format!("{}: HTTP {}", base, resp.status()));
                    continue;
                }
                match resp.text().await {
                    Err(e) => {
                        errors.push(format!("{}: body read failed: {}", base, e));
                        continue;
                    }
                    Ok(body) => return Ok((body, base)),
                }
            }
        }
    }
    Err(anyhow!(
        "all registry mirrors failed for /{}: {}",
        relative_path,
        errors.join("; ")
    ))
}

/// Fetch the registry index.
///
/// Halo protocol: `GET {source.url}/index.json` → [`RegistryIndex`].
/// Falls back through `source.fallback_urls` in order (e.g. Gitee mirror)
/// if the primary base returns an error (TLS handshake fail, 5xx, timeout).
/// Mirrors `HaloAdapter.fetchIndex()` from hello-halo, extended with
/// mirror fallback for GFW-affected users.
pub async fn fetch_index(source: &RegistrySource) -> Result<RegistryIndex> {
    let client = http_client()?;
    let bases: Vec<String> = source.url_candidates().map(String::from).collect();
    let (body, base_used) = fetch_with_fallback(&client, bases, "index.json")
        .await
        .with_context(|| "fetch registry index")?;
    tracing::info!(base = %base_used, "registry index fetched");

    let index: RegistryIndex = serde_json::from_str(&body)
        .with_context(|| format!("registry index JSON parse failed (from {})", base_used))?;

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
/// `entry.download_url` takes precedence when present (no fallback —
/// download_url is a registry-explicit absolute URL).
/// When using the default {base}/{path}/spec.yaml shape, falls back through
/// `source.fallback_urls` exactly like fetch_index.
pub async fn fetch_spec_yaml(source: &RegistrySource, entry: &RegistryEntry) -> Result<String> {
    let client = http_client()?;

    // Explicit download_url — single URL, no fallback (caller already chose).
    if let Some(url) = &entry.download_url {
        let resp = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;
        if !resp.status().is_success() {
            return Err(anyhow!("spec.yaml HTTP {}: {}", resp.status(), url));
        }
        return resp
            .text()
            .await
            .with_context(|| format!("body read failed: {}", url));
    }

    // Default shape — try mirrors in order.
    let relative = format!("{}/spec.yaml", entry.path.trim_matches('/'));
    let bases: Vec<String> = source.url_candidates().map(String::from).collect();
    let (body, _base_used) = fetch_with_fallback(&client, bases, &relative)
        .await
        .with_context(|| format!("fetch spec.yaml for slug '{}'", entry.slug))?;
    Ok(body)
}

/// Fetch a single file from a skill bundle inside an automation package.
/// Resolves to `{base}/{entry.path}/skills/{skill_id}/{filename}`.
/// Returns the body bytes. Text files go through `.bytes()` so binary
/// resources (rare today but legal in a bundle) work too.
///
/// Uses the same mirror-fallback pattern as fetch_spec_yaml — Gitee
/// fallback applies automatically for GFW-affected users.
pub async fn fetch_skill_file(
    source: &RegistrySource,
    entry: &RegistryEntry,
    skill_id: &str,
    filename: &str,
) -> Result<Vec<u8>> {
    let client = http_client()?;
    let relative = format!(
        "{}/skills/{}/{}",
        entry.path.trim_matches('/'),
        skill_id,
        filename,
    );
    let bases: Vec<String> = source.url_candidates().map(String::from).collect();

    let mut errors: Vec<String> = Vec::new();
    for base in bases {
        let url = format!(
            "{}/{}",
            base.trim_end_matches('/'),
            relative.trim_start_matches('/'),
        );
        match client.get(&url).send().await {
            Err(e) => {
                errors.push(format!("{}: send failed: {}", base, e));
                continue;
            }
            Ok(resp) => {
                if !resp.status().is_success() {
                    errors.push(format!("{}: HTTP {}", base, resp.status()));
                    continue;
                }
                match resp.bytes().await {
                    Err(e) => {
                        errors.push(format!("{}: body read failed: {}", base, e));
                        continue;
                    }
                    Ok(body) => return Ok(body.to_vec()),
                }
            }
        }
    }
    Err(anyhow!(
        "all registry mirrors failed for /{}: {}",
        relative,
        errors.join("; ")
    ))
}
