# 子项目 B — 知识摄入管线设计

**Date:** 2026-05-20
**Status:** Approved (ready for writing-plans)
**Program:** Agent Memory OS v2 — 第二大脑(子项目序列 A → E → C → **B** → D)
**Depends on:** A(gbrain 浏览器 + `gbrain_*` Tauri 命令,已合并)、Path 2(gbrain CLI→MCP-JSON 适配器,已合并 PR #303,E2E 通过 → `put_page`/`get_page` 现可用)
**North-star:** `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`;总纲 `docs/superpowers/specs/2026-05-20-agent-memory-os-v2-second-brain-design.md`(§3 子项目表、§6 B 的开放问题)

---

## 1. 目标

让用户"喂资料"长大 gbrain 知识层:拖放文件(md/txt/PDF/URL/音视频)→ 后台**静默**解析 → 分块抽实体 → agent 静默写入 gbrain 实体页 → 只出一个轻提示。**事后**用户在双星云(子项目 C)看新页、编辑、回滚(靠 gbrain 版本史,Path 2 已验 `revert` 可用)。

对应用户 Goal 5("用户喂资料来长知识库")。

## 2. 已敲定的决策(brainstorming)

| # | 决策 | 选择 |
|---|---|---|
| 1 | v1 源格式范围 | **全量**:md/txt/PDF/URL + 音频/视频(北极星愿景) |
| 2 | 抽取策略 | **抽实体成多页 + 智能合并**(LLM 抽实体/概念 → 每个一个 gbrain 实体页;已存在则读出合并更新,不盲覆) |
| 3 | 审核门控 | **全静默 + 事后双星云审**(只出轻提示;版本史兜底;不强制逐页审核) |
| 4 | 抽取成本 | **专用廉价 utility 模型 + 每文档块数上限**(经现有 role 路由加 `ingestion` role;chunk 预算 + `MAX_CHUNKS_PER_DOC`) |
| 5 ⚙️ | 音频解码库 | **symphonia**(纯 Rust,覆盖 mp3/wav/flac/m4a + mp4/mov 容器音轨;免外部二进制) |
| 6 ⚙️ | job 状态持久化 | **只放内存**(不落库;重启即清——符合"静默 + 事后看 gbrain"语义;无新 migration) |
| 7 ⚙️ | 前端入口 | **扩展现有 `QuickCaptureDialog`**(加"喂资料"模式),不另开入口 |

## 3. 架构(方案 1:专用后台 IngestionService + 轻量 job 模型)

新建 `src-tauri/src/ingestion/` 模块。一个 `IngestionService`(实现现有 `ManagedService` trait,见 `src-tauri/src/services/manager.rs:14`)持有 job 注册表。`ingest_*` Tauri 命令提交源 → 后台 tokio 任务跑管线 → 进度经 Tauri `app_handle.emit("ingestion:progress", …)` 推前端(沿用 `stt_download_model` 的 `stt:openflow-download-progress` 模式,见 `stt/commands.rs:181`;`InfraService` 总线已有 `MultimodalIngested`/`MemoryExtracted` 事件类型,见 `infra/types.rs:37`,可选地一并发布)。

**否掉的备选**:复用 proactive ScenarioManager(proactive 是后台自发触发,摄入是用户触发 + 文件输入 + 逐 job 进度,语义不匹配);前端编排(大文件 + 后台续跑 + STT/解码都在后端,前端编排长任务脆弱)。

```
QuickCaptureDialog(喂资料模式)
  → invoke ingest_files / ingest_url
  → IngestionService::submit → 建 IngestionJob(Queued)→ spawn tokio task
      → sources::detect + extract_text(parser / STT)        [emit Parsing]
      → chunk::split(token 预算, MAX_CHUNKS 上限, 超限截断)
      → 逐块 extract::entities(utility LLM)→ 按 slug 累积    [emit Extracting done/total]
      → 逐实体 merge::write:get_page 命中→LLM 合并→put_page;未命中→put_page 新建  [emit Writing]
      → job Done/Partial/Failed                              [emit 终态 + 轻提示]
  事后:用户在双星云(C)看/改/回滚
```

## 4. 模块拆分(按域拆,不堆 god file)

| 文件 | 职责 | 依赖 |
|---|---|---|
| `ingestion/mod.rs` | `IngestionService`(ManagedService)+ job 注册表(`Mutex<HashMap<JobId, IngestionJob>>`)+ `submit(source) -> JobId` / `status(id)` / `list()`;持 `Arc<ProviderService>` + `SharedMcpManager` + `Option<AppHandle>` | services、providers、mcp |
| `ingestion/job.rs` | `IngestionJob { id: JobId, source_label: String, status: IngestionStatus, progress: Progress{stage,done,total}, pages_written: Vec<String>, error: Option<String> }`;`IngestionStatus { Queued, Parsing, Extracting, Writing, Done, Partial, Failed }` | — |
| `ingestion/sources/mod.rs` | `SourceKind` 探测(扩展名 / URL scheme)+ `extract_text(source) -> Result<ExtractedDoc, IngestError>` 分派;`ExtractedDoc { text, source_label }` | 下列各 source |
| `ingestion/sources/text.rs` | md/txt:读文件为 String | std fs |
| `ingestion/sources/pdf.rs` | PDF → 文本层(`pdf-extract`)。无文本层(扫描件)→ 空文本 → 上层标 Partial + 提示(OCR 留 fast-follow) | pdf-extract |
| `ingestion/sources/url.rs` | `reqwest` GET → `scraper` 取正文(article/main/body,剥 nav/script)→ 文本 | reqwest、scraper(已有) |
| `ingestion/sources/media.rs` | 音视频文件 → `symphonia` 解容器/解码 → PCM f32 + sample_rate → `stt` 引擎 `transcribe`(引擎内部重采样到 16k,见 `stt/openflow/engine.rs:133`)→ 文本 | symphonia、stt |
| `ingestion/chunk.rs` | 文本按 `CHUNK_TOKEN_BUDGET` 切块(粗略 token≈字符/4 估算,段落边界优先),`MAX_CHUNKS_PER_DOC` 上限,超限截断 + 返回 `truncated: bool` | — |
| `ingestion/extract.rs` | 每块:构 prompt → utility LLM `complete` → 解析实体 JSON → 跨块按 slug 累积合并;畸形 JSON → 跳过该块标 Partial | providers/llm |
| `ingestion/merge.rs` | 每实体:`browse::get_page(slug)` 命中 → LLM 合并 prompt → `browse::put_page`;未命中 → `put_page` 新建。slug 规范化 helper | gbrain::browse |

**注册**(uClaw adjacency,见 CLAUDE.md):`IngestionService` 注册进 `main.rs` 的 `[Stage 3]`;`ingest_*` 命令在 `tauri_commands.rs` 定义**并**列入 `main.rs` 的 `invoke_handler!`。

## 5. 抽实体 + 智能合并细节

**抽取 prompt**(utility 模型,`extract.rs`):输入一个文本块,输出 JSON 数组
`[{ "slug": "concept/…", "type": "concept|person|org|…", "title": "…", "compiled_truth_md": "…", "links": ["slug", …] }]`。
- slug = **域前缀 kebab**,沿用 gbrain 现有约定(实跑 brain 见过 `people/ryanliu`、`personal/ryanliu-edu`、`ai-models/gpt-5`、`concept/…`)。
- `compiled_truth_md` = 该实体的可编译综述 markdown(可含 `## 小节`、`[[wikilink]]`)。

**跨块累积**:同一 slug 在多块出现 → 先把各块的 compiled_truth 收集,统一做一次合并(或增量合并),避免重复 `put_page`。

**写入(`merge.rs`)**:
- `browse::get_page(slug)` 命中现有页 → LLM 合并 prompt("把 NEW 信息整合进 EXISTING 页:保留结构、不丢已有事实、补充/更新而非替换")→ `put_page(slug, merged_md)`(gbrain 自动留版本)。
- 未命中(`page_not_found`)→ `put_page(slug, new_md)` 新建。
- `put_page` 即 `browse::put_page(&mcp, slug, content)`(见 `gbrain/browse.rs:373`,content 为整段 markdown,title/frontmatter 内嵌)。

## 6. 成本控制

- **模型**:加 role `ingestion` 进现有 `role_models`(机制见 `providers/service.rs:220` `set_role_model`);解析时先查 `ingestion` role → 未配则回退 active 模型。Settings UI 暴露该 role 的模型选择(默认建议廉价/快速模型如 Haiku/小模型)。
- **预算常量**:`CHUNK_TOKEN_BUDGET`(~2–3k token/块)、`MAX_CHUNKS_PER_DOC`(默认 20)。文档超上限 → 只处理前 N 块,job 终态提示"文档过长,只处理了前 N 块"。
- **并发**:逐块**串行**(并发=1)控成本/限速;逐 job 串行或小并发(由 IngestionService 决定,v1 串行)。

**后端 LLM 调用路径**(`extract.rs`/`merge.rs`,沿用 `proactive/service.rs:2516` 模式):
`provider_service.get_active_llm_config()`(或 role 解析)→ `llm::llm_config_from_provider(...)` → `llm::create_provider(&cfg)` → `provider.complete(messages, vec![], &CompletionConfig{...})`。`IngestionService` 持 `Arc<ProviderService>`。

## 7. 前端落点(扩展 QuickCaptureDialog)

`ui/src/components/memory/QuickCaptureDialog.tsx` 加"喂资料 / Feed"模式:
- 拖放区(用 Tauri `getPathForFile` 取原生路径,沿用 `AgentView.tsx:806` 的 drop 取路径手法)+ URL 输入框 + 文件选择按钮。
- 提交 → `invoke('ingest_files', { paths })` / `invoke('ingest_url', { url })`。
- 进度:提交即出 sonner 轻提示;订阅 `ingestion:progress` 事件,完成时提示"从 «source» 新增/更新 N 页";一个小 jobs popover 列活跃/最近 job(状态 + 页数,数据来自 `ingest_list_jobs`)。
- 事后:用户去双星云(C)/ WikiView(A)看新页、编辑、回滚。

新前端 IPC 封装放 `ui/src/lib/`(或现有 gbrain/memory bridge 旁),类型与 `IngestionJob` 对齐。

## 8. 错误处理

- **不支持格式** → `submit` 即拒(不建 job,返回明确错误给前端 toast)。
- **分阶段失败**记到 job:解析/解码/STT 失败 → `Failed` + 原因;某块 LLM 失败 → 跳过续跑、最终 `Partial`;某实体 `put_page` 失败 → 记录、续写其余、`Partial`;全部实体成功 → `Done`。
- PDF 无文本层 / URL 抓不到正文 → 空文本 → `Partial` + 提示。
- **不静默吞**:所有错误进 job.error,`ingest_job_status` 可查;坏输入不崩溃。

## 9. 测试

**Rust 单测**(LLM + mcp 用 mock/trait 注入):
- `sources`:格式探测(扩展名/URL)。
- `chunk`:token 预算切块 + `MAX_CHUNKS` 上限 + 超限 `truncated=true`。
- `extract`:fixture LLM 输出(合法 JSON 实体数组)→ 正确实体;畸形 JSON → 跳过且不 panic;跨块同 slug 累积。
- `merge`:`get_page` 命中 → 走合并分支;`page_not_found` → 走新建分支(用 mock mcp 断言调用)。
- slug 规范化(域前缀、kebab、防碰撞)。
- `media`:真解一个 tiny fixture wav → 非空 PCM f32 + 正确 sample_rate(STT 本身可 mock 或复用现有测试)。

**前端 vitest**:喂资料模式渲染、拖放/URL 提交触发 `invoke`、`ingestion:progress` 事件更新 jobs 列表。

**手动 E2E**(写进验证):`cargo tauri dev` → QuickCaptureDialog 喂资料:拖一个 PDF / 贴一个 URL / 拖一个 mp3 → 见轻提示 → 双星云(C)/ WikiView(A)出现新实体页、内容合理、可编辑、可回滚。

## 10. 范围 / 依赖 / 风险

**范围内**:md/txt/PDF/URL/音视频摄入 → gbrain 实体页(抽实体 + 智能合并)。
**范围外**:图片 OCR / vision(fast-follow);memory_nodes(认知层)与跨层桥(C/D);D 自学习闭环;扫描件 OCR。

**新依赖(2)**:`pdf-extract`(PDF 文本层)、`symphonia`(纯 Rust 音视频解码)。在 CLAUDE.md 的依赖说明 + PR body 标注。无新 migration。

**风险**:
- **symphonia 视频容器覆盖**:mp4/mov 取音轨依赖 isomp4 demux + AAC 解码;若某些容器/编码不支持 → 该文件 `Failed` + 原因,后续可加 ffmpeg 兜底(留风险位,不在 v1 范围)。
- **抽取质量/slug 碰撞**:廉价模型可能产出不一致 slug 或过细/过粗实体 → 全静默写入靠版本史 + 双星云事后审兜底;slug 规范化 helper 收敛。
- **大文档成本**:`MAX_CHUNKS_PER_DOC` 截断兜底;后续可加"重要资料才深抽"的智能门控(总纲列为 B 可选增强,不在 v1)。
- **job 内存态**:重启丢失活跃 job 进度(已写入 gbrain 的页不丢);符合语义,接受。

## 11. 提交形状(预期单 PR,bisectable)

按 writing-plans 细化,大致:`ingestion` 骨架(service + job + 注册)→ sources(text/pdf/url/media + 探测)→ chunk → extract(+ utility role)→ merge → Tauri 命令 → 前端 QuickCaptureDialog 喂资料模式 → 测试。每步带单测,最后手动 E2E。
