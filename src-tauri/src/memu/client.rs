use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::memu::bridge::{BridgeError, MemUBridge};
use crate::memory::{ScenarioMemorizeResult, EnrichedMemoryItem};

// ─── Response Types ────────────────────────────────────────────────────

/// Result of a `memorize` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorizeResult {
    /// Memory items that were created.
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
    /// Updated category summaries.
    #[serde(default)]
    pub categories: Vec<serde_json::Value>,
}

/// Result of a `retrieve` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrieveResult {
    /// Retrieved memory items.
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
    /// Retrieved category summaries.
    #[serde(default)]
    pub categories: Vec<serde_json::Value>,
}

/// Result of a `create_item` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateItemResult {
    /// The created memory item.
    pub memory_item: Option<serde_json::Value>,
}

/// Result of a `list_items` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItemsResult {
    /// Memory items.
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
    /// Total count.
    #[serde(default)]
    pub total: u64,
}

/// A memory category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCategory {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub summary: Option<String>,
}

// ─── Client ────────────────────────────────────────────────────────────

/// High-level Rust client for the memU memory service.
///
/// Wraps `MemUBridge` and provides typed methods for each memU API operation.
pub struct MemUClient {
    bridge: Arc<MemUBridge>,
}

impl MemUClient {
    /// Create a new `MemUClient` backed by the given bridge.
    pub fn new(bridge: Arc<MemUBridge>) -> Self {
        Self { bridge }
    }

    /// Process content through the memU memorize pipeline.
    ///
    /// This extracts memory items from the content and updates category summaries.
    ///
    /// # Arguments
    /// * `content` - The text content to memorize
    /// * `modality` - Content modality (e.g. "text", "conversation")
    /// * `user_scope` - Optional user scoping (e.g. `{"user_id": "xxx"}`)
    pub async fn memorize(
        &self,
        content: &str,
        modality: &str,
        user_scope: Option<serde_json::Value>,
    ) -> Result<MemorizeResult, BridgeError> {
        let params = serde_json::json!({
            "content": content,
            "modality": modality,
            "user_scope": user_scope,
        });

        let result = self.bridge.send_request("memorize", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }

    /// Retrieve relevant memories for the given queries.
    ///
    /// # Arguments
    /// * `queries` - List of query objects (e.g. `[{"role": "user", "content": "..."}]`)
    /// * `where_clause` - Optional filtering clause
    /// * `user_scope` - Optional user scoping
    pub async fn retrieve(
        &self,
        queries: Vec<serde_json::Value>,
        where_clause: Option<serde_json::Value>,
        user_scope: Option<serde_json::Value>,
    ) -> Result<RetrieveResult, BridgeError> {
        let params = serde_json::json!({
            "queries": queries,
            "where": where_clause,
            "user_scope": user_scope,
        });

        let result = self.bridge.send_request("retrieve", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }

    /// Create a single memory item directly (bypassing the memorize pipeline).
    ///
    /// # Arguments
    /// * `memory_type` - Type of memory (e.g. "profile", "event", "knowledge")
    /// * `memory_content` - Content of the memory item
    /// * `memory_categories` - Categories to associate with
    /// * `user_scope` - Optional user scoping
    pub async fn create_item(
        &self,
        memory_type: &str,
        memory_content: &str,
        memory_categories: Vec<String>,
        user_scope: Option<serde_json::Value>,
    ) -> Result<CreateItemResult, BridgeError> {
        let params = serde_json::json!({
            "memory_type": memory_type,
            "memory_content": memory_content,
            "memory_categories": memory_categories,
            "user_scope": user_scope,
        });

        let result = self.bridge.send_request("create_item", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }

    /// Delete a memory item by its ID.
    ///
    /// # Arguments
    /// * `id` - The memory item ID to delete
    /// * `user_scope` - Optional user scoping
    pub async fn delete_item(
        &self,
        id: &str,
        user_scope: Option<serde_json::Value>,
    ) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "id": id,
            "user_scope": user_scope,
        });

        self.bridge.send_request("delete_item", params).await?;
        Ok(())
    }

    /// List memory items with optional filtering.
    ///
    /// # Arguments
    /// * `category` - Optional category name filter
    /// * `memory_type` - Optional memory type filter
    /// * `limit` - Max number of items to return
    /// * `offset` - Pagination offset
    /// * `user_scope` - Optional user scoping
    pub async fn list_items(
        &self,
        category: Option<&str>,
        memory_type: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
        user_scope: Option<serde_json::Value>,
    ) -> Result<ListItemsResult, BridgeError> {
        let params = serde_json::json!({
            "category": category,
            "memory_type": memory_type,
            "limit": limit.unwrap_or(50),
            "offset": offset.unwrap_or(0),
            "user_scope": user_scope,
        });

        let result = self.bridge.send_request("list_items", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }

    /// List all memory categories.
    ///
    /// # Arguments
    /// * `user_scope` - Optional user scoping
    pub async fn list_categories(
        &self,
        user_scope: Option<serde_json::Value>,
    ) -> Result<Vec<MemoryCategory>, BridgeError> {
        let params = serde_json::json!({
            "user_scope": user_scope,
        });

        let result = self.bridge.send_request("list_categories", params).await?;

        // The result may be a list directly or wrapped in {"categories": [...]}
        if let Some(cats) = result.get("categories") {
            serde_json::from_value(cats.clone()).map_err(BridgeError::JsonError)
        } else if result.is_array() {
            serde_json::from_value(result).map_err(BridgeError::JsonError)
        } else {
            Ok(vec![])
        }
    }

    /// Perform a health check on the memU subprocess.
    ///
    /// Returns `true` if the subprocess is responsive.
    pub async fn health_check(&self) -> Result<bool, BridgeError> {
        if !self.bridge.is_alive() {
            return Ok(false);
        }

        match self
            .bridge
            .send_request_with_timeout(
                "health",
                serde_json::Value::Null,
                std::time::Duration::from_secs(5),
            )
            .await
        {
            Ok(result) => {
                let ok = result
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "ok")
                    .unwrap_or(true);
                Ok(ok)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check if the underlying bridge is alive.
    pub fn is_available(&self) -> bool {
        self.bridge.is_alive()
    }

    /// Stop the underlying bridge.
    pub async fn shutdown(&self) -> Result<(), BridgeError> {
        self.bridge.stop().await
    }

    // ── Scene-aware Bridge Methods ──────────────────────────────────────

    /// 带场景配置的 memorize — 支持指定记忆类型和分类
    ///
    /// # Arguments
    /// * `content` - The text content to memorize
    /// * `memory_types` - Types to extract (e.g. ["profile", "behavior", "skill"])
    /// * `categories` - Optional category hints for classification
    /// * `source_type` - Origin of the content ("conversation" | "execution_log" | "multimodal")
    pub async fn memorize_with_config(
        &self,
        content: &str,
        memory_types: &[&str],
        categories: Option<&[&str]>,
        source_type: &str,
    ) -> Result<ScenarioMemorizeResult, BridgeError> {
        let params = serde_json::json!({
            "input": {
                "content": content,
                "memory_types": memory_types,
                "categories": categories,
                "source_type": source_type,
            }
        });

        let result = self.bridge.send_request("memorize_with_config", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }

    /// 带场景配置的 retrieve — 支持类型过滤和分类信息
    ///
    /// # Arguments
    /// * `query` - The retrieval query string
    /// * `memory_types` - Optional filter by memory types
    /// * `limit` - Maximum number of items to return
    /// * `include_categories` - Whether to include category information in results
    pub async fn retrieve_with_context(
        &self,
        query: &str,
        memory_types: Option<&[&str]>,
        limit: usize,
        include_categories: bool,
    ) -> Result<Vec<EnrichedMemoryItem>, BridgeError> {
        let params = serde_json::json!({
            "input": {
                "query": query,
                "memory_types": memory_types,
                "limit": limit,
                "include_categories": include_categories,
            }
        });

        let result = self.bridge.send_request("retrieve_with_context", params).await?;

        // The result may be wrapped in {"items": [...]} or be a direct array
        let items_val = if let Some(items) = result.get("items") {
            items.clone()
        } else if result.is_array() {
            result
        } else {
            serde_json::Value::Array(vec![])
        };

        serde_json::from_value(items_val).map_err(BridgeError::JsonError)
    }

    /// 多模态 memorize — 处理预处理后的多模态内容
    ///
    /// Combines text and caption into `"[Caption: {caption}]\n\n{text}"` format
    /// and includes source_type in metadata.
    ///
    /// # Arguments
    /// * `text` - The main text content
    /// * `caption` - A descriptive caption for the content
    /// * `source_type` - Media type ("image" | "document" | "code" | "audio")
    /// * `metadata` - Additional metadata to attach
    pub async fn memorize_multimodal(
        &self,
        text: &str,
        caption: &str,
        source_type: &str,
        metadata: &serde_json::Value,
    ) -> Result<ScenarioMemorizeResult, BridgeError> {
        let combined_content = format!("[Caption: {}]\n\n{}", caption, text);

        // Merge source_type into metadata
        let mut enriched_metadata = metadata.clone();
        if let Some(obj) = enriched_metadata.as_object_mut() {
            obj.insert("source_type".into(), serde_json::Value::String(source_type.into()));
        } else {
            enriched_metadata = serde_json::json!({
                "source_type": source_type,
                "original_metadata": metadata,
            });
        }

        let params = serde_json::json!({
            "input": {
                "content": combined_content,
                "source_type": source_type,
                "metadata": enriched_metadata,
            }
        });

        let result = self.bridge.send_request("memorize_multimodal", params).await?;
        serde_json::from_value(result).map_err(BridgeError::JsonError)
    }
}
