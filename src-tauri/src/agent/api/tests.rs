//! Unit tests for AgentApi.

use super::*;
use crate::agent::tools::tool::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;

#[test]
fn new_agent_api_has_empty_registries() {
    let api = AgentApi::new();
    assert_eq!(api.tools.len(), 0);
    assert_eq!(api.providers.len(), 0);
    assert_eq!(api.commands.len(), 0);
    assert_eq!(api.renderers.len(), 0);
    assert_eq!(api.hooks.len(), 0);
    assert_eq!(api.plugin_index.len(), 0);
}

/// Minimal dummy Tool impl for the tests in this module.
struct DummyTool {
    name: String,
}

#[async_trait]
impl Tool for DummyTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "dummy tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
    }
}

#[test]
fn register_tool_stores_by_name() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    assert_eq!(api.tools.len(), 1);
    assert!(api.tools.contains_key("echo"));
}

#[test]
fn tool_query_returns_registered_tool() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    let got = api.tool("echo");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name(), "echo");
    assert!(api.tool("nonexistent").is_none());
}

#[test]
fn register_provider_stores_by_id() {
    let mut api = AgentApi::new();
    let provider = std::sync::Arc::new(make_test_provider_service().unwrap());
    api.register_provider("openai".to_string(), provider);
    assert_eq!(api.providers.len(), 1);
    assert!(api.providers.contains_key("openai"));
}

#[test]
fn provider_query_returns_registered() {
    let mut api = AgentApi::new();
    let provider = std::sync::Arc::new(make_test_provider_service().unwrap());
    api.register_provider("openai".to_string(), provider);
    assert!(api.provider("openai").is_some());
    assert!(api.provider("nonexistent").is_none());
}

/// Helper to construct a ProviderService for tests.
/// Uses a temporary directory so file I/O succeeds without side effects.
fn make_test_provider_service() -> Result<crate::providers::service::ProviderService, crate::error::Error> {
    let temp_dir = tempfile::tempdir().map_err(|e| {
        crate::error::Error::Internal(format!("Failed to create temp dir: {e}"))
    })?;
    crate::providers::service::ProviderService::new(temp_dir.path())
}

#[test]
fn register_command_stores_by_name() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    let cmd = crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({"out": "hello"})) }.boxed()
        }),
    };
    api.register_command(cmd);
    assert_eq!(api.commands.len(), 1);
}

#[test]
fn command_query_returns_registered() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({})) }.boxed()
        }),
    });
    assert!(api.command("hello").is_some());
    assert!(api.command("missing").is_none());
}

#[test]
fn register_renderer_stores_by_custom_type() {
    let mut api = AgentApi::new();
    let r = crate::agent::api::renderer::Renderer {
        custom_type: "echo.detail",
        render: std::sync::Arc::new(|v| Ok(format!("rendered: {}", v))),
    };
    api.register_renderer(r);
    assert_eq!(api.renderers.len(), 1);
    assert!(api.renderers.contains_key("echo.detail"));
}

#[test]
fn renderer_query_returns_registered() {
    let mut api = AgentApi::new();
    api.register_renderer(crate::agent::api::renderer::Renderer {
        custom_type: "echo.detail",
        render: std::sync::Arc::new(|v| Ok(format!("rendered: {}", v))),
    });
    let r = api.renderer("echo.detail");
    assert!(r.is_some());
    let out = r.unwrap()(&serde_json::json!({"x": 1})).unwrap();
    assert!(out.starts_with("rendered:"));
    assert!(api.renderer("missing").is_none());
}

use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn on_registers_hook_and_emit_fires_it() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let counter = std::sync::Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let ev = Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t1".into(), duration_ms: 0 },
        session_id: "s1".into(),
        cancellation_token: CancellationToken::new(),
    };

    let outcome = api.emit(ev).await.unwrap();
    assert!(matches!(outcome, EventOutcome::Continue));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn hooks_fire_in_registration_order() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    let o = order.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let o = o.clone();
        async move {
            o.lock().unwrap().push(1);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });
    let o = order.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let o = o.clone();
        async move {
            o.lock().unwrap().push(2);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let _ = api.emit(Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t".into(), duration_ms: 0 },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    }).await.unwrap();

    assert_eq!(*order.lock().unwrap(), vec![1, 2]);
}

#[tokio::test]
async fn emit_short_circuits_on_abort() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let saw_second = std::sync::Arc::new(AtomicUsize::new(0));

    api.on(EventKind::TurnEnd, |_ev| {
        async move { Ok(EventOutcome::Abort("nope".into())) }.boxed()
    });
    let s = saw_second.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let s = s.clone();
        async move {
            s.fetch_add(1, Ordering::SeqCst);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let outcome = api.emit(Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t".into(), duration_ms: 0 },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    }).await.unwrap();

    assert!(matches!(outcome, EventOutcome::Abort(ref msg) if msg == "nope"));
    assert_eq!(saw_second.load(Ordering::SeqCst), 0, "second hook must not fire after Abort");
}
