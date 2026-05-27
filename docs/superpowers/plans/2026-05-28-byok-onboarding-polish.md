# BYOK 引导打磨 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 闭合 BYOK 上手死路(空态一键跳服务商设置)+ 已配置密钥的 masked 可见性,提升新用户上手率。纯增量、行为保持。

**Architecture:** 后端 `get_provider_config` 新增 `masked_key`(末 4 位,经可测纯函数 `mask_key`;完整 key 永不回传)。前端:两处空态加「配置服务商 →」按钮(用已有 `settingsOpenAtom`+`settingsTabAtom` 跳 connectivity tab);`ChannelSettings` 改用 `SettingsSecretInput` + 显示 `••••{maskedKey}`。

**Tech Stack:** Rust(rusqlite/serde),React/TS,jotai,vitest。

**Spec:** `docs/superpowers/specs/2026-05-28-byok-onboarding-polish-design.md`
**Branch/worktree:** `codex/sprint4-byok-polish`(base = main `f9daea8b`)

---

## ⚠️ 规划期事实(实现以此为准)

- **Settings 打开机制已存在**(无需新 atom):`@/atoms/settings-tab` 导出 `settingsOpenAtom`(bool)+ `settingsTabAtom`(`SettingsTab`,值含 `'connectivity'`)。外部开法(8+ 处已用,如 `ToolSelectorPopover.tsx:48-49,63-66`):`const setSettingsOpen = useSetAtom(settingsOpenAtom); const setSettingsTab = useSetAtom(settingsTabAtom); ...(); setSettingsOpen(true); setSettingsTab('connectivity');`。`SettingsDialog` 挂在 `AppShell.tsx:401`,读 `settingsOpenAtom`/`settingsTabAtom`。
- `ProviderConfigResponse`:**两处重复定义**——`ipc.rs:623-632` 与 `tauri_commands.rs:626`(后者被 `get_provider_config` 用)。`#[serde(rename_all="camelCase")]`,字段 `provider_id, display_name, has_api_key: bool, base_url: Option<String>, api: Option<String>`。
- `get_provider_config`(`tauri_commands.rs:5444-5458`):`state.provider_service.get_provider_config(&provider_id).await -> Option<ProviderConfig>`(`c`);现 `has_api_key: c.api_key.is_some_and(|k| !k.is_empty())`。`ProviderConfig.api_key: Option<String>`。
- 无法直接单测 `get_provider_config`(`State<AppState>` 测试 harness 造不出,tauri_commands.rs:17816 注)。⇒ 抽纯函数 `mask_key` 单测。service 测试范式 `providers/readiness_tests.rs`:`ProviderService::new(temp.path())` + `configure_provider_with_models(config(...), &[...])`;`config(id, api_key, base_url)` helper(:14-22)。
- `ChannelSettings.tsx` `ProviderDetail`(149-443):`const [apiKey,setApiKey]=useState('')`(:150);load effect(157-186)`setApiKey('')` + `getProviderConfig(provider.id)` 读 `cfg.baseUrl`/`cfg.api`(**未读 hasApiKey**);裸 `<input type="password">`(316-325,`value=apiKey onChange placeholder authType==='none'?'无需API Key':'sk-…' disabled=authType none`);save(240-263)`configureProviderWithModels({..., apiKey: apiKey||null, ...})`。
- `SettingsSecretInput`(`settings/primitives/SettingsSecretInput.tsx`):`forwardRef`,props `extends Omit<InputHTMLAttributes,'type'> { label?:string; error?:string }`,内置 show/hide(显示/隐藏),`...props` 透传 value/onChange/placeholder/disabled/autoComplete/spellCheck/className。
- 空态:`ProviderModelSelector.tsx:124-135`(`{!hasModels && <p>请先在「服务商」配置 API Key 并读取模型</p>}`;`hasModels=groups.some(g=>g.models.length>0)`,`getAllConfiguredModels` 喂);`ModelSettings.tsx:142-148`(`{!hasModels && <div>...请先在「服务商」页面配置...</div>}`)。`ProviderModelSelector` 在 **ChatInput.tsx:376 + AgentView.tsx:1800 都用**(单组件,改一处覆盖两 composer);ChatInput 无另外的 picker 空态。
- bridge `getProviderConfig`(`tauri-bridge.ts:659`)→ `Promise<ProviderConfigResponse|null>`;TS `ProviderConfigResponse`(`lib/types.ts:609-615`)`{providerId, displayName, hasApiKey, baseUrl?, api?}`。
- 测试:`vi.mock('@/lib/tauri-bridge', () => ({ <fns> }))` + `renderWithProviders(ui, { store })`(`test-utils/render`,`createStore()` 可预置 atom);nav 测试预置/断言 `settingsOpenAtom`/`settingsTabAtom`。`BrowserRuntimeSettings.test.tsx` / `SystemTab.test.tsx` 范式。

---

## 验证命令
- 后端:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)+ `cargo test --lib <filter>`。
- 前端:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish/ui && npx tsc --noEmit 2>&1 | head`(无新错误)+ `npm test -- --run <file> 2>&1 | tail -12`。
- 已知后端预存失败 ~7;stale tauri 产物报旧路径则清 `target/debug/build/tauri-*`+`uclaw-*`。

---

## File Structure
| 文件 | 责任 |
|---|---|
| `src-tauri/src/tauri_commands.rs` | `mask_key` 纯函数 + `get_provider_config` 填 `masked_key` + `ProviderConfigResponse.masked_key` + mask_key 单测 |
| `src-tauri/src/ipc.rs` | `ProviderConfigResponse.masked_key`(同步重复定义)|
| `ui/src/lib/types.ts` | `ProviderConfigResponse.maskedKey?` |
| `ui/src/components/chat/ProviderModelSelector.tsx` | 空态「配置服务商 →」按钮 + nav |
| `ui/src/components/settings/ModelSettings.tsx` | 空态「配置服务商 →」按钮 + nav |
| `ui/src/components/settings/ChannelSettings.tsx` | SettingsSecretInput 替换 + masked 显示 |

commit 序列(spec §6):后端 masked_key(+测试)→ 空态按钮 nav(ProviderModelSelector + ModelSettings,含 bridge/types 若需)→ ChannelSettings SettingsSecretInput + masked → 前端测试收口。

---

## Task 1: 后端 masked_key(纯函数 + get_provider_config)

**Files:** Modify `src-tauri/src/tauri_commands.rs`, `src-tauri/src/ipc.rs`

- [ ] **Step 1: 失败测试(mask_key 纯函数)** — `tauri_commands.rs`(`get_provider_config` 附近或文件测试模块)加:

```rust
/// 把 API key 脱敏成「末 4 位」(完整 key 永不回传前端)。空 → None 由调用方处理。
pub(crate) fn mask_key(key: &str) -> String {
    let tail = &key[key.len().saturating_sub(4)..];
    tail.to_string()
}

#[cfg(test)]
mod mask_key_tests {
    use super::mask_key;
    #[test]
    fn returns_last_four() {
        assert_eq!(mask_key("sk-ant-api03-abcd3f9a"), "3f9a");
    }
    #[test]
    fn short_key_returns_all() {
        assert_eq!(mask_key("xy"), "xy");   // < 4 chars(真实 key 不会这么短;边界安全)
    }
}
```

- [ ] **Step 2: 确认红** — `cargo test --lib mask_key_tests 2>&1 | grep -E "cannot find|error\[" | head`(`mask_key` 未定义)。

- [ ] **Step 3: ProviderConfigResponse 加字段(两处同步)** — `ipc.rs:623-632` 与 `tauri_commands.rs:626` 的 `ProviderConfigResponse` 各加:
```rust
    pub masked_key: Option<String>,
```
(serde camelCase → `maskedKey`。)

- [ ] **Step 4: get_provider_config 填 masked_key** — `tauri_commands.rs:5450-5457` 的 `.map(|c| ...)` 改:
```rust
    Ok(config.map(|c| {
        let api_key = c.api_key.filter(|k| !k.is_empty());
        ProviderConfigResponse {
            provider_id: c.provider_id,
            display_name: c.display_name,
            has_api_key: api_key.is_some(),
            masked_key: api_key.as_deref().map(mask_key),
            base_url: c.base_url,
            api: c.api.map(|a| format!("{:?}", a)),
        }
    }))
```
> `api_key` 先 `filter(非空)` → `has_api_key` 与 `masked_key` 同源;`as_deref().map(mask_key)` 取末4。完整 key 不进 response。

- [ ] **Step 5: 绿 + 编译** — `cargo test --lib mask_key_tests 2>&1 | tail -6`(2 passed);`cargo build 2>&1 | grep -E "^error" | head`(空)。

- [ ] **Step 6: Commit**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish add src-tauri/src/tauri_commands.rs src-tauri/src/ipc.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish commit -m "feat(providers): get_provider_config returns masked_key (last 4)

ProviderConfigResponse gains masked_key (Option<String>) = last 4 chars of the
API key via the pure mask_key helper; full key never leaves the backend.
+ mask_key unit tests.

Verification: cargo test --lib mask_key_tests -> 2 passed; build clean"
```

---

## Task 2: 空态「配置服务商 →」按钮(ProviderModelSelector + ModelSettings)

**Files:** Modify `ui/src/components/chat/ProviderModelSelector.tsx`, `ui/src/components/settings/ModelSettings.tsx`

- [ ] **Step 1: ProviderModelSelector 空态加按钮** — 顶部加 import(若无):
```tsx
import { useSetAtom } from 'jotai'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
```
组件内(其它 hooks 处)加:
```tsx
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)
  const goToProviderSettings = () => {
    setOpen(false)            // 关 popover(用组件已有的 setOpen;若名不同则对齐)
    setSettingsOpen(true)
    setSettingsTab('connectivity')
  }
```
把 `:124-135` 的 `{!hasModels && (<p>请先在「服务商」配置 API Key 并读取模型</p>)}` 改为:
```tsx
        {!hasModels && (
          <div className="mt-1 flex flex-col items-center gap-1">
            <p className="text-[10px] text-muted-foreground/60">还没有可用模型</p>
            <button
              type="button"
              onClick={goToProviderSettings}
              className="rounded-md bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90"
            >
              配置服务商 →
            </button>
          </div>
        )}
```
> `setOpen` 是 popover 开关;读组件确认其真实名(可能是 popover 的 `onOpenChange` state setter)。若该组件用受控 popover,关闭后再开 settings;若无 popover state,省略 `setOpen(false)`。

- [ ] **Step 2: ModelSettings 空态加按钮** — 同样 import + setter,把 `:142-148` 的空态 `<div>` 内的第二个 `<p>请先在「服务商」页面配置…</p>` 换成按钮:
```tsx
            <button
              type="button"
              onClick={() => { setSettingsOpen(true); setSettingsTab('connectivity') }}
              className="mt-0.5 rounded-md bg-primary px-2 py-1 text-[10.5px] font-medium text-primary-foreground hover:bg-primary/90"
            >
              配置服务商 →
            </button>
```
(ModelSettings 在 settings 内,设 tab 即切到 connectivity;`settingsOpenAtom` 已 true,设它无害。)

- [ ] **Step 3: 测试** — 新建 `ui/src/components/chat/ProviderModelSelector.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { createStore } from 'jotai'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { ProviderModelSelector } from './ProviderModelSelector'
import { getAllConfiguredModels } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  getAllConfiguredModels: vi.fn(),
  getActiveModel: vi.fn(async () => null),
  setActiveModel: vi.fn(),
  setRoleModel: vi.fn(),
}))

describe('ProviderModelSelector empty state', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('shows 配置服务商 button when no models, opens settings to connectivity', async () => {
    (getAllConfiguredModels as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([])
    const store = createStore()
    const { user } = renderWithProviders(<ProviderModelSelector />, { store })
    // 打开 popover(读组件:可能需点触发器;若默认渲染列表则直接断言)
    // 断言空态按钮出现 + 点击后 atoms 被设
    const btn = await screen.findByRole('button', { name: /配置服务商/ })
    await user.click(btn)
    await waitFor(() => {
      expect(store.get(settingsOpenAtom)).toBe(true)
      expect(store.get(settingsTabAtom)).toBe('connectivity')
    })
  })
})
```
> 读 `ProviderModelSelector` 真实渲染:它是 popover —— 空态可能要先点开触发器。实现者按真实结构调整(点开 trigger → 找按钮 → 点 → 断言 atoms)。mock `getAllConfiguredModels` 返回 `[]` 触发空态。若 popover 难驱动,退化为直接测 `goToProviderSettings` 逻辑(导出或抽小函数);优先真实交互。

- [ ] **Step 4: tsc + 测试 + 提交** — `npx tsc --noEmit 2>&1 | head`(无新错误);`npm test -- --run ProviderModelSelector 2>&1 | tail -10`(过)。
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish add ui/src/components/chat/ProviderModelSelector.tsx ui/src/components/settings/ModelSettings.tsx ui/src/components/chat/ProviderModelSelector.test.tsx
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish commit -m "feat(ui): empty-state 配置服务商 button -> opens Settings connectivity tab

ProviderModelSelector (used in BOTH composers) + ModelSettings empty states now
show a primary 配置服务商 button that opens Settings to the connectivity tab via
the existing settingsOpenAtom/settingsTabAtom. Closes the onboarding dead-end.

Verification: npx tsc --noEmit clean; npm test ProviderModelSelector -> pass"
```

---

## Task 3: ChannelSettings 改 SettingsSecretInput + masked 显示

**Files:** Modify `ui/src/components/settings/ChannelSettings.tsx`, `ui/src/lib/types.ts`

- [ ] **Step 1: bridge TS 类型加 maskedKey** — `lib/types.ts:609-615` `ProviderConfigResponse` 加:
```ts
  maskedKey?: string | null;
```

- [ ] **Step 2: ChannelSettings 读 hasApiKey + maskedKey** — `ProviderDetail` 加 state:
```tsx
  const [hasApiKey, setHasApiKey] = useState(false)
  const [maskedKey, setMaskedKey] = useState<string | null>(null)
```
load effect(157-186):`setApiKey('')` 后,在 `if (cfg) {...}` 内补:
```tsx
      setHasApiKey(cfg.hasApiKey)
      setMaskedKey(cfg.maskedKey ?? null)
```
且在 effect 顶部 reset 时加 `setHasApiKey(false); setMaskedKey(null)`(与 `setApiKey('')` 一起,切 provider 清空)。

- [ ] **Step 3: 裸 input → SettingsSecretInput + masked placeholder** — import:
```tsx
import { SettingsSecretInput } from './primitives/SettingsSecretInput'
```
把 `:316-325` 的 `<input type="password">` 换成:
```tsx
              <SettingsSecretInput
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                autoComplete="off"
                spellCheck={false}
                disabled={provider.authType === 'none' || provider.authType === 'None'}
                placeholder={
                  provider.authType === 'none' || provider.authType === 'None'
                    ? '无需 API Key'
                    : hasApiKey && !apiKey
                      ? `已配置 ••••${maskedKey ?? ''}（输入以更新）`
                      : 'sk-…'
                }
              />
```
> masked 经 placeholder 呈现(字段 load 时 `apiKey=''` → 显示「已配置 ••••3f9a（输入以更新）」;用户键入即覆盖 → 正常录入)。完整 key 不回传;`maskedKey` 仅末 4。SettingsSecretInput 自带 show/hide(对新输入)。className 若需匹配栅格,传 `className`(参 SettingsSecretInput 透传 `...props`)。

- [ ] **Step 4: 测试** — 新建/扩 `ui/src/components/settings/ChannelSettings.test.tsx`:mock tauri-bridge(`getProviderConfig` 返回 `{providerId,displayName,hasApiKey:true, maskedKey:'3f9a', baseUrl:null, api:null}`、`getConfiguredModels`→`[]`、`listProviders`/`listConfiguredProviders` 等空),渲染 → 选中一个 provider → 断言密钥字段 placeholder 含 `••••3f9a`。
```tsx
// 关键断言(按真实交互调整):选中 provider 后
const input = await screen.findByPlaceholderText(/••••3f9a/)
expect(input).toBeInTheDocument()
```
> ChannelSettings 是双栏(左列表选 provider → 右 detail)。实现者读真实结构:可能需先点左侧某 provider 才渲染 ProviderDetail。mock `listProviders` 至少返回一个 provider 让左列表有项;`getProviderConfig` 返回带 maskedKey。若交互复杂,可把 ProviderDetail 导出单测(直接传 provider prop)。优先真实交互,退化导出。

- [ ] **Step 5: tsc + 测试 + 提交** — `npx tsc --noEmit 2>&1 | head`(无新错误);`npm test -- --run ChannelSettings 2>&1 | tail -10`(过)。
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish add ui/src/components/settings/ChannelSettings.tsx ui/src/lib/types.ts ui/src/components/settings/ChannelSettings.test.tsx
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish commit -m "feat(ui): ChannelSettings uses SettingsSecretInput + masked existing-key hint

Swaps the raw password input for SettingsSecretInput (show/hide). When a key is
configured, the field shows '已配置 ••••{last4}（输入以更新）' (from get_provider_config's
maskedKey); typing a new key overrides. Full key never round-trips.

Verification: npx tsc --noEmit clean; npm test ChannelSettings -> pass"
```

---

## Task 4: 验收收口

- [ ] **Step 1: 全量验证** — 后端 `cargo build 2>&1 | grep -E "^error" | head`(空)+ `cargo test --lib mask_key 2>&1 | tail -5`(过)+ 全量 `cargo test --lib 2>&1 | tail -6`(仅 ~7 已知预存,零新增)。前端 `cd ui && npx tsc --noEmit 2>&1 | head`(无新错误)+ `npm test -- --run "ProviderModelSelector" "ChannelSettings" 2>&1 | tail -12`(过)。
- [ ] **Step 2: 手动 smoke 提示(写入 PR)** — 未配置任何 provider 时打开 composer 模型选择器 → 见「配置服务商 →」→ 点击跳到 Settings 服务商 tab;在服务商配置一个 key 保存 → 重开该 provider → 密钥字段显示「已配置 ••••{last4}」。
- [ ] **Step 3: Commit(若有收口改动)**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish add -A
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-byok-polish commit -m "test(byok): acceptance verification for onboarding polish

Verification: cargo test --lib -> no new failures; tsc clean; vitest -> pass"
```

---

## 最终验收
- [ ] 后端 `cargo build` 空 error;`mask_key` 单测过;全量仅 ~7 已知预存失败
- [ ] 前端 `npx tsc --noEmit` 无新错误;`ProviderModelSelector` + `ChannelSettings` 测试过
- [ ] 空态「配置服务商 →」打开 Settings connectivity tab(两 composer 经同一组件覆盖)
- [ ] 已配置 provider 密钥字段显示 `••••{last4}`;完整 key 不回传
- [ ] 行为保持:masked_key 新增 optional;SettingsSecretInput 替换 UX-only;空态加按钮纯增量

---

## Self-Review
**Spec coverage:** §3 空态 deep-link(已有 atoms)→ Task2;§4 masked_key 后端 + SettingsSecretInput 前端 → Task1(后端)+ Task3(前端);§5 测试 → Task1(mask_key)/Task2(nav)/Task3(masked 显示)/Task4;§6 commit 序列 → Task1-4 顺序一致(bridge type 在 Task3,因 ChannelSettings 才消费 maskedKey)。

**Placeholder scan:** Task2/Task3 的「读组件真实结构(popover 触发 / 双栏选 provider),交互难则退化导出小函数单测」是给实现者的明确对齐指令 + 退化方案,非 TBD。`setOpen` 名「读组件确认」是对齐指令。无 TODO/TBD;代码块完整。

**Type consistency:** `mask_key(&str)->String`(Task1)→ get_provider_config 调一致;`ProviderConfigResponse.masked_key: Option<String>`(Task1,ipc.rs + tauri_commands.rs 两处)↔ TS `maskedKey?: string|null`(Task3,camelCase)一致;`settingsOpenAtom`/`settingsTabAtom`(Task2)与 `@/atoms/settings-tab` 真实导出一致(bool / `'connectivity'`);`SettingsSecretInput` props(value/onChange/placeholder/disabled,Task3)与其 `Omit<InputHTMLAttributes,'type'>+label?/error?` 一致;`getProviderConfig` 返回带 `maskedKey`(Task3)↔ 后端 camelCase 自动映射。
