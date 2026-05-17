// iLink HTTP long-poll sender — implemented in Task 4.
use crate::channels::types::{ImChannelSender, InboundMessage, ReplyHandle};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

pub struct IlinkSender;

impl IlinkSender {
    pub fn new(_instance_id: &str, _config: &Value, _credentials: &Value) -> Self { Self }

    pub fn start(
        self: Arc<Self>,
        _inbound_tx: mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
    ) -> AbortHandle {
        tokio::spawn(async {}).abort_handle()
    }
}

#[async_trait]
impl ImChannelSender for IlinkSender {
    async fn send_text(&self, _chat_id: &str, _text: &str, _ctx: Option<&Value>) -> Result<(), String> {
        Ok(())
    }
}
