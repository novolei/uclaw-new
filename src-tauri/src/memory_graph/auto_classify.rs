use std::sync::Arc;
use serde::Deserialize;
use tracing::{info, warn, error};

use crate::app::AppState;
use crate::agent::types::ChatMessage;
use crate::llm::{create_provider, CompletionConfig};
use crate::memory_graph::models::MemoryKeyword;
use crate::providers::service::ProviderService;

#[derive(Debug, Deserialize)]
struct ClassificationResult {
    subtype: Option<String>,
    title: Option<String>,
    keywords: Option<Vec<String>>,
    is_reminder: Option<bool>,
}

/// 碎片保存后异步触发 LLM 分类
/// 如果 LLM 未配置，graceful skip 不阻塞
pub async fn auto_classify_fragment(
    app_handle: tauri::AppHandle,
    node_id: String,
    content: String,
) {
    use tauri::Manager;

    // 1. 从 app_handle state 获取 ProviderService
    let state = match app_handle.try_state::<AppState>() {
        Some(s) => s,
        None => {
            warn!("[auto_classify] AppState not available, skipping");
            return;
        }
    };

    let provider_service: &Arc<ProviderService> = &state.provider_service;

    // 2. 获取用户配置的 LLM config
    let llm_params = match provider_service.get_active_llm_config().await {
        Some(params) => params,
        None => {
            info!("[auto_classify] No active LLM configured, skipping classification");
            return;
        }
    };

    let (provider_id, model, api_key, base_url) = llm_params;

    // 构建 LlmConfig 并创建 provider
    let llm_config = crate::config::llm::LlmConfig {
        provider: provider_id.clone(),
        model: model.clone(),
        api_key,
        base_url: if base_url.is_empty() { None } else { Some(base_url) },
        max_tokens: Some(1024),
        temperature: Some(0.3),
        api: None,
    };

    let provider = match create_provider(&llm_config) {
        Ok(p) => p,
        Err(e) => {
            warn!("[auto_classify] Failed to create LLM provider: {}", e);
            return;
        }
    };

    // 3. 构造分类 prompt
    let system_prompt = r#"你是一个记忆碎片分类助手。根据用户提供的内容，判断以下信息：
1. subtype: 从 [daily, credential, location, reminder, inspiration, bookmark] 中选择最匹配的一个
2. title: 为这段内容生成一个不超过12个字的简短标题
3. keywords: 提取3-5个关键词
4. is_reminder: 判断这段内容是否需要定时提醒用户回忆（true/false）

仅返回 JSON 格式，不要有其他文字：
{"subtype": "...", "title": "...", "keywords": ["...", "..."], "is_reminder": false}"#;

    // 4. 构造 messages 并调用 LLM
    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(&content),
    ];

    let config = CompletionConfig {
        model,
        max_tokens: 512,
        temperature: 0.3,
        thinking_enabled: false,
    };

    let response = match provider.complete(messages, vec![], &config).await {
        Ok(r) => r,
        Err(e) => {
            warn!("[auto_classify] LLM call failed: {}", e);
            return;
        }
    };

    // 从响应中提取文本内容
    let response_text = match &response {
        crate::agent::types::RespondOutput::Text { text, .. } => text.clone(),
        crate::agent::types::RespondOutput::ToolCalls { text, .. } => {
            text.clone().unwrap_or_default()
        }
    };

    if response_text.is_empty() {
        warn!("[auto_classify] LLM returned empty response");
        return;
    }

    // 5. 解析 JSON 结果
    // 尝试提取 JSON（可能被 markdown 代码块包裹）
    let json_str = extract_json_from_response(&response_text);
    let classification: ClassificationResult = match serde_json::from_str(&json_str) {
        Ok(c) => c,
        Err(e) => {
            error!("[auto_classify] Failed to parse LLM response as JSON: {} | raw: {}", e, response_text);
            return;
        }
    };

    // 6. 更新节点 metadata
    let store = &state.memory_graph_store;

    // 构建 metadata 更新
    let mut metadata_update = serde_json::Map::new();
    if let Some(ref subtype) = classification.subtype {
        metadata_update.insert("subtype".to_string(), serde_json::Value::String(subtype.clone()));
    }
    if let Some(ref keywords) = classification.keywords {
        metadata_update.insert(
            "keywords".to_string(),
            serde_json::Value::Array(
                keywords.iter().map(|k| serde_json::Value::String(k.clone())).collect(),
            ),
        );
    }
    if let Some(is_reminder) = classification.is_reminder {
        metadata_update.insert("is_reminder".to_string(), serde_json::Value::Bool(is_reminder));
    }
    metadata_update.insert("classified".to_string(), serde_json::Value::Bool(true));

    // 读取现有 metadata 并合并
    let merged_metadata = if let Ok(Some(node)) = store.get_node(&node_id) {
        let mut existing = node.metadata.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = existing.as_object_mut() {
            for (k, v) in metadata_update {
                obj.insert(k, v);
            }
        }
        existing
    } else {
        serde_json::Value::Object(metadata_update)
    };

    // 更新节点
    let title_ref = classification.title.as_deref();
    if let Err(e) = store.update_node(&node_id, title_ref, None, Some(&merged_metadata)) {
        error!("[auto_classify] Failed to update node metadata: {}", e);
        return;
    }

    info!("[auto_classify] Node {} classified: subtype={:?}, title={:?}",
        node_id, classification.subtype, classification.title);

    // 7. 如果 is_reminder = true，创建 fragment_reviews 记录
    if classification.is_reminder.unwrap_or(false) {
        let db = &state.db;
        if let Ok(conn) = db.lock() {
            if let Err(e) = crate::proactive::review_scheduler::schedule_review(&conn, &node_id) {
                error!("[auto_classify] Failed to schedule review: {}", e);
            }
        }
    }

    // 8. 如果有新 keywords，插入 memory_keywords 表
    if let Some(keywords) = classification.keywords {
        for kw in keywords {
            let keyword = MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".to_string(),
                node_id: node_id.clone(),
                keyword: kw,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            let _ = store.create_keyword(&keyword);
        }
    }
}

/// 从 LLM 响应中提取 JSON 字符串（处理 markdown 代码块包裹等情况）
fn extract_json_from_response(text: &str) -> String {
    let trimmed = text.trim();

    // 尝试从 ```json ... ``` 中提取
    if let Some(start) = trimmed.find("```json") {
        let after_tag = &trimmed[start + 7..];
        if let Some(end) = after_tag.find("```") {
            return after_tag[..end].trim().to_string();
        }
    }

    // 尝试从 ``` ... ``` 中提取
    if let Some(start) = trimmed.find("```") {
        let after_tag = &trimmed[start + 3..];
        if let Some(end) = after_tag.find("```") {
            return after_tag[..end].trim().to_string();
        }
    }

    // 尝试找到第一个 { 和最后一个 }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            return trimmed[start..=end].to_string();
        }
    }

    trimmed.to_string()
}
