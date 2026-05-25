use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::hook_bus::{HookBus, HookEvent};
use crate::runtime::contracts::HookDecision;

use super::receipts::build_receipt;
use super::targets::memory_graph::MemoryGraphPolicyTarget;
use super::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use super::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt, MemoryPolicyReasonCode,
    MemoryPolicyReceiptStatus, MemoryPolicyTarget,
};

#[derive(Debug, thiserror::Error)]
pub enum MemoryPolicyExecutorError {
    #[error("target error: {0}")]
    Target(#[from] MemoryPolicyTargetError),
}

pub struct MemoryPolicyExecutor {
    hook_bus: HookBus,
    gbrain: Arc<dyn MemoryPolicyTargetAdapter>,
    memu: Arc<dyn MemoryPolicyTargetAdapter>,
    browser_artifact: Arc<dyn MemoryPolicyTargetAdapter>,
    memory_graph: Arc<dyn MemoryPolicyTargetAdapter>,
}

impl MemoryPolicyExecutor {
    pub fn new(
        hook_bus: HookBus,
        gbrain: Arc<dyn MemoryPolicyTargetAdapter>,
        memu: Arc<dyn MemoryPolicyTargetAdapter>,
        browser_artifact: Arc<dyn MemoryPolicyTargetAdapter>,
    ) -> Self {
        Self {
            hook_bus,
            gbrain,
            memu,
            browser_artifact,
            memory_graph: Arc::new(MemoryGraphPolicyTarget),
        }
    }

    pub fn with_real_gbrain_and_artifacts(
        hook_bus: HookBus,
        gbrain_mcp: crate::mcp::SharedMcpManager,
        artifact_root: impl AsRef<std::path::Path>,
        memu: Arc<dyn MemoryPolicyTargetAdapter>,
    ) -> Self {
        Self::new(
            hook_bus,
            Arc::new(crate::memory_policy::targets::gbrain::GbrainPolicyTarget::new(gbrain_mcp)),
            memu,
            Arc::new(
                crate::memory_policy::targets::browser_artifact::BrowserArtifactPolicyTarget::new(
                    artifact_root,
                ),
            ),
        )
    }

    pub fn for_tests_allow_all() -> Self {
        Self::new(
            HookBus::new(),
            Arc::new(FakeTarget::succeeded("gbrain")),
            Arc::new(
                crate::memory_policy::targets::memu::MemuPolicyTarget::unavailable_for_tests(),
            ),
            Arc::new(FakeTarget::succeeded("browser_artifact")),
        )
    }

    pub fn for_tests_deny_all() -> Self {
        let mut bus = HookBus::new();
        bus.register(Arc::new(DenyMemoryWrites)).unwrap();
        Self::new(
            bus,
            Arc::new(FakeTarget::succeeded("gbrain")),
            Arc::new(
                crate::memory_policy::targets::memu::MemuPolicyTarget::unavailable_for_tests(),
            ),
            Arc::new(FakeTarget::succeeded("browser_artifact")),
        )
    }

    pub async fn execute(
        &mut self,
        decision: MemoryPolicyDecision,
    ) -> Result<Vec<MemoryPolicyExecutionReceipt>, MemoryPolicyExecutorError> {
        let mut receipts = Vec::new();
        for action in &decision.actions {
            if action.kind.is_write() && action.target != MemoryPolicyTarget::MemoryGraph {
                match self.gate_write(&decision, action).await {
                    HookDecision::Deny { .. } => {
                        receipts.push(build_receipt(
                            &decision,
                            action,
                            MemoryPolicyReceiptStatus::Rejected,
                            Some(MemoryPolicyReasonCode::PolicyDenied),
                            Some(format!("memory-policy://rejected/{}", action.action_id)),
                            None,
                            None,
                        ));
                        continue;
                    }
                    HookDecision::AskUser { .. } => {
                        receipts.push(build_receipt(
                            &decision,
                            action,
                            MemoryPolicyReceiptStatus::Deferred,
                            Some(MemoryPolicyReasonCode::ApprovalRequired),
                            Some(format!("memory-policy://deferred/{}", action.action_id)),
                            None,
                            None,
                        ));
                        continue;
                    }
                    HookDecision::Allow => {}
                }
            }
            receipts.push(self.target_for(action).execute(&decision, action).await?);
        }
        Ok(receipts)
    }

    async fn gate_write(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> HookDecision {
        let event = HookEvent::MemoryWrite {
            task_id: decision.input.task_id.clone(),
            topic: action.topic.clone(),
            size_bytes: action.size_bytes,
        };
        self.hook_bus.dispatch_with_decision(&event).await
    }

    fn target_for(&self, action: &MemoryPolicyAction) -> &dyn MemoryPolicyTargetAdapter {
        match action.target {
            MemoryPolicyTarget::Gbrain => self.gbrain.as_ref(),
            MemoryPolicyTarget::Memu => self.memu.as_ref(),
            MemoryPolicyTarget::BrowserArtifact => self.browser_artifact.as_ref(),
            MemoryPolicyTarget::MemoryGraph => self.memory_graph.as_ref(),
        }
    }
}

#[derive(Debug)]
struct FakeTarget {
    target_ref: String,
}

impl FakeTarget {
    fn succeeded(target_ref: impl Into<String>) -> Self {
        Self {
            target_ref: target_ref.into(),
        }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for FakeTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Succeeded,
            None,
            Some(format!("memory-policy://receipt/{}", action.action_id)),
            Some(self.target_ref.clone()),
            None,
        ))
    }
}

struct DenyMemoryWrites;

#[async_trait]
impl crate::agent::hook_bus::HookSubscriber for DenyMemoryWrites {
    fn id(&self) -> crate::agent::hook_bus::SubscriberId {
        crate::agent::hook_bus::SubscriberId::new("deny-memory-writes")
    }

    fn interest_in(&self) -> &'static [crate::agent::hook_bus::HookEventKind] {
        &[crate::agent::hook_bus::HookEventKind::MemoryWrite]
    }

    async fn on_event(&self, _event: &HookEvent) -> Option<HookDecision> {
        Some(HookDecision::Deny {
            reason: "test denial".into(),
        })
    }
}
