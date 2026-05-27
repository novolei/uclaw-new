# BYOK 引导打磨 设计 (Sprint 4 ②)

**状态:** 设计已逐节批准,待 spec 评审 → writing-plans
**分支/worktree:** `codex/sprint4-byok-polish`(base = main `f9daea8b`)
**前置:** Sprint 4 ①(#560)修对了 ApiType 路由 —— BYOK 录入的 provider 现在能走对 wire format。

---

## 0. 背景:BYOK UI 大部分已建成

探查结论:`ChannelSettings.tsx`(连接 tab「服务商与用量」)已是完整 provider 管理面:列 25+ registry providers(分类)、API Key 密码输入、base_url、API 类型、实时读模型、测试连接、模型多选、保存/删除;密钥**永不回传前端**(`get_provider_config` 回 `has_api_key:bool`),`providers.json` 0600;`ProviderModelSelector`/`ModelSettings` 消费已配置模型。故本 slice **不是**建 BYOK 页,而是**打磨引导**,闭合两处真实 UX gap。

---

## 1. 目标与范围

**目标:** 闭合 BYOK 上手死路 + 已配置密钥的可见性,提升新用户上手率。**纯增量、行为保持。**

**范围内:**
- **空状态一键跳转**:`ProviderModelSelector`(composer popover)+ `ModelSettings`(intelligence tab)的「请先在服务商配置」死路文字 → 文字 + 主按钮 **「配置服务商 →」**(样式 A),点击打开 Settings 并选中 `connectivity` tab(可选滚到 providers section anchor)。
- **已配置密钥 masked 显示**:`get_provider_config` 回传新增 `masked_key: Option<String>`(服务端算,key 末 4 位,完整 key 永不回传);`ChannelSettings` 改用现成 `SettingsSecretInput`(show/hide)原件,`has_api_key` 时显示 `••••{masked_key}` 提示 + 「更新」清空重录。

**明确范围外:**
- OAuth 流(`openai-codex-oauth`「即将上线」)。
- 自定义 provider 录入(registry 外)。
- `enabled` 软禁用标记。
- 后端 provider 管理逻辑(configure/test/list 等已就绪,不动)。

---

## 2. 现状锚点(实现以此为准)

- Settings:`ui/src/components/settings/SettingsNav.tsx`(tab 列表,「核心」组含 `connectivity`→「服务商与用量」、`intelligence`→「智能」、`tools` 等);`SettingsPanel.tsx` `switch(tab)` 路由(`case 'connectivity' => <ConnectivityTab/>`)。`ConnectivityTab` 组合 `ChannelSettings` + `UsageSettings`(section anchor `data-settings-section`)。
- `ChannelSettings.tsx`(~444 行):左 provider 列表(按 serviceCategory 分组 + 绿点 + 模型数);右 `ProviderDetail` 表单:API Key(**裸 `<input type="password">`**,load 时 `setApiKey('')` + placeholder `'sk-…'`)、Base URL、API 类型 select;「读取模型」`listProviderModels`、「测试连接」`testProviderConnection`、模型多选、保存 `configureProviderWithModels` / 删除 `removeProviderConfig`。load 调 `getProviderConfig` 取 `has_api_key`(不回 key)。
- `get_provider_config`(`tauri_commands.rs:~5460`)→ `ProviderConfigResponse`(`ipc.rs:~629`,字段含 `has_api_key: bool`,**不含 key**)。bridge `getProviderConfig`(tauri-bridge.ts)。
- `ProviderModelSelector.tsx`(`ui/src/components/chat/`,AgentView:1800 用):读 `getAllConfiguredModels`,空态文字「请先在「服务商」配置 API Key 并读取模型」**无可点入口**。`ModelSettings.tsx`(IntelligenceTab):同样空态文字「请先在「服务商」页面配置…」。
- `SettingsSecretInput`(`ui/src/components/settings/primitives/SettingsSecretInput.tsx`):show/hide 切换 + error 显示,**现成未被 ChannelSettings 用**。
- Settings 打开机制:**待查**(plan 期)—— 可能有控制 settings 面板可见性 + active tab 的 atom / 路由;实现以真实机制为准(复用或加最小 nav atom)。
- `ProviderConfig.api_key: Option<String>`(plaintext in providers.json,0600);后端从不回传明文。

---

## 3. 空状态一键跳转(已批准 Section 1)

- `ProviderModelSelector` + `ModelSettings` 的空态:裸文字 → 文字 + 主按钮 **「配置服务商 →」**(蓝色 primary,样式 A)。
- 点击 → 打开 Settings + 选中 `connectivity` tab(+ 可选滚到 providers section anchor)。
- **导航机制**:plan 期定位现有 settings 打开机制(很可能是控制 settings dialog/panel 可见 + active-tab 的 jotai atom,`SettingsNav`/`SettingsPanel` 读它)。若有跨组件「打开 settings 到 tab X」action 则复用;否则加最小 `settingsNavRequestAtom { open: true, tab: 'connectivity', section?: 'providers' }`,settings host observe 它打开 + 选 tab + 滚动。纯前端。

---

## 4. 已配置密钥 masked 显示(已批准 Section 2)

- **后端(小)**:`ProviderConfigResponse` 加 `masked_key: Option<String>`(`#[serde]` camelCase → `maskedKey`)。`get_provider_config` 计算:有 key → `Some(key 末 4 位)`(如 `"3f9a"`);无 key → `None`。**完整 key 永不回传**(仅末 4)。
- **前端 `ChannelSettings`**:
  - 裸 `<input type="password">` → `SettingsSecretInput`(show/hide + error)。
  - `has_api_key` 且用户未输入新 key 时:显示 masked 提示 `••••{maskedKey}`(非编辑指示)+ 「更新」清空字段重录(沿用「key 不回传、重录以改」语义);输入新 key 后保存替换(同现状)。
- bridge `getProviderConfig` 的返回类型加 `maskedKey?: string`(TS)。

---

## 5. 测试 + 行为保持(已批准 Section 2)

- 后端单测:`get_provider_config` 返回 `masked_key = 末4位` 且**非**完整 key(配一个有 key 的 ProviderConfig,断言 response.masked_key == last4 且 != full key;无 key → None)。
- 前端 vitest:(a) `ChannelSettings` 在 `has_api_key` 时渲染 `••••{last4}` + 用 `SettingsSecretInput`;(b) `ProviderModelSelector`/`ModelSettings` 空态渲染「配置服务商 →」按钮且点击 dispatch 打开-settings/connectivity 导航(mock 该 action/atom,断言被调 + 参数 tab='connectivity')。
- 行为保持:`masked_key` 是新 optional 字段(`has_api_key` 既有消费者不变);`SettingsSecretInput` 替换是 UX-only;完整 key 仍不离后端;空态加按钮是纯增量。
- 验收:`cargo build` 干净 + 后端测试过;`cd ui && npx tsc --noEmit` 无新错误 + `npm test -- --run <相关>` 过。

---

## 6. 文件结构 + commit 序列(可二分)

| 文件 | 责任 |
|---|---|
| `src-tauri/src/ipc.rs`(或 ProviderConfigResponse 所在)+ `tauri_commands.rs` | `ProviderConfigResponse.masked_key` + `get_provider_config` 计算末4 + 后端测试 |
| `ui/src/lib/tauri-bridge.ts` | `getProviderConfig` 返回类型加 `maskedKey?` |
| `ui/src/components/chat/ProviderModelSelector.tsx` | 空态加「配置服务商 →」按钮 + nav dispatch |
| `ui/src/components/settings/ModelSettings.tsx` | 同上空态按钮 |
| `ui/src/components/settings/`(settings host / 一个 nav atom)| 「打开 settings 到 tab」机制(复用或新加最小 atom)|
| `ui/src/components/settings/ChannelSettings.tsx` | 改用 SettingsSecretInput + masked 显示 + 「更新」|

commit 序列:`后端 masked_key(+ 测试)` → `bridge 类型 + settings 导航机制(atom 或复用)` → `空态「配置服务商 →」按钮接 nav(ProviderModelSelector + ModelSettings)` → `ChannelSettings 改 SettingsSecretInput + masked 显示` → `前端测试收口`。
