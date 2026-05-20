pub mod douyin;

use async_trait::async_trait;

use super::types::LiveComment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentBatch {
    pub next_cursor: Option<String>,
    pub comments: Vec<LiveComment>,
}

#[async_trait]
pub trait LiveRoomAdapter: Send + Sync {
    fn platform(&self) -> &'static str;
}
