use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftInterruptSource {
    User,
    System,
    Automation,
    BackgroundTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftInterruptMessage {
    pub source: SoftInterruptSource,
    pub content: String,
    pub urgent: bool,
}

impl SoftInterruptMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(SoftInterruptSource::User, content, false)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(SoftInterruptSource::System, content, false)
    }

    pub fn urgent_system(content: impl Into<String>) -> Self {
        Self::new(SoftInterruptSource::System, content, true)
    }

    pub fn new(source: SoftInterruptSource, content: impl Into<String>, urgent: bool) -> Self {
        Self {
            source,
            content: content.into(),
            urgent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftInterruptDrain {
    pub messages: Vec<SoftInterruptMessage>,
    pub urgent_count: usize,
    pub total_content_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SoftInterruptQueue {
    messages: Arc<Mutex<VecDeque<SoftInterruptMessage>>>,
}

impl SoftInterruptQueue {
    pub fn push(&self, message: SoftInterruptMessage) -> usize {
        let mut messages = self.lock_messages();
        messages.push_back(message);
        messages.len()
    }

    pub fn len(&self) -> usize {
        self.lock_messages().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock_messages().is_empty()
    }

    pub fn has_urgent(&self) -> bool {
        self.lock_messages().iter().any(|message| message.urgent)
    }

    pub fn snapshot(&self) -> Vec<SoftInterruptMessage> {
        self.lock_messages().iter().cloned().collect()
    }

    pub fn clear(&self) -> usize {
        let mut messages = self.lock_messages();
        let removed = messages.len();
        messages.clear();
        removed
    }

    pub fn drain(&self) -> SoftInterruptDrain {
        let messages: Vec<_> = self.lock_messages().drain(..).collect();
        let urgent_count = messages.iter().filter(|message| message.urgent).count();
        let total_content_bytes = messages.iter().map(|message| message.content.len()).sum();

        SoftInterruptDrain {
            messages,
            urgent_count,
            total_content_bytes,
        }
    }

    fn lock_messages(&self) -> std::sync::MutexGuard<'_, VecDeque<SoftInterruptMessage>> {
        self.messages
            .lock()
            .expect("soft interrupt queue mutex poisoned")
    }
}

#[cfg(test)]
#[path = "interrupts_tests.rs"]
mod tests;
