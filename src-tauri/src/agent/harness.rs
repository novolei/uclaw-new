use crate::agent::hook_bus::HookBus;
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, ReasoningContext};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use crate::agent::run_assembly::{
    AgentRunConfig as AgentHarnessRunConfig, AgentRunOutcome as AgentHarnessRunOutcome,
};

pub async fn run_agent_harness(
    delegate: &dyn LoopDelegate,
    ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    token: CancellationToken,
    hook_bus: Arc<HookBus>,
    run_config: AgentHarnessRunConfig,
) -> AgentHarnessRunOutcome {
    crate::agent::run_assembly::run_agent(crate::agent::run_assembly::AgentRunAssembly {
        delegate,
        ctx,
        config,
        token,
        hook_bus,
        run_config,
    })
    .await
}
