# uClaw 迁移方案（v2 — 基于 Steward UI 设计体系）

## Context

将 hello-halo（Electron + Express.js + React + Tailwind）完整迁移为 uClaw（Tauri v2 + Rust + Svelte 5 + 纯 CSS），100% 功能复刻。

**为什么要做这个迁移：**
- Electron 体积大（~200MB+），内存占用高，启动慢
- Node.js 后端性能瓶颈，不适合 CPU 密集型任务
- Tauri + Rust 可大幅减小应用体积（~10-20MB），降低内存占用（~50%），提升启动速度
- Steward 项目已验证了 Tauri v2 + Svelte 5 + 纯 CSS 的技术路线在生产环境中的可行性
- 统一技术栈利于长期维护

**UI/UX 设计基准：** 严格参照 [Steward](https://github.com/stealth/Steward) 项目的 UI 设计语言和 Tauri 框架配置。对于 hello-halo 独有的功能模块，基于 Steward 的设计语言扩展新的 UI 组件。

**期望结果：**
- 完整保留原项目所有功能
- 应用体积 ~15-20MB
- 内存占用 ~150-250MB
- 启动时间 < 1s
- 前端使用 Svelte 5 + 纯 CSS 变量（暖色系设计语言）
- 后端全部使用纯 Rust
- Tauri 配置严格参照 Steward（无框透明窗口、自定义标题栏、macOS 圆角）

---

## 设计语言（继承自 Steward）

### 色彩系统

**Light 主题**
```css
:root {
  /* 背景 */
  --bg-primary: #f5f0e8;          /* 麦秆色主背景 */
  --bg-sidebar: #faf8f5;          /* 侧栏略浅 */
  --bg-surface: #ffffff;          /* 卡片/表面 */
  --bg-elevated: #e8e4dc;         /* 高程（活跃/悬停） */
  --bg-hover: rgba(0, 0, 0, 0.04);
  --bg-active: #e8e4dc;
  --bg-input: #f0ece4;            /* 输入框背景 */
  --bg-badge: rgba(0, 0, 0, 0.08);

  /* 文本 */
  --text-primary: #3d3d3d;        /* 主文本 */
  --text-secondary: #5c5c5c;      /* 次级文本 */
  --text-tertiary: #6b6b6b;       /* 三级文本 */
  --text-muted: rgba(61, 61, 61, 0.4);

  /* 边框 */
  --border-default: rgba(0, 0, 0, 0.06);
  --border-subtle: rgba(0, 0, 0, 0.04);
  --border-input: rgba(0, 0, 0, 0.08);

  /* 强调色 */
  --accent-primary: #3d3d3d;
  --accent-gold: #c9963a;         /* 品牌金 — 链接、代码、重点 */
  --accent-green: #4ade80;
  --accent-danger: rgba(194, 63, 63, 0.14);
  --accent-danger-text: #9a2f2f;

  /* 阴影 */
  --shadow-dropdown: 0 8px 30px rgba(0,0,0,0.12), 0 0 0 1px rgba(0,0,0,0.06);
  --shadow-container: 0 10px 30px rgba(33,28,24,0.12);
  --shadow-card: 0 2px 12px rgba(0,0,0,0.04);
}
```

**Dark 主题**
```css
[data-theme="dark"] {
  --bg-primary: #1a1a1e;          /* 深黑 */
  --bg-sidebar: #202024;
  --bg-surface: #2a2a2e;
  --bg-elevated: #35353a;
  --bg-hover: rgba(255,255,255,0.06);
  --bg-active: #35353a;
  --bg-input: #2a2a2e;
  --bg-badge: rgba(255,255,255,0.1);

  --text-primary: #e4e4e7;
  --text-secondary: #a1a1aa;
  --text-tertiary: #8b8b94;
  --text-muted: rgba(228,228,231,0.35);

  --border-default: rgba(255,255,255,0.08);
  --border-subtle: rgba(255,255,255,0.04);
  --border-input: rgba(255,255,255,0.1);

  --accent-primary: #e4e4e7;
  --accent-gold: #d4a84b;
  --accent-green: #4ade80;
  --accent-danger: rgba(239,68,68,0.18);
  --accent-danger-text: #ef4444;

  --shadow-dropdown: 0 8px 30px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.08);
  --shadow-container: 0 10px 30px rgba(0,0,0,0.3);
  --shadow-card: 0 2px 12px rgba(0,0,0,0.2);
}
```

### 排版系统

```
字体栈:
  默认: "SF Pro Display", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif
  代码: "SF Mono", "Fira Code", "Cascadia Code", monospace
  Emoji: "Noto Emoji", "Apple Color Emoji", "Segoe UI Emoji"

字号层级:
  Eyebrow: 12px uppercase, letter-spacing 0.08em
  Caption: 13px
  Body: 15px, line-height 1.2
  H3: 1.05em
  H2: 1.2em
  H1: 1.4em
  Page Title: clamp(1.2rem, 2vw, 2rem), weight 700

代码: 13px, line-height 1.55
```

### 组件规范

```
按钮:
  .button            padding: 12px 16px, border-radius: 16px
  .button-primary    background: var(--bg-elevated), hover: translateY(-1px)
  .button-secondary  background: #eef2f5
  .button-ghost      background: #f3f5f7
  .button-link       transparent, color: #295f96
  transition: transform 140ms ease, opacity 140ms ease

卡片:
  .panel             padding: clamp(18px, 2vw, 24px)
  .soft-card         border-radius: 14px, border: 1px solid rgba(24,32,40,0.08)

输入框:
  .composer textarea padding: 14px 16px, border-radius: 18px
  .field input       border: 1px solid var(--border-input)

消息气泡:
  .message-bubble.user       background: linear-gradient(135deg, rgba(255,234,215,0.92), rgba(255,247,238,0.96))
  .message-bubble.assistant  background: linear-gradient(135deg, rgba(240,246,252,0.96), rgba(248,250,252,0.96))
  Dark: user → var(--bg-surface), assistant → var(--bg-elevated)

状态徽章:
  .status-badge.success  background: rgba(76,179,117,0.16), color: #1b7a3f
  .status-badge.warning  background: rgba(255,184,76,0.18), color: #9b5e00
  .status-badge.danger   background: rgba(255,107,107,0.15), color: #a23131
```

---

## 技术选型

### 桌面框架：Tauri v2

**配置严格参照 Steward：**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "uClaw",
  "version": "0.1.0",
  "identifier": "ai.uclaw.desktop",
  "build": {
    "beforeDevCommand": "npm run build",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "static"
  },
  "app": {
    "macOSPrivateApi": true,
    "windows": [
      {
        "title": "uClaw",
        "width": 1280,
        "height": 820,
        "resizable": true,
        "fullscreen": false,
        "dragDropEnabled": true,
        "decorations": false,
        "transparent": true,
        "shadow": true
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/icon.icns", "icons/icon.ico", "icons/icon.png"]
  }
}
```

**关键配置说明：**
- `decorations: false` + `transparent: true` — 无框窗口，自定义标题栏
- `macOSPrivateApi: true` — 支持 macOS 窗口圆角（18px）
- `dragDropEnabled: true` — 支持文件拖放到工作区
- `csp: null` — 开发期宽松，生产期收紧

### 前端框架：Svelte 5

| 对比维度 | React (原 hello-halo) | Svelte 5 (Steward 路线) |
|---------|----------------------|------------------------|
| 打包体积 | ~40KB (React + ReactDOM) | ~2KB (编译时消失) |
| 状态管理 | Zustand (12 个 store) | Svelte Rune API (`$state`) |
| 样式 | Tailwind CSS (JIT) | 纯 CSS + CSS 变量 |
| 组件数 | 130+ TSX | ~100 Svelte (重写) |
| 动画 | 无内置 | Transition API (fly, fade, scale) |
| TypeScript | 内置 | 内置 |
| 学习曲线 | 已有代码 | 语法简洁，与 Steward 对齐 |

### 样式方案：纯 CSS + CSS 变量

**放弃 Tailwind，采用 Steward 的纯 CSS 架构：**
- `app.css` (~350 行) — CSS 变量定义、主题切换、Markdown 样式、Highlight.js 主题
- `layout.css` (~470 行) — 布局网格、三栏布局、消息气泡、按钮、输入框
- `chat.css` (~200 行) — 聊天特定样式
- `settings.css` (~150 行) — 设置面板样式

### 状态管理：Svelte Rune API

```typescript
// lib/stores/sessions.svelte.ts
class SessionsState {
  list = $state<SessionSummary[]>([]);
  active = $state<SessionDetail | null>(null);
  streaming = $state(false);
  
  async fetch() { /* ... */ }
  async create(title?: string) { /* ... */ }
}

export const sessionsStore = new SessionsState();
```

### 后端技术栈（不变）

| 组件 | 选择 | 理由 |
|------|------|------|
| HTTP 框架 | Axum | tokio 生态，与 Tauri 无冲突 |
| 数据库 | rusqlite | 与 better-sqlite3 文件格式兼容 |
| WebSocket | axum::extract::ws | Axum 内置 |
| LLM 集成 | rig-core | 多提供商支持（参照 Steward） |
| Agent | 纯 Rust Agent Loop | 参照 Steward agentic_loop.rs |

---

## 架构设计

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      uClaw Desktop (Tauri v2)                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │     Svelte 5 Frontend (Vite)                                     │  │
│  │  ┌────────────────────────────────────────────────────────────┐  │  │
│  │  │  TitleBar.svelte (自定义标题栏)                             │  │  │
│  │  ├────────┬──────────────────────────┬────────┐               │  │  │
│  │  │ Left   │  Center Content          │ Right  │               │  │  │
│  │  │ Sidebar│  (ChatArea,              │ Sidebar│               │  │  │
│  │  │ 260px  │   Canvas, Pages)         │ 280px  │               │  │  │
│  │  │        │                          │        │               │  │  │
│  │  │ Spaces │  ChatView                │ Artifact│              │  │  │
│  │  │ Conv.  │  Canvas                  │ Browser │              │  │  │
│  │  │ List   │  Settings                │ Store   │              │  │  │
│  │  └────────┴──────────────────────────┴────────┘               │  │  │
│  │  ┌────────────────────────────────────────────────────────┐   │  │  │
│  │  │  API Layer (lib/api.ts)                                │   │  │  │
│  │  │  invoke() + listen() + stream.ts                       │   │  │  │
│  │  └────────────────────────┬───────────────────────────────┘   │  │  │
│  └───────────────────────────┼───────────────────────────────────┘  │  │
│                      │                                                   │  │
│              invoke()│ / listen()                                        │  │
│                      │                                                   │  │
│  ┌───────────────────▼───────────────────────────────────────────────────┤  │
│  │                    Tauri v2 Rust Backend                               │  │
│  │  ┌──────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │  │
│  │  │  tauri_commands  │  │  Services       │  │  Axum HTTP Server   │  │  │
│  │  │  (IPC ~200 cmd)  │  │  (Business Logic)│  │  (Remote Access)    │  │  │
│  │  │                  │  │                 │  │  ┌───────────────┐   │  │  │
│  │  │  get_settings    │  │  config         │  │  │ REST (150+)   │   │  │  │
│  │  │  list_spaces     │  │  space          │  │  │ WebSocket     │   │  │  │
│  │  │  send_message    │  │  conversation   │  │  │ Auth          │   │  │  │
│  │  │  search_artifact │  │  agent          │  │  └───────────────┘   │  │  │
│  │  │  install_app     │  │  artifact       │  └─────────────────────┘  │  │
│  │  │  ...             │  │  ai_browser     │                           │  │
│  │  └──────────────────┘  │  ai_sources     │  ┌─────────────────────┐  │  │
│  │                         │  apps_runtime   │  │  Platform           │  │  │
│  │                         │  im_channels    │  │  ┌───────────────┐   │  │  │
│  │                         │  health         │  │  │ Database      │   │  │  │
│  │                         │  notify         │  │  │ (rusqlite)    │   │  │  │
│  │                         └────────┬────────┘  │  │ Scheduler     │   │  │  │
│  │                                  │           │  │ EventBus      │   │  │  │
│  │                          ┌───────▼────────┐  │  │ FileWatcher   │   │  │  │
│  │                          │  Apps Manager  │  │  └───────────────┘   │  │  │
│  │                          └────────────────┘  └─────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────┤  │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                    Storage Layer                                     │  │
│  │  ~/.uclaw/uclaw.db (SQLite)  │  ~/.uclaw/config.json                │  │
│  │  ~/.uclaw/spaces.json        │  ~/.uclaw/apps/                      │  │
│  └────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 项目结构

```
uclaw/
├── src-tauri/                              # Rust 后端 + Tauri 配置
│   ├── Cargo.toml                          # Workspace 根
│   ├── tauri.conf.json                     # Tauri v2 配置（参照 Steward）
│   ├── capabilities/
│   │   └── default.json
│   ├── icons/
│   │   ├── icon.icns
│   │   ├── icon.ico
│   │   └── icon.png
│   ├── src/
│   │   ├── main.rs                         # Tauri 入口：窗口、插件、托盘
│   │   ├── lib.rs                          # 库入口：模块声明
│   │   ├── tauri_commands.rs               # IPC 命令（~200 个，参照 Steward 模式）
│   │   ├── desktop_runtime.rs              # 桌面运行时初始化
│   │   ├── app.rs                          # AppState 构建器
│   │   ├── settings.rs                     # 用户设置持久化
│   │   ├── error.rs                        # 错误类型
│   │   ├── runtime_events.rs               # 事件发射器（Tauri + SSE）
│   │   ├── ipc.rs                          # IPC 类型定义
│   │   ├── agent/                           # 核心代理循环
│   │   │   ├── mod.rs
│   │   │   ├── session.rs                  # 会话管理
│   │   │   ├── session_manager.rs
│   │   │   ├── agent_loop.rs               # 主代理循环
│   │   │   ├── dispatcher.rs               # 消息分派
│   │   │   ├── scheduler.rs                # 任务调度（数字人）
│   │   │   ├── compaction.rs               # 上下文压缩
│   │   │   ├── cost_guard.rs               # 成本管理
│   │   │   └── self_repair.rs              # 自我修复
│   │   ├── tools/                           # 工具系统
│   │   │   ├── mod.rs
│   │   │   ├── tool.rs                     # Tool trait
│   │   │   ├── registry.rs                 # 工具注册表
│   │   │   ├── mcp/                        # MCP 集成
│   │   │   └── builtin/                    # 内置工具
│   │   │       ├── file.rs, http.rs, shell.rs, web_fetch.rs, ...
│   │   ├── llm/                             # LLM 集成
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs                 # LLM Provider trait
│   │   │   ├── registry.rs                 # 提供者注册表
│   │   │   ├── reasoning.rs                # 推理与 token 计算
│   │   │   ├── failover.rs                 # 故障转移
│   │   │   └── providers/                  # 具体实现
│   │   │       ├── anthropic.rs, openai.rs, deepseek.rs, ...
│   │   ├── db/                              # 数据库
│   │   │   ├── mod.rs                      # Database trait
│   │   │   ├── manager.rs                  # rusqlite 连接管理
│   │   │   ├── migrations.rs               # 迁移系统
│   │   │   └── pragmas.rs                  # WAL/PRAGMA 配置
│   │   ├── workspace/                       # 工作区（参照 Steward）
│   │   │   ├── mod.rs
│   │   │   ├── search.rs                   # 文件搜索
│   │   │   ├── file_watcher.rs             # 文件监视
│   │   │   └── allowlists.rs              # 权限控制
│   │   ├── memory/                          # 记忆系统
│   │   │   └── mod.rs
│   │   ├── skills/                          # 技能注册表
│   │   │   ├── mod.rs
│   │   │   └── registry.rs
│   │   ├── secrets/                         # 密钥管理
│   │   │   ├── mod.rs
│   │   │   └── crypto.rs                   # AES-256-GCM
│   │   ├── config/                          # 配置管理
│   │   │   ├── mod.rs
│   │   │   ├── llm.rs
│   │   │   └── builder.rs
│   │   ├── channels/                        # IM 渠道
│   │   │   ├── mod.rs
│   │   │   ├── wecom.rs, dingtalk.rs, feishu.rs, weixin.rs
│   │   │   └── webhook.rs
│   │   ├── extensions/                      # 扩展系统
│   │   │   ├── mod.rs
│   │   │   └── registry.rs
│   │   ├── safety/                          # 安全层
│   │   │   ├── mod.rs
│   │   │   ├── sanitizer.rs
│   │   │   └── validator.rs
│   │   └── api/                             # Axum HTTP 服务器
│   │       ├── mod.rs
│   │       ├── router.rs
│   │       ├── middleware.rs
│   │       ├── handlers/
│   │       │   ├── mod.rs
│   │       │   ├── config_handler.rs
│   │       │   ├── space_handler.rs
│   │       │   ├── agent_handler.rs
│   │       │   ├── artifact_handler.rs
│   │       │   ├── app_handler.rs
│   │       │   └── ...
│   │       └── websocket.rs
│   └── build.rs
├── ui/                                      # Svelte 5 前端（参照 Steward 结构）
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── src/
│   │   ├── main.ts                          # 入口
│   │   ├── App.svelte                       # 根组件（路由、布局）
│   │   ├── app.css                          # 全局样式 + CSS 变量 + Markdown
│   │   ├── layout.css                       # 布局网格 + 组件样式
│   │   ├── chat.css                         # 聊天特定样式
│   │   ├── settings.css                     # 设置面板样式
│   │   ├── components/
│   │   │   ├── TitleBar.svelte              # 自定义标题栏（窗口控制、模型选择器）
│   │   │   ├── LeftSidebar.svelte           # 左侧栏（Space 列表、会话列表）
│   │   │   ├── RightSidebar.svelte          # 右侧栏（Artifact 文件树、AI 浏览器）
│   │   │   ├── ChatArea.svelte              # 主聊天区（消息流、输入框）
│   │   │   ├── CanvasArea.svelte            # Canvas 标签栏 + 查看器（新增）
│   │   │   ├── StatusBadge.svelte           # 状态标签
│   │   │   ├── ToastContainer.svelte        # 通知容器
│   │   │   ├── TaskApprovalCard.svelte      # 工具审批卡
│   │   │   ├── SpacesList.svelte            # Space 列表（新增）
│   │   │   ├── ConversationList.svelte      # 会话列表
│   │   │   ├── ArtifactTree.svelte          # 文件树（新增）
│   │   │   ├── ArtifactViewer.svelte        # 文件查看器容器（新增）
│   │   │   ├── CodeViewer.svelte            # 代码查看器（新增）
│   │   │   ├── HtmlPreview.svelte           # HTML 预览（新增）
│   │   │   ├── ImagePreview.svelte          # 图片预览（新增）
│   │   │   ├── AIBrowser.svelte             # AI 浏览器面板（新增）
│   │   │   ├── ModelSelector.svelte         # 模型选择器
│   │   │   ├── SearchPanel.svelte           # 搜索面板（新增）
│   │   │   ├── UpdaterBadge.svelte          # 更新提示
│   │   │   ├── Markdown.svelte              # Markdown 渲染组件
│   │   │   ├── brand/
│   │   │   │   └── UClawMark.svelte         # Logo SVG
│   │   │   └── settings/
│   │   │       ├── LlmConfigurationPanel.svelte  # AI 模型配置
│   │   │       ├── AppearanceSettings.svelte     # 外观设置（新增）
│   │   │       ├── RemoteAccessPanel.svelte      # 远程访问（新增）
│   │   │       ├── ImChannelsPanel.svelte        # IM 渠道管理（新增）
│   │   │       ├── NotifyChannelsPanel.svelte    # 通知渠道（新增）
│   │   │       ├── AdvancedSettings.svelte       # 高级设置（新增）
│   │   │       └── HealthPanel.svelte            # 健康系统（新增）
│   │   ├── views/
│   │   │   ├── HomeView.svelte               # Space 首页视图（新增）
│   │   │   ├── ChatView.svelte               # 聊天视图
│   │   │   ├── AppsView.svelte               # 数字人应用视图（新增）
│   │   │   ├── SettingsView.svelte           # 设置视图
│   │   │   ├── OnboardingView.svelte         # 初始化引导
│   │   │   ├── AppDetailView.svelte          # 应用详情（新增）
│   │   │   └── AppChatView.svelte            # 应用聊天（新增）
│   │   └── lib/
│   │       ├── api.ts                        # Tauri IPC 调用包装
│   │       ├── types.ts                      # TypeScript 类型定义
│   │       ├── markdown.ts                   # Markdown 渲染 + 代码高亮
│   │       ├── presentation.ts               # UI 展示逻辑
│   │       ├── stream.ts                     # 事件流订阅
│   │       ├── router.svelte.ts              # Hash-based 路由
│   │       └── stores/
│   │           ├── theme.svelte.ts           # 主题切换
│   │           ├── settings.svelte.ts        # 全局设置
│   │           ├── spaces.svelte.ts          # Space 管理（新增）
│   │           ├── sessions.svelte.ts        # 会话管理 + 实时流
│   │           ├── canvas.svelte.ts          # Canvas 状态（新增）
│   │           ├── artifact.svelte.ts        # Artifact 文件管理（新增）
│   │           ├── apps.svelte.ts            # 数字人应用（新增）
│   │           ├── ai_browser.svelte.ts      # AI 浏览器状态（新增）
│   │           ├── remote.svelte.ts          # 远程访问状态（新增）
│   │           ├── search.svelte.ts          # 搜索状态（新增）
│   │           ├── toast.svelte.ts           # 通知管理
│   │           └── perf.svelte.ts            # 性能监控（新增）
│   └── public/
│       └── fonts/
│           └── noto-emoji.css
├── static/                                   # Vite 构建输出
├── migrations/                               # 数据库迁移脚本
├── skills/                                   # SKILL.md 技能扩展
├── docs/                                     # 文档
└── CLAUDE.md
```

---

## 新增 UI 模块设计（hello-halo 独有功能）

以下是 hello-halo 有但 Steward 没有的功能模块，需要基于 Steward 设计语言新建：

### 1. Space 管理系统

**路由**: `#/home` (首页，显示 Space 列表)

**组件**：
- `SpacesList.svelte` — 卡片网格布局，Icon + 名称 + 最后活跃时间
- `CreateSpaceDialog` — 弹窗，输入名称、选择图标(Emoji)、可选自定义路径
- `SpaceCard` — `.soft-card` 样式，hover 效果

```
┌──────────────────────────────────────────────┐
│  [uClaw Logo]          Spacer      [Settings]│
├──────────────────────────────────────────────┤
│                                              │
│  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
│  │ 🚀      │  │ 📁      │  │ ＋ 创建空间  │  │
│  │ My Space│  │ Project │  │              │  │
│  │ 12会话  │  │ 3会话   │  │              │  │
│  └─────────┘  └─────────┘  └─────────────┘  │
│                                              │
└──────────────────────────────────────────────┘
```

### 2. Canvas 系统

**位置**: 聊天视图的中央区域（与 Steward 的 RightSidebar 不同，原 halo 用中央标签栏）

**组件**：
- `CanvasArea.svelte` — VS Code 风格标签栏 + 查看器主体
- `CanvasTab.svelte` — 单个标签（文件名、关闭按钮、右键菜单）
- `ArtifactViewer.svelte` — 查看器路由容器
  - `CodeViewer.svelte` → CodeMirror 6 嵌入
  - `HtmlPreview.svelte` → iframe 沙箱
  - `ImagePreview.svelte` → 图片缩放
  - `MarkdownViewer.svelte` → Markdown 渲染
  - `DiffViewer.svelte` → 差异对比

**样式**：标签栏使用 `.canvas-tabs` 样式，参照 VSCode 风格但使用 uClaw 色彩变量

### 3. AI 浏览器面板

**位置**: 右侧栏或 Canvas 中的一个标签

**组件**：
- `AIBrowser.svelte` — 嵌入式 WebView 容器
- 地址栏：仿 Steward 输入框样式
- 设备切换：`.status-badge` 样式切换 PC/H5
- 缩放控制：按钮组

### 4. 数字人应用管理

**路由**: `#/apps`

**组件**：
- `AppsView.svelte` — 应用市场/我的应用
- `AppCard.svelte` — `.soft-card` 样式，图标 + 名称 + 状态徽章
- `AppDetailView.svelte` — 应用详情（配置、历史运行、聊天）
- `AppChatView.svelte` — 与数字人的聊天界面
- `AppInstallDialog.svelte` — 安装向导

### 5. 远程访问面板

**位置**: 设置中的子面板

**组件**：
- `RemoteAccessPanel.svelte` — QR 码 + 服务器状态 + 端口管理
- QR 码使用 Canvas 渲染
- 状态指示：`.status-badge.success/warning/danger`

### 6. IM 渠道管理

**位置**: 设置中的子面板

**组件**：
- `ImChannelsPanel.svelte` — 渠道列表 + 配置
- 每个渠道一张 `.soft-card`，包含状态徽章、测试按钮
- 微信/企业微信/钉钉/飞书各自的配置表单

### 7. 搜索面板

**组件**：
- `SearchPanel.svelte` — 弹出式搜索面板
- 搜索输入框：`.composer textarea` 样式
- 结果列表：`.session-tile` 样式

### 8. 健康系统面板

**组件**：
- `HealthPanel.svelte` — 系统状态仪表板
- 进程状态卡片
- 恢复按钮：`.button-ghost` 样式

### 9. 性能监控面板

**组件**：
- `PerfMonitor.svelte` — FPS/内存/CPU 实时图表
- 隐藏在开发者工具下

---

## Tauri IPC 命令命名规范（参照 Steward）

```
Steward 模式:  {resource}_{action}
uClaw 沿用:    {resource}_{action}

示例:
  get_settings, patch_settings
  list_spaces, create_space, delete_space, get_space
  list_conversations, create_conversation, send_message
  list_artifacts, get_artifact, create_artifact_file, delete_artifact
  list_apps, install_app, uninstall_app, trigger_app
  search_workspace, search_conversations
  enable_remote, disable_remote, get_remote_status

事件命名:
  session:{event_type}  (参照 Steward 的 session:response 模式)
  agent:message, agent:complete, agent:tool_call, agent:error
  artifact:tree_update, artifact:changed
  app:status_changed, app:activity_entry
```

---

## 前端 Store 设计（Svelte Rune API）

参照 Steward 的 Store 模式：

```typescript
// lib/stores/spaces.svelte.ts
class SpacesState {
  list = $state<SpaceSummary[]>([]);
  active = $state<SpaceDetail | null>(null);
  loading = $state(false);

  async fetch() {
    this.loading = true;
    this.list = await apiClient.listSpaces();
    this.loading = false;
  }
  
  async create(input: CreateSpaceInput) {
    const space = await apiClient.createSpace(input);
    this.list = [...this.list, space];
    return space;
  }
}
export const spacesStore = new SpacesState();

// lib/stores/sessions.svelte.ts (参照 Steward)
class SessionsState {
  list = $state<SessionSummary[]>([]);
  active = $state<SessionDetail | null>(null);
  streaming = $state(false);

  async fetch(spaceId: string) { /* ... */ }
  async sendMessage(content: string, attachments: File[]) { /* ... */ }
}
export const sessionsStore = new SessionsState();

// lib/stores/canvas.svelte.ts (新增)
class CanvasState {
  tabs = $state<CanvasTab[]>([]);
  activeTabId = $state<string | null>(null);
  isMaximized = $state(false);
  
  open(file: FileInfo) { /* ... */ }
  close(tabId: string) { /* ... */ }
  closeOthers(tabId: string) { /* ... */ }
}
export const canvasStore = new CanvasState();
```

---

## 路由设计（Hash-based，参照 Steward）

```typescript
// lib/router.svelte.ts
export type View = 
  | "home"        // Space 列表首页
  | "chat"        // 聊天视图（带 spaceId 参数）
  | "apps"        // 数字人应用
  | "app-detail"  // 应用详情
  | "app-chat"    // 应用聊天
  | "settings"    // 设置
  | "onboarding"; // 初始化引导

// URL 格式:
// #/home
// #/chat?space=xxx
// #/apps
// #/apps/xxx
// #/apps/xxx/chat
// #/settings
// #/settings/ai-sources
// #/settings/remote
// #/onboarding
```

---

## 组件树与实际布局

```
App.svelte
├── TitleBar.svelte (窗口装饰 + 控件)
│   ├── [⊢ Toggle Sidebar] [uClaw Logo] [Space Name] [Model ▼] [⚙ Settings]
│   └── macOS traffic lights / Windows controls
├── TopNav.svelte (二级导航)
│   └── [Home] [Chat] [Apps] [Settings]
├── LeftSidebar.svelte (260px, 可折叠至 56px)
│   ├── Space 列表 (SpacesList)
│   ├── 会话列表 (ConversationList)
│   │   ├── .session-tile (会话卡片)
│   │   └── [+ 新会话] 按钮
│   └── 搜索按钮
├── CenterContent (flex: 1)
│   ├── HomeView.svelte (#/home)
│   ├── ChatView.svelte (#/chat)
│   │   ├── ChatArea.svelte
│   │   │   ├── CanvasArea.svelte (标签栏 + 查看器)
│   │   │   │   ├── CanvasTab × N
│   │   │   │   └── ArtifactViewer
│   │   │   │       ├── CodeViewer
│   │   │   │       ├── HtmlPreview
│   │   │   │       └── ImagePreview
│   │   │   ├── MessageStream (消息流)
│   │   │   │   ├── MessageBubble.user
│   │   │   │   ├── MessageBubble.assistant
│   │   │   │   │   └── Markdown
│   │   │   │   ├── ThinkingBlock (思考过程)
│   │   │   │   └── ToolCallCard (工具调用)
│   │   │   └── MessageComposer (输入框)
│   │   └── RightSidebar (280px, 可隐藏)
│   │       ├── ArtifactTree (文件树)
│   │       ├── AIBrowser (嵌入式浏览器)
│   │       └── SearchPanel (搜索结果)
│   ├── AppsView.svelte (#/apps)
│   ├── AppDetailView.svelte (#/apps/xxx)
│   ├── AppChatView.svelte (#/apps/xxx/chat)
│   ├── SettingsView.svelte (#/settings)
│   │   ├── LlmConfigurationPanel
│   │   ├── AppearanceSettings
│   │   ├── RemoteAccessPanel
│   │   ├── ImChannelsPanel
│   │   ├── NotifyChannelsPanel
│   │   ├── AdvancedSettings
│   │   └── HealthPanel
│   └── OnboardingView.svelte (#/onboarding)
├── TaskApprovalCard.svelte (底部浮动)
└── ToastContainer.svelte (右上角浮动)
```

---

## 分阶段实施计划

### Phase 1：基础框架（4 周）

**目标：** Tauri v2 窗口启动、Svelte 5 空壳渲染、纯 CSS 主题系统、基础 IPC

1. 初始化 Tauri v2 + Svelte 5 项目脚手架
   - 参照 Steward 的 `tauri.conf.json`（无框透明窗口）
   - Vite + Svelte 5 构建配置
2. 实现纯 CSS 设计系统
   - `app.css` — CSS 变量、Light/Dark 主题
   - `layout.css` — 三栏布局、基础组件样式
   - `TitleBar.svelte` — 自定义标题栏
3. 实现主题切换
   - `theme.svelte.ts` — Rune API store
   - `data-theme` 属性切换
4. 搭建 Rust 后端骨架
   - `main.rs` + `lib.rs` + `desktop_runtime.rs`
   - `AppState` 结构体
   - 数据库层（rusqlite + 迁移系统）
5. 实现 5 个基础 IPC 命令
   - `get_settings` / `patch_settings`
   - `get_platform` / `get_version`
   - `get_bootstrap_status`
6. 实现 Hash-based 路由

### Phase 2：核心 UI + 功能（6 周）

**目标：** 完整的聊天界面、Space 管理、Agent 基本可用

1. Space 管理系统
   - `spaces.svelte.ts` store
   - `SpacesList.svelte` — 首页卡片布局
   - `CreateSpaceDialog`
2. 聊天系统
   - `ChatArea.svelte` — 消息流 + 输入框
   - `MessageBubble` — 用户/助手 气泡（Steward 暖色渐变）
   - `Markdown.svelte` — Highlight.js 代码高亮
   - `ThinkingBlock.svelte` — 思考过程折叠
   - `ToolCallCard.svelte` — 工具调用卡片
3. 会话管理
   - `ConversationList.svelte` — 左侧栏会话列表
   - `sessions.svelte.ts` store — 实时流状态
4. Agent 循环（纯 Rust）
   - 参照 Steward 的 `agentic_loop.rs` + `dispatcher.rs` 实现 React 模式
   - LLM 集成（rig-core：Anthropic + OpenAI + DeepSeek）
   - 基础工具（Read、Write、Grep、Glob、WebFetch）
   - 事件流（TauriEventEmitter → 前端 `listen()` 接收）
   - 会话管理（`session.rs`）
5. Config 服务
   - LLM 配置面板（`LlmConfigurationPanel.svelte`）
   - AI API 验证
6. Artifact 基本功能
   - `ArtifactTree.svelte` — 文件树
   - 文件 CRUD IPC 命令

### Phase 3：高级功能（6 周）

**目标：** Canvas、AI 浏览器、数字人、通知

1. Canvas 系统
   - `CanvasArea.svelte` — 标签栏 + 查看器
   - `CodeViewer.svelte` — CodeMirror 6
   - `HtmlPreview.svelte` — iframe 沙箱
   - `ImagePreview.svelte` — 图片缩放
2. AI 浏览器
   - `AIBrowser.svelte` — 嵌入式 WebView
   - CDP 客户端（`chromiumoxide`）
3. 数字人应用管理
   - `AppsView.svelte` — 应用市场/我的应用
   - `AppDetailView.svelte` — 应用详情
   - `AppChatView.svelte` — 应用聊天
   - `AppInstallDialog` — 安装向导
4. 通知系统
   - `ToastContainer.svelte`
   - 通知渠道（Email、Webhook）
5. 搜索系统
   - `SearchPanel.svelte`
   - 全文搜索 IPC

### Phase 4：远程访问 & IM 渠道（4 周）

**目标：** Axum HTTP 服务器、IM 渠道

1. Axum HTTP 服务器
   - 全部 150+ API 路由
   - WebSocket 升级
   - JWT 认证中间件
2. 远程访问 UI
   - `RemoteAccessPanel.svelte` — QR 码 + 状态
3. IM 渠道
   - 企业微信/钉钉/飞书/微信 iLink
   - `ImChannelsPanel.svelte`
   - 渠道状态徽章

### Phase 5：测试 & 发布（4 周）

1. 测试
   - Rust 单元测试（`cargo test`）
   - Svelte 组件测试
   - E2E 测试（Playwright）
2. 跨平台适配
   - macOS 窗口圆角（18px）
   - Windows NSIS 安装器
   - Linux AppImage
3. 更新器集成
4. 性能基准测试

---

## Claude Agent SDK 现状分析与纯 Rust 迁移路径

### 一、hello-halo 中 SDK 的实现方式

#### 1.1 整体架构

hello-halo 使用 `@anthropic-ai/claude-agent-sdk`（Node.js 包），核心文件位于 `src/main/services/agent/`：

```
sendMessage() 入口
    ↓
session-manager.ts     ← 会话生命周期管理（创建/复用/销毁）
    ↓
resolved-sdk.ts        ← SDK 动态加载（支持 anthropic/halo 双引擎切换）
    ↓
createSession()        ← 启动 Claude Code CLI 子进程（ELECTRON_RUN_AS_NODE）
    ↓
session-consumer.ts    ← 持久 REPL 消费者循环
    ↓
stream-processor.ts    ← 流事件处理（token 级实时流）
```

#### 1.2 SDK 的实际运行方式

Claude Agent SDK 本质上是一个**子进程管理器**：
- 启动 **Headless Electron 子进程** 运行 Claude Code CLI
- 通过 **stdio**（stdin/stdout）进行双向 JSON-RPC 通信
- `v2Session.send(message)` → 写入 stdin
- `v2Session.stream()` → 从 stdout 读取流式事件

```typescript
// sdk-config.ts — SDK 配置核心
const sdkOptions = {
  model: credentials.sdkModel,         // 模型标识
  cwd: workDir,                        // 工作目录
  executable: electronPath,            // Headless Electron 二进制
  env: {
    ELECTRON_RUN_AS_NODE: 1,           // 用 Node.js 模式运行 Electron
    ANTHROPIC_API_KEY: apiKey,         // API 密钥
    ANTHROPIC_BASE_URL: baseUrl,       // API 端点
    CLAUDE_CONFIG_DIR: configDir,      // Skills/MCP 配置目录
  },
  systemPrompt: buildSystemPrompt(),   // 自定义系统提示词
  allowedTools: ['Read','Write','Edit','Grep','Glob','Bash','Skill'],
  mcpServers: {                        // MCP 服务器
    'ai-browser': createAIBrowserMcpServer(),
    'halo-apps':  createHaloAppsMcpServer(),
    'web-search': createWebSearchMcpServer(),
  },
  maxTurns: 50,                        // 最大工具调用轮数
  permissionMode: 'bypassPermissions',
  includePartialMessages: true,        // 启用 token 级流
}
```

#### 1.3 SDK 提供的核心能力

| 能力 | 实现方式 | 说明 |
|------|---------|------|
| **内置工具** | Claude Code CLI 内部 | Bash、Read、Write、Edit、Grep、Glob、Skill |
| **MCP 服务器** | SDK 的 `createSdkMcpServer()` | 注册为子进程的 MCP 客户端 |
| **流式响应** | `v2Session.stream()` | 返回 `AsyncIterable<SDKMessage>` |
| **子代理** | Teams 功能 | 多子进程并行处理 |
| **上下文管理** | SDK 内部 compaction | 自动压缩过长上下文 |
| **工具权限** | `canUseTool` 回调 | 细粒度工具批准 |

#### 1.4 流事件类型

```typescript
// stream-processor.ts 处理的事件
system:init           // 会话初始化
stream_event          // token 级实时流（content_block_start/delta/stop）
assistant             // 助手完整回复（含 tool_use blocks）
result                // 流结束（含 token 用量）
```

### 二、Steward 的纯 Rust Agent 实现（参照目标）

#### 2.1 核心循环架构

Steward 完全用 Rust 实现了 React 模式：

```rust
// agentic_loop.rs — 主循环
pub async fn run_agentic_loop() -> Result<LoopOutcome> {
    for iteration in 1..=max_iterations {
        check_signals();                       // 中断/消息注入
        delegate.before_llm_call();            // 成本检查、工具刷新
        let output = reasoning.respond_with_tools(); // LLM 调用
        match output {
            ToolCalls { .. } => execute_tools(),    // 执行→结果→继续
            Text(text) => return Response(text),    // 完成
        }
    }
}
```

**关键特性**：
- **工具意图检测**：LLM "说"要用工具但未实际调用 → 自动注入对齐提示
- **截断管理**：3 次截断后强制文本模式
- **多提供商**：通过 `LlmProvider` trait + rig-core 支持 OpenAI/Anthropic/DeepSeek 等
- **工具审批**：`Tool::requires_approval()` 返回 `ApprovalRequirement`
- **流式事件**：`StreamDelta::TextDelta` 实时转发到前端

#### 2.2 工具系统

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, params: Value, ctx: &JobContext) -> Result<ToolOutput>;
}

// 三类工具：builtin/ (20+) + mcp/ (动态) + wasm/ (沙箱)
```

### 三、纯 Rust 迁移的核心思路

**不需要在 Rust 中运行 Claude Agent SDK。** 参照 Steward，用 Rust 原生实现 Agent 循环。

```
hello-halo (Electron)              uClaw (Tauri + Rust)
══════════════════════              ══════════════════════
                                    
Claude Agent SDK (Node.js)    →   纯 Rust Agent Loop
  ├─ createSession()          →    Agent::create_session()
  ├─ v2Session.send()         →    Agent::send_message()
  ├─ v2Session.stream()       →    agentic_loop::run() + EventEmitter
  ├─ tool() defs              →    Tool trait + registry
  └─ createSdkMcpServer()     →    MCP client (纯 Rust)
Express HTTP                   →   Axum HTTP
ws WebSocket                   →   axum::extract::ws
better-sqlite3                 →   rusqlite
```

### 四、迁移对照表

| hello-halo 模块 | 大小 | uClaw Rust 实现 | 参照 |
|----------------|------|----------------|------|
| `session-manager.ts` | 36.7 KB | `agent/session.rs` | Steward 同模块 |
| `stream-processor.ts` | 44.8 KB | `llm/reasoning.rs` (3,909 行) | Steward 推理引擎 |
| `session-consumer.ts` | 16.2 KB | `agent/dispatcher.rs` (3,888 行) | Steward ChatDelegate |
| `sdk-config.ts` | 20.2 KB | `config/llm.rs` | Steward 配置系统 |
| `resolved-sdk.ts` | 9.9 KB | `llm/registry.rs` (Provider 注册) | Steward RigAdapter |
| `mcp-manager.ts` | 9.5 KB | `tools/mcp/` (1,500+ 行) | Steward MCP 客户端 |

### 五、需自行实现的 SDK 内置工具

| SDK 工具 | Rust 实现 | 难度 | 说明 |
|---------|----------|------|------|
| `Bash` | `tools/builtin/shell.rs` | 中 | portable-pty + 超时 + 安全沙箱 |
| `Read` | `tools/builtin/file.rs` | 低 | tokio::fs::read_to_string |
| `Write` | `tools/builtin/file.rs` | 低 | tokio::fs::write |
| `Edit` | `tools/builtin/file.rs` | 中 | similar crate 做 diff apply |
| `Grep` | `tools/builtin/search.rs` | 低 | ripgrep crate |
| `Glob` | `tools/builtin/search.rs` | 低 | glob crate |
| `Skill` | `skills/registry.rs` | 中 | SKILL.md 解析 + 注册表 |
| `WebFetch` | `tools/builtin/web_fetch.rs` | 低 | reqwest + HTML→text |
| `WebSearch` | `tools/builtin/web_search.rs` | 中 | 多搜索引擎 API |

Steward 已实现上述大部分工具，可直接参照。

### 六、分阶段迁移路径

```
Phase 1-2：纯 Rust Agent Loop 基本可用
  ├─ LLM 集成（rig-core：Anthropic + OpenAI + DeepSeek）
  ├─ 基础工具（Read、Write、Grep、Glob、WebFetch）
  ├─ 事件流（TauriEventEmitter → 前端 listen())
  └─ 会话管理（session.rs）

Phase 3：工具完善
  ├─ Bash/Shell（portable-pty）
  ├─ Edit（similar crate）
  ├─ Skill 系统（SKILL.md 解析 + 注册表）
  ├─ MCP 集成（纯 Rust MCP client）
  └─ WebSearch（多搜索引擎）

Phase 4+：高级特性
  ├─ AI 浏览器（chromiumoxide CDP）
  ├─ 上下文压缩（summarization）
  └─ 成本守卫（token 预算管理）
```

### 七、方案对比

| 维度 | 隐藏 WebView 运行 SDK | 纯 Rust Agent Loop |
|------|----------------------|-------------------|
| 开发速度 | 快（复用已有逻辑） | 中等（Steward 有参照） |
| 运行时依赖 | Node.js + Electron 二进制 (~100MB) | 零外部依赖 |
| 启动速度 | ~2s | ~100ms |
| 可定制性 | 受 SDK 接口限制 | 完全控制 |
| 维护成本 | SDK 升级适配风险 | 自主可控 |
| 生态对齐 | 无 | 与 Steward 完全一致 |

**结论**：纯 Rust Agent Loop 是长期最优方案。Steward 已验证可行性（283 个 Rust 文件、生产级质量），uClaw 直接参照其架构实现。

---

## 关键技术挑战与应对

| 挑战 | 风险等级 | 应对方案 |
|------|---------|---------|
| React → Svelte 5 重写 130+ 组件 | 高 | 按 Phase 逐步重写；优先核心组件，次要组件延后 |
| Agent 循环从零实现 | 中 | 直接参照 Steward 的 `agentic_loop.rs` + `dispatcher.rs`，不重复造轮子 |
| SDK 内置工具（Bash/Edit/Skill）需重写 | 中 | Steward 已实现大部分；Bash 用 `portable-pty`，Edit 用 `similar` crate |
| CDP 协议实现 | 中 | 使用 `chromiumoxide` crate（纯 Rust CDP 客户端） |
| node-pty 替代 | 低 | Unix 用 `portable-pty`；Windows 用 ConPTY |
| better-sqlite3 → rusqlite | 低 | SQLite 文件格式兼容 |
| Tailwind → 纯 CSS | 中 | 全部组件用 Steward 的 CSS 变量体系重写样式 |
| 流式响应兼容性 | 低 | Steward 已实现 token 级流式（`StreamDelta::TextDelta`），直接参照 |

---

## 依赖映射

| hello-halo (npm) | uClaw (Rust/JS) | 用途 |
|-----------------|-----------------|------|
| `react`, `react-dom` | `svelte` 5 | 前端框架 |
| `zustand` | Svelte Rune API | 状态管理 |
| `tailwindcss` | 纯 CSS + 变量 | 样式 |
| `better-sqlite3` | `rusqlite` (bundled) | 数据库 |
| `@anthropic-ai/claude-agent-sdk` | 纯 Rust Agent Loop | AI Agent |
| `express` | `axum` | HTTP 服务器 |
| `ws` | `axum::extract::ws` | WebSocket |
| `i18next` | 硬编码双语（参照 Steward） | 国际化 |
| `lucide-react` | `lucide-svelte` | 图标 |
| `@codemirror/*` | `@codemirror/*` (保留) | 代码编辑 |
| `react-markdown` | `marked` + `highlight.js` | Markdown |
| `electron-updater` | `tauri-plugin-updater` | 应用更新 |
| `node-pty` | `portable-pty` | 伪终端 |
| `nodemailer` | `lettre` | SMTP |

---

## 验证方案

### 功能验证
1. UI 视觉一致性：与 Steward 截图对比组件渲染
2. IPC 接口比对：preload/index.ts 每个方法 → Tauri command 映射检查
3. 功能矩阵测试：按模块逐一测试
4. 数据迁移测试：从 hello-halo 导入数据完整性检查

### 性能验证
- 启动时间：Tauri < 1s
- 内存占用：< 250MB（空载）
- 包体积：< 20MB

### 设计验证
- 所有组件使用 CSS 变量（零硬编码颜色）
- Light/Dark 主题完整切换
- 三栏布局响应式断点（1200px / 860px / 560px）
- 动画过渡效果一致（fly 220ms, fade 150-200ms, scale 220ms）
