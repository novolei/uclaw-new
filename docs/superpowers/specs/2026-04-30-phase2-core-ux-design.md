# Phase 2: Core UX Completion — Design Spec

> **Status**: Approved  
> **Date**: 2026-04-30  
> **Context**: uclaw 从 hello-halo 迁移，补齐基础文件管理和 Canvas 查看器，达到可日常使用水平

---

## 1. 目标

补齐 uclaw 相比 hello-halo 缺失的核心用户体验功能：文件树浏览、Canvas 内容查看器、Space 首页、对话收藏。完成这些后，uclaw 可作为日常 AI 编程助手独立使用。

---

## 2. 关键架构决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Canvas 布局 | 中央分栏（聊天区↔Canvas 左右分栏） | 对齐 hello-halo 体验，代码查看空间更大 |
| 文件树位置 | 右侧栏常驻 | 替代当前任务审批/通知区域，审批改为 Toast |
| 文件监视 | Rust `notify` crate 后端推送 | 实时可靠，不消耗前端轮询资源 |
| 代码编辑器 | CodeMirror 6（已在依赖中） | 只读查看为主，后续可扩展编辑能力 |

---

## 3. 系统架构

### 3.1 整体布局

```
┌─────────────────────────────────────────────────────────────┐
│ TitleBar (自定义标题栏 + 模型选择器)                          │
├────────┬──────────────────────────┬──────────────────────────┤
│ Left   │  Center Content          │  Right Sidebar (280px)    │
│ Sidebar│  (flex: 1, 分栏可变)      │  ArtifactTree.svelte      │
│ 260px  │                          │                           │
│        │  ┌────────────────────┐  │  文件树 + 右键菜单         │
│ Conv.  │  │ ChatView           │  │                           │
│ List   │  │ (Canvas 打开时缩窄) │  │  新建/重命名/删除          │
│        │  │                    │  │                           │
│ Space  │  │ 消息流 + 输入框     │  │                           │
│ 选择器 │  └────────────────────┘  │                           │
│        │                          │                           │
│        │  ┌────────────────────┐  │                           │
│        │  │ CanvasArea         │  │                           │
│        │  │ (条件渲染)          │  │                           │
│        │  │ [Tab1][Tab2][×]    │  │                           │
│        │  │ ┌────────────────┐ │  │                           │
│        │  │ │ 查看器          │ │  │                           │
│        │  │ └────────────────┘ │  │                           │
│        │  └────────────────────┘  │                           │
└────────┴──────────────────────────┴──────────────────────────┘
```

### 3.2 布局状态机

```
State A: 纯聊天 (无 Canvas)
  LeftSidebar(260px) | ChatView(flex:1) | RightSidebar(280px)

State B: Canvas 打开
  LeftSidebar(260px) | ChatView(~40%) | CanvasArea(~40%) | RightSidebar(280px)

State C: Canvas 最大化
  LeftSidebar(260px) | CanvasArea(flex:1) | RightSidebar(280px)
  ChatView 隐藏
```

---

## 4. 组件设计

### 4.1 文件树系统

#### `ArtifactTree.svelte` — 文件树主组件
- 位置：RightSidebar 内容区
- 顶部工具栏：刷新按钮、新建文件/文件夹按钮
- 树主体：递归渲染 `ArtifactTreeNode`
- 状态：loading skeleton → 树内容
- 从 `artifactStore` 读取数据

#### `ArtifactTreeNode.svelte` — 递归树节点
- Props: `node: ArtifactTreeNode`, `depth: number`
- 文件夹：箭头图标 + 名称，点击展开/折叠（懒加载子节点）
- 文件：文件图标（按类型）+ 名称，双击打开到 Canvas
- 选中态高亮（当前在 Canvas 打开的文件）
- 右键菜单：
  - 文件夹：新建文件、新建文件夹、重命名、删除
  - 文件：打开、重命名、删除、下载、在文件夹中显示
- 内联重命名：点击重命名后原地变为 `<input>`，Enter 确认，Esc 取消

#### `ArtifactTreeNode` 类型
```typescript
interface ArtifactTreeNode {
  path: string;           // 相对 workspace 的路径
  name: string;
  is_dir: boolean;
  children?: ArtifactTreeNode[];  // 文件夹时才加载
  size_bytes?: number;
  modified_at?: string;
  mime_type?: string;
}
```

### 4.2 Canvas 系统

#### `CanvasArea.svelte` — Canvas 容器
- 当 `canvasStore.tabs.length > 0` 时渲染
- 上部：标签栏 (`CanvasTabBar`)
- 下部：查看器主体 (`ArtifactViewer`)
- 宽度可拖拽调整（resize handle，最小 300px）

#### `CanvasTab.svelte` — 单个标签
- 显示文件图标 + 文件名
- 激活态高亮
- 关闭按钮（hover 显示）
- 右键菜单：关闭、关闭其他、关闭全部、关闭右侧
- 脏标记（如文件在外部被修改）

#### `ArtifactViewer.svelte` — 查看器路由
根据文件 MIME 类型/扩展名选择对应查看器：

| 文件类型 | 查看器 | 说明 |
|---------|--------|------|
| `.ts/.js/.rs/.py/.go/.svelte/...` | CodeViewer | 代码高亮，只读 |
| `.html` | HtmlPreview | iframe sandbox 预览 |
| `.md` | MarkdownViewer | Markdown 渲染 |
| `.png/.jpg/.gif/.svg/.webp` | ImagePreview | 图片显示 + 缩放 |
| `.json` | CodeViewer (JSON) | 格式化显示 |
| `.css` | CodeViewer (CSS) | 语法高亮 |
| 其他未知 | CodeViewer (plain) | 纯文本显示 |

#### `CodeViewer.svelte`
- 基于 CodeMirror 6（已在项目依赖中）
- 只读模式
- 语法高亮（根据扩展名自动选择语言）
- 行号显示
- 等宽字体

#### `HtmlPreview.svelte`
- `<iframe sandbox="allow-scripts">` 安全沙箱
- 加载文件内容为 blob URL
- 刷新按钮

#### `ImagePreview.svelte`
- `<img>` 标签
- 鼠标滚轮缩放
- 拖拽平移（当图片大于容器时）
- 适合窗口 / 原始大小 切换

#### `MarkdownViewer.svelte`
- 复用现有 `Markdown.svelte` 组件
- 传入文件内容

### 4.3 Space 首页

#### `HomeView.svelte` — Space 列表首页
- 路由：`#/home`
- 卡片网格布局（flex-wrap，3-4 列）
- 每个 Space 卡片：
  - 大 Emoji/图标
  - 空间名称
  - "X 个对话" 副标题
  - 最后活跃时间
- 最后一张卡片为 "创建新空间"（虚线边框，+图标）
- 点击空间卡片 → 导航到 `#/chat?space=<id>`

#### `CreateSpaceDialog.svelte` — 创建空间弹窗
- 输入空间名称
- Emoji 图标选择器（预设 20+ 常用 emoji）
- 可选：自定义工作目录（默认 `~/.uclaw/spaces/<name>/`）
- 确认/取消按钮

### 4.4 对话收藏

#### `ConversationList.svelte` 改造
- 每个 `ConversationItem` 右侧增加星标按钮 (⭐/☆)
- 列表顶部增加筛选：全部 / 已收藏
- 已收藏对话显示星标图标 + 置顶（可选）

---

## 5. 数据流

### 5.1 文件树加载

```
1. 进入 Space → 前端 invoke("list_artifacts_tree", { space_id, path: "" })
2. Rust: 读取 workspace 目录 → 构建根级 ArtifactTreeNode[] → 返回
3. 前端: artifactStore.tree = response → ArtifactTree 渲染根节点
4. 用户点击文件夹 → invoke("list_artifacts_tree", { space_id, path: "src/" })
5. Rust: 读取子目录 → 返回子节点列表
6. 前端: 将子节点挂载到对应文件夹的 children → 展开显示
```

### 5.2 文件打开到 Canvas

```
1. 用户双击文件 → canvasStore.open(node)
2. canvasStore.tabs.push({ path, name, mime_type, isModified: false })
3. canvasStore.activeTabId = tab.id
4. CanvasArea 渲染 → ArtifactViewer 根据类型选择查看器
5. 查看器 invoke("read_artifact", { space_id, path }) → 获取内容
6. 渲染文件内容
```

### 5.3 文件变化感知

```
1. Rust FileWatcher (notify crate) 监听 workspace 目录
2. 检测到文件变化 (Create/Modify/Delete/Rename)
3. 防抖 300ms → 通过 Tauri Event "artifact:tree_update" 推送
4. 前端 listen("artifact:tree_update") → artifactStore 标记脏节点
5. 用户下次展开目录时重新加载
6. Canvas 中的文件如果被外部修改 → 标签显示脏标记
```

### 5.4 文件操作

```
创建文件:  前端 → create_artifact_file(space_id, path, content)  → fs::write + cache
创建文件夹: 前端 → create_artifact_folder(space_id, path)         → fs::create_dir + cache
删除:      前端 → 确认弹窗 → delete_artifact(space_id, path)      → fs::remove + cache
重命名:    前端 → 内联编辑 → rename_artifact(space_id, old, new)  → fs::rename + cache
移动:      前端 → 拖拽 → move_artifact(space_id, src, dest)       → fs::rename + cache
下载:      前端 → download_artifact(space_id, path)               → 系统保存对话框
```

---

## 6. 数据库变更

### 6.1 artifact_cache 表 (新增)

```sql
CREATE TABLE IF NOT EXISTS artifact_cache (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  space_id TEXT NOT NULL,
  path TEXT NOT NULL,          -- 相对 workspace 的完整路径
  name TEXT NOT NULL,
  is_dir INTEGER NOT NULL DEFAULT 0,
  parent_path TEXT NOT NULL DEFAULT '',
  size_bytes INTEGER,
  mime_type TEXT,
  modified_at TEXT,
  cached_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(space_id, path)
);

CREATE INDEX IF NOT EXISTS idx_artifact_cache_space
  ON artifact_cache(space_id);

CREATE INDEX IF NOT EXISTS idx_artifact_cache_parent
  ON artifact_cache(space_id, parent_path);
```

### 6.2 conversations 表变更

```sql
ALTER TABLE conversations ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;
```

---

## 7. IPC 命令扩展

### 7.1 新增命令

```
# Artifact 树
list_artifacts_tree(space_id, path)      → ArtifactTreeNode[]
load_artifact_children(space_id, path)   → ArtifactTreeNode[]
reconcile_artifacts(space_id)            → void

# Artifact 操作
create_artifact_file(space_id, path, content?) → void
create_artifact_folder(space_id, path)         → void
delete_artifact(space_id, path)                → void
rename_artifact(space_id, old_path, new_path)  → void
move_artifact(space_id, src_path, dest_path)   → void
download_artifact(space_id, path)              → void (触发系统保存对话框)
detect_file_type(path)                         → { mime_type, category }

# Artifact 内容
read_artifact(space_id, path)                  → string (文件内容)
write_artifact(space_id, path, content)        → void (已存在，确认)

# 对话收藏
toggle_star_conversation(id)                   → bool (新状态)
```

### 7.2 Tauri Events

```
artifact:tree_update  → { space_id, changes: FileChange[] }
  用于文件监视器推送目录变化
```

---

## 8. 前端 Store 设计

### 8.1 `artifact.svelte.ts`

```typescript
class ArtifactState {
  tree = $state<ArtifactTreeNode[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  async loadRoot(spaceId: string): Promise<void>;
  async loadChildren(spaceId: string, path: string): Promise<void>;
  async createFile(spaceId: string, path: string): Promise<void>;
  async createFolder(spaceId: string, path: string): Promise<void>;
  async delete(spaceId: string, path: string): Promise<void>;
  async rename(spaceId: string, oldPath: string, newPath: string): Promise<void>;
  async move(spaceId: string, src: string, dest: string): Promise<void>;
  handleTreeUpdate(spaceId: string, changes: FileChange[]): void;
}
```

### 8.2 `canvas.svelte.ts` (增强)

```typescript
interface CanvasTab {
  id: string;
  path: string;
  name: string;
  mimeType: string;
  content: string | null;
  isModified: boolean; // 文件在外部被修改
}

class CanvasState {
  tabs = $state<CanvasTab[]>([]);
  activeTabId = $state<string | null>(null);
  isMaximized = $state(false);
  chatWidth = $state<number>(50); // 聊天区宽度百分比

  open(file: { path: string; name: string; mimeType: string }): void;
  close(tabId: string): void;
  closeOthers(tabId: string): void;
  closeAll(): void;
  setActive(tabId: string): void;
  toggleMaximize(): void;
  async loadContent(tabId: string): Promise<void>;
}
```

---

## 9. 现有组件改动点

### 9.1 `App.svelte`
- 布局容器增加 Canvas 区域的条件渲染
- CSS Grid/Flexbox 支持三栏 → 四栏动态切换

### 9.2 `RightSidebar.svelte`
- 内容替换为 `ArtifactTree`
- 移除 TaskApproval 区域（改为 Toast 通知）
- 移除通知面板（已有 ToastContainer）

### 9.3 `ChatView.svelte`
- 集成 `CanvasArea`，根据 canvasStore 状态决定布局
- Canvas 打开时聊天区宽度响应式缩小

### 9.4 `LeftSidebar.svelte`
- 增加 Space 选择器（顶部下拉/切换）
- 保留会话列表

### 9.5 `api.ts`
- 新增所有 artifact 相关的 invoke 方法
- 新增 toggle_star_conversation
- 新增 event listener 注册 (`onArtifactTreeUpdate`)

---

## 10. 错误处理策略

| 场景 | 处理方式 |
|------|---------|
| 文件树加载失败 | artifactStore.error 设置错误信息，UI 显示错误提示 + 重试按钮 |
| 文件不存在 (被外部删除) | Canvas 标签显示"文件已删除"，关闭标签 |
| 文件名冲突 (重命名) | 后端返回错误，前端内联编辑框保持打开，显示冲突提示 |
| 删除非空文件夹 | 确认弹窗特别警告，后端递归删除 |
| 大文件 (>5MB) | CodeViewer 截断显示前1000行 + "文件过大"提示 |
| 不支持的文件类型 | 降级为纯文本 CodeViewer (plain 模式) |
| FileWatcher 错误 | 静默降级，前端显示"实时监视不可用"图标，手动刷新可用 |

---

## 11. 性能考虑

- 目录下文件 > 500 个时，使用虚拟滚动（未来优化，当前不做）
- 文件树懒加载：仅在展开文件夹时加载子节点
- 文件监视防抖 300ms，避免频繁推送
- Canvas 标签内容按需加载：仅在激活标签时读取文件内容
- artifact_cache 表加速树构建，避免每次读取文件系统

---

## 12. 验收标准

1. 右侧栏文件树能正常显示 workspace 目录结构
2. 文件夹支持展开/折叠，子节点懒加载
3. 右键菜单支持新建文件/文件夹、重命名、删除
4. 双击文件在 Canvas 中打开，标签栏正常切换和关闭
5. CodeViewer 正确语法高亮，Markdown/HTML/Image 正确渲染
6. 外部修改文件时，文件树和 Canvas 标签及时更新
7. Space 首页卡片网格正常显示，创建/删除空间可用
8. 对话收藏/取消收藏正常，筛选功能正常
9. Canvas 打开/关闭时聊天区宽度正确调整
10. Canvas 最大化/还原切换正常
