# 子项目 E — MemoryHealthPanel 复活（Drift + Importance）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把两个已被 proactive 调度、表里有真数据、但无 UI 的 L3 算法（Drift Detection + Importance Decay）接进 `MemoryHealthPanel`，作为现有 findings 列表下方的两个可折叠 section —— Drift 可 resolve、Importance 只读。

**Architecture:** 后端在 `memory_graph/{drift_detection,importance_decay}.rs` 加读/写 fn（JOIN memory_nodes 取标题、按 space 过滤）；3 个薄 Tauri 命令（沿用 `memory_health_*` 的锁 + `ensure_memory_health_enabled` gate）；TS bridge + DTO；MemoryHealthPanel 加两节。无新 migration、无新依赖。

**Tech Stack:** Rust（rusqlite、serde）、React 18 + TS + Tailwind、Vitest。

---

## 已核对的事实（实现按这些来，别再猜）

- `memory_nodes` 列：`id, space_id, kind, title, metadata_json, created_at, updated_at`（标题=`title`，空间=`space_id`）。
- `drift_events`（V46）：`id, node_id, score, snapshot_version_ids, computed_at, status('open'默认), resolution_note, resolved_at`。索引 `idx_drift_events_status(status, computed_at DESC)`。
- `memory_importance_scores`（V44）：`node_id PK, base_value, citation_factor, edge_factor, recency_factor, status_bonus, penalty, importance, decay_half_life_days, last_computed_at, archive_pending_since`。索引 `idx_importance_scores_archive(archive_pending_since) WHERE archive_pending_since IS NOT NULL`。
- gate helper：`async fn ensure_memory_health_enabled(state: &State<'_, AppState>) -> Result<(), String>`（在 tauri_commands.rs，检查 `memubot_config.memory_os.memory_health_enabled`）。
- store：`state.memory_graph_store: Arc<MemoryGraphStore>`，连接 `store.conn.lock()`（`Mutex<rusqlite::Connection>`）。
- 输入/DTO 结构惯例：定义在 `src-tauri/src/ipc.rs`，`#[serde(rename_all = "camelCase")]`，字段 snake_case（线上 camelCase）。
- TS 类型在 `ui/src/lib/types.ts`（camelCase）；wrapper 在 `ui/src/lib/tauri-bridge.ts`：`(input: T) => invoke('cmd', { input })`。
- Rust 测试惯例（drift_detection.rs 已有）：`fn fresh_conn() -> Connection { let c = Connection::open_in_memory().unwrap(); crate::db::migrations::run(&c).unwrap(); c.execute("PRAGMA foreign_keys = OFF", []).unwrap(); c }`，种数据用 `INSERT INTO memory_nodes (id, space_id, kind, title) VALUES (...)`。

**验证命令：**
- Rust 编译：`cd src-tauri && cargo build > /tmp/e.txt 2>&1; echo "EXIT=$?"; grep -E "^error" /tmp/e.txt | head`
- Rust 测试：`cd src-tauri && cargo test --lib drift_detection > /tmp/e_d.txt 2>&1; grep "test result" /tmp/e_d.txt` 和 `cargo test --lib importance_decay`
- TS 检查：`cd ui && npx tsc --noEmit > /tmp/e_ts.txt 2>&1; grep -c "MemoryHealthPanel\|tauri-bridge\|types.ts" /tmp/e_ts.txt`（仓库另有 ~15 个无关 pre-existing 测试文件报错，忽略）
- Vitest：`cd ui && npm test -- --run MemoryHealthPanel > /tmp/e_v.txt 2>&1; grep -E "Tests " /tmp/e_v.txt`

**IRON RULE**：build/test 命令永远先重定向到文件再 grep，绝不用 `| tail` 取退出码（管道会让 `$?` 变成 tail 的）。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/src/memory_graph/drift_detection.rs` (改) | 加 `DriftEventRow` + `list_open_drift_events` + `resolve_drift_event` + 测试 |
| `src-tauri/src/memory_graph/importance_decay.rs` (改) | 加 `ImportanceRow` + `list_decay_candidates` + 测试 |
| `src-tauri/src/ipc.rs` (改) | 加 3 个 input 结构 + 2 个 DTO（camelCase serde） |
| `src-tauri/src/tauri_commands.rs` (改) | 3 个 `#[tauri::command]` |
| `src-tauri/src/main.rs` (改) | invoke_handler 注册 3 个命令 |
| `ui/src/lib/types.ts` (改) | 3 input + 2 DTO 的 TS 镜像 |
| `ui/src/lib/tauri-bridge.ts` (改) | 3 个 invoke 包装 |
| `ui/src/components/memory/MemoryHealthPanel.tsx` (改) | `CollapsibleSection` + Drift（resolve）+ Importance（只读）两节 |
| `ui/src/components/memory/MemoryHealthPanel.test.tsx` (新) | vitest |

---

## Task 1: 后端读/写函数 + Rust 单测

**Files:**
- Modify: `src-tauri/src/memory_graph/drift_detection.rs`
- Modify: `src-tauri/src/memory_graph/importance_decay.rs`

- [ ] **Step 1: drift_detection.rs — 加行结构 + 两个 fn**（放在 `list_open_events_for_node` 之后、`#[cfg(test)]` 之前）

```rust
/// 全局漂移事件行（join memory_nodes 取标题）。供 Health 面板列表用。
#[derive(Debug, Clone)]
pub struct DriftEventRow {
    pub id: String,
    pub node_id: String,
    pub title: String,
    pub score: f64,
    pub computed_at: i64,
}

/// 列出某 space 下所有 open 漂移事件，按 score 降序。
pub fn list_open_drift_events(
    conn: &Connection,
    space_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<DriftEventRow>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT d.id, d.node_id, n.title, d.score, d.computed_at
         FROM drift_events d
         JOIN memory_nodes n ON n.id = d.node_id
         WHERE d.status = 'open' AND n.space_id = ?1
         ORDER BY d.score DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![space_id, limit as i64], |r| {
            Ok(DriftEventRow {
                id: r.get(0)?,
                node_id: r.get(1)?,
                title: r.get(2)?,
                score: r.get(3)?,
                computed_at: r.get(4)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// 把一条漂移事件标记为已处理（open → resolved）。
pub fn resolve_drift_event(
    conn: &Connection,
    id: &str,
    note: Option<&str>,
    now_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE drift_events
         SET status = 'resolved', resolved_at = ?2, resolution_note = ?3
         WHERE id = ?1",
        params![id, now_ms, note],
    )?;
    Ok(())
}
```

- [ ] **Step 2: drift_detection.rs — 在 `mod tests` 末尾加测试**（复用已有 `fresh_conn`）

```rust
    #[test]
    fn list_open_drift_events_orders_by_score_and_filters_space() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n1','default','reference','Node One')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n2','default','reference','Node Two')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n3','other','reference','Other Space')",
            [],
        ).unwrap();
        // open events: n1 score 0.9, n2 score 0.4; n3 in other space; one resolved on n1
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e1','n1',0.9,'[]',100,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e2','n2',0.4,'[]',101,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e3','n3',0.99,'[]',102,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e4','n1',0.5,'[]',103,'resolved')", []).unwrap();

        let rows = list_open_drift_events(&conn, "default", 100).unwrap();
        assert_eq!(rows.len(), 2); // only open, only default space
        assert_eq!(rows[0].id, "e1"); // score DESC
        assert_eq!(rows[0].title, "Node One"); // join worked
        assert_eq!(rows[1].id, "e2");
        assert!(list_open_drift_events(&conn, "default", 0).unwrap().is_empty());
    }

    #[test]
    fn resolve_drift_event_flips_status_and_removes_from_list() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n1','default','reference','N')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e1','n1',0.9,'[]',100,'open')", []).unwrap();
        resolve_drift_event(&conn, "e1", Some("looked fine"), 999).unwrap();
        assert!(list_open_drift_events(&conn, "default", 100).unwrap().is_empty());
        let (status, note, resolved): (String, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT status, resolution_note, resolved_at FROM drift_events WHERE id='e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "resolved");
        assert_eq!(note.as_deref(), Some("looked fine"));
        assert_eq!(resolved, Some(999));
    }
```

- [ ] **Step 3: importance_decay.rs — 加行结构 + list fn**（放在 `batch_recompute_importance` 之后、`#[cfg(test)]` 之前；确认文件顶部已 `use rusqlite::{params, Connection};`，没有则补）

```rust
/// 衰减候选行（join memory_nodes 取标题）。算法已用 archive_pending_since 标记。
#[derive(Debug, Clone)]
pub struct ImportanceRow {
    pub node_id: String,
    pub title: String,
    pub importance: f64,
    pub archive_pending_since: Option<i64>,
    pub last_computed_at: i64,
}

/// 列出某 space 下的衰减候选（archive_pending_since 非空），importance 升序。
pub fn list_decay_candidates(
    conn: &Connection,
    space_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ImportanceRow>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT s.node_id, n.title, s.importance, s.archive_pending_since, s.last_computed_at
         FROM memory_importance_scores s
         JOIN memory_nodes n ON n.id = s.node_id
         WHERE s.archive_pending_since IS NOT NULL AND n.space_id = ?1
         ORDER BY s.importance ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![space_id, limit as i64], |r| {
            Ok(ImportanceRow {
                node_id: r.get(0)?,
                title: r.get(1)?,
                importance: r.get(2)?,
                archive_pending_since: r.get(3)?,
                last_computed_at: r.get(4)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}
```

- [ ] **Step 4: importance_decay.rs — 加测试**（若 `mod tests` 没有 `fresh_conn` helper，先加上面那个；用全列 INSERT 避免 NOT NULL 报错）

```rust
    #[test]
    fn list_decay_candidates_only_archive_pending_asc_filtered_by_space() {
        let conn = fresh_conn();
        for (id, sp, title) in [("n1","default","Low"),("n2","default","Lower"),("n3","default","NotPending"),("n4","other","OtherSpace")] {
            conn.execute(
                "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES (?1,?2,'reference',?3)",
                params![id, sp, title],
            ).unwrap();
        }
        // helper to insert a full importance row
        let ins = |c: &Connection, node: &str, imp: f64, pending: Option<i64>| {
            c.execute(
                "INSERT INTO memory_importance_scores
                 (node_id, base_value, citation_factor, edge_factor, recency_factor, status_bonus, penalty, importance, decay_half_life_days, last_computed_at, archive_pending_since)
                 VALUES (?1, 0.0,0.0,0.0,0.0,0.0,0.0, ?2, 30.0, 500, ?3)",
                params![node, imp, pending],
            ).unwrap();
        };
        ins(&conn, "n1", 0.20, Some(400));
        ins(&conn, "n2", 0.05, Some(401)); // lowest importance → first
        ins(&conn, "n3", 0.10, None);      // not pending → excluded
        ins(&conn, "n4", 0.01, Some(402)); // other space → excluded

        let rows = list_decay_candidates(&conn, "default", 100).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].node_id, "n2"); // importance ASC
        assert_eq!(rows[0].title, "Lower");
        assert_eq!(rows[1].node_id, "n1");
        assert!(list_decay_candidates(&conn, "default", 0).unwrap().is_empty());
    }
```

- [ ] **Step 5: 编译 + 跑两个模块测试**

Run: `cd src-tauri && cargo test --lib drift_detection > /tmp/e1d.txt 2>&1; grep "test result" /tmp/e1d.txt; cargo test --lib importance_decay > /tmp/e1i.txt 2>&1; grep "test result" /tmp/e1i.txt`
Expected: 两个模块测试都 `ok`（drift 既有测试 + 2 新；importance 既有 + 1 新）。若 importance_decay.rs 顶部缺 `use rusqlite::{params, Connection};` 会编译报错 → 补上。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/memory_graph/drift_detection.rs src-tauri/src/memory_graph/importance_decay.rs
git commit -m "feat(memory): drift list+resolve fns + importance decay-candidate list fn + tests"
```

---

## Task 2: ipc.rs 结构 + 3 个 Tauri 命令 + 注册

**Files:**
- Modify: `src-tauri/src/ipc.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: ipc.rs — 加 input + DTO 结构**（放在 `HealthFindingDto` 附近，保持 camelCase 惯例）

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftListInput {
    pub space_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftResolveInput {
    pub event_id: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportanceListInput {
    pub space_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftEventDto {
    pub id: String,
    pub node_id: String,
    pub title: String,
    pub score: f64,
    pub computed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportanceCandidateDto {
    pub node_id: String,
    pub title: String,
    pub importance: f64,
    pub archive_pending_since: Option<i64>,
    pub last_computed_at: i64,
}
```

> 确认 ipc.rs 顶部有 `use serde::{Deserialize, Serialize};`（HealthFindingDto 用了 Serialize，应已在）。

- [ ] **Step 2: tauri_commands.rs — 加 3 个命令**（放在 `memory_health_*` 命令附近；`use crate::ipc::{...}` 按文件现有惯例引入，或用全路径 `crate::ipc::DriftListInput`）

```rust
#[tauri::command]
pub async fn memory_drift_list_events(
    state: State<'_, AppState>,
    input: crate::ipc::DriftListInput,
) -> Result<Vec<crate::ipc::DriftEventDto>, String> {
    ensure_memory_health_enabled(&state).await?;
    let space_id = input.space_id.unwrap_or_else(|| "default".into());
    let limit = input.limit.unwrap_or(100);
    let conn = state
        .memory_graph_store
        .conn
        .lock()
        .map_err(|e| format!("DB lock: {e}"))?;
    let rows = crate::memory_graph::drift_detection::list_open_drift_events(&conn, &space_id, limit)
        .map_err(|e| format!("list drift: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|r| crate::ipc::DriftEventDto {
            id: r.id,
            node_id: r.node_id,
            title: r.title,
            score: r.score,
            computed_at: r.computed_at,
        })
        .collect())
}

#[tauri::command]
pub async fn memory_drift_resolve_event(
    state: State<'_, AppState>,
    input: crate::ipc::DriftResolveInput,
) -> Result<(), String> {
    ensure_memory_health_enabled(&state).await?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let conn = state
        .memory_graph_store
        .conn
        .lock()
        .map_err(|e| format!("DB lock: {e}"))?;
    crate::memory_graph::drift_detection::resolve_drift_event(
        &conn,
        &input.event_id,
        input.note.as_deref(),
        now_ms,
    )
    .map_err(|e| format!("resolve drift: {e}"))
}

#[tauri::command]
pub async fn memory_importance_list_candidates(
    state: State<'_, AppState>,
    input: crate::ipc::ImportanceListInput,
) -> Result<Vec<crate::ipc::ImportanceCandidateDto>, String> {
    ensure_memory_health_enabled(&state).await?;
    let space_id = input.space_id.unwrap_or_else(|| "default".into());
    let limit = input.limit.unwrap_or(100);
    let conn = state
        .memory_graph_store
        .conn
        .lock()
        .map_err(|e| format!("DB lock: {e}"))?;
    let rows = crate::memory_graph::importance_decay::list_decay_candidates(&conn, &space_id, limit)
        .map_err(|e| format!("list importance: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|r| crate::ipc::ImportanceCandidateDto {
            node_id: r.node_id,
            title: r.title,
            importance: r.importance,
            archive_pending_since: r.archive_pending_since,
            last_computed_at: r.last_computed_at,
        })
        .collect())
}
```

> 注意：`chrono::Utc::now().timestamp_millis()` —— 确认 chrono 在用（仓库广泛用）。`MemoryGraphStore.conn` 字段名按既有 `memory_health_list_findings` 的 `store.conn.lock()` 用法（已核对）。

- [ ] **Step 3: main.rs — 注册 3 个命令**（在 `tauri::generate_handler![` 里，靠近 `memory_health_*` 注册行插入）

```rust
            uclaw_core::tauri_commands::memory_drift_list_events,
            uclaw_core::tauri_commands::memory_drift_resolve_event,
            uclaw_core::tauri_commands::memory_importance_list_candidates,
```

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build > /tmp/e2.txt 2>&1; echo "EXIT=$?"; grep -E "^error" /tmp/e2.txt | head`
Expected: EXIT=0，无 error。常见坑：`ensure_memory_health_enabled` 是私有 fn（在 tauri_commands.rs 内，同模块可调，OK）；`State` 已在文件顶部导入（既有命令在用）。

- [ ] **Step 5: 确认 Task 1 测试仍过**

Run: `cd src-tauri && cargo test --lib drift_detection > /tmp/e2t.txt 2>&1; grep "test result" /tmp/e2t.txt`
Expected: `ok`。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(tauri): memory_drift_* + memory_importance_list_candidates commands + invoke_handler"
```

---

## Task 3: TS 类型 + bridge 包装

**Files:**
- Modify: `ui/src/lib/types.ts`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: types.ts — 加 input + DTO 镜像**（放在 Health 相关类型附近）

```ts
export interface DriftListInput {
  spaceId?: string;
  limit?: number;
}

export interface DriftResolveInput {
  eventId: string;
  note?: string;
}

export interface ImportanceListInput {
  spaceId?: string;
  limit?: number;
}

export interface DriftEventDto {
  id: string;
  nodeId: string;
  title: string;
  score: number;
  computedAt: number; // epoch millis
}

export interface ImportanceCandidateDto {
  nodeId: string;
  title: string;
  importance: number;
  archivePendingSince: number | null;
  lastComputedAt: number;
}
```

- [ ] **Step 2: tauri-bridge.ts — 加 3 个包装 + import 类型**（放在 `memoryHealth*` 包装附近；types import 块里加这 5 个新类型名）

```ts
export const memoryDriftListEvents = (
  input: DriftListInput,
): Promise<DriftEventDto[]> =>
  invoke('memory_drift_list_events', { input });

export const memoryDriftResolveEvent = (
  input: DriftResolveInput,
): Promise<void> =>
  invoke('memory_drift_resolve_event', { input });

export const memoryImportanceListCandidates = (
  input: ImportanceListInput,
): Promise<ImportanceCandidateDto[]> =>
  invoke('memory_importance_list_candidates', { input });
```

> 把 `DriftListInput, DriftResolveInput, ImportanceListInput, DriftEventDto, ImportanceCandidateDto` 加进 tauri-bridge.ts 顶部从 `'./types'`（或现有 types import 路径）的 import 列表。

- [ ] **Step 3: TS 检查**

Run: `cd ui && npx tsc --noEmit > /tmp/e3.txt 2>&1; echo done; grep -c "tauri-bridge\|types.ts" /tmp/e3.txt`
Expected: 0（这两文件无新错误；仓库 ~15 个 pre-existing 无关错误忽略）。

- [ ] **Step 4: 提交**

```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts
git commit -m "feat(ui): tauri-bridge wrappers + DTO types for drift events + importance candidates"
```

---

## Task 4: MemoryHealthPanel 两节（Drift resolve + Importance 只读）

**Files:**
- Modify: `ui/src/components/memory/MemoryHealthPanel.tsx`

> 现有面板结构：root `div[data-testid="memory-health-panel"]` → header → error banner → `<ScrollArea><div className="p-3 space-y-3"> {findings groups} </div></ScrollArea>`。两节加在 findings groups **之后**、仍在那个 `space-y-3` 容器内。保留全部现有 findings 逻辑不动。

- [ ] **Step 1: import 区补充**

把 lucide import（现 `import { Loader2, RefreshCw, ShieldCheck, X, ExternalLink } from 'lucide-react'`）改为：

```tsx
import { Loader2, RefreshCw, ShieldCheck, X, ExternalLink, ChevronRight, ChevronDown, TrendingDown, Activity } from 'lucide-react'
```

把 tauri-bridge import 块（现含 `memoryHealthListFindings` 等）追加：

```tsx
  memoryDriftListEvents,
  memoryDriftResolveEvent,
  memoryImportanceListCandidates,
```

把 types import（现 `import type { HealthFindingDto, HealthRunOutcome } from '@/lib/types'`）改为：

```tsx
import type { HealthFindingDto, HealthRunOutcome, DriftEventDto, ImportanceCandidateDto } from '@/lib/types'
```

- [ ] **Step 2: 加 CollapsibleSection 组件**（放在文件末尾，FindingRow 之后）

```tsx
function CollapsibleSection({
  title,
  icon,
  count,
  defaultOpen,
  children,
}: {
  title: string
  icon: React.ReactNode
  count: number
  defaultOpen?: boolean
  children: React.ReactNode
}): React.ReactElement {
  const [open, setOpen] = React.useState<boolean>(defaultOpen ?? true)
  return (
    <div className="border-t border-border/40 pt-2">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center gap-1.5 text-[10px] uppercase tracking-wide font-medium text-muted-foreground hover:text-foreground transition-colors"
      >
        {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        {icon}
        <span>{title}</span>
        <span className="text-muted-foreground/60">({count})</span>
      </button>
      {open && <div className="mt-1.5 space-y-1">{children}</div>}
    </div>
  )
}
```

- [ ] **Step 3: 加 state + fetchers**（在 `MemoryHealthPanel` 组件内，现有 `dismissingIds` state 之后）

```tsx
  const [drift, setDrift] = React.useState<DriftEventDto[]>([])
  const [driftError, setDriftError] = React.useState<string | null>(null)
  const [resolvingIds, setResolvingIds] = React.useState<Set<string>>(new Set())
  const [importance, setImportance] = React.useState<ImportanceCandidateDto[]>([])
  const [importanceError, setImportanceError] = React.useState<string | null>(null)

  const fetchDrift = React.useCallback(async () => {
    setDriftError(null)
    try {
      setDrift(await memoryDriftListEvents({ spaceId: space, limit: 100 }))
    } catch (e) {
      setDriftError(`加载漂移失败: ${String(e)}`)
    }
  }, [space])

  const fetchImportance = React.useCallback(async () => {
    setImportanceError(null)
    try {
      setImportance(await memoryImportanceListCandidates({ spaceId: space, limit: 100 }))
    } catch (e) {
      setImportanceError(`加载重要度失败: ${String(e)}`)
    }
  }, [space])
```

- [ ] **Step 4: 在现有 `React.useEffect(() => { void fetchFindings() }, [fetchFindings])` 之后并行拉两节**

```tsx
  React.useEffect(() => {
    void fetchDrift()
    void fetchImportance()
  }, [fetchDrift, fetchImportance])
```

- [ ] **Step 5: 加 resolve handler**（在现有 `handleDismiss` 之后）

```tsx
  const handleResolveDrift = async (id: string): Promise<void> => {
    setResolvingIds((prev) => new Set(prev).add(id))
    try {
      await memoryDriftResolveEvent({ eventId: id })
      setDrift((prev) => prev.filter((d) => d.id !== id))
    } catch (e) {
      toast.error(`标记失败: ${String(e)}`)
    } finally {
      setResolvingIds((prev) => {
        const next = new Set(prev)
        next.delete(id)
        return next
      })
    }
  }
```

- [ ] **Step 6: 在 findings `space-y-3` 容器内、findings 渲染块之后插入两节**

找到 ScrollArea 里 `{/* Any severities outside the canonical 3 (forward-compat) */}` 那段 map 之后（仍在 `<div className="p-3 space-y-3">` 内），插入：

```tsx
          {/* 子项目 E — 概念漂移 */}
          <CollapsibleSection
            title="概念漂移"
            icon={<Activity className="size-3" />}
            count={drift.length}
            defaultOpen={drift.length > 0}
          >
            {driftError ? (
              <p className="text-[10px] text-destructive">{driftError}</p>
            ) : drift.length === 0 ? (
              <p className="text-[10px] text-muted-foreground/60">无漂移事件</p>
            ) : (
              <div data-testid="health-drift-list" className="space-y-1">
                {drift.map((d) => (
                  <div
                    key={d.id}
                    className="flex items-start gap-2 px-2 py-1.5 rounded-sm text-xs hover:bg-muted/60 border border-transparent hover:border-border/40"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5">
                        <span className="font-medium truncate">{d.title || d.nodeId}</span>
                        <Badge
                          variant="outline"
                          className={cn(
                            'text-[9px] px-1 py-0',
                            d.score >= 0.6 ? 'border-destructive/50 text-destructive' : 'border-amber-500/50 text-amber-500',
                          )}
                        >
                          drift {d.score.toFixed(2)}
                        </Badge>
                      </div>
                      <span className="text-[10px] text-muted-foreground/50">
                        {formatDateTime(new Date(d.computedAt).toISOString())}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => void handleResolveDrift(d.id)}
                      disabled={resolvingIds.has(d.id)}
                      className="p-1 rounded-sm text-muted-foreground/60 hover:text-foreground hover:bg-muted transition-colors disabled:opacity-50"
                      title="标记已处理"
                      data-testid="health-drift-resolve"
                    >
                      {resolvingIds.has(d.id) ? <Loader2 className="size-3 animate-spin" /> : <X className="size-3" />}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </CollapsibleSection>

          {/* 子项目 E — 重要度衰减候选（只读） */}
          <CollapsibleSection
            title="重要度 · 衰减候选"
            icon={<TrendingDown className="size-3" />}
            count={importance.length}
            defaultOpen={false}
          >
            {importanceError ? (
              <p className="text-[10px] text-destructive">{importanceError}</p>
            ) : importance.length === 0 ? (
              <p className="text-[10px] text-muted-foreground/60">无衰减候选</p>
            ) : (
              <div data-testid="health-importance-list" className="space-y-1">
                {importance.map((it) => (
                  <div
                    key={it.nodeId}
                    className="flex items-center gap-2 px-2 py-1.5 rounded-sm text-xs hover:bg-muted/60 border border-transparent hover:border-border/40"
                  >
                    <span className="flex-1 min-w-0 font-medium truncate">{it.title || it.nodeId}</span>
                    <div className="w-16 h-1.5 rounded-full bg-muted overflow-hidden" title={`importance ${it.importance.toFixed(3)}`}>
                      <div className="h-full bg-muted-foreground/50" style={{ width: `${Math.round(it.importance * 100)}%` }} />
                    </div>
                    <span className="text-[10px] text-muted-foreground/60 tabular-nums">{it.importance.toFixed(2)}</span>
                  </div>
                ))}
              </div>
            )}
          </CollapsibleSection>
```

> 注意：两节放在 `findings.length === 0 ? <EmptyState/> : ...` 的三元**外面**、但仍在 `<div className="p-3 space-y-3">` 容器内 —— 这样即使无 findings，两节照样显示。实现时把现有 findings 三元包成一块，两节紧随其后，都是 `space-y-3` 的直接子节点。

- [ ] **Step 7: TS 检查**

Run: `cd ui && npx tsc --noEmit > /tmp/e4.txt 2>&1; echo done; grep -c "MemoryHealthPanel" /tmp/e4.txt`
Expected: 0 MemoryHealthPanel 错误。`Badge`/`cn`/`formatDateTime`/`toast` 现有 import 已具备。

- [ ] **Step 8: 提交**

```bash
git add ui/src/components/memory/MemoryHealthPanel.tsx
git commit -m "feat(ui): MemoryHealthPanel — collapsible Drift (resolve) + Importance (read-only) sections"
```

---

## Task 5: MemoryHealthPanel vitest

**Files:**
- Create: `ui/src/components/memory/MemoryHealthPanel.test.tsx`

- [ ] **Step 1: 创建测试**（mock invoke，覆盖两节 + resolve + 行内错误 + 空态）

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { MemoryHealthPanel } from './MemoryHealthPanel'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}))

function routeInvoke(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      memory_health_list_findings: [],
      memory_drift_list_events: [
        { id: 'e1', nodeId: 'n1', title: 'Drifting Page', score: 0.82, computedAt: 1715000000000 },
      ],
      memory_importance_list_candidates: [
        { nodeId: 'n9', title: 'Fading Note', importance: 0.07, archivePendingSince: 1714000000000, lastComputedAt: 1715000000000 },
      ],
      memory_drift_resolve_event: undefined,
      ...overrides,
    }
    const v = table[cmd]
    if (v instanceof Error) return Promise.reject(v)
    return Promise.resolve(v)
  })
}

describe('MemoryHealthPanel — E sections', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    routeInvoke()
  })

  it('renders drift + importance sections from their commands', async () => {
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText('Drifting Page')).toBeInTheDocument()
    expect(screen.getByText(/drift 0.82/)).toBeInTheDocument()
    // importance section is collapsed by default → expand it
    await screen.findByText('重要度 · 衰减候选')
  })

  it('resolves a drift event and removes the row', async () => {
    const { user } = renderWithProviders(<MemoryHealthPanel />)
    await screen.findByText('Drifting Page')
    await user.click(screen.getByTestId('health-drift-resolve'))
    await waitFor(() => expect(screen.queryByText('Drifting Page')).not.toBeInTheDocument())
    expect(invokeMock).toHaveBeenCalledWith('memory_drift_resolve_event', { input: { eventId: 'e1' } })
  })

  it('shows inline error when drift command fails (findings + importance unaffected)', async () => {
    routeInvoke({ memory_drift_list_events: new Error('boom') })
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText(/加载漂移失败/)).toBeInTheDocument()
  })

  it('shows empty-state text when a section has no rows', async () => {
    routeInvoke({ memory_drift_list_events: [], memory_importance_list_candidates: [] })
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText('无漂移事件')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 跑 vitest**

Run: `cd ui && npm test -- --run MemoryHealthPanel > /tmp/e5.txt 2>&1; echo "EXIT=$?"; grep -E "Tests " /tmp/e5.txt`
Expected: 4 passed。可能调整点：
- 重要度 section 默认折叠 → "Fading Note" 初始不在 DOM；测试 1 只断言 section 标题在（已如此）。若想断言行,先 `await user.click(screen.getByText('重要度 · 衰减候选'))` 展开。
- `renderWithProviders` 返回 `user`（已确认）；若某 `getByText` 命中多元素,用 `findAllByText` 或 `within` 缩小范围。允许调整**测试**,不许改组件。

- [ ] **Step 3: 提交**

```bash
git add ui/src/components/memory/MemoryHealthPanel.test.tsx
git commit -m "test(ui): MemoryHealthPanel vitest for Drift + Importance sections"
```

---

## 手动 E2E 验证清单（写进 PR）

跑真 app（drift/importance proactive 已调度产数据）→ 万花筒 › 记忆 › Health tab：
1. 现有结构性 findings 照常显示（未回归）。
2. 「概念漂移」节有数据、score 徽章按 ≥0.6 红/否则琥珀着色。
3. 点漂移行「标记已处理」→ 行消失；重启/刷新确认该事件不再 open。
4. 「重要度·衰减候选」节展开后显示低分节点 + importance 条（只读，无按钮）。
5. memory_os 关闭时两节显示加载失败/空态，不崩溃。

---

## 自检（对照 spec）

- **Spec 覆盖**：spec §3 两 fn + resolve → Task 1；§4 三命令 → Task 2；§5 两节 + CollapsibleSection + resolve + 只读 → Task 4；§6 错误隔离（每节独立 try/catch + 行内错误）→ Task 3/4；§7 测试 → Task 1（Rust）+ Task 5（vitest）+ 手动清单；§8 边界（不碰 findings/调度/归档/Spaced-Rep/Triangulation/Timeline/gbrain）→ 遵守。
- **占位符**：无 TBD。唯一"plan 阶段须核对"项（memory_nodes 列名）已在写计划前核对完（title/space_id 确认），SQL 用真列名。
- **类型一致**：Rust `DriftEventRow`→`DriftEventDto`(camelCase) ↔ TS `DriftEventDto`(`nodeId/computedAt`)；`ImportanceRow`→`ImportanceCandidateDto` ↔ TS（`archivePendingSince/lastComputedAt`）；命令名 snake_case 两侧一致；resolve 参数 `eventId`(TS) ↔ `event_id`(Rust serde camelCase)。
- **范围**：单 PR、5 commit、无新 migration、无新依赖。
