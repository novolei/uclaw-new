pub mod action;
pub mod action_registry;
pub mod agent_loop;
pub mod boundary;
pub mod context;
pub mod context_manager;
pub mod decision;
pub mod dom_state;
pub mod hosted_provider;
pub mod identity;
pub mod identity_authorization;
pub mod identity_ipc;
pub mod identity_tasks;
pub mod intervention_bridge;
pub mod loop_detector; // stub — full implementation in Plan 2 Task 15
pub mod memory_adapter;
pub mod observation;
pub mod perception;
pub mod playwright_cli;
pub mod playwright_cli_adapter;
pub mod playwright_discovery;
pub mod playwright_mcp;
pub mod playwright_mcp_adapter;
pub mod playwright_setup;
pub mod playwright_skills;
pub mod provider;
pub mod provider_defaults;
pub mod provider_execution;
pub mod recipes;
pub mod recovery;
pub mod runtime_contracts;
pub mod runtime_control_center;
pub mod runtime_execution;
pub mod runtime_memory_policy;
#[cfg(test)]
mod runtime_memory_policy_tests;
pub mod runtime_pack;
pub mod runtime_pack_ipc;
pub mod runtime_pack_runner;
pub mod runtime_pack_source;
pub mod runtime_provider_probe;
pub mod runtime_status;
pub mod runtime_supervisor;
pub mod script_runner;
pub mod session_state;
pub mod task_store;
pub mod tools;
pub mod types;

// M1-T4c — bridge BrowserTaskRun to runtime::contracts::TaskEvent.
pub mod rollout_bridge;
// Re-export the two primary public types so callers can write
// `crate::browser::BrowserContextManager` without the extra path.
pub use context_manager::BrowserContextManager;
pub use playwright_cli::{
    build_playwright_cli_request_envelope, playwright_cli_capabilities,
    playwright_cli_provider_status, run_playwright_cli_child_worker, PlaywrightCliAction,
    PlaywrightCliActionKind, PlaywrightCliAddress, PlaywrightCliChildWorkerConfig,
    PlaywrightCliEnvelopeError, PlaywrightCliRequestEnvelope, PlaywrightCliRuntimeEnv,
    PlaywrightCliWorkerError, PlaywrightCliWorkerErrorEnvelope, PlaywrightCliWorkerResultEnvelope,
    PlaywrightCliWorkerStatus, DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
    PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS, PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION,
    PLAYWRIGHT_CLI_PROVIDER_ID,
};
pub use playwright_cli_adapter::{
    PlaywrightCliActionCommand, PlaywrightCliAdapterError, PlaywrightCliCommand,
};
pub use playwright_discovery::{
    inspect_playwright_system, inspect_playwright_system_with_detector,
    ExperimentalNodeBootstrapStatus, PlaywrightCommandDetector, PlaywrightCommandStatus,
    PlaywrightSystemDiscoveryReport, PlaywrightSystemStatus, SystemPlaywrightCommandDetector,
};
pub use playwright_mcp::{
    playwright_mcp_capabilities, playwright_mcp_provider_result_from_adapter_call,
    playwright_mcp_provider_status, PlaywrightMcpAction, PlaywrightMcpActionKind,
    PlaywrightMcpProviderArtifactKind, PlaywrightMcpProviderArtifactRef,
    PlaywrightMcpProviderExecutionError, PlaywrightMcpProviderExecutionResult,
    PlaywrightMcpProviderExecutionStatus, PLAYWRIGHT_MCP_PACKAGE_NAME, PLAYWRIGHT_MCP_PROVIDER_ID,
    PLAYWRIGHT_MCP_UCLAW_ACTIONS,
};
pub use playwright_mcp_adapter::{
    validate_playwright_mcp_tool, PlaywrightMcpAdapterError, PlaywrightMcpAdapterToolCall,
    PLAYWRIGHT_MCP_SERVER_ID,
};
pub use playwright_setup::{
    execute_playwright_setup_plan_with_runner, plan_playwright_setup, PlaywrightSetupAction,
    PlaywrightSetupCommandRunner, PlaywrightSetupCommandStep, PlaywrightSetupExecutionReport,
    PlaywrightSetupExecutionStatus, PlaywrightSetupPlan, PlaywrightSetupStepExecutionReport,
    PlaywrightSetupStepExecutionStatus, SystemPlaywrightSetupCommandRunner,
};
pub use provider::{
    local_chromium_capabilities, local_chromium_status, BrowserCapabilityProbe, BrowserProbeStatus,
    BrowserProviderCapabilities, BrowserProviderReadiness, BrowserProviderReadinessProbe,
    BrowserProviderStatus, BrowserSetupCheck, LOCAL_CHROMIUM_PROVIDER_ID,
};
pub use runtime_contracts::{
    browser_provider_capability_card, browser_provider_capability_cards, browser_task_event_names,
    is_allowed_browser_runtime_transition, BrowserIdentityMode, BrowserIdentityProjection,
    BrowserProviderCapabilityCard, BrowserProviderLane, BrowserRuntimeFeatureFlags,
    BrowserRuntimeProjection, BrowserRuntimeState, BrowserRuntimeTransition,
    BrowserStartupDoctorProjection, BrowserTaskBoundaryProjection, BrowserTaskBoundaryStatus,
    BrowserTaskEventName, BrowserWorldProjectionSummary, StartupDoctorStatus,
};
pub use runtime_control_center::{
    BrowserRuntimeActiveProviderRoute, BrowserRuntimeControlCenterReport,
    BrowserRuntimeMcpIntegrationSummary, BrowserRuntimeProviderConfig, BrowserRuntimeProviderLane,
    BrowserRuntimeRouteRole,
};
pub use runtime_execution::{
    BrowserRuntimeActionBlocked, BrowserRuntimeActionExecutionOutcome,
    BrowserRuntimeActionExecutor, BrowserRuntimeActionRequest,
};
pub use runtime_pack::{
    decide_runtime_pack_update, diagnose_runtime_pack, execute_runtime_pack_plan_dry_run,
    execute_runtime_pack_plan_with_runner, inspect_runtime_pack_status, load_runtime_pack_manifest,
    plan_runtime_pack_operation, probe_runtime_pack_filesystem, BrowserRuntimePackAction,
    BrowserRuntimePackDoctorOutcome, BrowserRuntimePackDoctorStatus, BrowserRuntimePackEnvVar,
    BrowserRuntimePackExecutionMode, BrowserRuntimePackExecutionReport,
    BrowserRuntimePackExecutionStatus, BrowserRuntimePackExecutorPolicy,
    BrowserRuntimePackFilesystemProbeOptions, BrowserRuntimePackFilesystemProbeReport,
    BrowserRuntimePackFilesystemSnapshot, BrowserRuntimePackIssue, BrowserRuntimePackManifest,
    BrowserRuntimePackManifestLoadOutcome, BrowserRuntimePackManifestLoadStatus,
    BrowserRuntimePackNetworkState, BrowserRuntimePackOperation, BrowserRuntimePackOperationPlan,
    BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths, BrowserRuntimePackPlanStatus,
    BrowserRuntimePackPlanStep, BrowserRuntimePackPlanStepKind, BrowserRuntimePackPlanTrigger,
    BrowserRuntimePackProbe, BrowserRuntimePackReleaseChannel, BrowserRuntimePackStatusReport,
    BrowserRuntimePackStatusRequest, BrowserRuntimePackStepExecutionReport,
    BrowserRuntimePackStepExecutionStatus, BrowserRuntimePackStepRunOutcome,
    BrowserRuntimePackStepRunner, BrowserRuntimePackUpdateDecision, BrowserRuntimePackUpdateKind,
    BrowserRuntimePackUpdatePolicy,
};
pub use runtime_pack_runner::BrowserRuntimePackLocalStepRunner;
pub use runtime_pack_source::{
    validate_runtime_pack_source, BrowserRuntimePackSourceKind, BrowserRuntimePackSourceResolution,
    BrowserRuntimePackSourceResolutionStatus, BrowserRuntimePackSourceResolver,
};
pub use runtime_provider_probe::{
    append_probe_history, BrowserRuntimeProviderProbeState, BrowserRuntimeProviderProbeSummary,
};
pub use runtime_status::{
    compose_browser_runtime_status, BrowserRuntimeProviderReadinessSummary,
    BrowserRuntimeStatusReport, BrowserRuntimeStatusService, BrowserRuntimeSupervisorStatus,
};
pub use runtime_supervisor::{
    BrowserRuntimeArtifactPack, BrowserRuntimeDeadlineProfile, BrowserRuntimeDegradation,
    BrowserRuntimeDoctorOutcome, BrowserRuntimeSessionSummary, BrowserRuntimeSupervisor,
};
pub use types::{DOMState, ScreencastFramePayload};
