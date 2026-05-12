/**
 * Tauri Bridge — 统一 IPC 适配层
 *
 * 提供与 Proma 的 window.electronAPI 相同的接口签名，
 * 内部使用 Tauri invoke / listen 实现。
 * 供 Jotai atoms 和业务层调用。
 *
 * 覆盖所有 uClaw 后端 #[tauri::command] 命令与事件。
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { open as openShell } from '@tauri-apps/plugin-shell';
import type {
  Settings,
  PatchSettingsInput,
  PlatformInfo,
  VersionInfo,
  BootstrapStatus,
  ConversationResponse,
  CreateConversationInput,
  SendMessageInput,
  SendMessageResponse,
  GetMessagesInput,
  Message,
  SpaceSummary,
  CreateSpaceInput,
  LlmConfigInput,
  LlmConfigResponse,
  ArtifactNode,
  ArtifactContentResponse,
  ArtifactTreeNodeResponse,
  ListArtifactTreeInput,
  LoadArtifactChildrenInput,
  CreateArtifactInput,
  RenameArtifactInput,
  MoveArtifactInput,
  DetectFileTypeResponse,
  ToggleStarResponse,
  SearchInput,
  SearchResult,
  ProviderInfo,
  ProviderConfigInput,
  ProviderConfigResponse,
  ProviderConfigureInput,
  ModelInfo,
  ModelSelectionInfo,
  TestResultInfo,
  ListModelsInput,
  TestConnectionInput,
  ApproveToolCallInput,
  LearnedSkill,
  StreamTextDelta,
  StreamToolStart,
  StreamToolResult,
  StreamThinking,
  StreamThinkingDone,
  StreamDone,
  StreamError,
  TurnCost,
  ContextStats,
  ApprovalRequest,
  // Notifications
  NotificationItem,
  // Background Tasks
  BackgroundTask,
  // Memory KV
  MemorySetInput,
  MemoryGetInput,
  MemorySearchInput,
  MemoryListInput,
  MemoryClearInput,
  MemoryBulkImportInput,
  MemoryEntryResponse,
  MemoryBulkImportResponse,
  MemoryClearResponse,
  // MCP
  McpServerInfo,
  McpServerInput,
  // Built-in Skills
  SkillInfo,
  SkillToggleInput,
  SkillMatchInput,
  SkillMatchResult,
  SkillDetailResponse,
  // Channels
  ChannelInput,
  // Safety
  SafetyPolicyResponse,
  SetSafetyModeInput,
  SetToolOverrideInput,
  ToolNameInput,
  AssessCommandInput,
  CommandRiskResponse,
  // Memory Graph CRUD
  MemoryGraphSearchInput,
  MemoryGraphGetNodeInput,
  MemoryGraphListBootInput,
  MemoryGraphManageBootInput,
  MemoryGraphTimelineInput,
  MemoryGraphExplainRecallInput,
  MemoryGraphCreateNodeInput,
  MemoryGraphUpdateNodeInput,
  MemoryGraphDeleteNodeInput,
  // Cost dashboard
  DailyCostRollup,
  ModelCostRollup,
  SessionCostRollup,
  WorkspaceCostRollup,
  BudgetThresholdPayload,
  // Permission rules
  PermissionRule,
  PermissionAuditEntry,
  CreatePermissionRuleInput,
  DefaultPromptsResponse,
} from './types';
import type { Channel } from './chat-types';
import type { RecentThread, AskUserRequest, ExitPlanModeRequest } from './agent-types';
import type { MountRoot } from '@/atoms/files-rail-atoms';
import type { TreeNode } from '@/components/files-rail/utils/tree-patch';

// ─────────────────────────────────────────────────────────
// Bootstrap / Settings
// ─────────────────────────────────────────────────────────

export const getSettings = (): Promise<Settings> =>
  invoke('get_settings');

export const patchSettings = (input: PatchSettingsInput): Promise<Settings> =>
  invoke('patch_settings', { input });

export const getPlatform = (): Promise<PlatformInfo> =>
  invoke('get_platform');

export const getVersion = (): Promise<VersionInfo> =>
  invoke('get_version');

export const getBootstrapStatus = (): Promise<BootstrapStatus> =>
  invoke('get_bootstrap_status');

// ─────────────────────────────────────────────────────────
// Conversations
// ─────────────────────────────────────────────────────────

export const listConversations = (): Promise<ConversationResponse[]> =>
  invoke('list_conversations');

export const createConversation = (input: CreateConversationInput): Promise<ConversationResponse> =>
  invoke('create_conversation', { input });

export const deleteConversation = (id: string): Promise<boolean> =>
  invoke('delete_conversation', { id });

export const getMessages = (input: GetMessagesInput): Promise<Message[]> =>
  invoke('get_messages', { input });

export const sendMessage = (input: SendMessageInput): Promise<SendMessageResponse> =>
  invoke('send_message', { input });

export const approveToolCall = (input: ApproveToolCallInput): Promise<{ success: boolean }> =>
  invoke('approve_tool_call', { input });

export const toggleStarConversation = (conversationId: string): Promise<ToggleStarResponse> =>
  invoke('toggle_star_conversation', { input: { conversationId } });

// ─────────────────────────────────────────────────────────
// Shared workspace types
// ─────────────────────────────────────────────────────────

export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
  createdAt: string
  updatedAt: string
}

export interface TurnRecord {
  id: string
  sessionId: string
  turnIndex: number
  role: string
  content?: string
  toolName?: string
  toolArgs?: string
  toolResult?: string
  reasoning?: string
  isError: boolean
  durationMs: number
  createdAt: number
}

export interface TrajectorySearchHit {
  sessionId: string
  turnIndex: number
  toolName?: string
  snippet: string
  createdAt: number
}

export interface SessionTitleUpdate {
  sessionId: string
  title: string
  emoji: string
}

// ─────────────────────────────────────────────────────────
// Spaces
// ─────────────────────────────────────────────────────────

export const listSpaces = (): Promise<SpaceSummary[]> =>
  invoke('list_spaces');

export const listRecentThreads = (): Promise<RecentThread[]> =>
  invoke<RecentThread[]>('list_recent_threads')

export const createSpace = (input: CreateSpaceInput): Promise<SpaceSummary> =>
  invoke('create_space', { input });

export const deleteSpace = (id: string): Promise<boolean> =>
  invoke('delete_space', { id });

// ─── Workspace (active space management) ─────────────────────────────

export const getActiveWorkspaceId = (): Promise<string | null> =>
  invoke<string | null>('get_active_workspace_id')

export const setActiveWorkspaceId = (id: string): Promise<void> =>
  invoke('set_active_workspace_id', { id })

export const createWorkspace = (name: string, path?: string, icon?: string): Promise<{
  id: string; name: string; icon: string; path: string | null; createdAt: string
}> =>
  invoke('create_workspace', { name, path: path ?? null, icon: icon ?? null })

export const deleteWorkspace = (id: string): Promise<void> =>
  invoke('delete_workspace', { id })

export const updateWorkspace = (input: { id: string; name?: string; icon?: string }): Promise<{
  id: string; name: string; icon: string; path: string | null; sortOrder: number; createdAt: string; updatedAt: string
}> => invoke('update_workspace', input)

export const reorderWorkspaces = (orderedIds: string[]): Promise<void> =>
  invoke('reorder_workspaces', { orderedIds })

export const getWorkspaceDirectories = (workspaceId: string): Promise<string[]> =>
  invoke('get_workspace_directories', { workspaceId })

export const attachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_workspace_directory', { workspaceId, dirPath })

export const detachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_workspace_directory', { workspaceId, dirPath })

export const listSessionDirectories = (sessionId: string): Promise<string[]> =>
  invoke('list_session_directories', { sessionId })

export const attachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_session_directory', { sessionId, dirPath })

export const detachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_session_directory', { sessionId, dirPath })

export const renameAttachedFile = (path: string, newName: string): Promise<string> =>
  invoke('rename_attached_file', { path, newName })

export const moveAttachedFile = (path: string, destDir: string): Promise<string> =>
  invoke('move_attached_file', { path, destDir })

export const readAttachedFile = (path: string): Promise<number[]> =>
  invoke('read_attached_file', { path })

export const uploadWorkspaceFile = (
  workspaceId: string,
  filename: string,
  content: number[],
): Promise<string> => invoke('upload_workspace_file', { workspaceId, filename, content })

export const pathIsDirectory = (path: string): Promise<boolean> =>
  invoke('path_is_directory', { path })

export const copyFileIntoWorkspace = (workspaceId: string, sourcePath: string): Promise<string> =>
  invoke('copy_file_into_workspace', { workspaceId, sourcePath })

/** Delete a single file by absolute path. Backend rejects directories. */
export const deleteWorkspaceFile = (path: string): Promise<void> =>
  invoke('delete_workspace_file', { path })

// Path policy (Phase 3)
export const listAlwaysAllowedPaths = (): Promise<string[]> =>
  invoke('list_always_allowed_paths')

export const addAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('add_always_allowed_path', { path })

export const removeAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('remove_always_allowed_path', { path })

export const listSessionAllowedPaths = (sessionId: string): Promise<string[]> =>
  invoke('list_session_allowed_paths', { sessionId })

export const promoteSessionPathToGlobal = (sessionId: string, path: string): Promise<void> =>
  invoke('promote_session_path_to_global', { sessionId, path })

// ─── Session title ────────────────────────────────────────────────────

export const generateSessionTitle = (sessionId: string, firstMessage: string): Promise<void> =>
  invoke('generate_session_title', { sessionId, firstMessage })

// ─── Trajectory ───────────────────────────────────────────────────────

export const getSessionTrajectory = (sessionId: string): Promise<TurnRecord[]> =>
  invoke('get_session_trajectory', { sessionId })

export const searchTrajectories = (query: string, limit?: number): Promise<TrajectorySearchHit[]> =>
  invoke('search_trajectories', { query, limit: limit ?? 20 })

// ─────────────────────────────────────────────────────────
// LLM Config
// ─────────────────────────────────────────────────────────

export const getLlmConfig = (): Promise<LlmConfigResponse> =>
  invoke('get_llm_config');

export const updateLlmConfig = (input: LlmConfigInput): Promise<LlmConfigResponse> =>
  invoke('update_llm_config', { input });

// ─────────────────────────────────────────────────────────
// Artifacts
// ─────────────────────────────────────────────────────────

export const listArtifacts = (): Promise<ArtifactNode[]> =>
  invoke('list_artifacts');

export const readArtifact = (path: string): Promise<ArtifactContentResponse> =>
  invoke('read_artifact', { input: { path } });

export const writeArtifact = (path: string, content: string): Promise<ArtifactContentResponse> =>
  invoke('write_artifact', { input: { path, content } });

export const deleteArtifact = (path: string): Promise<boolean> =>
  invoke('delete_artifact', { path });

export const listArtifactsTree = (input: ListArtifactTreeInput): Promise<ArtifactTreeNodeResponse[]> =>
  invoke('list_artifacts_tree', { input });

export const loadArtifactChildren = (input: LoadArtifactChildrenInput): Promise<ArtifactTreeNodeResponse[]> =>
  invoke('load_artifact_children', { input });

/** List immediate children of an arbitrary directory path. Used by the
 * Files tab's FileBrowser to show real disk contents under the workspace
 * folder. Hidden files filtered server-side. */
export const listDirectoryEntries = (path: string): Promise<Array<{
  name: string
  path: string
  isDirectory: boolean
  isFile: boolean
  size?: number
  extension?: string
}>> => invoke('list_directory_entries', { path });

export const createArtifact = (input: CreateArtifactInput): Promise<ArtifactTreeNodeResponse> =>
  invoke('create_artifact', { input });

export const renameArtifact = (input: RenameArtifactInput): Promise<boolean> =>
  invoke('rename_artifact', { input });

export const moveArtifact = (input: MoveArtifactInput): Promise<boolean> =>
  invoke('move_artifact', { input });

export const deleteArtifactRecursive = (spaceId: string, path: string): Promise<boolean> =>
  invoke('delete_artifact_recursive', { spaceId, path });

export const detectFileType = (path: string): Promise<DetectFileTypeResponse> =>
  invoke('detect_file_type', { path });

// ─────────────────────────────────────────────────────────
// Search
// ─────────────────────────────────────────────────────────

export const searchWorkspace = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_workspace', { input });

export const searchConversations = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_conversations', { input });

export const searchAll = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_all', { input });

// ─────────────────────────────────────────────────────────
// Providers
// ─────────────────────────────────────────────────────────

export const listProviders = (): Promise<ProviderInfo[]> =>
  invoke('list_providers');

export const listConfiguredProviders = (): Promise<string[]> =>
  invoke('list_configured_providers');

export const getProviderConfig = (providerId: string): Promise<ProviderConfigResponse | null> =>
  invoke('get_provider_config', { providerId });

export const configureProvider = (input: ProviderConfigInput): Promise<void> =>
  invoke('configure_provider', { input });

export const configureProviderWithModels = (input: ProviderConfigureInput): Promise<void> => {
  const { modelIds, ...config } = input;
  return invoke('configure_provider_with_models', { providerConfig: config, modelIds });
};

export const removeProviderConfig = (providerId: string): Promise<void> =>
  invoke('remove_provider_config', { providerId });

export const testProviderConnection = (input: TestConnectionInput): Promise<TestResultInfo> =>
  invoke('test_provider_connection', { input });

export const listProviderModels = (input: ListModelsInput): Promise<ModelInfo[]> =>
  invoke('list_provider_models', { input });

export const getConfiguredModels = (providerId: string): Promise<string[]> =>
  invoke('get_configured_models', { providerId });

export const getAllConfiguredModels = (): Promise<[string, string[]][]> =>
  invoke('get_all_configured_models');

export const getActiveModel = (): Promise<ModelSelectionInfo | null> =>
  invoke('get_active_model');

export const setActiveModel = (providerId: string, modelId: string): Promise<void> =>
  invoke('set_active_model', { providerId, modelId });

export interface ModelRoleConfig {
  role: string
  model_ref: string | null
}

export const getRoleModels = (): Promise<ModelRoleConfig[]> =>
  invoke('get_role_models');

export const setRoleModel = (role: string, modelRef: string | null): Promise<void> =>
  invoke('set_role_model', { role, modelRef });

// ─────────────────────────────────────────────────────────
// Learned Skills
// ─────────────────────────────────────────────────────────

export const listLearnedSkills = (spaceId: string = 'default'): Promise<LearnedSkill[]> =>
  invoke('list_learned_skills', { spaceId });

export const getLearnedSkill = (skillId: string): Promise<LearnedSkill> =>
  invoke('get_learned_skill', { skillId });

export const toggleLearnedSkill = (skillId: string, enabled: boolean): Promise<void> =>
  invoke('toggle_learned_skill', { skillId, enabled });

export const deleteLearnedSkill = (skillId: string): Promise<void> =>
  invoke('delete_learned_skill', { skillId });

/**
 * Record that the LLM cited a learned skill in its response.
 * Bumps `cited_count` in metadata (separate from `usage_count` /
 * `recalled_count`). Returns the matched skill_id, or null if the
 * cited title doesn't match any skill in the DB.
 */
export const recordSkillCited = (
  title: string,
  spaceId: string = 'default',
): Promise<string | null> =>
  invoke('record_skill_cited', { spaceId, title });

/**
 * Backfill `memory_keywords` rows for learned skills that lack them.
 * Idempotent: skills already indexed are skipped. Re-running is safe.
 */
export interface SkillKeywordBackfillResult {
  totalLearnedSkills: number;
  alreadyIndexed: number;
  backfilledSkills: number;
  keywordsInserted: number;
}
export const backfillSkillKeywords = (
  spaceId: string = 'default',
): Promise<SkillKeywordBackfillResult> =>
  invoke('backfill_skill_keywords', { spaceId });

// — Skill consolidation —

export interface SkillConsolidationCluster {
  canonicalId: string
  canonicalTitle: string
  mergedTitle: string
  mergedContext: string
  mergedPrinciples: string
  mergedSteps: string
  mergedPitfalls: string
  duplicateIds: string[]
  duplicateTitles: string[]
  reason: string
}

export interface SkillConsolidationProposal {
  clusters: SkillConsolidationCluster[]
  totalSkills: number
  proposedCanonicalCount: number
}

export interface SkillConsolidationResult {
  appliedClusters: number
  deprecatedSkills: number
  updatedSkills: number
}

export const proposeSkillConsolidation = (
  spaceId: string = 'default',
): Promise<SkillConsolidationProposal> =>
  invoke('propose_skill_consolidation', { spaceId });

export const applySkillConsolidation = (
  plan: SkillConsolidationProposal,
): Promise<SkillConsolidationResult> =>
  invoke('apply_skill_consolidation', { plan });

// — Skill version history —

export interface SkillVersionInfo {
  id: string;
  status: string;
  content: string;
  createdAt: string;
}

export const getSkillVersions = (nodeId: string): Promise<SkillVersionInfo[]> =>
  invoke<SkillVersionInfo[]>('get_skill_versions', { nodeId }).catch((e) => {
    console.error('[getSkillVersions]', e);
    return [];
  });

// ─────────────────────────────────────────────────────────
// Memory Graph
// ─────────────────────────────────────────────────────────

export const memoryGraphListBoot = (input: MemoryGraphListBootInput): Promise<unknown> =>
  invoke('memory_graph_list_boot', { input });

export const memoryGraphGetNode = (input: MemoryGraphGetNodeInput): Promise<unknown> =>
  invoke('memory_graph_get_node', { input });

export const memoryGraphSearch = (input: MemoryGraphSearchInput): Promise<unknown> =>
  invoke('memory_graph_search', { input });

export const memoryGraphGetFullGraph = (): Promise<unknown> =>
  invoke('memory_graph_get_full_graph');

export const memoryGraphManageBoot = (input: MemoryGraphManageBootInput): Promise<unknown> =>
  invoke('memory_graph_manage_boot', { input });

export const memoryGraphListTimeline = (input: MemoryGraphTimelineInput): Promise<unknown> =>
  invoke('memory_graph_list_timeline', { input });

export const memoryGraphExplainRecall = (input: MemoryGraphExplainRecallInput): Promise<unknown> =>
  invoke('memory_graph_explain_recall', { input });

export const memoryGraphCreateNode = (input: MemoryGraphCreateNodeInput): Promise<unknown> =>
  invoke('memory_graph_create_node', { input });

export const memoryGraphUpdateNode = (input: MemoryGraphUpdateNodeInput): Promise<unknown> =>
  invoke('memory_graph_update_node', { input });

export const memoryGraphDeleteNode = (input: MemoryGraphDeleteNodeInput): Promise<unknown> =>
  invoke('memory_graph_delete_node', { input });

// ─────────────────────────────────────────────────────────
// Notifications
// ─────────────────────────────────────────────────────────

export const getNotifications = (): Promise<NotificationItem[]> =>
  invoke('get_notifications');

export const clearNotifications = (): Promise<boolean> =>
  invoke('clear_notifications');

// ─────────────────────────────────────────────────────────
// Background Tasks
// ─────────────────────────────────────────────────────────

export const getBackgroundTasks = (): Promise<BackgroundTask[]> =>
  invoke('get_background_tasks');

// ─────────────────────────────────────────────────────────
// Memory KV Store
// ─────────────────────────────────────────────────────────

export const memorySet = (input: MemorySetInput): Promise<MemoryEntryResponse> =>
  invoke('memory_set', { input });

export const memoryGet = (input: MemoryGetInput): Promise<MemoryEntryResponse | null> =>
  invoke('memory_get', { input });

export const memoryDelete = (input: MemoryGetInput): Promise<boolean> =>
  invoke('memory_delete', { input });

export const memorySearch = (input: MemorySearchInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_search', { input });

export const memoryList = (input: MemoryListInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_list', { input });

export const memoryClearNamespace = (input: MemoryClearInput): Promise<MemoryClearResponse> =>
  invoke('memory_clear_namespace', { input });

export const memoryPruneExpired = (): Promise<MemoryClearResponse> =>
  invoke('memory_prune_expired');

export const memoryBulkImport = (input: MemoryBulkImportInput): Promise<MemoryBulkImportResponse> =>
  invoke('memory_bulk_import', { input });

export const memoryExport = (input: MemoryListInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_export', { input });

export const memoryListNamespaces = (spaceId?: string): Promise<string[]> =>
  invoke('memory_list_namespaces', { spaceId: spaceId ?? null });

// ─────────────────────────────────────────────────────────
// MCP Servers
// ─────────────────────────────────────────────────────────

export const listMcpServers = (): Promise<McpServerInfo[]> =>
  invoke('list_mcp_servers');

export const addMcpServer = (input: McpServerInput): Promise<McpServerInfo> =>
  invoke('add_mcp_server', { input });

export const removeMcpServer = (id: string): Promise<boolean> =>
  invoke('remove_mcp_server', { id });

export const toggleMcpServer = (id: string, enabled: boolean): Promise<boolean> =>
  invoke('toggle_mcp_server', { id, enabled });

export const connectMcpServer = (id: string): Promise<boolean> =>
  invoke('connect_mcp_server', { id });

export const disconnectMcpServer = (id: string): Promise<boolean> =>
  invoke('disconnect_mcp_server', { id });

export const restartMcpServer = (id: string): Promise<boolean> =>
  invoke('restart_mcp_server', { id });

export const listMcpTools = (): Promise<unknown[]> =>
  invoke('list_mcp_tools');

// ─────────────────────────────────────────────────────────
// Built-in Skills
// ─────────────────────────────────────────────────────────

export const listSkills = (): Promise<SkillInfo[]> =>
  invoke('list_skills');

export const toggleSkill = (input: SkillToggleInput): Promise<boolean> =>
  invoke('toggle_skill', { input });

export const discoverSkills = (): Promise<SkillInfo[]> =>
  invoke('discover_skills');

export const reloadSkills = (): Promise<SkillInfo[]> =>
  invoke('reload_skills');

export const getSkillDetail = (name: string): Promise<SkillDetailResponse> =>
  invoke('get_skill_detail', { name });

export const matchSkills = (input: SkillMatchInput): Promise<SkillMatchResult[]> =>
  invoke('match_skills', { input });

// ─────────────────────────────────────────────────────────
// Channels
// ─────────────────────────────────────────────────────────

export const listChannels = (): Promise<Channel[]> =>
  invoke('list_channels');

export const addChannel = (input: ChannelInput): Promise<Channel> =>
  invoke('add_channel', { input });

export const removeChannel = (id: string): Promise<boolean> =>
  invoke('remove_channel', { id });

export const toggleChannel = (id: string, enabled: boolean): Promise<boolean> =>
  invoke('toggle_channel', { id, enabled });

// ─────────────────────────────────────────────────────────
// Safety Policy
// ─────────────────────────────────────────────────────────

export const getSafetyPolicy = (): Promise<SafetyPolicyResponse> =>
  invoke('get_safety_policy');

export const setSafetyMode = (input: SetSafetyModeInput): Promise<SafetyPolicyResponse> =>
  invoke('set_safety_mode', { input });

export const setToolSafetyOverride = (input: SetToolOverrideInput): Promise<SafetyPolicyResponse> =>
  invoke('set_tool_safety_override', { input });

export const removeToolSafetyOverride = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('remove_tool_safety_override', { input });

export const addAutoApprovedTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('add_auto_approved_tool', { input });

export const removeAutoApprovedTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('remove_auto_approved_tool', { input });

export const blockTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('block_tool', { input });

export const unblockTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('unblock_tool', { input });

export const assessCommandRisk = (input: AssessCommandInput): Promise<CommandRiskResponse> =>
  invoke('assess_command_risk', { input });

// ─────────────────────────────────────────────────────────
// Services / Memubot
// ─────────────────────────────────────────────────────────

export const servicesHealth = (): Promise<unknown> =>
  invoke('services_health');

export const memorizationStatus = (): Promise<unknown> =>
  invoke('memorization_status');

export const proactiveStatus = (): Promise<unknown> =>
  invoke('proactive_status');

export const proactiveStart = (): Promise<void> =>
  invoke('proactive_start');

export const proactiveStop = (): Promise<void> =>
  invoke('proactive_stop');

export const metricsSummary = (): Promise<unknown> =>
  invoke('metrics_summary');

export const memubotConfigGet = (): Promise<unknown> =>
  invoke('memubot_config_get');

// ─────────────────────────────────────────────────────────
// Dev / Testing
// ─────────────────────────────────────────────────────────

export const triggerProactiveScenario = (scenarioName: string): Promise<unknown> =>
  invoke('trigger_proactive_scenario', { scenarioName });

// ─────────────────────────────────────────────────────────
// Event Listeners (Tauri global events)
// ─────────────────────────────────────────────────────────

// — Streaming events —

export const onTextDelta = (cb: (payload: StreamTextDelta) => void): Promise<UnlistenFn> =>
  listen('agent:text-delta', (e) => cb(e.payload as StreamTextDelta));

export const onToolStart = (cb: (payload: StreamToolStart) => void): Promise<UnlistenFn> =>
  listen('agent:tool-start', (e) => cb(e.payload as StreamToolStart));

export const onToolResult = (cb: (payload: StreamToolResult) => void): Promise<UnlistenFn> =>
  listen('agent:tool-result', (e) => cb(e.payload as StreamToolResult));

export const onDone = (cb: (payload: StreamDone) => void): Promise<UnlistenFn> =>
  listen('agent:done', (e) => cb(e.payload as StreamDone));

export const onError = (cb: (payload: StreamError) => void): Promise<UnlistenFn> =>
  listen('agent:error', (e) => cb(e.payload as StreamError));

export const onThinking = (cb: (payload: StreamThinking) => void): Promise<UnlistenFn> =>
  listen('agent:thinking', (e) => cb(e.payload as StreamThinking));

export const onThinkingDone = (cb: (payload: StreamThinkingDone) => void): Promise<UnlistenFn> =>
  listen('agent:thinking-done', (e) => cb(e.payload as StreamThinkingDone));

export const onTurnCost = (cb: (payload: TurnCost) => void): Promise<UnlistenFn> =>
  listen('agent:turn_cost', (e) => cb(e.payload as TurnCost));

// — Cost dashboard —

export const getDailyCosts = (daysBack = 30): Promise<DailyCostRollup[]> =>
  invoke<DailyCostRollup[]>('get_daily_costs', { daysBack });

export const getModelCosts = (daysBack = 30): Promise<ModelCostRollup[]> =>
  invoke<ModelCostRollup[]>('get_model_costs', { daysBack });

export const getSessionCosts = (daysBack = 30, limit = 50): Promise<SessionCostRollup[]> =>
  invoke<SessionCostRollup[]>('get_session_costs', { daysBack, limit });

export const listWorkspaceCostRollup = (sinceMs: number): Promise<WorkspaceCostRollup[]> =>
  invoke<WorkspaceCostRollup[]>('list_workspace_cost_rollup', { sinceMs });

export const getMonthCostTotal = (sinceMs: number): Promise<number> =>
  invoke<number>('get_month_cost_total', { sinceMs });

export const onBudgetThreshold = (cb: (payload: BudgetThresholdPayload) => void): Promise<UnlistenFn> =>
  listen('budget:threshold', (e) => cb(e.payload as BudgetThresholdPayload));

export const onContextStats = (cb: (payload: ContextStats) => void): Promise<UnlistenFn> =>
  listen('agent:context_stats', (e) => cb(e.payload as ContextStats));

// — System events —

export const onNeedApproval = (cb: (payload: ApprovalRequest) => void): Promise<UnlistenFn> =>
  listen('agent:need_approval', (e) => cb(e.payload as ApprovalRequest));

export const onArtifactTreeUpdate = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('artifact:tree_update', (e) => cb(e.payload));

// — Reflection events —

export const onReflectionUpdate = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('agent:reflection-update', (e) => cb(e.payload));

export const onProactiveLearning = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('agent:proactive-learning', (e) => cb(e.payload));

// ─────────────────────────────────────────────────────────
// Proma 兼容桩函数
// ─────────────────────────────────────────────────────────
// 以下函数为 Proma 迁移组件提供兼容性 API。
// 对于尚未在 Tauri 后端实现的命令，返回合理的桩数据。
// 随着后端逐步补全，桩函数将被替换为真实的 invoke 调用。

/* eslint-disable @typescript-eslint/no-explicit-any */

// --- Settings compat ---
export const updateSettings = (_patch: any): Promise<void> =>
  patchSettings(_patch).then(() => {})

// --- Conversation compat ---
export const updateConversationTitle = (id: string, title: string): Promise<any> =>
  invoke('update_conversation_title', { id, title }).catch(() => ({ id, title, updatedAt: Date.now() }))

export const togglePinConversation = (id: string): Promise<any> =>
  invoke('toggle_pin_conversation', { id }).catch(() => ({ id, pinned: true, updatedAt: Date.now() }))

export const updateConversationModel = (conversationId: string, modelId: string, channelId: string): Promise<any> =>
  invoke('update_conversation_model', { conversationId, modelId, channelId }).catch(() => ({ id: conversationId, modelId, channelId, updatedAt: Date.now() }))

export const getConversationMessages = (conversationId: string): Promise<any[]> =>
  getMessages({ conversationId }).then((msgs) => msgs as any[])

export const getRecentMessages = (conversationId: string, limit: number): Promise<{ messages: any[]; hasMore: boolean }> =>
  invoke<{ messages: any[]; hasMore: boolean }>('get_recent_messages', { conversationId, limit }).catch(() => getMessages({ conversationId }).then((msgs) => ({ messages: msgs as any[], hasMore: false })))

export const updateContextDividers = (conversationId: string, dividers: string[]): Promise<void> =>
  invoke<void>('update_context_dividers', { conversationId, dividers }).catch(() => {})

export const stopGeneration = (conversationId: string): Promise<void> =>
  invoke<void>('stop_generation', { conversationId }).catch(() => {})

export const truncateMessagesFrom = (conversationId: string, messageId: string, preserveFirstMessageAttachments?: boolean): Promise<any[]> =>
  invoke<any[]>('truncate_messages_from', { conversationId, messageId, preserveFirstMessageAttachments: preserveFirstMessageAttachments ?? false }).catch(() => [])

export const deleteMessage = (conversationId: string, messageId: string): Promise<any[]> =>
  invoke<any[]>('delete_message', { conversationId, messageId }).catch(() => [])

export const deleteAttachment = (localPath: string): Promise<void> =>
  invoke<void>('delete_attachment', { localPath }).catch(() => {})

export const saveAttachment = (input: any): Promise<any> =>
  invoke('save_attachment', { input })

export const readAttachment = (localPath: string): Promise<string> =>
  invoke<string>('read_attachment', { localPath })

export const getSystemPromptConfig = (): Promise<any> =>
  invoke('get_system_prompt_config').catch(() => ({ prompts: [] }))

export const generateTitle = (input: any): Promise<string> =>
  invoke<string>('generate_title', { input }).catch(() => '')

// --- File dialogs ---
export const openFileDialog = (): Promise<{ files: any[] }> =>
  invoke<{ files: any[] }>('open_file_dialog').catch(() => ({ files: [] }))


// --- Agent session compat ---
export const listAgentSessions = (): Promise<any[]> =>
  invoke<any[]>('list_agent_sessions').catch(() => [])

export const createAgentSession = (title?: string, channelId?: string, workspaceId?: string): Promise<any> =>
  invoke('create_agent_session', { title: title ?? null, channelId: channelId ?? null, workspaceId: workspaceId ?? null })
    .catch(() => ({ id: crypto.randomUUID(), title: title || '新会话', createdAt: Date.now(), updatedAt: Date.now() }))

export const getAgentSessionMessages = (sessionId: string): Promise<any[]> =>
  invoke<any[]>('get_agent_session_messages', { sessionId }).catch(() => [])

export const sendAgentMessage = (input: any): Promise<void> => {
  return invoke<void>('send_agent_message', { input: {
    sessionId: input.sessionId ?? input.conversationId ?? '',
    userMessage: input.userMessage ?? input.content ?? '',
    channelId: input.channelId ?? null,
    modelId: input.modelId ?? null,
    workspaceId: input.workspaceId ?? null,
    strategy: input.strategy ?? null,
  }})
}

export const stopAgent = (sessionId: string): Promise<void> =>
  invoke<void>('stop_agent', { sessionId }).catch(() => {})

export const queueAgentMessage = (input: any): Promise<void> =>
  invoke<void>('queue_agent_message', { input })

export const migrateChatToAgent = (conversationId: string, sessionId: string): Promise<void> =>
  invoke<void>('migrate_chat_to_agent', { conversationId, sessionId }).catch(() => {})

export const forkAgentSession = (input: { sessionId: string; upToMessageUuid: string }): Promise<any> =>
  invoke('fork_agent_session', { input })

export const rewindSession = (input: { sessionId: string; assistantMessageUuid: string }): Promise<any> =>
  invoke('rewind_session', { input })

export const getAgentSessionPath = (workspaceId: string, sessionId: string): Promise<string> =>
  invoke<string>('get_agent_session_path', { workspaceId, sessionId }).catch(() => '')

export const saveFilesToAgentSession = (input: any): Promise<any[]> =>
  invoke<any[]>('save_files_to_agent_session', { input }).catch(() => [])

// --- Workspace / directory compat ---
export const getWorkspaceCapabilities = (slug: string): Promise<{ mcpServers: any[]; skills: any[] }> =>
  invoke<{ mcpServers: any[]; skills: any[] }>('get_workspace_capabilities', { slug }).catch(() => ({ mcpServers: [], skills: [] }))



export const saveImageAs = (path: string, filename: string): Promise<void> =>
  invoke<void>('save_image_as', { path, filename }).catch(() => {})

// --- File / dialog actions (Tauri plugin-backed) ---

export const openFolderDialog = async (): Promise<{ path: string; name: string } | null> => {
  const selected = await openDialog({ directory: true, multiple: false })
  if (!selected || typeof selected !== 'string') return null
  const name = selected.split('/').pop() ?? selected
  return { path: selected, name }
}

export const openFile = (path: string): Promise<void> => openShell(path)

export const openExternal = (url: string): Promise<void> => openShell(url)

export const showInFinder = (path: string): Promise<void> => {
  // tauri-plugin-shell v2 doesn't expose `open -R` (reveal-in-Finder).
  // Open the path directly: macOS `open <path>` resolves to:
  //   - directory → opens in Finder ✓ (the workspace-folder use case)
  //   - file      → opens in its default app (best we can do without -R)
  // Previously this returned `openShell(parent)` which opened the
  // grandparent when the caller passed a workspace folder — wrong UX.
  return openShell(path)
}

export const getPathForFile = (_file: File): string | null => {
  // Electron 的 webUtils.getPathForFile 无法在 Tauri 中使用
  // 返回 null 使调用方走 fallback 路径
  return null
}

export const checkPathsType = (paths: string[]): Promise<{ directories: string[]; files: string[] }> =>
  invoke<{ directories: string[]; files: string[] }>('check_paths_type', { paths }).catch(() => ({ directories: [], files: paths }))

// --- Safety mode quick-toggle ---
//
// The previous `getPermissionMode` / `setPermissionMode` wrappers called
// Tauri commands that never existed and silenced errors via `.catch()`, so
// the input-bar selector visibly cycled but the backend never received the
// value. Replaced by the existing `getSafetyPolicy` / `setSafetyMode` (above)
// which talk to the real SafetyManager.
//
// `SafetyModeWire` mirrors the Rust enum's serde shape
// (`#[serde(rename_all = "lowercase")]` in `src-tauri/src/safety/mod.rs:11`).

export type SafetyModeWire = 'ask' | 'acceptedits' | 'plan' | 'supervised' | 'yolo'

// --- System prompt compat ---
export const createSystemPrompt = (input: any): Promise<any> =>
  invoke('create_system_prompt', { input }).catch(() => ({ id: crypto.randomUUID(), name: input?.name ?? '', content: input?.content ?? '', isBuiltin: false }))

export const deleteSystemPrompt = (id: string): Promise<void> =>
  invoke<void>('delete_system_prompt', { id }).catch(() => {})

export const setDefaultPrompt = (id: string): Promise<void> =>
  invoke<void>('set_default_prompt', { id }).catch(() => {})

export const updateSystemPrompt = (id: string, input: any): Promise<any> =>
  invoke('update_system_prompt', { id, input }).catch(() => ({ id, ...input }))

export const updateAppendSetting = (enabled: boolean): Promise<void> =>
  invoke<void>('update_append_setting', { enabled }).catch(() => {})

// --- Chat tools compat ---
export const updateChatToolState = (toolId: string, patch: any): Promise<void> =>
  invoke<void>('update_chat_tool_state', { toolId, patch }).catch(() => {})

export const getChatTools = (): Promise<any[]> =>
  invoke<any[]>('get_chat_tools').catch(() => [])

// --- Feishu compat ---
export const setFeishuSessionNotify = (sessionId: string, mode: string): Promise<void> =>
  invoke<void>('set_feishu_session_notify', { sessionId, mode }).catch(() => {})


// --- Agent permission / ask-user / exit-plan compat ---
export const respondPermission = (input: any): Promise<void> =>
  invoke<void>('respond_permission', { input }).catch(() => {})

export const respondAskUser = (input: { requestId: string; answers: Record<string, string> }): Promise<void> =>
  invoke<void>('respond_ask_user', { input })

export const onAskUserRequest = (cb: (payload: AskUserRequest) => void): Promise<UnlistenFn> =>
  listen('agent:ask_user_request', (e) => cb(e.payload as AskUserRequest))

export interface RespondExitPlanModeInput {
  requestId: string
  decision: 'accept_and_auto' | 'accept_keep_plan' | 'reject'
  feedback?: string
  allowedPrompts?: string[]
  sessionId: string
}

export const respondExitPlanMode = (input: RespondExitPlanModeInput): Promise<void> =>
  invoke<void>('respond_exit_plan_mode', { input })

export const onExitPlanRequest = (cb: (payload: ExitPlanModeRequest) => void): Promise<UnlistenFn> =>
  listen('agent:exit_plan_request', (e) => cb(e.payload as ExitPlanModeRequest))

// --- Agent session management compat ---
export const moveAgentSessionToWorkspace = (input: any): Promise<any> =>
  invoke('move_agent_session_to_workspace', { input })

/**
 * Delete an agent session and its derived rows. Surfaces backend errors —
 * the previous `.catch(() => {})` silenced "command not found" so the
 * delete UI optimistically closed the tab while the session lingered in
 * the DB. Callers should toast on rejection. Returns true when a row
 * was actually deleted.
 */
export const deleteAgentSession = (id: string): Promise<boolean> =>
  invoke<boolean>('delete_agent_session', { id })

export const updateAgentSessionTitle = (id: string, title: string): Promise<any> =>
  invoke('update_agent_session_title', { id, title }).catch(() => ({ id, title, updatedAt: Date.now() }))

/**
 * Toggle pin state on an agent session. Returns the new pinnedAt:
 * number (ms) when pinned, null when unpinned. Surfaces backend errors —
 * the previous `.catch(() => fake)` masked a missing Tauri command and
 * pretended every toggle succeeded. The Rust command now exists
 * (Phase 6-A Task 2).
 */
export const togglePinAgentSession = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_pin_agent_session', { id })

export const toggleManualWorkingAgentSession = (id: string): Promise<any> =>
  invoke('toggle_manual_working_agent_session', { id }).catch(() => ({ id, manualWorking: true, updatedAt: Date.now() }))

export const toggleArchiveAgentSession = (id: string): Promise<any> =>
  invoke('toggle_archive_agent_session', { id }).catch(() => ({ id, archived: true, updatedAt: Date.now() }))

export const toggleArchiveConversation = (id: string): Promise<any> =>
  invoke('toggle_archive_conversation', { id }).catch(() => ({ id, archived: true, updatedAt: Date.now() }))

// --- User profile compat ---
export const getUserProfile = (): Promise<any> =>
  invoke('get_user_profile').catch(() => ({ userName: 'User', avatar: '' }))

// --- Search compat ---
export const searchConversationMessages = (query: string): Promise<any[]> =>
  invoke<any[]>('search_conversation_messages', { query }).catch(() => [])

export const searchAgentSessionMessages = (query: string): Promise<any[]> =>
  invoke<any[]>('search_agent_session_messages', { query }).catch(() => [])

// --- Chat stream event listeners (Proma-style synchronous unlisten) ---
type CleanupFn = () => void

// Helper: register a Tauri event listener and return a synchronous cleanup.
// React StrictMode fires effects twice: cleanup runs *before* the listen() Promise
// resolves, so a plain `let unlisten = null` guard leaves the first listener alive.
// The `cancelled` flag closes this gap — if cleanup runs first, the unlisten fn is
// called immediately when the Promise eventually settles.
function makeListener(event: string, cb: (payload: any) => void): CleanupFn {
  let cancelled = false
  let unlisten: (() => void) | null = null
  listen(event, (e) => cb(e.payload)).then((fn) => {
    if (cancelled) {
      fn() // cleanup already ran — unlisten immediately
    } else {
      unlisten = fn
    }
  }).catch(console.error)
  return () => {
    cancelled = true
    unlisten?.()
  }
}

export const onStreamChunk = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-chunk', cb)

export const onStreamReasoning = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-reasoning', cb)

export const onStreamComplete = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-complete', cb)

export const onStreamError = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-error', cb)

export const onStreamToolActivity = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-tool-activity', cb)

/* eslint-enable @typescript-eslint/no-explicit-any */

// ─── Agent Teams ──────────────────────────────────────────────────────
export interface TeamChannelMessage {
  id: string
  fromRole: string
  toRole: string | null
  message: string
  createdAt: number
}

export const startAgentTeams = (sessionId: string, task: string, maxReviewCycles?: number): Promise<string> =>
  invoke<string>('start_agent_teams', { input: { sessionId, task, maxReviewCycles } })

export const getTeamChannel = (teamId: string): Promise<TeamChannelMessage[]> =>
  invoke<TeamChannelMessage[]>('get_team_channel', { teamId })

export const stopAgentTeams = (teamId: string): Promise<void> =>
  invoke('stop_agent_teams', { teamId })

// ─── Browser (Phase 3) ────────────────────────────────────────────────
export interface BrowserStateResponse {
  running: boolean
  tabs: { tabId: string; url: string; title: string }[]
  activeTabId: string | null
}

export const browserGetState = (): Promise<BrowserStateResponse> =>
  invoke<BrowserStateResponse>('browser_get_state')

export const browserLaunch = (): Promise<boolean> =>
  invoke<boolean>('browser_launch')

export const browserShutdown = (): Promise<boolean> =>
  invoke<boolean>('browser_shutdown')

export const browserTakeScreenshot = (tabId: string): Promise<string> =>
  invoke<string>('browser_take_screenshot', { tab_id: tabId })

// ─── Automation (Phase 3) ─────────────────────────────────────────────
export interface AutomationSpecRow {
  id: string
  name: string
  description: string
  tomlContent: string
  enabled: boolean
  createdAt: number
  updatedAt: number
}

export interface AutomationActivity {
  id: string
  specId: string
  runId: string
  trigger: string
  status: 'running' | 'completed' | 'failed' | 'cancelled'
  result: string | null
  error: string | null
  durationMs: number
  createdAt: number
  completedAt: number | null
}

export const installAutomation = (tomlContent: string): Promise<AutomationSpecRow> =>
  invoke<AutomationSpecRow>('install_automation', { tomlContent })

export const listAutomations = (): Promise<AutomationSpecRow[]> =>
  invoke<AutomationSpecRow[]>('list_automations')

export const triggerAutomationManual = (specId: string): Promise<boolean> =>
  invoke<boolean>('trigger_automation_manual', { specId })

export const getAutomationActivity = (specId: string, limit?: number): Promise<AutomationActivity[]> =>
  invoke<AutomationActivity[]>('get_automation_activity', { specId, limit })

// ─── Badge ────────────────────────────────────────────────────────────
export const updateBadgeCount = (count: number): Promise<boolean> =>
  invoke<boolean>('update_badge_count', { count })

// ─── Permission Rules ─────────────────────────────────────────────────
export const listPermissionRules = (): Promise<PermissionRule[]> =>
  invoke<PermissionRule[]>('list_permission_rules')

export const createPermissionRule = (input: CreatePermissionRuleInput): Promise<PermissionRule> =>
  invoke<PermissionRule>('create_permission_rule', { input })

export const deletePermissionRule = (id: string): Promise<boolean> =>
  invoke<boolean>('delete_permission_rule', { id })

export const listPermissionAudit = (sessionId?: string, limit = 100): Promise<PermissionAuditEntry[]> =>
  invoke<PermissionAuditEntry[]>('list_permission_audit', { sessionId, limit })

// ─── Prompts (workspace uclaw.md + default mode prompts) ──────────────
export const readWorkspaceUclawMd = (): Promise<string> =>
  invoke<string>('read_workspace_uclaw_md')

export const writeWorkspaceUclawMd = (content: string): Promise<void> =>
  invoke<void>('write_workspace_uclaw_md', { content })

export const readDefaultPrompts = (): Promise<DefaultPromptsResponse> =>
  invoke<DefaultPromptsResponse>('read_default_prompts')

/**
 * Open `<active_workspace>/uclaw.md` in the OS default application (file
 * manager / text editor). Creates the file if it doesn't exist.
 * Errors propagate (caller should show toast).
 */
export const openWorkspaceUclawMdExternally = (): Promise<void> =>
  invoke<void>('open_workspace_uclaw_md_externally')

// ============================================================================
// Files Rail (W3)
// ============================================================================

interface BackendFileNode {
  path: string
  rel_path: string
  name: string
  kind: 'file' | 'directory'
  size: number
  mtime_ms: number
  is_ignored: boolean
}

const toTreeNode = (n: BackendFileNode): TreeNode => ({
  kind: n.kind,
  relPath: n.rel_path,
  name: n.name,
  size: n.size,
  mtimeMs: n.mtime_ms,
})

interface BackendMountRoot {
  id: string
  label: string
  path: string
  kind: 'workspace' | 'session' | 'attached_dir'
  editable: boolean
}

const toMountRoot = (m: BackendMountRoot): MountRoot => ({ ...m })

export async function filesRailListMounts(sessionId: string | null): Promise<MountRoot[]> {
  const raw = await invoke<BackendMountRoot[]>('files_rail_list_mounts', { sessionId })
  return raw.map(toMountRoot)
}

export async function filesRailReadDir(
  mountId: string,
  relPath: string,
  sessionId: string | null = null,
): Promise<TreeNode[]> {
  const raw = await invoke<BackendFileNode[]>('files_rail_read_dir', { mountId, relPath, sessionId })
  return raw.map(toTreeNode)
}

export async function filesRailWatchStart(
  mountId: string,
  sessionId: string | null = null,
): Promise<void> {
  await invoke<void>('files_rail_watch_start', { mountId, sessionId })
}

export async function filesRailWatchStop(mountId: string): Promise<void> {
  await invoke<void>('files_rail_watch_stop', { mountId })
}

// ============================================================================
// Preview (W4a)
// ============================================================================

interface BackendPreviewBytes {
  resolved_path: string
  /** Tauri serializes Vec<u8> as a number[] by default. */
  bytes: number[]
  size: number
  truncated: boolean
  mtime_ms: number
}

export interface PreviewBytes {
  resolvedPath: string
  /** Owned byte buffer for the file content. */
  bytes: Uint8Array
  size: number
  truncated: boolean
  mtimeMs: number
}

export async function previewReadBytes(
  mountId: string,
  relPath: string,
  sessionId: string | null = null,
): Promise<PreviewBytes> {
  const raw = await invoke<BackendPreviewBytes>('preview_read_bytes', {
    mountId,
    relPath,
    sessionId,
  })
  return {
    resolvedPath: raw.resolved_path,
    bytes: new Uint8Array(raw.bytes),
    size: raw.size,
    truncated: raw.truncated,
    mtimeMs: raw.mtime_ms,
  }
}
