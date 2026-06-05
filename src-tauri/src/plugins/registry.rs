//! Pi-3b — remote MCP registry (registry.modelcontextprotocol.io) browse + manifest mapping.
use serde::{Deserialize, Serialize};
use crate::plugins::catalog::EnvHint;

const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers?limit=100";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env_hints: Vec<EnvHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// Map non-`[A-Za-z0-9_-]` chars to `-`, collapse repeats, trim `-`, lowercase.
/// Empty input → "plugin".
pub fn sanitize_plugin_id(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "plugin".to_string()
    } else {
        trimmed
    }
}

/// Pure: parse a registry JSON body into stdio-package entries, filtered by an optional query.
pub fn parse_servers(body: &serde_json::Value, query: Option<&str>) -> Vec<RegistryEntry> {
    let mut out = Vec::new();
    let Some(servers) = body.get("servers").and_then(|v| v.as_array()) else {
        return out;
    };
    for item in servers {
        let Some(server) = item.get("server") else {
            continue;
        };
        let name = server.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let title = server
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(name)
            .to_string();
        let description = server
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let homepage = server
            .get("repository")
            .and_then(|r| r.get("url"))
            .and_then(|v| v.as_str())
            .map(String::from);

        // First stdio package wins.
        let pkgs = server.get("packages").and_then(|v| v.as_array());
        let Some(pkg) = pkgs.and_then(|arr| {
            arr.iter().find(|p| {
                p.get("transport")
                    .and_then(|t| t.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("stdio")
            })
        }) else {
            continue; // no stdio package → skip (remotes-only servers)
        };

        let reg_type = pkg.get("registryType").and_then(|v| v.as_str()).unwrap_or("");
        let identifier = pkg
            .get("identifier")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if identifier.is_empty() {
            continue;
        }
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("latest");

        let (command, args) = match reg_type {
            "npm" => (
                "npx".to_string(),
                vec!["-y".to_string(), format!("{identifier}@{version}")],
            ),
            "pypi" => ("uvx".to_string(), vec![identifier.to_string()]),
            _ => continue, // unsupported package type
        };

        let env_hints: Vec<EnvHint> = pkg
            .get("environmentVariables")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        let n = e.get("name").and_then(|v| v.as_str())?;
                        let d = e
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        Some(EnvHint {
                            name: n.to_string(),
                            description: d.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let needs_env = pkg
            .get("environmentVariables")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter().any(|e| {
                    e.get("isRequired")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        let needs_args = pkg
            .get("packageArguments")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter().any(|e| {
                    e.get("isRequired")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        let setup_note = if needs_env || needs_args {
            Some(
                "此服务需要额外配置（环境变量/参数）。安装后在插件详情里设置环境变量，或编辑 plugin.toml 添加参数。"
                    .to_string(),
            )
        } else {
            None
        };

        let entry = RegistryEntry {
            id: sanitize_plugin_id(name),
            name: name.to_string(),
            title,
            description,
            command,
            args,
            env_hints,
            setup_note,
            homepage,
        };

        // Query filter: substring match over name/title/description (lowercased).
        if let Some(q) = query {
            let q = q.to_lowercase();
            let hay =
                format!("{} {} {}", entry.name, entry.title, entry.description).to_lowercase();
            if !hay.contains(&q) {
                continue;
            }
        }

        out.push(entry);
    }
    out
}

/// Fetch the live registry page, filter to stdio-package servers, optionally filter by query.
pub async fn fetch_registry(query: Option<&str>) -> Result<Vec<RegistryEntry>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(8000))
        .user_agent("uclaw")
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(REGISTRY_URL)
        .send()
        .await
        .map_err(|e| format!("registry request: {e}"))?;
    let resp = resp
        .error_for_status()
        .map_err(|e| format!("registry status: {e}"))?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("registry decode: {e}"))?;
    Ok(parse_servers(&body, query))
}

/// Build a PluginManifest from a RegistryEntry — mirrors `manifest_from_catalog`.
pub fn manifest_from_registry(
    e: &RegistryEntry,
) -> crate::plugin_manifest::schema::PluginManifest {
    use crate::plugin_manifest::schema::*;
    PluginManifest {
        id: e.id.clone(),
        version: "0.0.0".to_string(),
        display_name: if e.title.is_empty() {
            e.name.clone()
        } else {
            e.title.clone()
        },
        description: Some(e.description.clone()),
        author: PluginAuthor {
            name: "registry".into(),
            email: None,
            url: e.homepage.clone(),
        },
        runtime: PluginRuntimeRequirement {
            min_uclaw_version: "0.1.0".into(),
            kind: Some("subprocess".into()),
            executable: Some(e.command.clone()),
            args: e.args.clone(),
            working_dir: None,
        },
        permissions: PluginPermissions {
            network: true,
            filesystem_read: false,
            filesystem_write: true,
            memory_read: false,
            memory_write: false,
            run_subprocess: true,
            additional: vec![],
        },
        contributes: PluginContribution {
            mcp_servers: vec![e.id.clone()],
            skills: vec![],
            commands: vec![],
            tools: vec![],
            themes: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_handles_registry_names() {
        assert_eq!(sanitize_plugin_id("io.github.foo/bar"), "io-github-foo-bar");
        let s = sanitize_plugin_id("io.github.foo/bar");
        assert!(!s.contains('/') && !s.contains('\\') && !s.contains(".."));
        assert_eq!(sanitize_plugin_id("--A.B--"), "a-b");
        assert_eq!(sanitize_plugin_id("///"), "plugin");
    }

    #[test]
    fn parse_skips_remotes_and_maps_packages() {
        let body = serde_json::json!({ "servers": [
            { "server": { "name": "io.x/remote", "description": "r", "remotes": [{"type":"streamable-http","url":"https://x"}] } },
            { "server": { "name": "io.x/npmsrv", "title": "NPM", "description": "n", "packages": [{ "registryType":"npm","identifier":"@a/b","version":"1.2.3","transport":{"type":"stdio"} }] } },
            { "server": { "name": "io.x/pysrv", "description": "p", "packages": [{ "registryType":"pypi","identifier":"mcp-x","version":"latest","transport":{"type":"stdio"} }] } }
        ]});
        let out = parse_servers(&body, None);
        assert_eq!(out.len(), 2); // remote-only entry skipped
        let npm = out.iter().find(|e| e.name == "io.x/npmsrv").unwrap();
        assert_eq!(npm.command, "npx");
        assert_eq!(
            npm.args,
            vec!["-y".to_string(), "@a/b@1.2.3".to_string()]
        );
        assert_eq!(npm.id, "io-x-npmsrv");
        let py = out.iter().find(|e| e.name == "io.x/pysrv").unwrap();
        assert_eq!(py.command, "uvx");
        assert_eq!(py.args, vec!["mcp-x".to_string()]);
    }

    #[test]
    fn parse_query_filters() {
        let body = serde_json::json!({ "servers": [
            { "server": { "name":"a/github","title":"GitHub","description":"git","packages":[{"registryType":"npm","identifier":"gh","version":"1","transport":{"type":"stdio"}}] } },
            { "server": { "name":"a/weather","title":"Weather","description":"w","packages":[{"registryType":"npm","identifier":"we","version":"1","transport":{"type":"stdio"}}] } }
        ]});
        assert_eq!(parse_servers(&body, Some("github")).len(), 1);
    }

    #[test]
    fn manifest_from_registry_shapes() {
        let e = RegistryEntry {
            id: "x".into(),
            name: "a/x".into(),
            title: "X".into(),
            description: "d".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@a/x@1".into()],
            env_hints: vec![],
            setup_note: None,
            homepage: None,
        };
        let m = manifest_from_registry(&e);
        assert_eq!(m.id, "x");
        assert_eq!(m.runtime.executable.as_deref(), Some("npx"));
        assert!(m.permissions.network && m.permissions.run_subprocess);
        let _ = toml::to_string(&m).unwrap();
    }
}
