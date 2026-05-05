# Phase 3: AI Browser — Design Spec

> 目标：在 uclaw (Tauri v2 + Rust + Svelte 5) 中实现 AI 浏览器子系统，对标 hello-halo BrowserViewService + BrowserContext + CDP 工具集。

---

## 1. 架构总览

```
┌─── Tauri App ───────────────────────────────────────────┐
│                                                          │
│  Frontend (Svelte 5)                                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │  CanvasArea.svelte (已有)                          │ │
│  │    └─ BrowserViewer.svelte (新建)                  │ │
│  │       ├─ 地址栏 + 导航按钮 + 设备切换 + 缩放        │ │
│  │       ├─ Tauri WebviewWindow 嵌入浏览页面           │ │
│  │       └─ 策略阻断覆盖层                              │ │
│  │                                                     │ │
│  │  ToolTaskCard 增强 — AI 工具结果中嵌入截图/链接       │ │
│  └────────────────────────────────────────────────────┘ │
│                          ↕ IPC                           │
│  Backend (Rust)                                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │  browser/mod.rs — BrowserService                   │ │
│  │    ├─ Chromium 进程生命周期 (launch/kill)          │ │
│  │    ├─ CDP 连接 (chromiumoxide crate)                │ │
│  │    ├─ Page 管理 (多个 tab)                          │ │
│  │    └─ AccessibilitySnapshotProvider                 │ │
│  │                                                     │ │
│  │  browser/tools.rs — CDP 工具注册表                   │ │
│  │    ├─ browser_snapshot   ├─ browser_screenshot      │ │
│  │    ├─ browser_navigate   ├─ browser_click           │ │
│  │    ├─ browser_fill       ├─ browser_hover           │ │
│  │    ├─ browser_press_key  ├─ browser_drag            │ │
│  │    ├─ browser_inspect    ├─ browser_download         │ │
│  │    ├─ browser_emulate    ├─ browser_tab              │ │
│  │    ├─ browser_wait_for   └─ browser_evaluate         │ │
│  │                                                     │ │
│  │  browser/discovery.rs — Chromium 路径发现            │ │
│  └──────────────────┬─────────────────────────────────┘ │
│                     │ CDP (WebSocket)                    │
│  ┌─ External ───────┴─────────────────────────────────┐ │
│  │  Headless Chromium                                   │ │
│  │    --remote-debugging-port=0 --headless=new          │ │
│  │    CDP endpoint: ws://127.0.0.1:{port}              │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

### 与 hello-halo 的差异

| 项目 | hello-halo (Electron) | uclaw (Tauri) |
|------|-----------------------|---------------|
| 浏览器引擎 | Electron BrowserView (内嵌 Chromium) | chromiumoxide 启动外部 headless Chromium |
| 用户可见浏览 | BrowserView 直接附加到窗口 | Tauri WebviewWindow 加载同一 URL |
| CDP 通信 | `webContents.debugger` | chromiumoxide crate (直接的 WebSocket CDP) |
| 截图 | `webContents.capturePage()` (原生) | CDP `Page.captureScreenshot` |
| 下载 | `will-download` 事件 (原生) | CDP `Browser.setDownloadBehavior` + 监听 |
| 设备模拟 | CDP Emulation (原生) | CDP Emulation (相同) |
| 策略引擎 | 内置 allowlist/blocklist | 自建 policy 模块 |

---

## 2. 文件结构

### 后端新增

```
src-tauri/
├── Cargo.toml                        ← 新增 chromiumoxide
├── src/
│   ├── browser/
│   │   ├── mod.rs                    ← BrowserService, BrowserState
│   │   ├── tools.rs                  ← CDP 工具注册表 (15 个工具)
│   │   ├── discovery.rs              ← Chromium 路径发现
│   │   ├── snapshot.rs              ← AccessibilitySnapshot 生成
│   │   ├── policy.rs                 ← 浏览策略 (allowlist/blocklist)
│   │   └── types.rs                  ← 浏览器内部类型
│   ├── ipc.rs                        ← 新增浏览器 IPC 类型
│   ├── tauri_commands.rs             ← 新增浏览器 IPC 命令
│   ├── app.rs                        ← AppState 新增 browser_service
│   └── main.rs                       ← 注册新命令
```

### 前端新增

```
ui/src/
├── lib/
│   ├── types.ts                      ← 新增浏览器类型
│   ├── api.ts                        ← 新增浏览器 API 方法
│   └── stores/
│       └── browser.svelte.ts         ← 新建 browserStore
├── components/
│   └── canvas/
│       ├── CanvasArea.svelte         ← 增强：支持 browser tab
│       └── BrowserViewer.svelte      ← 新建浏览器查看器
```

---

## 3. 后端详细设计

### 3.1 Cargo.toml 新增依赖

```toml
# AI Browser
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }
```

### 3.2 BrowserService（`browser/mod.rs`）

管理 Chromium 进程生命周期、CDP 会话、Page 实例：

```rust
use chromiumoxide::{Browser, BrowserConfig, Page, handler::viewport::Viewport};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct BrowserService {
    browser: Arc<RwLock<Option<Browser>>>,
    pages: Arc<RwLock<Vec<PageHandle>>>,
    active_page_idx: Arc<RwLock<usize>>,
    chromium_path: PathBuf,
    is_launched: Arc<RwLock<bool>>,
    view_states: Arc<RwLock<HashMap<String, BrowserViewState>>>,
    snapshot_provider: Arc<SnapshotProvider>,
}

pub struct PageHandle {
    id: String,         // viewId
    page: Page,
    url: String,
    title: String,
    device_mode: DeviceMode,
}

pub struct BrowserViewState {
    pub id: String,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,   // base64 data URL
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub zoom_level: f64,
    pub device_mode: DeviceMode,
    pub error: Option<String>,
    pub blocked_by_policy: bool,
}

pub enum DeviceMode { Pc, H5 }
```

**核心方法：**

```rust
impl BrowserService {
    /// 发现 Chromium 可执行文件路径
    pub fn discover_chromium() -> Option<PathBuf> { ... }

    /// 配置并启动 headless Chromium
    pub async fn launch(&self) -> Result<(), Error> { ... }

    /// 创建新 tab (Page)
    pub async fn create_view(&self, url: Option<&str>, device: DeviceMode) -> Result<PageHandle, Error> { ... }

    /// 导航到 URL
    pub async fn navigate(&self, view_id: &str, url: &str) -> Result<(), Error> { ... }

    /// 获取页面状态
    pub fn get_state(&self, view_id: &str) -> Option<BrowserViewState> { ... }

    /// 获取所有页面状态
    pub fn get_all_states(&self) -> Vec<BrowserViewState> { ... }

    /// 关闭指定 view
    pub async fn close_view(&self, view_id: &str) -> Result<(), Error> { ... }

    /// 关闭浏览器进程
    pub async fn shutdown(&self) -> Result<(), Error> { ... }

    /// 获取当前活跃 Page (供工具使用)
    pub async fn get_active_page(&self) -> Result<Page, Error> { ... }

    /// 获取 accessibility snapshot
    pub async fn get_snapshot(&self, verbose: bool) -> Result<AccessibilitySnapshot, Error> { ... }
}
```

### 3.3 CDP 工具集（`browser/tools.rs`）

对标 hello-halo 的完整工具集，共计 **15 个工具**：

#### 3.3.1 快照工具（3 个）

| 工具 | CDP 域/方法 | 输入参数 | 输出 |
|------|------------|---------|------|
| `browser_snapshot` | Accessibility.getFullAXTree | `verbose?: bool` | 结构化文本 (带 UID) |
| `browser_screenshot` | Page.captureScreenshot | `format?, quality?, uid?, fullPage?` | base64 image (JPEG 默认) |
| `browser_evaluate` | Runtime.evaluate | `function: string, args?` | JSON 返回值 |

**Snapshot 输出格式示例**（对标 hello-halo）：
```
[snap_1_0] HEADING level=1 "Main Title"
  [snap_1_1] LINK "Home" href="/"
  [snap_1_2] TEXTBOX "Search" value=""
  [snap_1_3] BUTTON "Submit"
```

**Screenshot 压缩管线**（对标 hello-halo）：
1. CDP 默认 JPEG quality 80
2. 超过 1568px 则等比缩放到 <= 1568px
3. 输出始终 JPEG (减小 API 传输体积)

#### 3.3.2 导航工具（2 个）

| 工具 | CDP 域/方法 | 输入参数 |
|------|------------|---------|
| `browser_navigate` | Page.navigate | `url?, action?: back\|forward\|reload, device?: pc\|h5` |
| `browser_wait_for` | Page 事件监听 | `text: string, timeout?: number (30s)` |

#### 3.3.3 Tab 管理（1 个）

| 工具 | 功能 |
|------|------|
| `browser_tab` | `action: list\|select\|close, pageIdx?` |

#### 3.3.4 输入工具（5 个）

| 工具 | CDP 域/方法 | 说明 |
|------|------------|------|
| `browser_click` | Input.dispatchMouseEvent | 支持单/双击、元素坐标中心点击 |
| `browser_fill` | Input.insertText / DOM.setFileInputFiles | 文本输入 + select 选项 (batch) |
| `browser_hover` | Input.dispatchMouseEvent(mouseMoved) | 悬停元素 |
| `browser_drag` | Input.dispatchMouseEvent | 拖拽：源→10步动画→目标 |
| `browser_press_key` | Input.dispatchKeyEvent | 支持组合键 "Control+A", "Meta+K" |

**点击流程**（对标 hello-halo）：
1. `DOM.scrollIntoViewIfNeeded` → 滚动到元素可见
2. `DOM.getBoxModel` → 计算中心点 `(x + width/2, y + height/2)`
3. 双击：两次完整 press/release 周期，间隔 50ms
4. `Input.dispatchMouseEvent(mousePressed)` → `Input.dispatchMouseEvent(mouseReleased)`

**拖拽流程**：
1. 获取源/目标 bounding box
2. mouseMoved → 源位置
3. mousePressed → 10 步动画 (每步 16ms) → 目标位置
4. mouseReleased → 目标位置

#### 3.3.5 检查工具（1 个）

| 工具 | CDP 域/方法 | 子目标 |
|------|------------|--------|
| `browser_inspect` | Network + Runtime | `network: { id?, resourceTypes?, limit?, offset? }` |
|  |  | `console: { id?, types?: ['error','warning'], limit?, offset? }` |

**Network 跟踪** (CDP Network domain)：
```
Network.enable → 
  Network.requestWillBeSent → 捕获请求头/方法/URL
  Network.responseReceived → 捕获状态码/响应头/MIME
  Network.loadingFailed → 错误标记
```

**Console 跟踪** (CDP Runtime domain)：
```
Runtime.enable →
  Runtime.consoleAPICalled → 捕获 type/text/timestamp/stackTrace
```

#### 3.3.6 下载工具（1 个）

| 工具 | CDP 域/方法 |
|------|------------|
| `browser_download` | Browser.setDownloadBehavior + 文件系统监听 |

**下载流程**：
1. 设置下载目录 `Browser.setDownloadBehavior('allow', downloadDir)`
2. CDP `Browser.downloadProgress` 事件跟踪进度
3. 返回 `{ path, size, mimeType, state }`

#### 3.3.7 模拟工具（1 个）

| 工具 | CDP 域/方法 | 输入参数 |
|------|------------|---------|
| `browser_emulate` | Emulation.setDeviceMetricsOverride | `networkConditions?, cpuThrottlingRate?, geolocation?` |

**设备模拟预设**（对标 hello-halo）：
```
PC:  { width: 1280, height: 720,  deviceScaleFactor: 1, mobile: false }
H5:  { width: 430,  height: 932,  deviceScaleFactor: 3, mobile: true }
```

#### 3.3.8 脚本工具（1 个）

| 工具 | 说明 |
|------|------|
| `browser_run` | 从磁盘加载 .js 文件并通过 Runtime.evaluate 执行；路径限于 workspace 内 |

### 3.4 工具结果格式

对标 hello-halo 的统一返回格式：

```rust
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
}

pub enum ToolContent {
    Text { text: String },
    Image { data: String, mime_type: String },
}
```

### 3.5 Chromium 发现（`browser/discovery.rs`）

按平台自动搜索 Chromium 可执行文件：

```
优先级：
1. CHROME_PATH 环境变量
2. macOS: /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
           ~/Applications/Chromium.app/Contents/MacOS/Chromium
   Linux:  /usr/bin/chromium-browser, /usr/bin/google-chrome-stable
   Windows: %PROGRAMFILES%\Google\Chrome\Application\chrome.exe
3. PATH 中查找 (which chromium, which google-chrome)
4. 返回 None → 前端弹出安装提示对话框
```

**启动参数**：
```
--headless=new
--no-sandbox
--disable-gpu
--remote-debugging-port=0  (自动分配端口)
--disable-dev-shm-usage
--window-size=1280,720
```

### 3.6 浏览策略（`browser/policy.rs`）

对标 hello-halo 的三种策略模式：

```rust
pub enum BrowserPolicy {
    Unrestricted,           // 无限制
    Allowlist(Vec<String>), // 仅允许列表中的域名
    Blocklist(Vec<String>), // 阻止列表中的域名
}
```

- 每次导航前检查 URL 是否符合策略
- 被阻止时：不加载页面，返回 `blocked_by_policy: true` 给前端

---

## 4. IPC 设计

### 4.1 IPC 类型（`ipc.rs` 新增）

```rust
// 浏览器状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewStateResponse {
    pub id: String,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub zoom_level: f64,
    pub device_mode: String,  // "pc" | "h5"
    pub error: Option<String>,
    pub blocked_by_policy: bool,
}

// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolCallInput {
    pub tool_name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolResultResponse {
    pub content: Vec<ToolContentResponse>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolContentResponse {
    #[serde(rename = "type")]
    pub content_type: String,     // "text" | "image"
    pub text: Option<String>,
    pub data: Option<String>,     // base64 for image
    pub mime_type: Option<String>,
}

// 导航
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNavigateInput {
    pub view_id: String,
    pub url: String,
}

// Chromium 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromiumStatusResponse {
    pub found: bool,
    pub path: Option<String>,
    pub is_launched: bool,
    pub cdp_url: Option<String>,
}
```

### 4.2 IPC 命令（`tauri_commands.rs` 新增）

```rust
// 浏览器生命周期
#[tauri::command] async fn browser_get_chromium_status() -> Result<ChromiumStatusResponse, Error>
#[tauri::command] async fn browser_launch() -> Result<ChromiumStatusResponse, Error>
#[tauri::command] async fn browser_shutdown() -> Result<bool, Error>

// View 管理
#[tauri::command] async fn browser_create_view(url: Option<String>, device: Option<String>) -> Result<BrowserViewStateResponse, Error>
#[tauri::command] async fn browser_close_view(view_id: String) -> Result<bool, Error>
#[tauri::command] async fn browser_get_state(view_id: String) -> Result<BrowserViewStateResponse, Error>
#[tauri::command] async fn browser_get_all_states() -> Result<Vec<BrowserViewStateResponse>, Error>

// 导航
#[tauri::command] async fn browser_navigate(input: BrowserNavigateInput) -> Result<BrowserViewStateResponse, Error>
#[tauri::command] async fn browser_go_back(view_id: String) -> Result<bool, Error>
#[tauri::command] async fn browser_go_forward(view_id: String) -> Result<bool, Error>
#[tauri::command] async fn browser_reload(view_id: String) -> Result<bool, Error>

// 控制
#[tauri::command] async fn browser_set_zoom(view_id: String, level: f64) -> Result<bool, Error>
#[tauri::command] async fn browser_set_device(view_id: String, mode: String) -> Result<bool, Error>

// 工具调用
#[tauri::command] async fn browser_call_tool(input: BrowserToolCallInput) -> Result<BrowserToolResultResponse, Error>
```

### 4.3 Tauri 事件（Backend → Frontend）

```rust
// 状态变更推送
app.emit("browser:state-changed", BrowserViewStateResponse);

// 50ms debounce (对标 hello-halo)
```

---

## 5. 前端设计

### 5.1 browserStore（`stores/browser.svelte.ts`）

```typescript
// 状态
let _chromiumStatus = $state<ChromiumStatus | null>(null);
let _views = $state<BrowserViewState[]>([]);
let _activeViewId = $state<string | null>(null);
let _toolInProgress = $state(false);
let _toolResult = $state<BrowserToolResult | null>(null);

export const browserStore = {
  get chromiumStatus() { return _chromiumStatus; },
  get views() { return _views; },
  get activeViewId() { return _activeViewId; },
  get activeView() { return _views.find(v => v.id === _activeViewId) || null; },
  get toolInProgress() { return _toolInProgress; },
  get toolResult() { return _toolResult; },

  async init() {
    _chromiumStatus = await apiClient.browserGetChromiumStatus();
    if (!_chromiumStatus.found) { /* show install dialog */ }
    await this.listenStateChanges();
  },

  async launch() { ... },
  async createView(url?: string, device?: string) { ... },
  async navigate(viewId: string, url: string) { ... },
  // ...
}
```

### 5.2 BrowserViewer 组件（`BrowserViewer.svelte`）

```
┌──────────────────────────────────────┐
│ ← → ↻  https://example.com    [PC▼] │  控制栏
├──────────────────────────────────────┤
│                                      │
│        Tauri WebviewWindow            │  <webview> 标签
│        或 fallback <iframe>          │
│                                      │
├──────────────────────────────────────┤
│ 状态栏: ⬆️ 4 reqs | ⚠ 2 errors       │  底部状态
└──────────────────────────────────────┘
```

**功能清单**：
- 地址栏：URL 输入 + Enter 导航
- 导航按钮：后退/前进/刷新
- 设备切换：PC (1280×720) ↔ H5 (430×932)
- 缩放控制：+ / - / 重置
- 策略阻断覆盖层（blockedByPolicy 时显示）
- 加载状态指示器
- 错误状态显示
- Tauri WebviewWindow 嵌入（通过 `<webview>` 或 IPC 控制子窗口）

### 5.3 CanvasArea 集成

`CanvasArea.svelte` 中当 tab 路径以 `browser://` 开头或 language 为 `browser` 时，渲染 `BrowserViewer`：

```svelte
{#if activeTab.language === 'browser'}
  <BrowserViewer viewId={activeTab.browserViewId} />
{:else if canvasStore.isImageViewable(activeTab.path)}
  ...
```

### 5.4 ToolTaskCard 增强

当 AI 执行浏览器工具时，在聊天消息中嵌入：
- 截图预览（`browser_screenshot` 结果）
- "在浏览器中查看" 按钮（导航到对应 Canvas tab）
- 操作状态指示器（运行中 → 成功/失败）

---

## 6. 数据流

```
用户点击链接在浏览器中打开
  → browserStore.createView(url)
    → IPC: browser_create_view
      → BrowserService.create_view() → chromiumoxide.new_page() → Page.navigate(url)
        → emit("browser:state-changed", state)  (50ms debounce)
          → browserStore 更新 _views
            → CanvasArea 显示 BrowserViewer

AI 调用 browser_click 工具
  → IPC: browser_call_tool({ tool_name: "browser_click", args: { uid } })
    → tools::execute("browser_click", args)
      → BrowserService.get_active_page()
        → SnapshotProvider.get_element_bounds(uid)
          → Input.dispatchMouseEvent({ x, y })
            → ToolResult { content: [Text { text: "Clicked..." }] }
              → 返回给 AI agent
                → 触发 browser:screenshot 获取新截图
```

---

## 7. 错误处理

| 层级 | 策略 | 对标 hello-halo |
|------|------|-----------------|
| 工具层 | try-catch → `ToolResult { is_error: true }` | ✅ |
| CDP 层 | 默认 15s 超时 (`CDP_TIMEOUT`) | ✅ |
| 导航层 | `waitForNavigation(30s)` + 失败回调 | ✅ |
| 下载层 | `waitForDownload(60s)` + consumed 标记 | ✅ |
| IPC 层 | 标准 `Result<T, Error>` 模式 | ✅ |
| UI 层 | 错误状态直接显示在 BrowserViewer 中 | ✅ |

---

## 8. 部署与 Chromium 策略

**阶段 1（当前）**：自动发现 + 提示安装
- macOS: `mdfind "kMDItemCFBundleIdentifier == 'com.google.Chrome'"` 或已知路径
- Linux: `which chromium-browser`, `which google-chrome-stable`
- Windows: 注册表 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe`
- 发现失败 → 弹出系统对话框，提供 Chrome/Chromium 下载链接

**阶段 2（可选后续）**：Tauri sidecar 捆绑
- 构建时嵌入 Chromium (~130MB)
- 开箱即用，无需用户安装

---

## 9. 测试策略

| 测试 | 覆盖 |
|------|------|
| Chromium 发现 | 各平台路径搜索逻辑 |
| CDP 连接 | chromiumoxide launch/navigate/screenshot 端到端 |
| Snapshot | Accessibility.getFullAXTree 解析 + UID 一致性 |
| 工具执行 | 全部 15 个工具的函数级测试 |
| IPC | Tauri command 调用 → 响应验证 |
| 前端渲染 | BrowserViewer 状态变化、Canvas 集成 |
| 策略 | allowlist/blocklist URL 匹配 |

---

## 10. 自审检查清单

- [ ] 是否有 TBD/TODO 占位符？→ **无**
- [ ] 架构是否与 Phase 2 的 CanvasArea 集成一致？→ **是**，复用现有 tab 系统
- [ ] 工具集是否完整对标 hello-halo？→ **是**，15/15 工具全部覆盖
- [ ] 是否有模糊需求？→ **无**，CDP 域/方法/参数均已明确
- [ ] 是否聚焦于 AI 浏览器一个子系统？→ **是**，数字人/通知/搜索不在此范围
