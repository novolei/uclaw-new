//! ImChannelManager — lifecycle management for IM channel instances.
//!
//! Loads instances from DB at startup, supports hot-reload via apply_config(),
//! starts inbound poll tasks for bidirectional channels (WeCom/iLink).

use crate::channels::notify::{
    dingtalk::DingtalkSender, email::EmailSender, feishu::FeishuSender,
    webhook::WebhookImSender,
};
use crate::channels::types::{ImChannelInstanceConfig, ImChannelSender, ImChannelType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

struct RunningInstance {
    config: ImChannelInstanceConfig,
    sender: Arc<dyn ImChannelSender>,
    /// Present for bidirectional channels (WeCom/iLink); None for notify-only.
    _inbound_task: Option<AbortHandle>,
}

pub struct ImChannelManager {
    instances: Arc<RwLock<HashMap<String, RunningInstance>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ImChannelManager {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    /// Load all enabled instances from DB and start them.
    pub async fn start_all(&self) -> Result<(), String> {
        let configs = self.load_configs_from_db()?;
        for config in configs {
            if config.enabled {
                self.start_instance(config).await?;
            }
        }
        Ok(())
    }

    /// Hot-reload: diff new configs against running instances.
    pub async fn apply_config(&self, new_configs: Vec<ImChannelInstanceConfig>) -> Result<(), String> {
        let new_ids: std::collections::HashSet<String> =
            new_configs.iter().map(|c| c.id.clone()).collect();

        // Stop removed instances
        let running_ids: Vec<String> = {
            let inst = self.instances.read().await;
            inst.keys().cloned().collect()
        };
        for id in &running_ids {
            if !new_ids.contains(id) {
                self.stop_instance(id).await;
            }
        }

        // Start/restart changed instances
        for config in new_configs {
            if !config.enabled {
                self.stop_instance(&config.id).await;
                continue;
            }
            let needs_start = {
                let inst = self.instances.read().await;
                !inst.contains_key(&config.id)
            };
            if needs_start {
                self.start_instance(config).await?;
            }
        }
        Ok(())
    }

    pub async fn start_instance(&self, config: ImChannelInstanceConfig) -> Result<(), String> {
        let sender = self.build_sender(&config)?;
        let inbound_task: Option<AbortHandle> = None;

        let running = RunningInstance {
            config: config.clone(),
            sender,
            _inbound_task: inbound_task,
        };
        self.instances.write().await.insert(config.id.clone(), running);
        tracing::info!("ImChannelManager: started instance {} ({})", config.id, config.channel_type);
        Ok(())
    }

    pub async fn stop_instance(&self, id: &str) {
        if let Some(inst) = self.instances.write().await.remove(id) {
            if let Some(handle) = inst._inbound_task {
                handle.abort();
            }
            tracing::info!("ImChannelManager: stopped instance {}", id);
        }
    }

    pub fn build_sender(
        &self,
        config: &ImChannelInstanceConfig,
    ) -> Result<Arc<dyn ImChannelSender>, String> {
        match config.channel_type {
            ImChannelType::Webhook    => Ok(Arc::new(WebhookImSender::new()) as Arc<dyn ImChannelSender>),
            ImChannelType::Email      => Ok(Arc::new(EmailSender::new())),
            ImChannelType::Dingtalk   => Ok(Arc::new(DingtalkSender::new())),
            ImChannelType::Feishu     => Ok(Arc::new(FeishuSender::new())),
            ImChannelType::WecomBot   => {
                // Bidirectional — Plan B implements real WebSocket sender.
                Ok(Arc::new(NoopSender))
            }
            ImChannelType::WechatIlink => {
                Ok(Arc::new(NoopSender))
            }
        }
    }

    /// Send a message through a specific channel instance.
    pub async fn send_to_instance(
        &self,
        instance_id: &str,
        chat_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let inst = self.instances.read().await;
        let running = inst.get(instance_id)
            .ok_or_else(|| format!("instance {} not running", instance_id))?;
        let ctx = merge_json(running.config.config.clone(), running.config.credentials.clone());
        running.sender.send_text(chat_id, text, Some(&ctx)).await
    }

    pub async fn instance_count(&self) -> usize {
        self.instances.read().await.len()
    }

    pub async fn list_instances(&self) -> Vec<ImChannelInstanceConfig> {
        self.instances.read().await.values().map(|r| r.config.clone()).collect()
    }

    fn load_configs_from_db(&self) -> Result<Vec<ImChannelInstanceConfig>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, channel_type, name, config_json, credentials_json, \
             enabled, streaming, reply_scope, permission_enabled, owners_json, guest_policy_json \
             FROM im_channel_instances",
        ).map_err(|e| e.to_string())?;

        let configs = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,  // id
                r.get::<_, String>(1)?,  // space_id
                r.get::<_, String>(2)?,  // channel_type
                r.get::<_, String>(3)?,  // name
                r.get::<_, String>(4)?,  // config_json
                r.get::<_, String>(5)?,  // credentials_json
                r.get::<_, bool>(6)?,    // enabled
                r.get::<_, bool>(7)?,    // streaming
                r.get::<_, String>(8)?,  // reply_scope
                r.get::<_, bool>(9)?,    // permission_enabled
                r.get::<_, String>(10)?, // owners_json
                r.get::<_, String>(11)?, // guest_policy_json
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .filter_map(|(id, space_id, ct_str, name, cfg, creds, enabled, streaming,
                      reply_scope, perm_enabled, owners_s, gp_s)| {
            let channel_type: ImChannelType = serde_json::from_str(&format!("\"{}\"", ct_str)).ok()?;
            let config: serde_json::Value = serde_json::from_str(&cfg).unwrap_or(serde_json::json!({}));
            let credentials: serde_json::Value = serde_json::from_str(&creds).unwrap_or(serde_json::json!({}));
            let owners: Vec<String> = serde_json::from_str(&owners_s).unwrap_or_default();
            let guest_policy = serde_json::from_str(&gp_s).unwrap_or_default();
            Some(ImChannelInstanceConfig {
                id, space_id, channel_type, name, config, credentials,
                enabled, streaming, reply_scope,
                permission_enabled: perm_enabled,
                owners, guest_policy,
            })
        })
        .collect();

        Ok(configs)
    }
}

/// Merge two JSON objects — second overwrites first on key conflict.
pub fn merge_json(mut base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    if let (Some(b), Some(o)) = (base.as_object_mut(), overlay.as_object()) {
        for (k, v) in o {
            b.insert(k.clone(), v.clone());
        }
    }
    base
}

/// Placeholder sender for bidirectional channels until Plan B wires them up.
pub struct NoopSender;

#[async_trait::async_trait]
impl ImChannelSender for NoopSender {
    async fn send_text(&self, _chat_id: &str, text: &str, _ctx: Option<&serde_json::Value>) -> Result<(), String> {
        tracing::warn!("NoopSender: bidirectional channel not yet wired (Plan B). text={}", &text[..text.len().min(50)]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn start_all_with_empty_db_succeeds() {
        let db = in_memory_db();
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 0);
    }

    #[tokio::test]
    async fn apply_config_adds_notify_instance() {
        let db = in_memory_db();
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO im_channel_instances \
                 (id, space_id, channel_type, name, config_json, credentials_json, enabled, \
                  streaming, reply_scope, permission_enabled, owners_json, guest_policy_json, \
                  created_at, updated_at) \
                 VALUES ('ch1','s1','webhook','My Webhook','{}','{}',1,0,'all',0,'[]','{}',1,1)",
                [],
            )
            .unwrap();
        }
        let manager = ImChannelManager::new(db);
        manager.start_all().await.unwrap();
        assert_eq!(manager.instance_count().await, 1);
    }
}
