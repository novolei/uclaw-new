export interface PlatformInfo {
  os: string;
  arch: string;
  version: string;
}

export interface VersionInfo {
  appVersion: string;
  tauriVersion: string;
  rustVersion: string;
}

export interface BootstrapStatus {
  initialized: boolean;
  dbReady: boolean;
  configReady: boolean;
}

export interface Settings {
  language: string;
  theme: string;
  configPath: string;
  dataPath: string;
  monthlyBudgetUsd?: number | null;
}

export interface PatchSettingsInput {
  language?: string;
  theme?: string;
  /** Send `null` to clear; omit to leave unchanged. */
  monthlyBudgetUsd?: number | null;
}

export type View = "home" | "chat" | "settings" | "onboarding" | "apps";

// ─── Conversations ─────────────────────────────────────────────────────

export interface ConversationSummary {
  id: string;
  spaceId: string;
  title: string;
  titleEmoji?: string;
  titlePending?: boolean;
  messageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateConversationInput {
  title?: string;
  spaceId?: string;
}

export interface ConversationResponse {
  id: string;
  spaceId: string;
  title: string;
  titleEmoji?: string;
  titlePending?: boolean;
  messageCount: number;
  starred?: boolean;
  createdAt: string;
  updatedAt: string;
}

// ─── Messages ──────────────────────────────────────────────────────────

export interface Message {
  id: string;
  conversationId: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: string;
  /** Concatenated thinking-block text — assistant only. */
  reasoning?: string;
  /** Per-message tool activity records (frontend ChatToolActivity shape). */
  toolActivities?: unknown[];
  /** Model used for this assistant turn. */
  model?: string;
}

export type SafetyMode = "ask" | "supervised" | "yolo";

export interface SendMessageInput {
  conversationId: string;
  content: string;
  attachments?: string[];
  safetyMode?: SafetyMode;
  /** Override the active model for this message. */
  providerId?: string;
  modelId?: string;
  /** Enable extended thinking/reasoning for this message. */
  thinkingEnabled?: boolean;
}

export interface SendMessageResponse {
  messageId: string;
  conversationId: string;
  response: string;
}

export interface GetMessagesInput {
  conversationId: string;
}

// ─── Streaming Events ──────────────────────────────────────────────────

export interface StreamTextDelta {
  chunk: string;
  timestamp: string;
}

export interface StreamToolStart {
  toolName: string;
  toolCallId: string;
  input: unknown;
  timestamp: string;
}

export interface StreamToolResult {
  toolName: string;
  toolCallId: string;
  result: unknown;
  durationMs: number;
  timestamp: string;
}

export interface StreamThinking {
  text: string;
  timestamp: string;
}

export interface StreamThinkingDelta {
  text: string;
  timestamp: string;
}

export interface StreamThinkingDone {
  durationMs: number;
  timestamp: string;
}

export interface StreamDone {
  text: string;
  timestamp: string;
}

export interface StreamError {
  error: string;
  timestamp: string;
}

// ─── Spaces ────────────────────────────────────────────────────────────

export interface SpaceSummary {
  id: string;
  name: string;
  icon: string;
  conversationCount?: number;
  lastUpdated?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateSpaceInput {
  name: string;
  icon?: string;
  path?: string;
}

// ─── LLM Config ────────────────────────────────────────────────────────

export interface LlmConfigInput {
  provider: string;
  model: string;
  apiKey: string;
  baseUrl?: string;
  maxTokens?: number;
  temperature?: number;
}

export interface LlmConfigResponse {
  provider: string;
  model: string;
  hasApiKey: boolean;
  baseUrl?: string;
  maxTokens?: number;
  temperature?: number;
}

// ─── Artifacts ─────────────────────────────────────────────────────────

export interface ArtifactNode {
  name: string;
  path: string;
  isDir: boolean;
  size?: number;
  children?: ArtifactNode[];
}

export interface ArtifactContentResponse {
  path: string;
  content: string;
  size: number;
}

// ─── Enhanced Artifact Types ────────────────────────────────────────────

export interface ArtifactTreeNodeResponse {
  path: string;
  name: string;
  isDir: boolean;
  parentPath: string;
  sizeBytes?: number;
  mimeType?: string;
  modifiedAt?: string;
  children?: ArtifactTreeNodeResponse[];
}

export interface ListArtifactTreeInput {
  spaceId: string;
  path: string;
}

export interface LoadArtifactChildrenInput {
  spaceId: string;
  path: string;
}

export interface CreateArtifactInput {
  spaceId: string;
  path: string;
  content?: string;
  isDir?: boolean;
}

export interface RenameArtifactInput {
  spaceId: string;
  oldPath: string;
  newPath: string;
}

export interface MoveArtifactInput {
  spaceId: string;
  srcPath: string;
  destPath: string;
}

export interface DetectFileTypeResponse {
  mimeType: string;
  category: string;
}

export interface FileChangeEvent {
  spaceId: string;
  changeType: string;
  path: string;
  oldPath?: string;
  isDir: boolean;
}

export interface ToggleStarResponse {
  conversationId: string;
  starred: boolean;
}

// ─── Search ────────────────────────────────────────────────────────────

export interface SearchInput {
  query: string;
  scope?: string; // "workspace" | "conversations" | "all"
}

export interface SearchResult {
  id: string;
  title: string;
  snippet: string;
  source: string; // "conversation" | "file" | "message"
  sourceId: string;
  messageId?: string;
  workspaceId?: string;
  createdAt: string;
}

// ─── Remote API ────────────────────────────────────────────────────────

export interface RemoteConfig {
  baseUrl: string;
  token: string | null;
}

export interface PairDeviceRequest {
  deviceName: string;
}

export interface PairDeviceResponse {
  userId: string;
  deviceId: string;
  token: string;
  expiresAt: string;
}

export interface AuthStatusResponse {
  version: string;
  authenticated: boolean;
  userId?: string;
  deviceId?: string;
}

export interface CreateApiTokenRequest {
  label?: string;
  expiresInDays?: number;
}

export interface CreateApiTokenResponse {
  token: string;
  label: string;
  expiresAt?: string;
}

export interface HealthCheckResponse {
  status: string;
  version: string;
  name: string;
}

// ─── Enhanced Chat Types (Steward-level) ───────────────────────────────

/** Cost tracking per turn */
export interface TurnCost {
  inputTokens: number;
  outputTokens: number;
  costUsd: string;
}

/** File attachment in composer */
export interface ComposerAttachment {
  path: string;
  name: string;
  size: number;
  mimeType?: string;
}

/** Tool call record (enhanced) */
export interface ToolCallRecord {
  id: string;
  name: string;
  input: unknown;
  result?: unknown;
  error?: string;
  status: "running" | "done" | "error";
  rationale?: string;
  resultPreview?: string;
  durationMs?: number;
  startedAt?: string;
  completedAt?: string;
}

/** Chat message (enhanced Steward-level) */
export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  thinking?: string;
  thinkingDurationMs?: number;
  isThinking?: boolean;
  toolCalls?: ToolCallRecord[];
  turnCost?: TurnCost;
  attachments?: ComposerAttachment[];
  reflection?: ReflectionDetail;
  proactiveLearning?: ProactiveLearningEvent;
  isStreaming: boolean;
  timestamp: string;
}

/** Task operation for approval */
export interface TaskOperation {
  tool_name: string;
  description?: string;
  path?: string;
  destination_path?: string;
  parameters?: Record<string, unknown>;
}

/** Task pending approval details */
export interface TaskPendingApproval {
  operations: TaskOperation[];
  allow_always?: boolean;
}

/** Task record for pending approval */
export interface TaskRecord {
  pending_approval?: TaskPendingApproval;
}

/** YOLO / Ask mode */
export type TaskMode = "ask" | "yolo";

/** Approval request from backend */
export interface ApprovalRequest {
  toolName: string;
  toolId: string;
  command?: string;
  arguments?: Record<string, unknown>;
  riskLevel?: string;
  sessionId: string;
  /** Phase 3: kind="path" payload carries this for the path-variant modal. */
  kind?: 'tool' | 'bash_command' | 'path';
  /** Phase 3: absolute paths the agent is trying to access (kind=="path"). */
  paths?: string[];
  /** Phase 3: human-readable reason from PathPolicy (kind=="path"). */
  reason?: string;
}

/** Approval response to backend */
export interface ApproveToolCallInput {
  sessionId: string;
  toolId: string;
  approved: boolean;
  alwaysAllow?: boolean;
  pathScope?: 'once' | 'session' | 'deny';
  paths?: string[];
}

/** Reflection detail */
export interface ReflectionDetail {
  assistant_message_id?: string;
  status: "queued" | "running" | "completed" | "failed" | "missing" | "unknown";
  outcome?: "updated" | "created" | "no_op" | null;
  summary?: string | null;
  detail?: string | null;
  run_started_at?: string | null;
  run_completed_at?: string | null;
  tool_calls?: ReflectionToolCall[];
  messages?: ReflectionMessage[];
}

export interface ReflectionToolCall {
  id: string;
  created_at: string;
  name: string;
  status: "completed" | "running" | "failed";
  parameters?: string | null;
  result_preview?: string | null;
  error?: string | null;
}

export interface ReflectionMessage {
  id: string;
  content: string;
  created_at: string;
}

/** Proactive learning event from agent */
export interface ProactiveLearningEvent {
  scenario: "conversation_learning" | "skill_extraction" | "multimodal_context";
  items_extracted: number;
  categories: string[];
  timestamp: string;
  summary: string;
  /** Session ID that sourced the extraction context. Used by AgentMessages
   *  to scope the chip to that session. May be null for legacy events. */
  sessionId?: string | null;
}

/** Context token stats */
export interface ContextStats {
  totalTokens: number;
  maxTokens: number;
  usagePercent: number;
  categories?: ContextStatsCategory[];
  /** Session this event belongs to. Required for multi-session routing. */
  conversationId?: string;
  /** Detailed breakdown fields (optional, from agent:context_stats event) */
  modelContextLength?: number;
  systemPromptTokens?: number;
  mcpPromptsTokens?: number;
  skillsTokens?: number;
  messagesTokens?: number;
  toolUseTokens?: number;
  compactBufferTokens?: number;
  freeTokens?: number;
  /** Cumulative API token usage across all iterations */
  cumulativeInputTokens?: number;
  cumulativeOutputTokens?: number;
}

export interface ContextStatsCategory {
  label: string;
  tokens: number;
  color: string;
}

/** Alias for TurnCost (compatibility with spec naming) */
export type TurnCostInfo = TurnCost;

/** Model option for selector */
export interface ModelOption {
  value: string;
  label: string;
  provider?: string;
  description?: string;
}

/** MCP server configuration */
export interface McpServerConfig {
  id: string;
  name: string;
  type: "http" | "stdio";
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  status: "connected" | "disconnected" | "error";
}

/** Remote access status */
export interface RemoteStatus {
  enabled: boolean;
  tunnelActive: boolean;
  localUrl: string;
  remoteUrl?: string;
  pairedDevices: number;
}

/** Performance metrics */
export interface PerfMetrics {
  fps: number;
  memoryMb: number;
  cpuPercent: number;
  uptimeSeconds: number;
}

/** Notification entry */
export interface NotificationEntry {
  id: string;
  type: "info" | "success" | "warning" | "error";
  title: string;
  message: string;
  timestamp: string;
  read: boolean;
}

/** App / digital human */
export interface AppSummary {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  status: "installed" | "not_installed" | "running" | "paused" | "error";
  version?: string;
}

export interface AppDetail extends AppSummary {
  capabilities: string[];
  triggers: AppTrigger[];
  config: Record<string, unknown>;
  lastRunAt?: string;
}

export interface AppTrigger {
  type: "schedule" | "webhook" | "file_watch" | "manual";
  config: Record<string, unknown>;
}

/** Canvas tab */
export interface CanvasTab {
  id: string;
  path: string;
  name: string;
  content: string;
  language: string;
  isDirty: boolean;
  lineCount: number;
}

/** Skill toggle state */
export interface SkillToggle {
  name: string;
  description?: string;
  version?: string;
  enabled: boolean;
}

// ─── Provider Types (Multi-Provider Management) ─────────────────────────

export interface ProviderInfo {
  id: string;
  displayName: string;
  authType: string;
  defaultBaseUrl: string;
  defaultApi: string;
  serviceCategory: string;
  geoCategory: string;
  supportsModels: boolean;
}

export interface ProviderConfigInput {
  providerId: string;
  displayName: string;
  apiKey?: string | null;
  baseUrl?: string | null;
  api?: string | null;
}

export interface ProviderConfigResponse {
  providerId: string;
  displayName: string;
  hasApiKey: boolean;
  baseUrl?: string | null;
  api?: string | null;
}

export interface ProviderConfigureInput {
  providerId: string;
  displayName: string;
  apiKey?: string | null;
  baseUrl?: string | null;
  api?: string | null;
  modelIds: string[];
}

export interface ModelInfo {
  id: string;
  name: string;
  contextWindow?: number | null;
  maxTokens?: number | null;
  modality: string;
  reasoning: boolean;
  supportsReasoningEffort: boolean;
}

export interface ModelSelectionInfo {
  providerId: string;
  modelId: string;
}

export interface TestResultInfo {
  success: boolean;
  message: string;
  latencyMs?: number | null;
  details?: string | null;
}

export interface ListModelsInput {
  providerId: string;
  baseUrl: string;
  apiKey?: string | null;
}

export interface TestConnectionInput {
  providerId: string;
  baseUrl: string;
  apiKey?: string | null;
}

// --- Memory Graph Types ---

export type MemoryNodeKind = 'boot' | 'identity' | 'value' | 'user_profile' | 'directive' | 'curated' | 'episode' | 'procedure' | 'reference';

export interface MemoryNode {
  id: string;
  spaceId: string;
  kind: MemoryNodeKind;
  title: string;
  metadata?: Record<string, any>;
  createdAt: string;
  updatedAt: string;
}

export interface MemoryVersion {
  id: string;
  nodeId: string;
  supersedesVersionId?: string;
  status: 'active' | 'deprecated' | 'orphaned';
  content: string;
  metadata?: Record<string, any>;
  createdAt: string;
}

export interface MemoryNodeDetail {
  node: MemoryNode;
  activeVersion?: MemoryVersion;
  allVersions?: MemoryVersion[];
  routes: MemoryRoute[];
  keywords: string[];
}

export interface MemoryRoute {
  id: string;
  spaceId: string;
  nodeId: string;
  domain: string;
  path: string;
  isPrimary: boolean;
}

export interface MemoryRecallCandidate {
  nodeId: string;
  title: string;
  content: string;
  kind: MemoryNodeKind;
  source: string;
  reason: string;
  score?: number;
  matchedKeywords: string[];
}

export interface MemoryTimelineEntry {
  nodeId: string;
  title: string;
  contentSnippet: string;
  kind: MemoryNodeKind;
  updatedAt: string;
}

export interface MemoryEdge {
  id: string;
  spaceId: string;
  parentNodeId?: string | null;
  childNodeId: string;
  relationKind: string;
  visibility: string;
  priority: number;
  triggerText?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface MemoryGraphData {
  nodes: MemoryNode[];
  edges: MemoryEdge[];
  routes: MemoryRoute[];
}

export interface MemoryRecallPlan {
  boot: MemoryRecallCandidate[];
  triggered: MemoryRecallCandidate[];
  relevant: MemoryRecallCandidate[];
  expanded: MemoryRecallCandidate[];
  recent: MemoryTimelineEntry[];
}

// ─── Notifications ──────────────────────────────────────────────────────

export interface NotificationItem {
  id: string;
  title: string;
  message: string;
  level: string;
  source: string;
  timestamp: string;
}

// ─── Background Tasks ───────────────────────────────────────────────────

export interface BackgroundTask {
  id: string;
  name: string;
  status: string;
  progress?: number;
  startedAt?: string;
  completedAt?: string;
  error?: string;
}

// ─── Memory KV Store ────────────────────────────────────────────────────

export interface MemorySetInput {
  key: string;
  value: unknown;
  kind?: string;
  namespace?: string;
  spaceId?: string;
  tags?: string[];
  metadata?: unknown;
  ttlSeconds?: number;
}

export interface MemoryGetInput {
  key: string;
  namespace?: string;
  spaceId?: string;
}

export interface MemorySearchInput {
  query: string;
  namespace?: string;
  spaceId?: string;
  kind?: string;
  limit?: number;
}

export interface MemoryListInput {
  namespace?: string;
  spaceId?: string;
  kind?: string;
  tag?: string;
  limit?: number;
  offset?: number;
}

export interface MemoryClearInput {
  namespace: string;
  spaceId?: string;
}

export interface MemoryBulkImportEntry {
  key: string;
  value: unknown;
  kind?: string;
  namespace?: string;
  spaceId?: string;
  tags?: string[];
  metadata?: unknown;
  ttlSeconds?: number;
}

export interface MemoryBulkImportInput {
  entries: MemoryBulkImportEntry[];
}

export interface MemoryEntryResponse {
  id: string;
  key: string;
  value: unknown;
  kind: string;
  namespace: string;
  spaceId: string;
  tags: string[];
  metadata?: unknown;
  createdAt: string;
  updatedAt: string;
  expiresAt?: string;
}

export interface MemoryBulkImportResponse {
  imported: number;
  skipped: number;
  errors: string[];
}

export interface MemoryClearResponse {
  deleted: number;
}

// ─── MCP ────────────────────────────────────────────────────────────────

export interface McpServerInfo {
  id: string;
  name: string;
  description: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
  enabled: boolean;
  autoApprove: boolean;
  status: string;
}

export interface McpServerInput {
  id?: string;
  name: string;
  description: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

// ─── Built-in Skills ────────────────────────────────────────────────────

export interface SkillInfo {
  name: string;
  version: string;
  description: string;
  author: string;
  enabled: boolean;
  category: string;
  /** Disk-tier provenance from the three-tier bundling model:
   *  - "bundled": shipped read-only with the app (resource dir)
   *  - "user":    ~/.uclaw/skills/, read-write, survives upgrades
   *  - "project": dev-only fallback (<cwd>/skills/)
   *
   *  The Fork button is only offered for "bundled" skills — User
   *  and Project are already directly editable.
   */
  provenance?: 'bundled' | 'user' | 'project';
}

/** A row in the active-manifest debug panel — surfaces exactly what
 *  the agent loop's system prompt sees, in the order it sees it.
 *  "learned" provenance is for graph-stored skills (kind=Procedure);
 *  the three other values are the disk tiers. */
export interface ActiveManifestSkill {
  rank: number;
  name: string;
  summary: string;
  provenance: 'bundled' | 'user' | 'project' | 'learned';
  citedCount: number;
}

/** Composer `/`-autocomplete row. Mirrors the `InvocableSkill` payload
 *  from PR #120's `list_invocable_skills` IPC. `lifecycle` is only set
 *  for learned skills; the frontend uses it to flag draft / deprecated
 *  rows with a subdued style. */
export interface InvocableSkill {
  name: string;
  description: string;
  provenance: 'static' | 'borrowed' | 'learned';
  lifecycle?: 'draft' | 'promoted' | 'deprecated';
}

/** Composer `@`-autocomplete row. Returned by
 *  `search_workspace_files_for_mention`. The popup renders `name` on
 *  top, `relative_path` underneath; on select it inserts a file_path
 *  chip carrying `absolutePath` so the agent loop's path-policy and
 *  attach-as-context can do their work. */
export interface WorkspaceFileMatch {
  name: string;
  absolutePath: string;
  relativePath: string;
  /** Lowercased extension without the dot (e.g. "tsx"), or "" for files
   *  without one. Drives the icon hint in the popup. */
  extension: string;
}

export interface SkillToggleInput {
  name: string;
  enabled: boolean;
}

export interface SkillMatchInput {
  message: string;
}

export interface SkillMatchResult {
  name: string;
  score: number;
  promptPreview: string;
}

export interface SkillDetailResponse {
  name: string;
  version: string;
  description: string;
  author: string;
  enabled: boolean;
  category: string;
  keywords: string[];
  tags: string[];
  patterns: string[];
  parameters: SkillParamInfo[];
  promptLength: number;
  path: string;
}

export interface SkillParamInfo {
  name: string;
  paramType: string;
  required: boolean;
  description: string;
  default?: string;
}

// ─── Channels ───────────────────────────────────────────────────────────

export interface ChannelInfo {
  id: string;
  name: string;
  channelType: string;
  enabled: boolean;
  webhookUrl?: string;
}

export interface ChannelInput {
  name: string;
  channelType: string;
  webhookUrl?: string;
  config?: unknown;
}

// ─── Safety ─────────────────────────────────────────────────────────────

export interface SafetyPolicyResponse {
  globalMode: string;
  toolOverrides: Record<string, string>;
  autoApprovedTools: string[];
  blockedTools: string[];
}

export interface SetSafetyModeInput {
  mode: string;
}

export interface SetToolOverrideInput {
  toolName: string;
  mode: string;
}

export interface ToolNameInput {
  toolName: string;
}

export interface AssessCommandInput {
  command: string;
}

export interface CommandRiskResponse {
  level: string;
  reasons: string[];
  suggestedAction: string;
}

// ─── Memory Graph CRUD ──────────────────────────────────────────────────

export interface MemoryGraphSearchInput {
  query: string;
  spaceId?: string;
}

export interface MemoryGraphGetNodeInput {
  nodeId: string;
}

export interface MemoryGraphListBootInput {
  spaceId?: string;
  limit?: number;
}

export interface MemoryGraphManageBootInput {
  nodeId: string;
  action: 'add' | 'remove';
  spaceId?: string;
  priority?: number;
}

export interface MemoryGraphTimelineInput {
  spaceId?: string;
  limit?: number;
}

export interface MemoryGraphExplainRecallInput {
  query: string;
  spaceId?: string;
}

export interface MemoryGraphCreateNodeInput {
  spaceId: string;
  kind: string;
  title: string;
  metadata?: Record<string, unknown>;
}

export interface MemoryGraphUpdateNodeInput {
  nodeId: string;
  title?: string;
  kind?: string;
  metadata?: Record<string, unknown>;
}

export interface MemoryGraphDeleteNodeInput {
  nodeId: string;
}

// ─── Learned Skills ─────────────────────────────────────────────────────

export interface LearnedSkill {
  id: string;
  name: string;
  context: string;
  principles: string;
  steps: string;
  pitfalls: string;
  enabled: boolean;
  usageCount: number;
  createdAt: string;
}

// ===== Cost dashboard =====

export interface DailyCostRollup {
  day: string // YYYY-MM-DD
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
}

export interface ModelCostRollup {
  model: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
}

export interface SessionCostRollup {
  sessionId: string
  title: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
  lastUsedAt: number
}

export interface WorkspaceCostRollup {
  workspaceId: string
  workspaceName: string
  workspaceIcon: string
  totalCostUsd: number
  totalTokens: number
}

export interface BudgetThresholdPayload {
  threshold: 80 | 100
  current: number
  budget: number
}

// ===== Permission rules =====

export interface PermissionRule {
  id: string
  scope: 'session' | 'pattern'
  sessionId?: string
  toolName: string
  /** For pattern scope: argument prefix to match. Undefined for session scope. */
  target?: string
  mode: 'allow' | 'block' | 'ask'
  createdAt: number
}

// ===== Prompts =====

export interface DefaultPromptsResponse {
  baseline: string
  modeAsk: string
  modeAcceptEdits: string
  modePlan: string
  modeBypass: string
}

export interface PermissionAuditEntry {
  id: string
  sessionId: string
  toolName: string
  argsHash: string
  decision: 'auto_approve' | 'user_approve' | 'user_deny' | 'blocked'
  ruleId?: string
  createdAt: number
}

export interface CreatePermissionRuleInput {
  scope: 'session' | 'pattern'
  sessionId?: string
  toolName: string
  target?: string
  mode: 'allow' | 'block' | 'ask'
}
