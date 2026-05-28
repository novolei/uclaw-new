//! Slash command (in-session, NOT Tauri command) registration shape.
//!
//! A `Command` is what shows up when a user types `/something` in the chat.
//! Distinct from Tauri commands (IPC entries in `tauri::generate_handler!`).

use std::sync::Arc;
use futures::future::BoxFuture;

pub type CommandHandlerFn = Arc<
    dyn Fn(serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value, String>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub handler: CommandHandlerFn,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("handler", &"<fn>")
            .finish()
    }
}
