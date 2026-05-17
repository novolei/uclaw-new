# ulooi —— UCLAW Agent 的实体化 (M0 Umbrella Spec)

**日期：** 2026-05-17
**状态：** Draft (umbrella, pending user review)
**作者：** Ryan + Claude
**类型：** Program-level design（不是单一 PR 的实施 spec；含里程碑分解）

---

## 1. Vision & 北极星

> **ulooi 让 UCLAW Agent 有了真正的身体。** 用户在客厅听到 Looi 机器人开口说话、看到它转头、摸它头时感觉到反应 —— 同一个 Agent 在 macOS / Windows 桌面端、iOS、机器人三处"同时在场"，状态实时联动，UCLAW 桌面端关机时机器人仍能活。

**北极星指标（用户语言版）：**

1. **"它认得我家"** —— 机器人持续在场，长期记忆增量写入 UCLAW memory_graph。
2. **"三端同在"** —— 桌面端打字时机器人能"听到"；摸机器人时桌面端能"感觉到"；任何一端的状态另两端即时可见。
3. **"它有反射"** —— 摸它、叫它，反应不经过任何 Wi-Fi 往返。
4. **"它有大本营"** —— 复杂思考、记忆查询、工具调用走 UCLAW；UCLAW 离线时降级但不死。

**非目标 (Out of scope，至少 v1):**

- 不做泛机器人 SDK（不覆盖 Lovot / Bambot / Anki Vector / 其它 OEM）
- 不替 UCLAW 做远程访问账号系统（用配对 token，不做用户云账号）
- 不在 iOS 上跑 Python（memU bridge 不下沉，留在 UCLAW）
- 不做云端 LLM provider 同步到 iOS（UCLAW 仍是 provider key 的 source of truth）
- 不替换 UCLAW 桌面端 chat UI；ulooi 体验通过新增"embodied space"类型注入现有 UI

---

## 2. 架构选择：Reflex / Cortex 分离

**选定方案：B — iOS 持反射弧、UCLAW 持大脑皮层。**

```
┌───────────────────────────────────────────────────────────────────┐
│                        UCLAW (macOS / Windows)                    │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  CORTEX                                                      │ │
│  │  · Agent loop · LLM providers · MCP · memory_graph · Skills  │ │
│  │  · Proactive scenarios · Cost & safety                       │ │
│  └────────────────────────────┬─────────────────────────────────┘ │
│                               │                                   │
│  ┌────────────────────────────┴─────────────────────────────────┐ │
│  │  uclaw_remote_bridge   (新模块 — M2 引入)                    │ │
│  │  · WebSocket server (CBOR)                                   │ │
│  │  · Device pairing & token rotation                           │ │
│  │  · Event bus ↔ ulooi                                         │ │
│  └────────────────────────────┬─────────────────────────────────┘ │
└───────────────────────────────┼───────────────────────────────────┘
                                │  LAN (mDNS) or Tailscale (P2P)
                                │  CBOR over WSS
┌───────────────────────────────┼───────────────────────────────────┐
│                        ulooi (iOS / iPadOS)                       │
│                  ★ 感官 IO 在 iPhone ★                            │
│  ┌────────────────────────────┴─────────────────────────────────┐ │
│  │  SENSORY (iPhone 原生能力)                                   │ │
│  │  · iPhone 麦克风 → ASR (Apple Speech, on-device)             │ │
│  │  · iPhone 摄像头 → vision frames                             │ │
│  │  · iPhone 扬声器 ← TTS (AVSpeechSynthesizer / ElevenLabs)    │ │
│  └──────────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  REFLEX                                                      │ │
│  │  · Wake word · VAD · Idle animations · Touch reaction        │ │
│  │  · Apple Foundation Model (only when CORTEX unreachable)     │ │
│  │  · Local SQLite WAL queue (offline event store)              │ │
│  │  · Lipsync coordinator (TTS 播放 ↔ 机器人灯光/嘴动同步)      │ │
│  └────────────────────────────┬─────────────────────────────────┘ │
│  ┌────────────────────────────┴─────────────────────────────────┐ │
│  │  LooiKit  (Swift package — 动觉身体三域抽象)                 │ │
│  │  · MotionController · LightController · SensorStream         │ │
│  └────────────────────────────┬─────────────────────────────────┘ │
└───────────────────────────────┼───────────────────────────────────┘
                                │  BLE (commands + sensor telemetry)
                                │  ← 不传音频；音频在 iPhone 端就地处理
                          ┌─────┴─────┐
                          │   LOOI    │  (动觉身体：动、亮、触觉)
                          │  hardware │
                          └───────────┘

★ 物理放置：iPhone 与 Looi 的相对位置（docked / 旁置 / 用户手持）影响
  embodiment 体感；M1 spec 决定推荐放置 + 应对策略。
```

**为什么不是 A（瘦客户端）：** 兑现不了"机器人是活的实体"—— UCLAW 关机就死，触摸反应有 Wi-Fi 往返延迟，不像生物。

**为什么不是 C（完整 Rust 下沉到 iOS）：** Python 子进程在 iOS 沙盒不可用 → memU bridge 砍掉或重写；provider key 多端同步是另一个项目；范围会失控（半年起）。

**iOS 端技术栈（2026-05-17 补记）：** **Pure SwiftUI / Swift**，iOS 端不引入 Rust。Apple Foundation Model 只能从 Swift 调；AVFoundation / Speech / CoreBluetooth 全 Swift-native；reflex 层逻辑 < 2000 行 Swift Concurrency 完全胜任；避免 cargo + xcframework + uniffi 复杂度。协议一致性靠 **schema-first codegen**：单一 CDDL 文件（`ulooi/Schemas/wire-envelope-v1.cddl`）同时生成 Swift `Codable` 类型和 Rust serde 结构。详见 [`ulooi/docs/architecture.md`](https://github.com/novolei/ulooi/blob/main/docs/architecture.md) §2。

---

## 3. V1 范围：S3 (full embodied agent)

锁定为 v1 的功能集（按 M1→M8 滚动交付）：

| 能力 | 状态 | 由哪个里程碑兑现 |
|---|---|---|
| BLE 配对 + 控制机器人 motion/light | P0 | M1 |
| iOS ⟷ UCLAW 双向消息总线 | P0 | M2 |
| 语音对话：iPhone mic → ASR → UCLAW → TTS → iPhone speaker（+ Looi 灯光/嘴动 lipsync） | P0 | M3 |
| 三端 presence：touch/state 双向同步 | P0 | M4 |
| 离线降级：UCLAW 关机时 reflex 接管 | P0 | M5 |
| iOS 摄像头当机器人的眼睛（vision-LLM） | P1 | M6 |
| 多用户说话人识别 | P1 | M7 |
| 互动写入 memory_graph（机器人记得家庭成员、习惯） | P1 | M8 |

P0 = v1 必须；P1 = v1 后续滚动加。

---

## 4. 里程碑序列

每个里程碑独立可演示、可合入，沿用 uClaw 的 "one PR per plan, bisectable commits" 风格。

### M0 — Umbrella Spec（本文档）

**Definition of Done：** 用户 review 本文档并 approve；启动 M0.5。

### M0.5 — Hardware Reachability Prototype

**目标：** 1-2 天 throwaway Swift CLI / playground，能让真实 Looi 机器人挥手（motion）+ 读到触摸事件 + 灯光命令响应。验证两个参考 repo（[andrey-tut/LOOI-Robot](https://github.com/andrey-tut/LOOI-Robot), [splattydoesstuff/sooperchargeforbots](https://github.com/splattydoesstuff/sooperchargeforbots)）描述的 BLE 协议对当前固件仍有效。

**Definition of Done：** 一段 30 秒视频展示三件事：connect → 挥手（motion 命令）→ 读取触摸事件 → 灯光命令响应。命令字典/服务 UUID 记录在 M1 spec 输入。**音频/视频不在 prototype 范围**（这些是 iPhone 原生能力，不依赖 BLE）。

**产物：** 一个 throwaway 的 `tools/looi-probe/` 目录 + 一份"prototype findings"文档（写入 M1 spec 前言）。

### M1 — LooiKit Foundation + iOS Shell + Pairing UX

**目标：** 不依赖 UCLAW，能在 iOS app 里：连 Looi → 在 UI 里点按钮控制动作 → 看到传感器读数。

**范围：**
- `LooiKit` Swift package：`LooiDevice` + `MotionController` + `SensorStream`（`LightController` M3 再做；音频不在 LooiKit 范围）
- iOS app shell：替换当前 Xcode 模板，引入分屏导航 + 设备配对/状态/调试面板三块
- Bonjour 设备发现 stub（连接 UCLAW 部分 M2 实装）

**Definition of Done：** 一台 iPhone + 一台 Looi 完成端到端 demo（连接 → 控制 → 状态可视化），无 UCLAW 参与。LooiKit 单元测试覆盖 envelope encode/decode + 命令构造。

### M2 — UCLAW Transport Layer

**目标：** UCLAW 端开出对外可达的双向消息总线，iOS 能连进来收发消息。

**范围：**
- UCLAW 后端：新增 `uclaw_remote_bridge` 模块（独立 service，注册到 ServiceManager Stage 3）
  - 绑定 `0.0.0.0` (受 `MemubotConfig.remote_bridge.enabled` 控制，默认关闭)
  - 设备 pairing：QR code 一次性配对，交换 ed25519 长期 token；token 持久化到 `~/.uclaw/devices.db`
  - WebSocket server（rust `tokio-tungstenite`），CBOR 帧
- iOS：mDNS 扫描 `_uclaw._tcp` + Tailscale 探测；持有 token；建立 WS；发心跳
- Schema **V27**：`spaces.kind TEXT DEFAULT 'normal'` (枚举 'normal'|'embodied')
- Schema **V28**：新增 `uclaw_remote_devices` 表（device_id, public_key, paired_at, last_seen, name）

**Definition of Done：** iOS 与 UCLAW 完成配对；在 UCLAW UI 里能看到"已配对设备"列表；iOS 端能 ping UCLAW 拿到 `system.pong`。

### M3 — Voice Loop (S1 体验)

**目标：** 用户对着 iPhone（旁置于 Looi）说话 → Agent 回答 → 从 iPhone 扬声器出，**同步**驱动 Looi 灯光/嘴部呼吸/小幅头部摆动制造"机器人在说话"的视听耦合幻觉（lipsync illusion）。

**范围：**
- iOS：唤醒词（Apple Speech `SFSpeechRecognizer`）+ VAD + 流式 ASR
- iOS：流式 TTS（AVSpeechSynthesizer 默认；ElevenLabs 配置可选）
- iOS：**Lipsync coordinator** —— TTS playback 进度 → BLE `actuation.light_pulse` / `actuation.motion_micro` 节拍命令到 Looi
- UCLAW：`agent.text_in` / `agent.token` 事件类型注册到 dispatcher
- Schema **V29**：`agent_sessions.kind TEXT DEFAULT 'chat'`（枚举 'chat'|'ambient'|'focused'）
- UCLAW：proactive 注入"Looi 客厅" embodied space + ambient session（首次连接 ulooi 时）

**Definition of Done：** "hey Looi, what's on my calendar today" → 1.5s 内 iPhone 开口同时 Looi 灯光跟节奏脉动；UCLAW 桌面端 "Looi 客厅" ambient session 实时显示对话。

### M4 — Presence / 三端同在 (S2 体验)

**目标：** touch/motion 双向；UCLAW 桌面端打字时机器人轻摆头表示"在听"；机器人头被摸时桌面端右下角弹出 toast。

**范围：**
- `LooiKit.LightController` + 触摸传感事件流（M1 sensor stream 扩展）
- 事件：`embodiment.touch` / `embodiment.motion` / `agent.state`（thinking/tool_running/awaiting_approval）
- UCLAW 前端：在 chat input 旁加 "Looi 状态" indicator；onfocus → 推 `agent.state=user_typing`
- UCLAW 前端：toast 系统订阅 `embodiment.touch`
- proactive scenarios 可订阅 ulooi 事件作为触发条件

**Definition of Done：** 三向 demo 录屏：(1) 桌面打字 → 机器人摆头 (2) 摸机器人 → 桌面 toast (3) Agent 调工具 → 机器人灯光呼吸。

### M5 — Reflex Layer (绿/黄/红 降级)

**目标：** UCLAW 关机时机器人仍能对话（降级），UCLAW 回来时所有暂存自动 sync。

**范围：**
- iOS：状态机（绿/黄/红）+ 5s ping/pong + BLE RSSI 监测
- iOS：Apple Foundation Model 接入（iOS 18.2+）做基础对话
- iOS：本地 SQLite WAL 暂存所有未送达事件（含语音转写、touch 事件、Agent 应回未回的请求）
- UCLAW：恢复连接时收到"我离线期间发生了这些"的批量摘要事件
- iOS UI：状态 badge + 黄/红状态下的诚实提示文案

**Definition of Done：** 关闭 UCLAW 桌面端 → 机器人继续可对话（限基础话题）→ 重开 UCLAW → 暂存事件批量回放 → Agent 用一句话承认"刚才你跟我提了 X，我现在能查了"。

### M6 — Vision

**目标：** "Looi 你看那个是什么"—— iPhone 摄像头当机器人的眼睛（iPhone 物理位置决定视野，旁置/docked 时正对用户）。

**范围：**
- iOS：触发条件下激活 iPhone 摄像头（用户语音 + 显式手势），抓帧
- 事件：`vision.frame` + `vision.intent`
- UCLAW：vision-capable provider 路由（已有 Claude / GPT-4o vision 接入）
- 隐私：摄像头激活时 Looi 灯光显式提示（红 → 绿过渡）+ iPhone UI 同步显示"Looi 正在看"

**Definition of Done：** 桌上放一个物品 → 用户问 Looi 那是什么 → Agent 回答正确并提到具体细节。

### M7 — Speaker Identification

**目标：** 机器人能区分家里不同成员；memory_graph 里区分"妈妈说过 X" vs "我说过 X"。

**范围：**
- iOS：on-device speaker embedding（Apple `SoundAnalysis` 或开源 ECAPA-TDNN coreml 模型）
- 用户首次声纹注册流程（UI 引导每位家庭成员说 10 秒）
- 事件：`voice.speaker_id` 附在 `voice.final` 上

**Definition of Done：** 两个家庭成员各跟机器人对话一次；memory_graph 里查询"妈妈最近说了什么"返回正确条目。

### M8 — Memory Write-back

**目标：** 机器人观察到的事实/偏好/事件写入 UCLAW memory_graph，形成长期记忆。

**范围：**
- proactive scenarios 新增 `embodiment_observation`：ambient session 累积一定信息后批量提取
- LLM-based observation extraction（参考已有 conversation_learning scenario）
- 写入路径走现有 memory_graph_* Tauri commands，但加 `source='ulooi'` 标记
- UCLAW UI：memory graph 浏览器加 "embodied source" 过滤

**Definition of Done：** 持续使用 ulooi 一周后，memory_graph 中关于"家庭成员偏好/日常作息/客厅环境"的条目数 ≥ 20，且 Agent 回答时会主动引用。

---

## 5. 跨切设计

### 5.1 网络拓扑：LAN-first + Tailscale 兜底

- **LAN 路径：** UCLAW 启动时 advertise `_uclaw._tcp`（mDNS）；iOS 用 `NWBrowser` 扫描。
- **Tailscale 路径：** 检测到 iOS 不在 UCLAW 的 LAN 网段 → 查询 Tailscale CLI（`tailscale ip -4`）拿 UCLAW 的 tailnet IP → 直连。
- **配对：** 一次性 QR code，桌面端 UI 显示，iOS 相机扫。QR 内容 = `{host, port, fingerprint, one_time_token}`。配对完成后交换长期 ed25519 device token（90 天到期，重连时滚动续期）。
- **TLS：** UCLAW 自签证书，iOS 配对时存指纹 pin。

### 5.2 Wire 协议：CBOR over WebSocket

```cbor
{
  v: 1,                          // wire major version
  id: "01HQA7XX...",            // ULID, 也作请求-响应 correlation
  ts: 1715900000123,            // sender wall clock (ms)
  src: "ulooi-iphone-r9k",      // device id (UUID v4 第一次启动生成)
  kind: "embodiment.touch",      // 命名空间事件类型
  reply_to: "01HQA7WW...",      // optional, 关联前一帧
  payload: <kind-specific bytes>
}
```

**事件命名空间：**

| 命名空间 | 用途 | 方向 |
|---|---|---|
| `system.*` | ping/pong, version, capabilities | 双向 |
| `pairing.*` | 配对握手、token 续期 | 双向 |
| `voice.*` | `partial`/`final`/`speaker_id` | iOS → UCLAW |
| `agent.*` | `state`/`token`/`tool_event`/`approval_request` | UCLAW → iOS |
| `embodiment.*` | `touch`/`motion`/`battery`/`rssi` | iOS → UCLAW |
| `actuation.*` | `motion_cmd`/`light_cmd`（**无** tts_chunk —— TTS 在 iOS 端就地播放） | UCLAW → iOS |
| `tts.*` | `text_chunk`（UCLAW Agent 输出，iOS 端 TTS 渲染）/ `playback_progress`（iOS → UCLAW 反馈，用于驱动桌面端"说话中"指示） | 双向 |
| `vision.*` | `frame`/`intent` | iOS → UCLAW |
| `memory.*` | `observation`/`recall_request` | 双向 |
| `sync.*` | offline queue 批量回放 | iOS → UCLAW |

完整 payload schemas 在 M2 spec 锁定（M0 这里只锁命名空间约定）。

### 5.3 会话模型：Space + Ambient + Focused

- 新增 `space.kind = 'embodied'`：每个配对的 Looi 设备对应一个 embodied space。
- 每个 embodied space 持有：
  - **1 个 ambient session** (`agent_sessions.kind = 'ambient'`)：永不关闭，承载所有偶发对话与 memory observation 触发；UI 里展示"最近活动 N 分钟前"
  - **N 个 focused sessions** (`agent_sessions.kind = 'focused'`)：显式开/关，用户/Agent 说"新话题/记一下这个项目"时创建，达成 DoD 后归档
- Schema migrations：
  - V27 `spaces.kind TEXT DEFAULT 'normal'`
  - V28 `uclaw_remote_devices` 表
  - V29 `agent_sessions.kind TEXT DEFAULT 'chat'`

### 5.4 降级模型：绿 / 黄 / 红

| 状态 | 条件 | 表现 |
|---|---|---|
| 绿 | BLE ✓ + UCLAW WS ✓ | 满血 S3 |
| 黄 | BLE ✓ + UCLAW WS ✗ | reflex 接管，本地暂存所有事件，恢复后批量 sync |
| 红 | BLE ✗ | iOS 作为普通 UCLAW chat 客户端；机器人 affordance 全部 disabled |

- 状态判定：5s ping/pong + BLE RSSI（< -85 dBm 持续 10s 视为弱连接，<-95 视为断）
- 状态切换是 first-class 事件 `system.state_changed`
- 所有改 UCLAW 持久状态的事件带 envelope `id`，UCLAW 侧 dedup（防 sync 重复写）

### 5.5 音频 / 视觉路径：iPhone 是感官 IO

**关键决定（用户明确）：** 音频和视觉全部走 iPhone 原生能力，不经过 BLE。Looi 是动觉身体（motion / light / touch），不充当音视频管道。

```
[用户说话]
   ↓ iPhone 麦克风
[iOS: Apple Speech on-device ASR streaming]
   ↓ voice.partial → UCLAW（边说边送）
   ↓ voice.final (endpoint detected)
[UCLAW Agent: LLM streaming, TTFT ~400ms]
   ↓ tts.text_chunk (流式文本) → iOS
[iOS: 流式 TTS]
   ├─ AVSpeechSynthesizer (on-device, 即出)
   └─ 高质量模式: ElevenLabs streaming (cloud)
   ↓ iPhone 扬声器开口
   ↘ 并行：iOS Lipsync Coordinator → BLE actuation.light_pulse / motion_micro
[Looi 灯光/小幅头动跟随 TTS 节奏 → 视听耦合幻觉]

端到端目标延迟：< 1.5s (用户停说话 → iPhone 出声 + Looi 反应)
```

**Lipsync coordinator 是 embodiment 核心：** iPhone 的 TTS 进度（已播放 ms / 总时长）→ 节拍化为 BLE actuation 命令。这是让"声音 + 身体反应"看起来一体的关键 —— 即使音源在 iPhone 而非 Looi，节奏一致就有"机器人在说话"的错觉。

**物理放置（M1 决定）：** 用户体验取决于 iPhone 与 Looi 的相对位置。三种放置场景：
1. **Docked** —— iPhone 物理嵌在 Looi 上当"脸" (Embodied Moxie 模式)
2. **旁置** —— iPhone 立在 Looi 旁的桌面
3. **手持** —— 用户拿着 iPhone 走动，Looi 固定

每种场景下 lipsync 的"距离瑕疵"不同；M1 spec 推荐默认场景 + 文档化其它场景的体感差异。

**好处（相对原方案）：** 不再依赖 BLE audio 带宽，音视频质量上限 = iPhone 原生（远高于玩具机器人的扬声器/麦克风）。M0.5 prototype 不需要测 BLE 音频。

### 5.6 LooiKit 通用框架（三域，不含音频）

```swift
public protocol LooiDevice {
    var motion: any MotionController { get }   // 头/轮/臂 命令 + 反馈
    var light: any LightController { get }     // RGB / patterns / lipsync pulses
    var sensor: any SensorStream { get }       // touch / motion / battery / RSSI
    var battery: LooiBatteryState { get }
}
```

- **音频不在 LooiKit 范围：** iPhone 原生 AVFoundation / Speech 处理所有音频；如果 Looi 硬件本身有 native beep/chirp（开关机提示音之类），归到 `LightController` 同级的 `NativeSoundEffects` —— 但这是 M1 prototype 后视情况补的小模块，不是 v1 P0。
- **"通用"的定义：** LOOI 这一类玩具机器人的可扩展 Swift 抽象；命令字典随固件版本可扩展；**不**覆盖 Lovot / Bambot / Anki Vector / 其它 OEM。
- **演化路径：** 长期可独立为 GitHub repo；v1 留在 `ulooi/Packages/LooiKit/` 内部。
- **范围节奏：** M1 只实装 `MotionController` + `SensorStream`；`LightController` M3（lipsync 需要）。

---

## 6. UCLAW 后端改动汇总

| 模块 | 变更 | 里程碑 |
|---|---|---|
| `services/` | 新增 `RemoteBridgeService`（注册到 ServiceManager Stage 3） | M2 |
| `db/migrations.rs` | V27 (spaces.kind) | M2 |
| `db/migrations.rs` | V28 (uclaw_remote_devices) | M2 |
| `db/migrations.rs` | V29 (agent_sessions.kind) | M3 |
| `memubot_config.rs` | 新增 `RemoteBridgeConfig`（默认关） | M2 |
| `tauri_commands.rs` + `main.rs` invoke_handler | `pair_device` / `unpair_device` / `list_paired_devices` | M2 |
| `agent/dispatcher.rs` | 注册新事件 sources（embodiment.*, voice.*） | M3 |
| `proactive/scenarios/` | 新增 `embodiment_observation` scenario | M8 |
| UI: `ui/src/views/` | 新增 "Embodied Space" 类型渲染 + Device pairing settings | M2 + M3 |

注意：每次新增 Tauri command 都要同时改 `tauri_commands.rs` AND `main.rs` 的 `invoke_handler!` 宏。每次新增 service 都要注册到 `[Stage 3]`。每次新增 migration 必须更新 CLAUDE.md 的 Active migration registry。

---

## 7. 风险 & 开放问题

### 高风险（M0.5 必须降级）

1. **两个参考 repo 的 BLE 协议对当前 Looi 固件是否仍有效？** prototype 验证；如有偏差，命令字典需重新逆向。
2. **Lipsync illusion 的体感门槛：** TTS 进度 → BLE 命令的网络延迟（BLE 命令往返 ~30-50ms）是否会让人感知不同步？M3 实测；如果不行，降级方案是"非节拍化"的常态灯光呼吸 + Agent 状态切换大动作（思考时一种灯光、说话时另一种），不追求毫秒级口型对齐。

### 中风险（M2-M5 期间观察）

3. **iOS 后台 BLE + audio session 保持的可靠性。** Apple 对 BLE 后台运行限制严格；同时持有 `AVAudioSession`（TTS 播放）+ `CBCentralManager`（BLE）+ `SFSpeechRecognizer`（ASR）三者要协调 background mode；M3 设计时需完整调研。
4. **Tailscale 检测可靠性。** 不是所有用户都装 Tailscale；自动检测 + 优雅 fallback 提示 UX。
5. **memory_graph 写入频率。** ambient session 高频写入可能造成噪音；M8 设计 observation extraction 时要批量化 + 阈值控制。
6. **iPhone 物理放置的产品形态。** docked / 旁置 / 手持三种放置体感差很多；M1 spec 必须先定推荐场景再设计 UI（横屏 docked 模式跟竖屏旁置模式的 SwiftUI 布局是两套）。

### 开放问题（M0 不决，留给后续里程碑 spec）

- 多设备：一户多个 iPhone 都装 ulooi、配同一台 Looi —— BLE 排他性如何协调？（M2 涉及，留给 M2 spec）
- iPhone 物理放置的推荐场景：docked / 旁置 / 手持？（M1 spec 必须先定，影响 UI 形态）
- ulooi iPad 体验：iPad 旁置变成"机器人的脸屏"？（M1 spec 决定）
- Tailscale 之外的远程访问：用户没 Tailscale 怎么办？（M2 后置 backlog item，不是 v1 阻断）
- Apple Foundation Model 的能力上限：reflex 层能 hold 住到什么对话深度？（M5 spec 实测）
- 配对 token 90 天到期的滚动续期 UX：用户感知如何最小化？（M2 spec）

---

## 8. 附录

### 8.1 参考资料

- [andrey-tut/LOOI-Robot](https://github.com/andrey-tut/LOOI-Robot) — BLE 协议逆向
- [splattydoesstuff/sooperchargeforbots](https://github.com/splattydoesstuff/sooperchargeforbots) — Looi mod 工具
- UCLAW `local_api/` 与 `api/` 模块（现有 HTTP 层，作为 transport 参考）
- UCLAW 已有 spaces / agent_sessions / memory_graph 抽象（被 ulooi 复用 + 扩展）
- [`novolei/ulooi`](https://github.com/novolei/ulooi) — iOS 实现 repo（含 [PRD](https://github.com/novolei/ulooi/blob/main/docs/prd.md) 和 [总体框架设计](https://github.com/novolei/ulooi/blob/main/docs/architecture.md)）

### 8.2 术语

- **CORTEX** —— 在 UCLAW 桌面端运行的 Agent 推理/记忆/工具调用层
- **REFLEX** —— 在 iOS ulooi 端运行的本地响应/降级层
- **embodied space** —— `spaces.kind = 'embodied'` 的 space，对应一台配对的 Looi 设备
- **ambient session** —— `agent_sessions.kind = 'ambient'` 的永久会话，承载日常碎语和 memory observation 来源
- **focused session** —— `agent_sessions.kind = 'focused'`，显式开启的话题聚焦会话

### 8.3 与现有 uClaw 设计文档的关系

- 复用：spaces (V17/V19) / agent_sessions (V18) / memory_graph / proactive / Tauri command 模式
- 扩展：spaces.kind / agent_sessions.kind / new RemoteBridgeService / new uclaw_remote_devices
- 不动：LLM provider 层 / MCP / Skills / cost_records / safety
- 与 IM framework spec (`2026-05-17-im-framework-design.md`) 的关系：独立。IM framework 是 hello-halo 移植；ulooi 是 embodied agent。两者长期可能共享 RemoteBridgeService 的传输层。

---

## 下一步

1. **用户 review** 本文档。
2. Approve 后 → 启动 **M0.5 hardware reachability prototype**（throwaway, 1-2 天）。
3. Prototype findings 文档化 → 进入 **M1 detailed brainstorm**（新 spec 文件 `2026-05-XX-ulooi-m1-foundation-design.md`）。
4. M1 spec → `writing-plans` → `subagent-driven-development`。
