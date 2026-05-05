//! Notification channel system for distributing messages.
//!
//! Supports multiple notification backends: webhook, email, IM channels.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Channel type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    WeChat,
    DingTalk,
    Feishu,
    Custom,
}

/// Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub id: String,
    pub name: String,
    pub channel_type: ChannelType,
    pub enabled: bool,
    /// Webhook URL for webhook channels
    pub webhook_url: Option<String>,
    /// Additional config as JSON
    pub config: Option<serde_json::Value>,
}

/// Notification payload to send through channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelNotification {
    pub title: String,
    pub body: String,
    pub level: String,
    pub metadata: Option<serde_json::Value>,
}

/// Abstract channel sender trait
#[async_trait::async_trait]
pub trait ChannelSender: Send + Sync {
    async fn send(&self, notification: &ChannelNotification, config: &ChannelConfig) -> Result<(), String>;
    fn name(&self) -> &str;
}

/// Webhook channel sender
pub struct WebhookSender {
    client: reqwest::Client,
}

impl WebhookSender {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl ChannelSender for WebhookSender {
    async fn send(&self, notification: &ChannelNotification, config: &ChannelConfig) -> Result<(), String> {
        let url = config.webhook_url.as_ref()
            .ok_or_else(|| "No webhook URL configured".to_string())?;

        self.client
            .post(url)
            .json(notification)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Webhook send failed: {}", e))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "webhook"
    }
}

/// Channel manager
pub struct ChannelManager {
    channels: HashMap<String, (ChannelConfig, bool)>,
    senders: HashMap<String, Box<dyn ChannelSender>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        let mut manager = Self {
            channels: HashMap::new(),
            senders: HashMap::new(),
        };
        // Register built-in senders
        manager.register_sender("webhook", Box::new(WebhookSender::new()));
        manager
    }

    /// Register a channel sender
    pub fn register_sender(&mut self, kind: &str, sender: Box<dyn ChannelSender>) {
        self.senders.insert(kind.to_string(), sender);
    }

    /// Add a channel
    pub fn add_channel(&mut self, config: ChannelConfig) {
        self.channels.insert(config.id.clone(), (config, true));
    }

    /// Remove a channel
    pub fn remove_channel(&mut self, id: &str) -> Option<ChannelConfig> {
        self.channels.remove(id).map(|(c, _)| c)
    }

    /// Enable/disable a channel
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some((config, _)) = self.channels.get_mut(id) {
            config.enabled = enabled;
            return true;
        }
        false
    }

    /// Send notification through all enabled channels
    pub async fn broadcast(&self, notification: &ChannelNotification) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();
        for (id, (config, _)) in &self.channels {
            if !config.enabled {
                continue;
            }
            let sender_key = match config.channel_type {
                ChannelType::Webhook => "webhook",
                ChannelType::Email => "email",
                ChannelType::WeChat => "wechat",
                ChannelType::DingTalk => "dingtalk",
                ChannelType::Feishu => "feishu",
                ChannelType::Custom => "custom",
            };
            if let Some(sender) = self.senders.get(sender_key) {
                let result = sender.send(notification, config).await;
                results.push((id.clone(), result));
            }
        }
        results
    }

    /// List all channels
    pub fn list(&self) -> Vec<&ChannelConfig> {
        self.channels.values().map(|(c, _)| c).collect()
    }
}
