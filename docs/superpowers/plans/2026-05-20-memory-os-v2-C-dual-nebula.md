# 子项目 C — 万花筒融合双星云 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 memory_nodes 记忆星云 + gbrain pages 知识星云渲进一个融合 3D 场景（两层按层着色、各自内部边、统一交互），并复用已有编辑器（MemoryNodeCard / WikiView）做编辑与版本史。

**Architecture:** 先把 `MemoryNebulaView.tsx`（772 行）里层无关的 Three.js 原语抽到 `nebula/` 共享模块（StarNode 改为接受 color prop、computeGalaxyLayout 泛化加 centerOffset）；新建 `gbrain_full_graph`（list_pages + 逐页 get_links 拼装）；新 `DualNebulaView` 用纯函数 `buildUnifiedScene` 把两层映射成统一节点/边再渲染。无新 migration、无新依赖。

**Tech Stack:** Rust（rusqlite/serde via MCP 代理）、React 18 + TS、@react-three/fiber + drei + three、Vitest。

---

## 已核对的事实

- **MemoryNebulaView.tsx 结构图**（来自侦察）：
  - 层无关「REUSABLE」：`gaussian`(138–142)、`hashCode`(383–389)、`NodePosition`(131–136)、glow+star 四段 shader 字符串(243–370)、`NebulaThemeConfig`(54–66)+`getNebulaThemeConfig`(68–127)、`createNebulaTexture`(578–591)+`NebulaDust`(593–638)、`AutoRotateControls`(642–671)、`MemoryEdgeLines`(534–574)。
  - 「需改造后可复用」：`StarNode`(391–489) —— 内部调 `getNodeConfig(node)` 从 kind 推色，**无 color prop**；`StarNodeProps`(374–381)；`computeGalaxyLayout`(144–239) —— 读 `node.kind` 对比硬编码 `coreKinds=['boot','identity','directive']` / `midKinds=['value','user_profile','curated']`。
  - 「MEMORY-SPECIFIC，留在原文件」：`KIND_CONFIG`(19–29)、`FRAGMENT_NEBULA_CONFIG`(32–42)、`getNodeConfig`(45–50)、`MemoryNodesMesh`(493–530)、`SceneContent`(675–712)、`MemoryNebulaView`(716–772)。
  - 主组件用 jotai：`import { resolvedThemeAtom, themeStyleAtom } from '@/atoms/theme'`；`useAtomValue(resolvedThemeAtom)`→`'light'|'dark'`，`useAtomValue(themeStyleAtom)`→`ThemeStyle`。
- gbrain `get_links(slug)` = "outgoing links"，返回 `Link[]`（`{from_slug,to_slug,link_type,...}`，与 `get_backlinks` 同形）。`list_pages` 返回 `{slug,type,title,updated_at}[]`。A 的 `browse.rs` 已有 `PageSummary{slug,title,page_type,updated_at}`、`call_gbrain`、`list_pages`、`parse_*` 模式、`GbrainError`。
- `MemoryGraphData{nodes: MemoryNode[], edges: MemoryEdge[], routes}`；`MemoryNode{id,spaceId,kind,title,...}`；`MemoryEdge{id,parentNodeId?,childNodeId,relationKind,...}`；`MemoryNodeKind` 9 值。
- `MemoryModule.tsx`：`graphData` state ← `memoryGraphGetFullGraph()`；`selectedNodeId` → `MemoryNodeCard` 弹窗；`activeTab` + `TABS` 数组（8 tab）；nebula tab 渲 `<MemoryNebulaView graphData onSelectNode className/>`。
- A 的 `WikiView` props 现为 `{spaceId?, className?}`，内部 `openPage(slug)` 加载详情。

**验证命令：**
- Rust：`cd src-tauri && cargo test --lib gbrain::browse > /tmp/c.txt 2>&1; grep "test result" /tmp/c.txt`；`cargo build > /tmp/cb.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/cb.txt|head`
- TS：`cd ui && npx tsc --noEmit > /tmp/cts.txt 2>&1; grep -cE "MemoryNebulaView|DualNebulaView|nebula/|gbrain-browse|MemoryModule|WikiView" /tmp/cts.txt`（仓库另有 ~15 个无关 pre-existing 测试文件报错）
- Vitest：`cd ui && npm test -- --run <name> > /tmp/cv.txt 2>&1; grep -E "Tests " /tmp/cv.txt`
- **IRON RULE**：build/test 先重定向到文件再 grep，绝不 `| tail` 取退出码。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `ui/src/components/memory/nebula/shaders.ts` (新) | 四段 GLSL 字符串（glow/star × vertex/fragment） |
| `ui/src/components/memory/nebula/theme.ts` (新) | `NebulaThemeConfig` + `getNebulaThemeConfig` |
| `ui/src/components/memory/nebula/layout.ts` (新) | `NodePosition`、`gaussian`、`hashCode`、泛化 `computeGalaxyLayout(nodes,edges,centerOffset?)` |
| `ui/src/components/memory/nebula/primitives.tsx` (新) | `StarNode`(+color/emissive props)、`EdgeLines`、`NebulaDust`+`createNebulaTexture`、`AutoRotateControls` |
| `ui/src/components/memory/MemoryNebulaView.tsx` (改) | 删被抽走的符号，import 共享原语；保留 KIND_CONFIG/getNodeConfig/MemoryNodesMesh/SceneContent；适配 StarNode 新签名 + layout 新签名 |
| `src-tauri/src/gbrain/browse.rs` (改) | `KnowledgeNode/Edge/Graph` 类型 + 纯 `assemble_graph` + `parse_links` + `full_graph` async + tests |
| `src-tauri/src/tauri_commands.rs` + `main.rs` (改) | `gbrain_full_graph` 命令 + 注册 |
| `ui/src/lib/gbrain-browse.ts` (改) | `KnowledgeGraph` 类型 + `gbrainFullGraph` 包装 |
| `ui/src/components/memory/dual-nebula/buildUnifiedScene.ts` (新) | 纯映射函数 + 类型 |
| `ui/src/components/memory/DualNebulaView.tsx` (新) | 融合画布组件 |
| `ui/src/components/memory/WikiView.tsx` (改) | 加 `initialSlug?` prop |
| `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx` (改) | 'dual' tab + 知识图拉取 + 点击路由 |
| 测试 | `buildUnifiedScene.test.ts`、`DualNebulaView.test.tsx`，扩展 `browse.rs`/`WikiView.test.tsx` |

---

## Task 1: 抽取 nebula 共享原语（重构，行为不变）

**Files:** Create `nebula/shaders.ts`、`nebula/theme.ts`、`nebula/layout.ts`、`nebula/primitives.tsx`；Modify `MemoryNebulaView.tsx`。

> 这是**机械搬迁现有可工作代码** + 3 处语义改造。搬迁部分按符号名搬（别重打 GLSL/Three 代码）。3 处语义改造给出完整新代码。3D 无法单测，验证靠 `tsc` 干净 + 手动看 nebula tab 渲染不变。

- [ ] **Step 1: 创建 `nebula/shaders.ts`** —— 从 MemoryNebulaView.tsx **剪切**四段 shader 字符串常量（`glowVertexShader`/`glowFragmentShader`(243–266)、`starVertexShader`/`starFragmentShader`(270–370)），每个加 `export const`。

- [ ] **Step 2: 创建 `nebula/theme.ts`** —— 剪切 `NebulaThemeConfig` 接口(54–66) + `getNebulaThemeConfig`(68–127)，加 `export`。顶部需 `import type { ThemeStyle } from '@/lib/chat-types'`。

- [ ] **Step 3: 创建 `nebula/layout.ts`** —— 剪切 `NodePosition`(131–136)、`gaussian`(138–142)、`hashCode`(383–389) + `computeGalaxyLayout`(144–239)，加 `export`。**泛化 computeGalaxyLayout 签名**（语义改造 1）：

  把签名从 `(nodes: MemoryNode[], edges: MemoryEdge[])` 改成层无关：
  ```ts
  export interface LayoutNode { id: string; kind: string }
  export interface LayoutEdge { from: string; to: string }

  export function computeGalaxyLayout(
    nodes: LayoutNode[],
    edges: LayoutEdge[],
    centerOffset: [number, number, number] = [0, 0, 0],
  ): NodePosition[] {
    // … 原有 3-ring + force-directed 逻辑保持不变，但：
    // (a) coreKinds/midKinds 比较改成对 node.kind 字符串比较（已是字符串，照旧；
    //     不命中的 kind（含所有 gbrain type）自然落 outer ring —— 正是知识层想要的）。
    // (b) 最终每个 NodePosition 的 x/y/z 各加上对应 centerOffset 分量。
    // 其余不动。
  }
  ```
  - 原逻辑里 `coreKinds: MemoryNodeKind[]` 改为 `const coreKinds = ['boot','identity','directive']`、`midKinds = ['value','user_profile','curated']`（普通 string[]），`node.kind` 与之 `.includes()` 比较。
  - 在 return 前对每个 position 做 `x += centerOffset[0]; y += centerOffset[1]; z += centerOffset[2]`。

- [ ] **Step 4: 创建 `nebula/primitives.tsx`** —— 剪切 `createNebulaTexture`(578–591)、`NebulaDust`(593–638)、`AutoRotateControls`(642–671)、`MemoryEdgeLines`(534–574)（**重命名为 `EdgeLines`** + 其 props 接口重命名 `EdgeLinesProps`，并把内部对 positions 的消费保持泛型——它只用 `{id,x,y,z}` 和 edge 的 from/to，不依赖 MemoryNode）、`StarNode`(391–489) + `StarNodeProps`(374–381)。顶部 import 共享 shaders/layout/theme + drei/three/fiber（照原文件 import）。

  **StarNode 改造（语义改造 2）**：把它从「接 `node` 内部 `getNodeConfig` 推色」改成「接显式视觉 props」：
  ```ts
  export interface StarNodeProps {
    id: string
    position: [number, number, number]
    color: string
    emissive: string
    radius: number
    isHovered: boolean
    onHover: (id: string | null) => void
    onClick: (id: string) => void
    themeConfig: NebulaThemeConfig
  }
  // StarNode 内部：删掉 `const config = getNodeConfig(node)`；
  // baseColorVec/emissiveColorVec 改为 new THREE.Color(color)/new THREE.Color(emissive)；
  // 原先从 config 取的 radius 改用 radius prop；
  // phase 用 hashCode(id)（不再 node.id）。其余 shader/glow 逻辑不变。
  ```

- [ ] **Step 5: 改 `MemoryNebulaView.tsx`** —— 删掉已搬走的符号；顶部加：
  ```ts
  import { StarNode } from './nebula/primitives'
  import { EdgeLines, NebulaDust, AutoRotateControls } from './nebula/primitives'
  import { computeGalaxyLayout, type NodePosition } from './nebula/layout'
  import { getNebulaThemeConfig, type NebulaThemeConfig } from './nebula/theme'
  ```
  保留 `KIND_CONFIG`/`FRAGMENT_NEBULA_CONFIG`/`getNodeConfig`/`MemoryNodesMesh`/`SceneContent`/`MemoryNebulaView`。**适配两处调用（语义改造 3）**：
  - `MemoryNodesMesh`：渲染每个 StarNode 时，先 `const config = getNodeConfig(node)`，再 `<StarNode id={node.id} color={config.color} emissive={config.emissive} radius={config.radius} position={pos} … />`（把原本传 `node` 改成传显式视觉 props）。
  - `MemoryNebulaView` 里 `computeGalaxyLayout(graphData.nodes, graphData.edges)` 改为传规范化后的 LayoutNode/LayoutEdge：`computeGalaxyLayout(graphData.nodes.map(n=>({id:n.id,kind:n.kind})), graphData.edges.map(e=>({from:e.parentNodeId??'', to:e.childNodeId})))`（centerOffset 用默认 0）。
  - `MemoryEdgeLines` 用法改为 `EdgeLines`，传入规范化 edges（`{from,to}`）。

- [ ] **Step 6: tsc + 手动验证**
  - `cd ui && npx tsc --noEmit > /tmp/c1.txt 2>&1; echo done; grep -E "MemoryNebulaView|nebula/" /tmp/c1.txt`（应 0 条相关错误）
  - **手动**：`cargo tauri dev` 打开 万花筒›记忆›星云图 tab，确认星云渲染、配色、hover/click 与重构前一致（3D 无单测，必须肉眼确认无回归）。若无法跑 dev，至少 tsc 干净 + 在 PR 注明"未手验"。

- [ ] **Step 7: 提交**
  ```bash
  git add ui/src/components/memory/nebula/ ui/src/components/memory/MemoryNebulaView.tsx
  git commit -m "refactor(ui): extract nebula Three.js primitives to nebula/ (StarNode color props, generalized layout)"
  ```

---

## Task 2: gbrain full_graph 拼装（browse.rs）+ 测试

**Files:** Modify `src-tauri/src/gbrain/browse.rs`.

- [ ] **Step 1: 加类型 + 纯拼装/解析函数**（放在 A 既有 `parse_*` 附近）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub from_slug: String,
    pub to_slug: String,
    #[serde(default)]
    pub link_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
}

/// 解析 get_links 的 JSON（Link[]，与 get_backlinks 同形）为出边。
pub fn parse_links(json_text: &str) -> Result<Vec<KnowledgeEdge>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

/// 纯拼装：节点集 + 各页出边 → 图，丢弃指向未加载页的悬空边。单测目标。
pub fn assemble_graph(nodes: Vec<KnowledgeNode>, edges: Vec<KnowledgeEdge>) -> KnowledgeGraph {
    use std::collections::HashSet;
    let slugs: HashSet<&str> = nodes.iter().map(|n| n.slug.as_str()).collect();
    let edges = edges
        .into_iter()
        .filter(|e| slugs.contains(e.from_slug.as_str()) && slugs.contains(e.to_slug.as_str()))
        .collect();
    KnowledgeGraph { nodes, edges }
}
```

> `KnowledgeEdge` 直接从 gbrain Link JSON 反序列化（取 `from_slug`/`to_slug`/`link_type`，多余字段被 serde 忽略）。

- [ ] **Step 2: 加 async `full_graph`**（复用 A 既有 `list_pages` + `call_gbrain`）

```rust
pub async fn full_graph(
    mcp: &SharedMcpManager,
    limit: u32,
) -> Result<KnowledgeGraph, GbrainError> {
    let pages = list_pages(mcp, limit, Some("updated_desc".into()), None, None, None).await?;
    let nodes: Vec<KnowledgeNode> = pages
        .into_iter()
        .map(|p| KnowledgeNode { slug: p.slug, title: p.title, page_type: p.page_type })
        .collect();
    let mut all_edges: Vec<KnowledgeEdge> = Vec::new();
    for n in &nodes {
        // best-effort：单页 get_links 失败则跳过该页的边，不中断整图
        if let Ok(text) = call_gbrain(mcp, "get_links", serde_json::json!({ "slug": n.slug })).await {
            if let Ok(mut edges) = parse_links(&text) {
                all_edges.append(&mut edges);
            }
        }
    }
    Ok(assemble_graph(nodes, all_edges))
}
```

> 确认 A 的 `list_pages` 签名：`list_pages(mcp, limit, sort, page_type, tag, updated_after)`（来自 A 的 browse.rs）。若参数顺序/类型不符，按真实签名调用。

- [ ] **Step 3: 加单测**（用 A 既有的纯函数测试风格；assemble/parse 不需真 MCP）

```rust
#[cfg(test)]
mod full_graph_tests {
    use super::*;

    #[test]
    fn parse_links_reads_from_to_type() {
        let json = r#"[{"from_slug":"a","to_slug":"b","link_type":"mentions","context":""}]"#;
        let e = parse_links(json).unwrap();
        assert_eq!(e[0].from_slug, "a");
        assert_eq!(e[0].to_slug, "b");
        assert_eq!(e[0].link_type, "mentions");
    }

    #[test]
    fn assemble_drops_dangling_edges() {
        let nodes = vec![
            KnowledgeNode { slug: "a".into(), title: "A".into(), page_type: "concept".into() },
            KnowledgeNode { slug: "b".into(), title: "B".into(), page_type: "person".into() },
        ];
        let edges = vec![
            KnowledgeEdge { from_slug: "a".into(), to_slug: "b".into(), link_type: "x".into() },
            KnowledgeEdge { from_slug: "a".into(), to_slug: "ghost".into(), link_type: "x".into() }, // dangling
        ];
        let g = assemble_graph(nodes, edges);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1); // ghost edge dropped
        assert_eq!(g.edges[0].to_slug, "b");
    }

    #[test]
    fn assemble_empty_is_safe() {
        let g = assemble_graph(vec![], vec![]);
        assert!(g.nodes.is_empty() && g.edges.is_empty());
    }

    #[test]
    fn parse_links_handles_empty_and_malformed() {
        assert!(parse_links("[]").unwrap().is_empty());
        assert!(matches!(parse_links("nope"), Err(GbrainError::ParseFailed(_))));
    }
}
```

- [ ] **Step 4: 编译 + 测试**
  - `cd src-tauri && cargo test --lib gbrain::browse > /tmp/c2.txt 2>&1; grep "test result" /tmp/c2.txt`（A 既有 11 + 新 4 = 15）
  - 若 `list_pages` 调用签名不符 → 读 browse.rs 既有定义改正。

- [ ] **Step 5: 提交**
  ```bash
  git add src-tauri/src/gbrain/browse.rs
  git commit -m "feat(gbrain): full_graph assemble (list_pages + get_links) + parse + tests"
  ```

---

## Task 3: gbrain_full_graph Tauri 命令 + 注册

**Files:** Modify `src-tauri/src/tauri_commands.rs`、`src-tauri/src/main.rs`.

- [ ] **Step 1: tauri_commands.rs 加命令**（贴近 A 既有 `gbrain_*` 命令）
```rust
#[tauri::command]
pub async fn gbrain_full_graph(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<crate::gbrain::browse::KnowledgeGraph, String> {
    crate::gbrain::browse::full_graph(&state.mcp_manager, limit.unwrap_or(150))
        .await
        .map_err(|e| e.to_command_string())
}
```

- [ ] **Step 2: main.rs 注册**（贴近 A 的 `gbrain_*` 注册行）
```rust
            uclaw_core::tauri_commands::gbrain_full_graph,
```

- [ ] **Step 3: 编译**
  `cd src-tauri && cargo build > /tmp/c3.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/c3.txt | head`（EXIT=0）。确认定义 + 注册各 1。

- [ ] **Step 4: 提交**
  ```bash
  git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
  git commit -m "feat(tauri): gbrain_full_graph command + invoke_handler"
  ```

---

## Task 4: gbrain-browse.ts 类型 + 包装

**Files:** Modify `ui/src/lib/gbrain-browse.ts`.

- [ ] **Step 1: 加类型 + 包装**（贴近 A 既有类型/包装）
```ts
export interface KnowledgeNode {
  slug: string
  title: string
  type: string
}

export interface KnowledgeEdge {
  from_slug: string
  to_slug: string
  link_type: string
}

export interface KnowledgeGraph {
  nodes: KnowledgeNode[]
  edges: KnowledgeEdge[]
}

export const gbrainFullGraph = (limit?: number): Promise<KnowledgeGraph> =>
  invoke('gbrain_full_graph', { limit })
```

- [ ] **Step 2: TS 检查**
  `cd ui && npx tsc --noEmit > /tmp/c4.txt 2>&1; echo done; grep -c "gbrain-browse" /tmp/c4.txt`（0 相关错误）。

- [ ] **Step 3: 提交**
  ```bash
  git add ui/src/lib/gbrain-browse.ts
  git commit -m "feat(ui): gbrain-browse KnowledgeGraph types + gbrainFullGraph wrapper"
  ```

---

## Task 5: buildUnifiedScene 纯映射 + 测试

**Files:** Create `ui/src/components/memory/dual-nebula/buildUnifiedScene.ts`、`buildUnifiedScene.test.ts`.

- [ ] **Step 1: 写 buildUnifiedScene.ts**
```ts
import { computeGalaxyLayout, type NodePosition } from '../nebula/layout'
import type { MemoryGraphData } from '@/lib/types'
import type { KnowledgeGraph } from '@/lib/gbrain-browse'

export type NebulaLayer = 'memory' | 'knowledge'

export interface UnifiedNode {
  id: string
  layer: NebulaLayer
  kind: string
  title: string
  x: number
  y: number
  z: number
}

export interface UnifiedEdge {
  from: string
  to: string
  layer: NebulaLayer
}

export interface UnifiedScene {
  nodes: UnifiedNode[]
  edges: UnifiedEdge[]
  bridges: UnifiedEdge[] // 占位，V1 恒空（桥留 D）
}

const CLUSTER_GAP = 220

export function buildUnifiedScene(
  memory: MemoryGraphData | null,
  knowledge: KnowledgeGraph | null,
): UnifiedScene {
  const nodes: UnifiedNode[] = []
  const edges: UnifiedEdge[] = []

  // 记忆层：暖簇偏 -X
  if (memory && memory.nodes.length > 0) {
    const layoutNodes = memory.nodes.map((n) => ({ id: n.id, kind: n.kind }))
    const layoutEdges = memory.edges.map((e) => ({ from: e.parentNodeId ?? '', to: e.childNodeId }))
    const pos = computeGalaxyLayout(layoutNodes, layoutEdges, [-CLUSTER_GAP, 0, 0])
    const posMap = new Map<string, NodePosition>(pos.map((p) => [p.id, p]))
    for (const n of memory.nodes) {
      const p = posMap.get(n.id)
      if (!p) continue
      nodes.push({ id: n.id, layer: 'memory', kind: n.kind, title: n.title, x: p.x, y: p.y, z: p.z })
    }
    for (const e of memory.edges) {
      if (e.parentNodeId) edges.push({ from: e.parentNodeId, to: e.childNodeId, layer: 'memory' })
    }
  }

  // 知识层：冷簇偏 +X
  if (knowledge && knowledge.nodes.length > 0) {
    const layoutNodes = knowledge.nodes.map((n) => ({ id: n.slug, kind: n.type }))
    const layoutEdges = knowledge.edges.map((e) => ({ from: e.from_slug, to: e.to_slug }))
    const pos = computeGalaxyLayout(layoutNodes, layoutEdges, [CLUSTER_GAP, 0, 0])
    const posMap = new Map<string, NodePosition>(pos.map((p) => [p.id, p]))
    for (const n of knowledge.nodes) {
      const p = posMap.get(n.slug)
      if (!p) continue
      nodes.push({ id: n.slug, layer: 'knowledge', kind: n.type, title: n.title, x: p.x, y: p.y, z: p.z })
    }
    for (const e of knowledge.edges) {
      edges.push({ from: e.from_slug, to: e.to_slug, layer: 'knowledge' })
    }
  }

  return { nodes, edges, bridges: [] }
}
```

- [ ] **Step 2: 写 buildUnifiedScene.test.ts**
```ts
import { describe, it, expect } from 'vitest'
import { buildUnifiedScene } from './buildUnifiedScene'

const mem = {
  nodes: [
    { id: 'm1', spaceId: 'default', kind: 'boot', title: 'Boot', createdAt: '', updatedAt: '' },
    { id: 'm2', spaceId: 'default', kind: 'episode', title: 'Ep', createdAt: '', updatedAt: '' },
  ],
  edges: [{ id: 'e', spaceId: 'default', parentNodeId: 'm1', childNodeId: 'm2', relationKind: 'r', visibility: 'private', priority: 0, createdAt: '' }],
  routes: [],
} as any

const know = {
  nodes: [
    { slug: 'k1', title: 'Entity', type: 'entity' },
    { slug: 'k2', title: 'Concept', type: 'concept' },
  ],
  edges: [{ from_slug: 'k1', to_slug: 'k2', link_type: 'mentions' }],
}

describe('buildUnifiedScene', () => {
  it('maps both layers with layer tags', () => {
    const s = buildUnifiedScene(mem, know)
    expect(s.nodes.filter((n) => n.layer === 'memory')).toHaveLength(2)
    expect(s.nodes.filter((n) => n.layer === 'knowledge')).toHaveLength(2)
    expect(s.edges.filter((e) => e.layer === 'memory')).toHaveLength(1)
    expect(s.edges.filter((e) => e.layer === 'knowledge')).toHaveLength(1)
    expect(s.bridges).toHaveLength(0)
  })

  it('offsets memory cluster to -X and knowledge to +X', () => {
    const s = buildUnifiedScene(mem, know)
    const memAvg = s.nodes.filter((n) => n.layer === 'memory').reduce((a, n) => a + n.x, 0) / 2
    const knowAvg = s.nodes.filter((n) => n.layer === 'knowledge').reduce((a, n) => a + n.x, 0) / 2
    expect(memAvg).toBeLessThan(0)
    expect(knowAvg).toBeGreaterThan(0)
  })

  it('handles either layer null', () => {
    expect(buildUnifiedScene(mem, null).nodes.every((n) => n.layer === 'memory')).toBe(true)
    expect(buildUnifiedScene(null, know).nodes.every((n) => n.layer === 'knowledge')).toBe(true)
    expect(buildUnifiedScene(null, null).nodes).toHaveLength(0)
  })

  it('knowledge node id is slug', () => {
    const s = buildUnifiedScene(null, know)
    expect(s.nodes.map((n) => n.id).sort()).toEqual(['k1', 'k2'])
  })
})
```

- [ ] **Step 3: 跑测试**
  `cd ui && npm test -- --run buildUnifiedScene > /tmp/c5.txt 2>&1; echo EXIT=$?; grep -E "Tests " /tmp/c5.txt`（4 passed）。
  > 若 `computeGalaxyLayout` 对极小输入返回的 x 偏移被随机扰动盖过导致 -X/+X 断言偶发失败：buildUnifiedScene 的 offset 是确定性叠加（在 layout 内 return 前加 centerOffset），CLUSTER_GAP=220 远大于布局半径量级，断言稳定。若仍失败，核对 Task 1 Step 3 的 centerOffset 是否真的加到了每个 position。

- [ ] **Step 4: 提交**
  ```bash
  git add ui/src/components/memory/dual-nebula/
  git commit -m "feat(ui): buildUnifiedScene pure mapper for fused dual-nebula + tests"
  ```

---

## Task 6: DualNebulaView 融合画布组件

**Files:** Create `ui/src/components/memory/DualNebulaView.tsx`.

- [ ] **Step 1: 写组件**（用共享原语 + buildUnifiedScene；层着色）

```tsx
import * as React from 'react'
import { Canvas } from '@react-three/fiber'
import { useAtomValue } from 'jotai'
import { resolvedThemeAtom, themeStyleAtom } from '@/atoms/theme'
import { cn } from '@/lib/utils'
import type { MemoryGraphData } from '@/lib/types'
import type { KnowledgeGraph } from '@/lib/gbrain-browse'
import { StarNode, EdgeLines, NebulaDust, AutoRotateControls } from './nebula/primitives'
import { getNebulaThemeConfig } from './nebula/theme'
import { buildUnifiedScene, type NebulaLayer } from './dual-nebula/buildUnifiedScene'

// 层色板（暖=记忆 / 冷=知识）。用固定语义色，与 nebula themeConfig 共存。
const LAYER_COLOR: Record<NebulaLayer, { color: string; emissive: string }> = {
  memory: { color: '#f4a259', emissive: '#e0883a' },
  knowledge: { color: '#5b8def', emissive: '#3a6fe0' },
}
const STAR_RADIUS = 6

interface DualNebulaViewProps {
  memory: MemoryGraphData | null
  knowledge: KnowledgeGraph | null
  onSelect?: (id: string, layer: NebulaLayer) => void
  className?: string
}

export function DualNebulaView({ memory, knowledge, onSelect, className }: DualNebulaViewProps): React.ReactElement {
  const [hoveredId, setHoveredId] = React.useState<string | null>(null)
  const resolvedTheme = useAtomValue(resolvedThemeAtom)
  const themeStyle = useAtomValue(themeStyleAtom)
  const themeConfig = React.useMemo(() => getNebulaThemeConfig(resolvedTheme, themeStyle), [resolvedTheme, themeStyle])

  const scene = React.useMemo(() => buildUnifiedScene(memory, knowledge), [memory, knowledge])
  const layerOf = React.useMemo(() => {
    const m = new Map<string, NebulaLayer>()
    for (const n of scene.nodes) m.set(n.id, n.layer)
    return m
  }, [scene])

  if (scene.nodes.length === 0) {
    return (
      <div className={cn('flex items-center justify-center text-muted-foreground text-sm', className)} data-testid="dual-nebula-empty">
        暂无可视化数据
      </div>
    )
  }

  const cameraZ = Math.max(400, Math.sqrt(scene.nodes.length) * 30)
  const posMap = new Map(scene.nodes.map((n) => [n.id, [n.x, n.y, n.z] as [number, number, number]]))

  return (
    <div className={cn('relative', className)} data-testid="dual-nebula-view">
      <Canvas camera={{ position: [0, 0, cameraZ], fov: 60 }} style={{ background: 'transparent' }}>
        <ambientLight intensity={0.6} />
        <pointLight position={[0, 0, 200]} intensity={0.8} />
        <NebulaDust themeConfig={themeConfig} />
        {scene.nodes.map((n) => {
          const c = LAYER_COLOR[n.layer]
          return (
            <StarNode
              key={n.id}
              id={n.id}
              position={[n.x, n.y, n.z]}
              color={c.color}
              emissive={c.emissive}
              radius={STAR_RADIUS}
              isHovered={hoveredId === n.id}
              onHover={setHoveredId}
              onClick={(id) => onSelect?.(id, layerOf.get(id) ?? 'memory')}
              themeConfig={themeConfig}
            />
          )
        })}
        <EdgeLines edges={scene.edges} positions={posMap} themeConfig={themeConfig} resolvedTheme={resolvedTheme} />
        <AutoRotateControls />
      </Canvas>
    </div>
  )
}
```

> `EdgeLines` 的 props 形状以 Task 1 抽出的实际签名为准（positions map + edges {from,to} + themeConfig + resolvedTheme）。若抽出的 `EdgeLines` 期望不同的 positions 结构，按其真实签名传参（plan Task 1 已把它泛化为消费 `{id,x,y,z}`/from-to）。`NebulaDust`/`AutoRotateControls`/`StarNode` 的 props 同理以 Task 1 抽出的签名为准。

- [ ] **Step 2: TS 检查**
  `cd ui && npx tsc --noEmit > /tmp/c6.txt 2>&1; echo done; grep -E "DualNebulaView" /tmp/c6.txt`（0 相关错误）。

- [ ] **Step 3: 提交**
  ```bash
  git add ui/src/components/memory/DualNebulaView.tsx
  git commit -m "feat(ui): DualNebulaView fused canvas (layer-colored, intra-layer edges)"
  ```

---

## Task 7: WikiView initialSlug + MemoryModule 接线 + 测试

**Files:** Modify `ui/src/components/memory/WikiView.tsx`、`MemoryModule.tsx`；扩展 `WikiView.test.tsx`；Create `DualNebulaView.test.tsx`.

- [ ] **Step 1: WikiView 加 `initialSlug?`**
  - props 接口加 `initialSlug?: string`。
  - 组件内加 effect（在 `openPage` 定义之后）：
    ```tsx
    React.useEffect(() => {
      if (initialSlug) void openPage(initialSlug)
    }, [initialSlug, openPage])
    ```

- [ ] **Step 2: MemoryModule 接线**
  - import：`import { DualNebulaView } from '@/components/memory/DualNebulaView'`；`import { gbrainFullGraph, type KnowledgeGraph } from '@/lib/gbrain-browse'`；图标如 `Orbit`（lucide）。
  - `MemoryTab` 类型加 `'dual'`；`TABS` 数组加 `{ value: 'dual', label: '双星云', icon: Orbit }`。
  - state：`const [knowledgeGraph, setKnowledgeGraph] = React.useState<KnowledgeGraph | null>(null)`；`const [pendingWikiSlug, setPendingWikiSlug] = React.useState<string | undefined>(undefined)`。
  - effect：当 `activeTab === 'dual'` 且 `knowledgeGraph === null` 时拉取：
    ```tsx
    React.useEffect(() => {
      if (activeTab !== 'dual' || knowledgeGraph !== null) return
      let cancelled = false
      gbrainFullGraph(150).then((g) => { if (!cancelled) setKnowledgeGraph(g) }).catch(() => {})
      return () => { cancelled = true }
    }, [activeTab, knowledgeGraph])
    ```
  - 渲染：
    ```tsx
    {activeTab === 'dual' && (
      <DualNebulaView
        memory={graphData}
        knowledge={knowledgeGraph}
        onSelect={(id, layer) => {
          if (layer === 'memory') setSelectedNodeId(id)
          else { setPendingWikiSlug(id); setActiveTab('wiki') }
        }}
        className="h-full w-full rounded-xl overflow-hidden border border-border/40"
      />
    )}
    ```
  - 把 wiki tab 的渲染改为传 initialSlug：`{activeTab === 'wiki' && (<WikiView initialSlug={pendingWikiSlug} className="…" />)}`。

- [ ] **Step 3: DualNebulaView vitest**（mock fiber Canvas + 子组件，断言渲染 + 空态 + 点击路由）

创建 `ui/src/components/memory/DualNebulaView.test.tsx`：
```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'

// mock Three canvas + 原语为占位，使 jsdom 不跑 WebGL
vi.mock('@react-three/fiber', () => ({ Canvas: ({ children }: any) => <div data-testid="r3f-canvas">{children}</div> }))
vi.mock('./nebula/primitives', () => ({
  StarNode: ({ id, onClick }: any) => <button data-testid={`star-${id}`} onClick={() => onClick(id)} />,
  EdgeLines: () => null,
  NebulaDust: () => null,
  AutoRotateControls: () => null,
}))

import { DualNebulaView } from './DualNebulaView'

const know = { nodes: [{ slug: 'k1', title: 'K', type: 'entity' }], edges: [] }
const mem = { nodes: [{ id: 'm1', spaceId: 'default', kind: 'boot', title: 'B', createdAt: '', updatedAt: '' }], edges: [], routes: [] } as any

describe('DualNebulaView', () => {
  it('empty state when both null', () => {
    renderWithProviders(<DualNebulaView memory={null} knowledge={null} />)
    expect(screen.getByTestId('dual-nebula-empty')).toBeInTheDocument()
  })

  it('renders stars for both layers and routes clicks by layer', async () => {
    const onSelect = vi.fn()
    const { user } = renderWithProviders(<DualNebulaView memory={mem} knowledge={know} onSelect={onSelect} />)
    await user.click(screen.getByTestId('star-m1'))
    expect(onSelect).toHaveBeenCalledWith('m1', 'memory')
    await user.click(screen.getByTestId('star-k1'))
    expect(onSelect).toHaveBeenCalledWith('k1', 'knowledge')
  })
})
```

- [ ] **Step 4: WikiView initialSlug 测试**（在既有 `WikiView.test.tsx` 加一例）
```tsx
  it('opens initialSlug on mount', async () => {
    renderWithProviders(<WikiView initialSlug="person-alice" />)
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('gbrain_get_page', { slug: 'person-alice' }))
  })
```
> 复用该文件已有的 `routeInvoke()` mock（A 的 Task 8 建立）。`invokeMock`/`waitFor` 已在文件作用域。

- [ ] **Step 5: 跑测试 + tsc**
  - `cd ui && npm test -- --run "DualNebulaView|WikiView|buildUnifiedScene" > /tmp/c7.txt 2>&1; echo EXIT=$?; grep -E "Tests " /tmp/c7.txt`
  - `cd ui && npx tsc --noEmit > /tmp/c7ts.txt 2>&1; echo done; grep -cE "DualNebulaView|MemoryModule|WikiView" /tmp/c7ts.txt`（0 相关错误）
  - 全绿。

- [ ] **Step 6: 提交**
  ```bash
  git add ui/src/components/memory/WikiView.tsx ui/src/components/memory/WikiView.test.tsx ui/src/components/memory/DualNebulaView.test.tsx ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx
  git commit -m "feat(ui): WikiView initialSlug + MemoryModule dual tab wiring + tests"
  ```

---

## 手动 E2E 验证清单（写进 PR）

1. 重构后 nebula tab 渲染/配色/交互不回归（Task 1 关键）。
2. 双星云 tab：两簇（暖记忆 -X / 冷知识 +X）+ 各自内部连线 + 统一轨道旋转/缩放。
3. 点记忆星 → MemoryNodeCard 弹窗；点知识星 → 切 wiki tab 并自动打开该页（可编辑/版本史）。
4. gbrain 断开 → 知识层空、记忆层照渲，不崩溃。
5. 大知识库 → 截断到 150 页。

---

## 自检（对照 spec）

- **Spec 覆盖**：§2 后端 full_graph → Task 2/3；§3 抽原语 → Task 1；§4 DualNebulaView → Task 6；§5 buildUnifiedScene → Task 5；§6 接线 + WikiView initialSlug 复用编辑/版本 → Task 7；§7 错误隔离（两源各 try/catch、悬空边丢弃、空态）→ Task 2/6/7；§8 测试 → Task 2/5/6/7 + 手动；§9 边界（桥占位、不归档、不在 nebula 内编辑知识、不碰 memory 写、不重做视觉）→ 遵守。
- **占位符**：无 TBD。`bridges` 是 spec 明确的占位接口（恒空），非未完成。Task 1 的"移动现有代码"是正确指令（重打 400 行 GLSL 反而引错），3 处语义改造给了完整新代码。
- **类型一致**：Rust `KnowledgeNode{slug,title,page_type(rename type)}` ↔ TS `KnowledgeNode{slug,title,type}`；`KnowledgeEdge{from_slug,to_slug,link_type}` 两侧一致；`buildUnifiedScene` 的 `UnifiedNode/Edge` 在 Task 5 定义、Task 6 消费一致；`computeGalaxyLayout(nodes,edges,centerOffset)` 新签名在 Task 1 定、Task 5 用一致；`StarNode` 新 props（id/position/color/emissive/radius/…）Task 1 定、Task 6 用一致。
- **范围**：单 PR、7 commit、无新 migration、无新依赖。最高风险是 Task 1 重构（3D 无单测）→ 已要求 tsc + 手动 nebula tab 不回归确认。
