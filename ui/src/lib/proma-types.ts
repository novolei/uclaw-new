/**
 * Proma 共享类型本地化
 *
 * 从 Proma @proma/shared 包中提取的类型定义，
 * 供迁移过来的 Jotai 原子文件使用。
 * 随着 uClaw 后端能力完善，这些类型会逐步由真实 Rust 后端接口驱动。
 */

// ─────────────────────────────────────────────────────────
// Agent Types
// ─────────────────────────────────────────────────────────

/** Agent 会话元数据 */
export interface AgentSessionMeta {
  id: string
  title: string
  /** 会话标题 emoji（自动生成） */
  titleEmoji?: string
  /** 标题生成中（占位动画） */
  titlePending?: boolean
  workspaceId?: string
  channelId?: string
  modelId?: string
  sdkSessionId?: string
  /** 是否置顶 */
  pinned?: boolean
  /** 是否已归档 */
  archived?: boolean
  /** 手动标记为工作中 */
  manualWorking?: boolean
  /** 附加的额外目录 */
  attachedDirectories?: string[]
  messageCount: number
  createdAt: number
  updatedAt: number
}

/** Agent 消息 */
export interface AgentMessage {
  id: string
  sessionId?: string
  role: 'user' | 'assistant' | 'system' | 'status'
  content: string
  model?: string
  createdAt: number
  durationMs?: number
  usage?: AgentEventUsage
  errorCode?: string
  events?: AgentEvent[]
  attachedDirectories?: string[]
}

/** SDK 消息格式 (Phase 4) */
export interface SDKMessage {
  type: string
  uuid?: string
  message?: {
    content?: SDKContentBlock[] | string
    model?: string
    usage?: AgentEventUsage
  }
  subtype?: string
  tool_use_id?: string
  usage?: {
    duration_ms?: number
    total_tokens?: number
    tool_uses?: number
    input_tokens?: number
    output_tokens?: number
    cache_read_input_tokens?: number
    cache_creation_input_tokens?: number
  }
  parent_tool_use_id?: string | null
  _createdAt?: number
  model?: string
  durationMs?: number
  error?: { message?: string; code?: string }
  recovery_actions?: RecoveryAction[]
  inputTokens?: number
  contextWindow?: number
  [key: string]: unknown
}

export type SDKAssistantMessage = SDKMessage & {
  type: 'assistant'
  _channelModelId?: string
  isReplay?: boolean
  isSynthetic?: boolean
}
export type SDKUserMessage = SDKMessage & { type: 'user' }
export type SDKSystemMessage = SDKMessage & { type: 'system'; subtype?: string; tool_use_id?: string }
export type SDKResultMessage = SDKMessage & {
  type: 'result'
  modelUsage?: Record<string, { contextWindow?: number; [key: string]: unknown }>
  total_cost_usd?: number
}

/** SDK 内容块 */
export interface SDKContentBlock {
  type: string
  text?: string
  id?: string
  name?: string
  input?: Record<string, any>
  thinking?: string
  tool_use_id?: string
  content?: string | SDKContentBlock[]
  is_error?: boolean
  [key: string]: unknown
}

export type SDKTextBlock = SDKContentBlock & { type: 'text'; text: string }
export type SDKToolUseBlock = SDKContentBlock & { type: 'tool_use'; id: string; name: string; input: Record<string, any> }
export type SDKThinkingBlock = SDKContentBlock & { type: 'thinking'; thinking: string }
export type SDKToolResultBlock = SDKContentBlock & { type: 'tool_result'; tool_use_id: string; content?: string | SDKContentBlock[]; is_error?: boolean }

export interface SDKMessageContent {
  type: string
  text?: string
  [key: string]: unknown
}

/** Agent 事件类型（流式） */
export interface AgentEvent {
  type: string
  sessionId?: string
  text?: string
  toolUseId?: string
  toolName?: string
  input?: any
  result?: string
  isError?: boolean
  intent?: string
  displayName?: string
  parentToolUseId?: string
  taskId?: string
  taskType?: string
  description?: string
  status?: any
  summary?: string
  outputFile?: string
  usage?: any
  lastToolName?: string
  elapsedSeconds?: number
  shellId?: string
  imageAttachments?: Array<{ localPath: string; filename: string; mediaType: string }>
  attempt?: number
  maxAttempts?: number
  attemptData?: RetryAttempt
  finalAttempt?: RetryAttempt
}

/** Agent 工作区 */
export interface AgentWorkspace {
  id: string
  name: string
  slug: string
  createdAt: number
  updatedAt: number
}

/** Agent 待发送文件 */
export interface AgentPendingFile {
  id: string
  filename: string
  mediaType: string
  size: number
  content?: string
  previewUrl?: string
  sourcePath?: string
}

/** 重试尝试记录 */
export interface RetryAttempt {
  attempt: number
  reason: string
  errorMessage: string
  timestamp: number
  delaySeconds: number
  environment?: {
    runtime: string
    platform: string
    model: string
    workspace?: string
  }
  stderr?: string
  stack?: string
}

/** 任务用量统计 */
export interface TaskUsage {
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
}

/** Agent 事件用量 */
export interface AgentEventUsage {
  inputTokens: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
}

/** 模型选择选项（简版，Agent 模块使用） */
export interface SelectedModelOption {
  channelId: string
  modelId: string
}

/** 危险等级 */
export type DangerLevel = 'safe' | 'normal' | 'dangerous'

/** 恢复操作 */
export interface RecoveryAction {
  action: string
  label: string
  description?: string
  payload?: string
}

// ─────────────────────────────────────────────────────────
// Permission Types
// ─────────────────────────────────────────────────────────

/** 权限模式 */
export type PromaPermissionMode = 'auto' | 'ask' | 'deny' | 'bypassPermissions' | 'plan'

/** 权限模式循环顺序 */
export const PROMA_PERMISSION_MODE_ORDER: PromaPermissionMode[] = ['auto', 'bypassPermissions', 'plan']

/** 权限请求 */
export interface PermissionRequest {
  requestId: string
  sessionId: string
  toolName: string
  toolInput: Record<string, unknown>
  input?: Record<string, unknown>
  riskLevel?: string
  dangerLevel: DangerLevel
  description?: string
  command?: string
  sdkDisplayName?: string
  sdkTitle?: string
  sdkDescription?: string
}

/** 权限响应 */
export interface PermissionResponse {
  requestId: string
  sessionId: string
  allowed: boolean
  rememberForSession?: boolean
  rememberForWorkspace?: boolean
}

/** AskUser 请求 */
export interface AskUserRequest {
  requestId: string
  sessionId: string
  questions: AskUserQuestion[]
}

/** AskUser 问题 */
export interface AskUserQuestion {
  question: string
  header?: string
  multiSelect: boolean
  options: Array<{ label: string; description?: string; preview?: string }>
}

/** AskUser 响应 */
export interface AskUserResponse {
  requestId: string
  answers: Record<string, string>
}

/** ExitPlanMode 请求 */
export interface ExitPlanModeRequest {
  requestId: string
  sessionId: string
  allowedPrompts: ExitPlanAllowedPrompt[]
}

/** ExitPlanMode 允许的操作提示 */
export interface ExitPlanAllowedPrompt {
  prompt: string
}

/** ExitPlanMode 动作类型 */
export type ExitPlanModeAction = 'approve_auto' | 'approve_edit' | 'deny' | 'feedback'

/** ExitPlanMode 响应 */
export interface ExitPlanModeResponse {
  requestId: string
  action: ExitPlanModeAction
  feedback?: string
}

/** 思考模式配置 */
export interface ThinkingConfig {
  type: 'enabled' | 'disabled' | 'adaptive'
  budgetTokens?: number
}

/** Agent 推理深度 */
export type AgentEffort = 'low' | 'medium' | 'high'

// ─────────────────────────────────────────────────────────
// Chat Types
// ─────────────────────────────────────────────────────────

/** Chat 对话元数据 (Proma 格式) */
export interface ConversationMeta {
  id: string
  title: string
  modelId?: string
  channelId?: string
  contextDividers?: string[]
  contextLength?: number | 'infinite'
  /** 是否置顶 */
  pinned?: boolean
  /** 是否已归档 */
  archived?: boolean
  createdAt: number
  updatedAt: number
}

/** Chat 消息 (Proma 格式) */
export interface PrimaChatMessage {
  id: string
  conversationId: string
  role: 'user' | 'assistant' | 'system'
  content: string
  reasoning?: string
  model?: string
  toolActivities?: ChatToolActivity[]
  attachments?: FileAttachment[]
  createdAt: number
}

/** ChatMessage — Proma 组件使用的消息类型（兼容别名） */
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  reasoning?: string
  model?: string
  error?: string
  stopped?: boolean
  toolActivities?: ChatToolActivity[]
  attachments?: FileAttachment[]
  createdAt: number
}

/** 文件附件 */
export interface FileAttachment {
  id?: string
  filename: string
  localPath: string
  mediaType: string
  size: number
  extractedText?: string
}

/** 附件保存输入 */
export interface AttachmentSaveInput {
  conversationId: string
  filename: string
  mediaType: string
  data: string
}

/** Chat 工具活动 */
export interface ChatToolActivity {
  toolCallId: string
  type: 'start' | 'result'
  toolId?: string
  toolName: string
  status?: 'running' | 'completed' | 'failed'
  input?: Record<string, unknown>
  result?: string
  isError?: boolean
  error?: string
  durationMs?: number
}

/** 渠道模型 */
export interface ChannelModel {
  id: string
  name: string
  enabled: boolean
}

/** 渠道（AI 供应商）*/
export interface Channel {
  id: string
  name: string
  provider: string
  providerId?: string
  baseUrl: string
  apiKey?: string
  modelId?: string
  enabled: boolean
  models: ChannelModel[]
  createdAt?: number
  updatedAt?: number
}

/** 模型选项（用于 ModelSelector） */
export interface ModelOption {
  channelId: string
  channelName?: string
  modelId: string
  modelName?: string
  provider?: string
}

/** Chat 发送输入 */
export interface ChatSendInput {
  conversationId: string
  userMessage: string
  messageHistory: unknown[]
  channelId: string
  modelId: string
  contextLength?: number | 'infinite'
  contextDividers?: string[]
  attachments?: FileAttachment[]
  thinkingEnabled?: boolean
  systemMessage?: string
  enabledToolIds?: string[]
}

/** Agent 发送输入 */
export interface AgentSendInput {
  sessionId: string
  userMessage: string
  channelId: string
  modelId?: string
  workspaceId?: string
  startedAt?: number
  permissionMode?: PromaPermissionMode
  thinking?: ThinkingConfig
  effort?: AgentEffort
  maxBudgetUsd?: number
  maxTurns?: number
  additionalDirectories?: string[]
  files?: AgentPendingFile[]
  mentionedSkills?: string[]
  mentionedMcpServers?: string[]
}

/** Agent 队列消息输入 */
export interface AgentQueueMessageInput {
  sessionId: string
  userMessage: string
  uuid?: string
  interrupt?: boolean
}

/** Agent 流式事件 */
export interface AgentStreamEvent {
  sessionId: string
  event: AgentEvent
}

/** Agent 流式完成载荷 */
export interface AgentStreamCompletePayload {
  sessionId: string
}

/** Agent 生成标题输入 */
export interface AgentGenerateTitleInput {
  sessionId: string
}

// ─────────────────────────────────────────────────────────
// Settings & Theme Types
// ─────────────────────────────────────────────────────────

/** 主题模式 */
export type ThemeMode = 'light' | 'dark' | 'system' | 'special'

/** 特殊风格主题 */
export type ThemeStyle = 'default' | 'ocean-light' | 'ocean-dark' | 'forest-light' | 'forest-dark' | 'slate-light' | 'slate-dark'

/** 通知音场景类型 */
export type NotificationSoundType = 'taskComplete' | 'permissionRequest' | 'exitPlanMode'

/** 可选通知音 ID */
export type NotificationSoundId = 'ding' | 'ding-dong' | 'discord' | 'done' | 'down-power' | 'food' | 'lite' | 'quiet' | 'none'

/** 各场景通知音配置 */
export interface NotificationSoundSettings {
  taskComplete?: NotificationSoundId
  permissionRequest?: NotificationSoundId
  exitPlanMode?: NotificationSoundId
}

/** 用户档案 */
export interface UserProfile {
  userName: string
  avatar: string
}

export const DEFAULT_USER_AVATAR = '🧑‍💻'
export const DEFAULT_USER_NAME = '用户'

// ─────────────────────────────────────────────────────────
// Environment & Runtime Types
// ─────────────────────────────────────────────────────────

/** 环境检测结果 */
export interface EnvironmentCheckResult {
  hasIssues: boolean
  details?: Record<string, unknown>
}

/** 运行时状态 */
export interface RuntimeStatus {
  node?: { available: boolean; version?: string }
  shell?: {
    gitBash?: { available: boolean }
    wsl?: { available: boolean }
  }
}

/** 安装包清单 */
export interface InstallerManifest {
  items: any[]
}

// ─────────────────────────────────────────────────────────
// File System Types
// ─────────────────────────────────────────────────────────

/** 文件系统条目 */
export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  isFile: boolean
  size?: number
  modifiedAt?: number
  children?: FileEntry[]
  extension?: string
}

// ─────────────────────────────────────────────────────────
// Proxy Types
// ─────────────────────────────────────────────────────────

/** 代理配置 */
export interface ProxyConfig {
  mode: 'system' | 'manual' | 'none'
  httpProxy?: string
  httpsProxy?: string
  noProxy?: string
}

// ─────────────────────────────────────────────────────────
// System Prompt Types
// ─────────────────────────────────────────────────────────

/** 系统提示词 */
export interface SystemPrompt {
  id: string
  name: string
  content: string
  isBuiltin?: boolean
  createdAt?: number
  updatedAt?: number
}

/** 系统提示词创建输入 */
export interface SystemPromptCreateInput {
  name: string
  content: string
}

/** 系统提示词更新输入 */
export interface SystemPromptUpdateInput {
  name?: string
  content?: string
}

/** 系统提示词配置 */
export interface SystemPromptConfig {
  prompts: SystemPrompt[]
  defaultPromptId?: string
  appendDateTimeAndUserName: boolean
}

/** 内置默认提示词 ID */
export const BUILTIN_DEFAULT_ID = 'builtin-default'

/** 内置默认提示词 */
export const BUILTIN_DEFAULT_PROMPT: SystemPrompt = {
  id: BUILTIN_DEFAULT_ID,
  name: '默认',
  content: 'You are a helpful assistant.',
  isBuiltin: true,
}

// ─────────────────────────────────────────────────────────
// Chat Tool Types
// ─────────────────────────────────────────────────────────

/** Chat 工具信息 */
export interface ChatToolInfo {
  meta: ChatToolMeta
  enabled: boolean
  available: boolean
}

/** Chat 工具元数据 */
export interface ChatToolMeta {
  id: string
  name: string
  description: string
  category?: string
  icon?: string
}

/** Chat 工具状态 */
export interface ChatToolState {
  enabled: boolean
}

// ─────────────────────────────────────────────────────────
// IM Integration Types (飞书 / 钉钉 / 微信)
// ─────────────────────────────────────────────────────────

/** 飞书 Bridge 状态 */
export interface FeishuBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'error'
  activeBindings?: number
  error?: string
}

/** 飞书 Bot Bridge 状态 */
export interface FeishuBotBridgeState extends FeishuBridgeState {
  botId?: string
}

/** 飞书通知模式 */
export type FeishuNotifyMode = 'auto' | 'always' | 'off'

/** 飞书聊天绑定 */
export interface FeishuChatBinding {
  chatId: string
  chatName: string
  botId: string
  workspaceSlug?: string
  sessionId?: string
}

/** 钉钉 Bridge 状态 */
export interface DingTalkBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'error'
  error?: string
}

/** 钉钉 Bot Bridge 状态 */
export interface DingTalkBotBridgeState extends DingTalkBridgeState {
  botId?: string
}

/** 微信 Bridge 状态 */
export interface WeChatBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'scanning' | 'confirming' | 'error'
  error?: string
}

// ─────────────────────────────────────────────────────────
// Shortcut Types
// ─────────────────────────────────────────────────────────

/** 用户自定义快捷键覆盖 */
export interface ShortcutOverrides {
  [shortcutId: string]: {
    mac?: string
    win?: string
  }
}

// ─────────────────────────────────────────────────────────
// Pending Requests Snapshot
// ─────────────────────────────────────────────────────────

export interface PendingRequestsSnapshot {
  permissions: Array<{ sessionId: string; requests: PermissionRequest[] }>
  askUser: Array<{ sessionId: string; requests: AskUserRequest[] }>
  exitPlan: Array<{ sessionId: string; requests: ExitPlanModeRequest[] }>
}

// ─────────────────────────────────────────────────────────
// Workspace Capabilities
// ─────────────────────────────────────────────────────────

export interface WorkspaceCapabilities {
  mcpServers: Array<{ name: string; enabled: boolean }>
  skills: Array<{ name: string }>
}

// ─────────────────────────────────────────────────────────
// Search Result Types
// ─────────────────────────────────────────────────────────

export interface MessageSearchResult {
  conversationId: string
  conversationTitle: string
  snippet: string
  matchStart: number
  matchLength: number
  archived?: boolean
}

export interface AgentMessageSearchResult {
  sessionId: string
  sessionTitle: string
  snippet: string
  matchStart: number
  matchLength: number
  archived?: boolean
}

// ─────────────────────────────────────────────────────────
// Utility Functions (stub)
// ─────────────────────────────────────────────────────────

/** Diff workspace capabilities (stub) */
export function diffCapabilities(_prev: WorkspaceCapabilities, _next: WorkspaceCapabilities): unknown[] {
  return []
}

/** Migrate old permission mode values (stub) */
export function migratePermissionMode(mode: string): string {
  if (mode === 'acceptEdits' || mode === 'smart' || mode === 'supervised') {
    return 'auto'
  }
  return mode
}
