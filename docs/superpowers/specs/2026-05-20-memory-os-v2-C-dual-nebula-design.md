# 子项目 C — 万花筒融合双星云 设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** [Agent Memory OS v2 north-star](2026-05-20-agent-memory-os-v2-second-brain-design.md) · 子项目 C（万花筒双星云）
**Depends on:** A（gbrain 代理命令，PR #288 已完成）、E（PR #288）。

---

## 1. 背景与决策（已与用户确认）

把两层记忆放进**一个融合 3D 场景**：记忆星云（`memory_nodes`，已有 `MemoryNebulaView`）+ 知识星云（gbrain pages，新）。解决 north-star §6 的开放问题。

| 决策 | 选择 |
|---|---|
| 画布布局 | **融合单画布** —— 一个 3D 场景、两层同场、按层着色（记忆=暖/知识=冷）、统一轨道/缩放/拾取 |
| 知识层数据 | **nodes + edges**，来自**新建 `gbrain_full_graph`** 命令（list_pages + 逐页 get_links 拼装） |
| 跨层桥 | **缓 D** —— 渲染预留 `bridges` 入参，V1 恒空（今天无桥数据；north-star §6 倾向 C/D 引入，放 D 保持 C 边界） |
| 组件 | **抽共享 Three.js 原语** → 新 `DualNebulaView`；顺手拆 772 行的 `MemoryNebulaView`（符合"按域拆分不堆 god file"） |

已核对的事实：
- `MemoryNebulaView` 接 `graphData: MemoryGraphData` prop（父组件 `MemoryModule` 经 `memoryGraphGetFullGraph()` 取）；其 `computeGalaxyLayout(nodes, edges)` 按 `MemoryNode.kind` 分层放置。
- `MemoryNode { id, spaceId, kind: MemoryNodeKind, title, metadata?, createdAt, updatedAt }`；9 种 kind（boot/identity/value/user_profile/directive/curated/episode/procedure/reference）。
- `MemoryEdge { id, parentNodeId?, childNodeId, relationKind, ... }`（parent→child）。
- gbrain `get_links(slug)` = "List outgoing links from a page"，返回 `Link[]`（`{from_slug, to_slug, link_type, ...}`）；`list_pages` 返回 `{slug, type, title, updated_at}[]`。
- `MemoryModule` 现有 8 tab；点节点经 `selectedNodeId` 开 `MemoryNodeCard` 弹窗。A 的 `WikiView` 是全功能 3 区面板（浏览/编辑/版本史）。

---

## 2. 架构 & 数据流

```
MemoryModule.tsx  (新增 'dual' tab)
  │  并行: memoryGraphGetFullGraph()        gbrainFullGraph()
  ▼                                              ▼
MemoryGraphData{nodes,edges}            KnowledgeGraph{nodes,edges}
  └──────────────┬───────────────────────────────┘
                 ▼
DualNebulaView  ── buildUnifiedScene() ──▶ 统一 UnifiedNode[]/UnifiedEdge[]
                 │  (记忆暖色偏 -X 簇 / 知识冷色偏 +X 簇 / bridges 占位恒空)
                 ▼
  <Canvas> + OrbitControls + 共享原语 (StarNode/EdgeLines/NebulaDust)
                 │  onSelect(id, layer)
                 ├─ layer='memory'    → MemoryNodeCard 弹窗 (已有)
                 └─ layer='knowledge' → 切 'wiki' tab + WikiView initialSlug (复用 A)

后端: gbrain_full_graph 命令 → browse.rs::full_graph → mcp list_pages + 逐页 get_links
```

---

## 3. 后端：`gbrain_full_graph`

**`src-tauri/src/gbrain/browse.rs`** 新增：
- 类型 `KnowledgeNode { slug, title, page_type }`、`KnowledgeEdge { from_slug, to_slug, link_type }`、`KnowledgeGraph { nodes: Vec<KnowledgeNode>, edges: Vec<KnowledgeEdge> }`（`#[serde(rename_all/rename)]` 与 A 既有 DTO 同风格；`page_type` rename `type`）。
- 纯解析函数 `parse_links(json_text) -> Vec<KnowledgeEdge>`（复用既有 `Backlink`/Link 解析风格；单测目标）。
- `async fn full_graph(mcp, limit: u32) -> Result<KnowledgeGraph, GbrainError>`：
  1. `list_pages(mcp, limit, sort=updated_desc, …)` → nodes（复用 A 既有 `list_pages`，映射 PageSummary→KnowledgeNode）。
  2. 对每个 node `call_gbrain("get_links", {slug})` → 解析为 KnowledgeEdge，累加。仅保留 `to_slug` 也在 node 集合内的边（丢弃指向未加载页的悬空边）。
  3. 返回 `{nodes, edges}`。
- **成本边界**：`limit` 默认 150（节点上限）；get_links 是 N 次 MCP 往返，一次性加载（同记忆全图量级）可接受。gbrain 未连接 → `GbrainError::NotConnected`（前端知识层空，记忆层照渲）。

**Tauri 命令**（tauri_commands.rs + main.rs 注册）：
- `gbrain_full_graph(limit: Option<u32>) -> Result<KnowledgeGraph, String>`（gbrain 未连接返回 `gbrain_not_connected`，与 A 一致）。

**TS（`ui/src/lib/gbrain-browse.ts`）**：`KnowledgeNode/KnowledgeEdge/KnowledgeGraph` 类型 + `gbrainFullGraph(limit?)` 包装。

> plan 阶段核对：`get_links` 返回 JSON 的确切字段（`from_slug/to_slug/link_type`，应与 `get_backlinks` 的 Link 同形）。

---

## 4. 前端：抽共享原语

新目录 `ui/src/components/memory/nebula/`，从 `MemoryNebulaView.tsx`（772 行）**移出**可复用 Three.js 原语，`MemoryNebulaView` 改为 import（**行为不变**，星云 tab 手验不回归）：

- `nebula/StarNode.tsx` —— 单星渲染（着色器 uniforms / glow / billboard），props 含 `color/emissive`（让调用方按层传色）。
- `nebula/EdgeLines.tsx` —— 边连线（`<Line>` 批量）。
- `nebula/NebulaDust.tsx` —— 星尘背景。
- `nebula/texture.ts` —— `createNebulaTexture()`。
- `nebula/layout.ts` —— `computeGalaxyLayout` + `gaussian`/`hashCode` helper，**泛化**：接受 `centerOffset: [x,y,z]` 参数（默认 0），使两簇能偏置。
- `nebula/types.ts` —— 共享 props/类型。

> 拆分是机械搬迁 + re-import；不改渲染逻辑。验证靠 `tsc` + 星云 tab 手动观察（3D 无法单测）。

---

## 5. 前端：`DualNebulaView`

**`ui/src/components/memory/DualNebulaView.tsx`**（新）：
- Props `{ memory: MemoryGraphData | null, knowledge: KnowledgeGraph | null, onSelect: (id: string, layer: 'memory'|'knowledge') => void, className? }`。
- **纯函数 `buildUnifiedScene(memory, knowledge)`**（单测目标，不碰 Three）：
  - `UnifiedNode { id, layer, kind, title, x, y, z }`、`UnifiedEdge { from, to, layer }`。
  - 记忆节点：`computeGalaxyLayout(memNodes, memEdges, centerOffset=[-CLUSTER_GAP,0,0])`，layer='memory'。边 from=`parentNodeId` to=`childNodeId`。
  - 知识节点：复用同一个 `computeGalaxyLayout`，centerOffset=`[+CLUSTER_GAP,0,0]`，layer='knowledge'。gbrain 的 `type`（entity/concept/person/…）不命中记忆的 coreKinds/midKinds，故知识节点自然全落"叶子层"= 一个均匀簇（无需为知识单写分层逻辑，保持 DRY）。边 from=`from_slug` to=`to_slug`。
  - 空输入安全（任一层 null/空 → 只渲另一层）。
  - 预留 `bridges: UnifiedEdge[]` 返回字段，V1 恒 `[]`。
- 渲染：一个 `<Canvas>` + `<OrbitControls>` + 共享 `StarNode`（按 `layer` 取暖/冷色板）+ 两层 `EdgeLines` + `NebulaDust`。hover/click → `onSelect(id, layer)`。
- 主题：层色板用主题 token 衍生（暖=primary/amber 系，冷=blue/cyan 系），不硬编码裸 hex（沿用 MemoryNebulaView 的 themeConfig 取色方式）。

---

## 6. 前端：接线 + 编辑/版本复用

**`MemoryModule.tsx`**：
- 加 tab「双星云」(`value: 'dual'`, icon 如 `Orbit`/`Sparkles`)。
- 选中 dual tab 时并行拉两源：已有 `graphData`（`memoryGraphGetFullGraph`）+ 新 `knowledgeGraph`（`gbrainFullGraph()`，独立 state + try/catch）。
- 渲 `<DualNebulaView memory={graphData} knowledge={knowledgeGraph} onSelect={handleDualSelect} />`。
- `handleDualSelect(id, layer)`：`memory` → 现有 `setSelectedNodeId(id)`（开 MemoryNodeCard 弹窗）；`knowledge` → `setActiveTab('wiki')` + 记下 `pendingWikiSlug=id`。

**`WikiView.tsx`（A 组件，加一个小 prop）**：
- 加 `initialSlug?: string`：挂载时若有则 `openPage(initialSlug)`。这样知识星点击 → 切 wiki tab → 自动打开该页（复用 A 的浏览/编辑/版本史，零新编辑 UI）。MemoryModule 把 `pendingWikiSlug` 作为 `initialSlug` 传给 WikiView。

满足 north-star 的"可编辑/版本控制"全靠复用（记忆走 MemoryNodeCard、知识走 WikiView），C 不新写编辑/版本 UI。

---

## 7. 错误处理

| 场景 | 行为 |
|---|---|
| gbrain 未连接 | `gbrain_full_graph` 返回 `gbrain_not_connected`；知识层空（只渲记忆星云）+ 角标提示"知识层未连接" |
| 记忆图加载失败 | 记忆层空（只渲知识星云）；不崩溃 |
| 两源都空 | 空状态卡片"暂无可视化数据" |
| 某页 get_links 失败 | 该页边跳过（best-effort 累加），不中断整图 |
| 知识图过大（>limit） | 截断到 limit + 角标提示"已显示前 N 页"（与 WikiView 100 上限同思路） |

---

## 8. 测试

**Rust（`browse.rs` 单测，mock MCP 文本）：**
- `parse_links_to_knowledge_edges` —— get_links JSON → `Vec<KnowledgeEdge>`
- `full_graph` 的边过滤逻辑：悬空边（to_slug 不在 node 集）被丢弃（可把组装拆成纯函数 `assemble_graph(pages, links_per_page)` 单测，避免依赖真 MCP）
- 空 list_pages → 空图，不 panic

**TS（Vitest）：**
- `buildUnifiedScene` 纯函数：两层映射 + centerOffset 偏置（记忆簇 x<0 / 知识簇 x>0）+ 层着色字段 + 空输入（任一层 null）+ bridges 恒空
- `DualNebulaView` vitest（mock `@react-three/fiber` Canvas 为占位 div + mock invoke）：渲染、两源都拉、点击 `onSelect` 按 layer 路由
- `WikiView` initialSlug：挂载传 slug → 调 `gbrain_get_page`（扩展 A 的既有 vitest）

**手动 E2E（写进验证清单）：** 真 app + gbrain 连接 → 双星云 tab → 看两簇（暖/冷）+ 各自连线 → 点记忆星开 MemoryNodeCard → 点知识星跳 wiki 并打开该页 → gbrain 断开后知识层空、记忆层正常。

> Three.js 3D 渲染本身不单测（vitest mock fiber）。逻辑都隔离在纯函数（buildUnifiedScene / assemble_graph / layout）里测。

---

## 9. 范围边界（明确不做）

- ❌ 跨层桥连线（D；C 仅留 `bridges` 占位接口）
- ❌ 节点归档/删除（D）
- ❌ 在 nebula 内直接编辑知识页（走 WikiView）/ 不碰 memory_nodes 写
- ❌ 不重做 MemoryNebulaView 的视觉（只搬原语，行为不变）
- ❌ 不碰 B/D；无新 migration、无新依赖

---

## 10. 提交形状（bisectable，预计单 PR ~7 commit）

1. `refactor(ui): extract nebula Three.js primitives to nebula/ shared module`
2. `feat(gbrain): full_graph assemble (list_pages + get_links) + parse + tests`
3. `feat(tauri): gbrain_full_graph command + invoke_handler`
4. `feat(ui): gbrain-browse KnowledgeGraph types + gbrainFullGraph wrapper`
5. `feat(ui): buildUnifiedScene pure mapper + tests`
6. `feat(ui): DualNebulaView fused canvas (layer-colored, intra-layer edges)`
7. `feat(ui): WikiView initialSlug + MemoryModule dual tab wiring + tests`

无新 migration（C 不碰 schema）。
