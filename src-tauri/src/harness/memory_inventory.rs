use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::mcp::{CallToolResult, ContentBlock, JsonRpcRequest, McpServerStatus, SharedMcpManager};
use crate::memu::client::MemUClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryProbeStatus {
    Pass,
    Empty,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInventoryTargetReport {
    pub target: String,
    pub status: InventoryProbeStatus,
    pub item_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInventorySmokeReport {
    pub ok: bool,
    pub generated_at: String,
    pub memu: MemoryInventoryTargetReport,
    pub gbrain: MemoryInventoryTargetReport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<String>,
}

impl MemoryInventorySmokeReport {
    fn from_targets(
        memu: MemoryInventoryTargetReport,
        gbrain: MemoryInventoryTargetReport,
    ) -> Self {
        let mut observations = Vec::new();
        collect_observation(&mut observations, &memu);
        collect_observation(&mut observations, &gbrain);
        let ok = matches!(memu.status, InventoryProbeStatus::Pass | InventoryProbeStatus::Empty)
            && matches!(gbrain.status, InventoryProbeStatus::Pass | InventoryProbeStatus::Empty);

        Self {
            ok,
            generated_at: chrono::Utc::now().to_rfc3339(),
            memu,
            gbrain,
            observations,
        }
    }
}

fn collect_observation(out: &mut Vec<String>, target: &MemoryInventoryTargetReport) {
    match target.status {
        InventoryProbeStatus::Pass => out.push(format!(
            "{} inventory reachable with {} items",
            target.target, target.item_count
        )),
        InventoryProbeStatus::Empty => out.push(format!("{} inventory reachable but empty", target.target)),
        InventoryProbeStatus::Unavailable => out.push(format!("{} inventory unavailable", target.target)),
        InventoryProbeStatus::Error => out.push(format!("{} inventory probe failed", target.target)),
    }
}

pub async fn run_memory_inventory_smoke(
    memu_client: Option<Arc<MemUClient>>,
    mcp_manager: SharedMcpManager,
) -> MemoryInventorySmokeReport {
    let (memu, gbrain) = tokio::join!(
        probe_memu_inventory(memu_client),
        probe_gbrain_inventory(mcp_manager)
    );
    MemoryInventorySmokeReport::from_targets(memu, gbrain)
}

async fn probe_memu_inventory(
    memu_client: Option<Arc<MemUClient>>,
) -> MemoryInventoryTargetReport {
    let Some(client) = memu_client else {
        return target_report(
            "memu",
            InventoryProbeStatus::Unavailable,
            0,
            None,
            None,
            Vec::new(),
            Some("memU client is not initialized".to_string()),
        );
    };

    match client.diagnostic_health_check().await {
        Ok(true) => {}
        Ok(false) => {
            return target_report(
                "memu",
                InventoryProbeStatus::Unavailable,
                0,
                None,
                None,
                Vec::new(),
                Some("memU health check returned false".to_string()),
            )
        }
        Err(error) => {
            return target_report(
                "memu",
                InventoryProbeStatus::Error,
                0,
                None,
                None,
                Vec::new(),
                Some(error.to_string()),
            )
        }
    }

    let items = match client.list_items(None, None, Some(20), Some(0), None).await {
        Ok(result) => result,
        Err(error) => {
            return target_report(
                "memu",
                InventoryProbeStatus::Error,
                0,
                None,
                None,
                Vec::new(),
                Some(error.to_string()),
            )
        }
    };
    let category_count = client
        .list_categories(None)
        .await
        .map(|categories| categories.len() as u64)
        .ok();
    let sample_keys = items
        .items
        .iter()
        .filter_map(memu_sample_key)
        .take(5)
        .collect::<Vec<_>>();
    let count = items.total.max(items.items.len() as u64);
    target_report(
        "memu",
        if count == 0 {
            InventoryProbeStatus::Empty
        } else {
            InventoryProbeStatus::Pass
        },
        count,
        category_count,
        None,
        sample_keys,
        None,
    )
}

fn memu_sample_key(item: &serde_json::Value) -> Option<String> {
    for key in ["id", "memory_id", "memory_type", "summary", "memory_content"] {
        if let Some(value) = item.get(key).and_then(|value| value.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.chars().take(80).collect());
            }
        }
    }
    None
}

async fn probe_gbrain_inventory(mcp_manager: SharedMcpManager) -> MemoryInventoryTargetReport {
    let (transport, req_id, tool_count, status_label) = {
        let manager = mcp_manager.read().await;
        let status = manager.status("gbrain");
        let status_label = status
            .as_ref()
            .map(gbrain_status_label)
            .unwrap_or_else(|| "not_registered".to_string());
        let tool_count = manager.server_tool_count("gbrain").unwrap_or(0);
        if !matches!(status, Some(McpServerStatus::Connected)) {
            return target_report(
            "gbrain",
            InventoryProbeStatus::Unavailable,
            0,
            None,
            Some(tool_count as u64),
            Vec::new(),
            Some(format!("gbrain MCP is {}", status_label)),
        );
        }
        match manager.get_transport("gbrain") {
            Ok((transport, req_id)) => (transport, req_id, tool_count, status_label),
            Err(error) => {
                return target_report(
                    "gbrain",
                    InventoryProbeStatus::Error,
                    0,
                    None,
                    Some(tool_count as u64),
                    Vec::new(),
                    Some(error.to_string()),
                )
            }
        }
    };

    let request = JsonRpcRequest::call_tool(
        req_id,
        "list_pages",
        json!({ "limit": 100, "sort": "updated_desc" }),
    );
    let response = match transport.send(&request).await {
        Ok(response) => response,
        Err(error) => {
            return target_report(
                "gbrain",
                InventoryProbeStatus::Error,
                0,
                None,
                Some(tool_count as u64),
                Vec::new(),
                Some(error.to_string()),
            )
        }
    };
    if let Some(error) = response.error {
        return target_report(
            "gbrain",
            InventoryProbeStatus::Error,
            0,
            None,
            Some(tool_count as u64),
            Vec::new(),
            Some(error.to_string()),
        );
    }
    let result = match response
        .result
        .ok_or_else(|| "gbrain list_pages returned no result".to_string())
        .and_then(|value| {
            serde_json::from_value::<CallToolResult>(value)
                .map_err(|error| format!("Invalid gbrain list_pages result: {}", error))
        }) {
        Ok(result) => result,
        Err(error) => {
            return target_report(
                "gbrain",
                InventoryProbeStatus::Error,
                0,
                None,
                Some(tool_count as u64),
                Vec::new(),
                Some(error),
            )
        }
    };
    if result.is_error {
        return target_report(
            "gbrain",
            InventoryProbeStatus::Error,
            0,
            None,
            Some(tool_count as u64),
            Vec::new(),
            Some(call_tool_text(&result)),
        );
    }
    let text = call_tool_text(&result);
    let sample_keys = parse_gbrain_list_slugs(&text);
    let item_count = sample_keys.len() as u64;
    target_report(
        "gbrain",
        if item_count == 0 {
            InventoryProbeStatus::Empty
        } else {
            InventoryProbeStatus::Pass
        },
        item_count,
        None,
        Some(tool_count as u64),
        sample_keys.into_iter().take(5).collect(),
        Some(format!("MCP status: {}", status_label)),
    )
}

fn call_tool_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn gbrain_status_label(status: &McpServerStatus) -> String {
    match status {
        McpServerStatus::Disconnected => "disconnected",
        McpServerStatus::Connecting => "connecting",
        McpServerStatus::Connected => "connected",
        McpServerStatus::Error => "error",
    }
    .to_string()
}

fn parse_gbrain_list_slugs(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .filter(|slug| !slug.is_empty())
        .filter(|slug| !slug.starts_with('#'))
        .filter(|slug| !slug.eq_ignore_ascii_case("No pages found."))
        .map(ToString::to_string)
        .collect()
}

fn target_report(
    target: &str,
    status: InventoryProbeStatus,
    item_count: u64,
    category_count: Option<u64>,
    tool_count: Option<u64>,
    sample_keys: Vec<String>,
    detail: Option<String>,
) -> MemoryInventoryTargetReport {
    MemoryInventoryTargetReport {
        target: target.to_string(),
        status,
        item_count,
        category_count,
        tool_count,
        sample_keys,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gbrain_tsv_slugs_from_list_output() {
        let slugs = parse_gbrain_list_slugs(
            "people/ryanliu\tYour profile\nai-models/gpt-5\tGPT-5\n\n# comment\n",
        );
        assert_eq!(slugs, vec!["people/ryanliu", "ai-models/gpt-5"]);
    }

    #[test]
    fn parses_gbrain_empty_list_as_no_slugs() {
        assert!(parse_gbrain_list_slugs("No pages found.\n").is_empty());
    }

    #[test]
    fn report_marks_reachable_empty_inventory_as_ok_with_observation() {
        let report = MemoryInventorySmokeReport::from_targets(
            target_report("memu", InventoryProbeStatus::Empty, 0, Some(0), None, Vec::new(), None),
            target_report("gbrain", InventoryProbeStatus::Pass, 2, None, Some(7), vec!["a".into()], None),
        );
        assert!(report.ok);
        assert!(report
            .observations
            .iter()
            .any(|observation| observation.contains("memu inventory reachable but empty")));
    }
}
