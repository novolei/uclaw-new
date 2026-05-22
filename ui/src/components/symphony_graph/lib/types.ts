export interface LifecycleHooks {
  after_create: string | null;
  before_run: string | null;
  after_run: string | null;
  before_remove: string | null;
  timeout_secs: number;
}

export interface RunConfig {
  agent_type: string;
  auto_approve: boolean;
  max_concurrent: number;
  poll_interval_secs: number;
  issue_label: string | null;
  max_turns: number;
  notifications_enabled: boolean;
  notification_sound: boolean;
  max_retries: number;
  retry_backoff_secs: number;
  retry_base_delay_secs: number;
  retry_max_backoff_secs: number;
  cleanup_on_failure: boolean;
  cleanup_on_stop: boolean;
  workspace_ttl_days: number;
  max_concurrent_by_stage: Record<string, number>;
  stage_prompts: Record<string, string>;
  hooks: LifecycleHooks;
  priority_labels: string[];
  stall_timeout_secs: number;
  stage_skip_labels: Record<string, string[]>;
  approval_gates: Record<string, boolean>;
  local_repos: Record<string, string>;
  custom_agent_command: string;
}

export interface AgentRun {
  id: string;
  repo: string;
  issue_number: number;
  issue_title: string;
  status: string;
  stage: string;
  started_at: string;
  finished_at: string | null;
  workspace_path?: string;
  error?: string | null;
  attempt?: number;
  max_retries?: number;
  logs?: string[];
  issue_labels?: string[];
  skipped_stages?: string[];
  pending_next_stage?: string | null;
  command_display?: string | null;
  agent_type?: string;
  last_log_line?: string | null;
  log_count?: number;
  activity?: string | null;
}

export interface Issue {
  number: number;
  title: string;
  body: string | null;
  state: string;
  labels: string[];
  assignee: string | null;
  url: string;
  created_at: string;
  updated_at: string;
  _repo?: string;
}

export interface Repo {
  full_name: string;
  name: string;
  owner: string;
  description: string | null;
  url: string;
  default_branch: string;
  is_private: boolean;
}

export interface RepoIssue extends Issue {
  _repo: string;
}

export interface OrchestratorStatus {
  is_running: boolean;
  repos: string[];
  runs: AgentRun[];
  config: RunConfig;
  total_completed: number;
  total_failed: number;
  active_count: number;
  total_input_tokens?: number;
  total_output_tokens?: number;
  total_cost_usd?: number;
  total_runtime_secs?: number;
}

export interface AgentLogLine {
  run_id: string;
  timestamp: string;
  line: string;
}

export interface WorkspaceInfo {
  name: string;
  path: string;
  size_bytes: number;
  size_display: string;
  modified_at: string;
  age_days: number;
  is_worktree: boolean;
}

export interface LocalRepoInfo {
  path: string;
  full_name: string;
}

export interface BlockedIssueEntry {
  repo: string;
  issue_number: number;
  blocked_by: number[];
}

export interface OrchestratorBlockedPayload {
  blocked: BlockedIssueEntry[];
}

export interface StageReport {
  name: string;
  status: string;
  duration_secs: number | null;
  duration_display: string;
  files_modified: string[];
  lines_added: number;
  lines_removed: number;
  commands_executed: string[];
  summary: string;
  attempt: number;
}

export interface PipelineReport {
  issue_number: number;
  issue_title: string;
  repo: string;
  total_duration_secs: number;
  total_duration_display: string;
  stages: StageReport[];
  pr_number: number | null;
  pr_url: string | null;
  issue_url: string;
  code_review_summary: string;
  testing_summary: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

