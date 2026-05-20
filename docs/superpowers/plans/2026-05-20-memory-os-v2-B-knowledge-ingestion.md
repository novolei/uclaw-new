# 知识摄入管线(子项目 B)实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新建 `src-tauri/src/ingestion/` 模块,让用户拖放文件(md/txt/PDF/URL/音视频)→ 后台静默解析 → LLM 抽实体 → 智能合并 `put_page` 写入 gbrain 知识页,并在 `QuickCaptureDialog` 加"喂资料"入口。

**Architecture:** `IngestionService`(存于 AppState)持有内存 job 注册表;`ingest_*` Tauri 命令提交源 + 拿 `AppHandle` → `submit` 建 job 并 `tokio::spawn` 跑管线(探测→抽文本→切块→逐块 LLM 抽实体→按 slug 累积→逐实体 get_page→合并或新建→put_page),全程 `app.emit("ingestion:progress", job)`。纯逻辑(切块/JSON 解析/合并判定/slug 规范化/格式探测/媒体解码)做单测;LLM/mcp 编排靠手动 E2E。

**Tech Stack:** Rust(serde/serde_json、tokio、thiserror、uuid 已有;新增 `pdf-extract`、`symphonia`)、reqwest+scraper(已有)、复用 `stt/`、`gbrain::browse`、`providers::ProviderService`、`llm`。前端 React + Jotai。

**关键事实(实跑核对的真实签名,直接用):**
- `ManagedService` trait 存在但本服务**不实现**(无 start/stop 后台循环——job 按 submit 即 spawn;YAGNI)。`IngestionService` 直接存 AppState 供命令访问。**这是对 spec §3/§4「impl ManagedService」的有意偏离**,理由:无生命周期工作,注册成 ManagedService 是空仪式。
- LLM 一次性调用(`proactive/service.rs:2516` 模式):`provider_service.get_active_llm_config().await -> Option<(provider_id, model, api_key, base_url)>` → `crate::llm::llm_config_from_provider(&pid,&model,&key,&url, max_tokens, temp)` → `crate::llm::create_provider(&cfg) -> Result<Arc<dyn LlmProvider>, Error>` → `provider.complete(messages: Vec<ChatMessage>, vec![], &CompletionConfig{model,max_tokens,temperature,thinking_enabled}).await -> Result<RespondOutput, Error>`;取文本 `match out { RespondOutput::Text{text,..}=>text, RespondOutput::ToolCalls{text,..}=>text.unwrap_or_default() }`。
- `ChatMessage::user(&str)` / `ChatMessage::system(&str)` 构造器存在(`agent/types.rs`)。`CompletionConfig{ model:String, max_tokens:u32, temperature:f32, thinking_enabled:bool }`。
- gbrain 写/读:`crate::gbrain::browse::put_page(mcp: &SharedMcpManager, slug:&str, content:&str) -> Result<PageDetail, GbrainError>`;`get_page(mcp, slug) -> Result<PageDetail, GbrainError>`。**页不存在** → `Err(GbrainError::CallFailed(s))` 且 `s` 含 `"page_not_found"`(无独立 variant)。`PageDetail{ slug, title, page_type(#[serde(rename="type")]), compiled_truth, frontmatter, created_at, updated_at, tags, raw_markdown }`。`GbrainError{ NotConnected, CallFailed(String), ParseFailed(String) }`。
- STT 引擎是 `stt/commands.rs` 的模块级 `Lazy<Mutex<Option<Arc<OpenFlowAsrEngine>>>>`,经私有 `ensure_openflow_engine() -> Result<Arc<OpenFlowAsrEngine>, String>` 懒加载。`engine.transcribe(audio: Vec<f32>, sample_rate: u32, language: Option<&str>) -> Result<TranscribeResult, String>`;`TranscribeResult{ text, language, elapsed_seconds }`。需新增 public 包装(Task 3)。
- AppState 字段:`mcp_manager: SharedMcpManager`(=`Arc<RwLock<McpManager>>`)、`provider_service: Arc<ProviderService>`、`service_manager: Arc<ServiceManager>`、`infra_service: Arc<InfraService>`。
- Tauri 命令可直接收 `app: tauri::AppHandle` 参数(见 `stt_transcribe`);`app.emit("event-name", payload)`(payload 是 `Serialize` 结构体或 `serde_json::json!`)。命令经 `state: State<'_, AppState>` 访问服务。命令需在 `tauri_commands.rs` 定义**且**列入 `main.rs` 的 `invoke_handler!`(gbrain_* 命令区,~983)。
- 前端:`QuickCaptureDialog`(`ui/src/components/memory/QuickCaptureDialog.tsx`)无 props,由 `quickCaptureOpenAtom` 控制开关,内部 `mode: 'fragment'|'entity_page'` 用两个 `<button>` 切换。文件 drop 取原生路径用 `getPathForFile(file)`(`@tauri-apps/api`,见 `AgentView.tsx:814`)。

**验证命令(IRON RULE:重定向到文件再 grep):**
- `cd src-tauri && cargo test --lib ingestion > /tmp/ing.txt 2>&1; grep "test result" /tmp/ing.txt`
- `cargo build > /tmp/b.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b.txt | head`
- 前端:`cd ui && npx tsc --noEmit 2>&1 | head; npm test -- --run QuickCapture 2>&1 | tail -15`

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/Cargo.toml` (改) | 加 `pdf-extract`、`symphonia` |
| `src-tauri/src/lib.rs` 或模块根 (改) | `pub mod ingestion;` |
| `src-tauri/src/ingestion/mod.rs` (新) | 模块导出 + `IngestionService`(job 注册表 + submit/status/list + 管线编排) |
| `src-tauri/src/ingestion/job.rs` (新) | `JobId`/`IngestionStatus`/`Progress`/`IngestionJob`/`IngestError`/`IngestionSource` |
| `src-tauri/src/ingestion/sources/mod.rs` (新) | `SourceKind` 探测 + `extract_text` 分派 + `ExtractedDoc` |
| `src-tauri/src/ingestion/sources/text.rs` (新) | md/txt 读取 |
| `src-tauri/src/ingestion/sources/url.rs` (新) | reqwest+scraper 取正文 |
| `src-tauri/src/ingestion/sources/pdf.rs` (新) | pdf-extract 取文本层 |
| `src-tauri/src/ingestion/sources/media.rs` (新) | symphonia 解码 → PCM → STT |
| `src-tauri/src/ingestion/chunk.rs` (新) | token 预算切块 + 上限截断 |
| `src-tauri/src/ingestion/extract.rs` (新) | 实体 JSON 解析 + LLM 抽取 + 跨块累积 + slug 规范化 |
| `src-tauri/src/ingestion/merge.rs` (新) | 合并判定 + 写入(get_page→合并/新建→put_page) |
| `src-tauri/src/stt/commands.rs` (改) | 加 `pub async fn transcribe_samples` |
| `src-tauri/src/stt/mod.rs` (改) | `pub use commands::transcribe_samples;` |
| `src-tauri/src/providers/service.rs` (改) | 加 `get_ingestion_llm_config`(role "ingestion" → active 回退) |
| `src-tauri/src/tauri_commands.rs` (改) | `ingest_files`/`ingest_url`/`ingest_job_status`/`ingest_list_jobs` |
| `src-tauri/src/app.rs` (改) | AppState 加 `ingestion: Arc<IngestionService>` + 构造 |
| `src-tauri/src/main.rs` (改) | `invoke_handler!` 加 4 命令 |
| `ui/src/lib/ingestion.ts` (新) | IPC 封装 + `IngestionJob` 类型 |
| `ui/src/components/memory/QuickCaptureDialog.tsx` (改) | 加 "feed" 模式 + 拖放/URL + 进度订阅 |

---

## Task 1: Cargo 依赖 + 模块骨架 + job 类型

**Files:** Modify `src-tauri/Cargo.toml`; Create `src-tauri/src/ingestion/mod.rs`, `src-tauri/src/ingestion/job.rs`; Modify the crate module root (`src-tauri/src/lib.rs`).

- [ ] **Step 1: 加依赖到 `src-tauri/Cargo.toml`**(在 `[dependencies]` 区,任意位置)

```toml
pdf-extract = "0.7"
symphonia = { version = "0.5", features = ["mp3", "aac", "isomp4", "pcm", "wav"] }
```

- [ ] **Step 2: 注册模块**。在 crate 模块根(`src-tauri/src/lib.rs`,与已有 `pub mod gbrain;` 同区)加:

```rust
pub mod ingestion;
```

- [ ] **Step 3: 写 `src-tauri/src/ingestion/job.rs`**

```rust
//! 摄入 job 的内存态类型。job 不落库(重启即清);已写入 gbrain 的页不丢。

use serde::{Deserialize, Serialize};

pub type JobId = String;

/// 待摄入的源。一个源 = 一个 job。
#[derive(Debug, Clone)]
pub enum IngestionSource {
    File(String),
    Url(String),
}

impl IngestionSource {
    pub fn label(&self) -> String {
        match self {
            IngestionSource::File(p) => p.rsplit('/').next().unwrap_or(p).to_string(),
            IngestionSource::Url(u) => u.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestionStatus {
    Queued,
    Parsing,
    Extracting,
    Writing,
    Done,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Progress {
    pub stage: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionJob {
    pub id: JobId,
    pub source_label: String,
    pub status: IngestionStatus,
    pub progress: Progress,
    pub pages_written: Vec<String>,
    pub error: Option<String>,
}

impl IngestionJob {
    pub fn new(id: JobId, source_label: String) -> Self {
        Self {
            id,
            source_label,
            status: IngestionStatus::Queued,
            progress: Progress::default(),
            pages_written: Vec::new(),
            error: None,
        }
    }
}

/// 管线各阶段错误。不静默吞——全部记入 job.error。
#[derive(Debug, Clone, thiserror::Error)]
pub enum IngestError {
    #[error("unsupported source: {0}")]
    Unsupported(String),
    #[error("parse failed: {0}")]
    Parse(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("stt failed: {0}")]
    Stt(String),
    #[error("llm failed: {0}")]
    Llm(String),
    #[error("gbrain failed: {0}")]
    Gbrain(String),
    #[error("io failed: {0}")]
    Io(String),
}
```

- [ ] **Step 4: 写 `src-tauri/src/ingestion/mod.rs` 骨架**(仅模块导出 + 占位,服务体在 Task 7)

```rust
//! 知识摄入管线(子项目 B):拖放文件 → 后台静默解析 → LLM 抽实体 →
//! 智能合并 put_page 写入 gbrain。job 内存态,事后用户在双星云(C)看/改/回滚。

pub mod job;
pub mod sources;
pub mod chunk;
pub mod extract;
pub mod merge;

pub use job::{IngestError, IngestionJob, IngestionSource, IngestionStatus, JobId, Progress};
```

> 注:`sources`/`chunk`/`extract`/`merge` 文件在后续 Task 创建;本 Task 先建 `sources/mod.rs` 等空壳避免编译失败 —— 见 Step 5。

- [ ] **Step 5: 建空壳避免 mod 缺失编译错误**。创建以下文件,各写一行注释占位(后续 Task 填充):
  - `src-tauri/src/ingestion/sources/mod.rs` → `//! 源探测 + 文本抽取(Task 2 填充)`
  - `src-tauri/src/ingestion/chunk.rs` → `//! token 预算切块(Task 4 填充)`
  - `src-tauri/src/ingestion/extract.rs` → `//! 实体抽取(Task 5 填充)`
  - `src-tauri/src/ingestion/merge.rs` → `//! 合并写入(Task 6 填充)`

  同时把 `mod.rs` 里 `pub mod sources; pub mod chunk; pub mod extract; pub mod merge;` 暂时改成只 `pub mod job;`,其余在对应 Task 解注释。**或**直接保留空壳文件(空模块合法)。选后者:保留空壳文件 + mod.rs 全 mod 声明。

- [ ] **Step 6: job 单测**(加到 `job.rs` 末尾)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_new_is_queued() {
        let j = IngestionJob::new("abc".into(), "x.pdf".into());
        assert_eq!(j.status, IngestionStatus::Queued);
        assert!(j.pages_written.is_empty());
        assert!(j.error.is_none());
    }

    #[test]
    fn source_label_strips_path() {
        assert_eq!(IngestionSource::File("/a/b/c.pdf".into()).label(), "c.pdf");
        assert_eq!(IngestionSource::Url("https://x.com/p".into()).label(), "https://x.com/p");
    }

    #[test]
    fn status_serializes_snake_case() {
        let s = serde_json::to_string(&IngestionStatus::Partial).unwrap();
        assert_eq!(s, "\"partial\"");
    }
}
```

- [ ] **Step 7: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/b1.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b1.txt | head`(EXIT=0;新依赖首次编译会慢)
  - `cargo test --lib ingestion::job > /tmp/t1.txt 2>&1; grep "test result" /tmp/t1.txt`(3 passed)

- [ ] **Step 8: 提交**
```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/ingestion/
git commit -m "feat(ingestion): module scaffold + job types + pdf-extract/symphonia deps"
```

---

## Task 2: sources — 格式探测 + text/url/pdf 抽取

**Files:** Modify `src-tauri/src/ingestion/sources/mod.rs`; Create `src-tauri/src/ingestion/sources/{text,url,pdf}.rs`.

- [ ] **Step 1: 写 `sources/mod.rs`**

```rust
//! 源探测 + 文本抽取分派。

pub mod text;
pub mod url;
pub mod pdf;

use crate::ingestion::job::{IngestError, IngestionSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Text,
    Pdf,
    Url,
    Media,
    Unsupported,
}

/// 抽出的纯文本 + 给用户看的来源标签。
#[derive(Debug, Clone)]
pub struct ExtractedDoc {
    pub text: String,
    pub source_label: String,
}

/// 按扩展名 / URL 探测源类型。
pub fn detect(source: &IngestionSource) -> SourceKind {
    match source {
        IngestionSource::Url(_) => SourceKind::Url,
        IngestionSource::File(path) => {
            let ext = path
                .rsplit('.')
                .next()
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            match ext.as_str() {
                "md" | "markdown" | "txt" | "text" => SourceKind::Text,
                "pdf" => SourceKind::Pdf,
                "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "mp4" | "mov" | "webm" => {
                    SourceKind::Media
                }
                _ => SourceKind::Unsupported,
            }
        }
    }
}

/// 抽文本入口。Media 在 Task 3 接入 —— 这里先对 Media 返回 Unsupported 占位,Task 3 改。
pub async fn extract_text(source: &IngestionSource) -> Result<ExtractedDoc, IngestError> {
    let label = source.label();
    match (detect(source), source) {
        (SourceKind::Text, IngestionSource::File(p)) => {
            Ok(ExtractedDoc { text: text::read_text_file(p)?, source_label: label })
        }
        (SourceKind::Pdf, IngestionSource::File(p)) => {
            Ok(ExtractedDoc { text: pdf::extract_pdf(p)?, source_label: label })
        }
        (SourceKind::Url, IngestionSource::Url(u)) => {
            Ok(ExtractedDoc { text: url::fetch_readable(u).await?, source_label: label })
        }
        (SourceKind::Media, _) => Err(IngestError::Unsupported("media (wired in Task 3)".into())),
        _ => Err(IngestError::Unsupported(label)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::job::IngestionSource;

    #[test]
    fn detect_by_extension() {
        assert_eq!(detect(&IngestionSource::File("a/b.md".into())), SourceKind::Text);
        assert_eq!(detect(&IngestionSource::File("a/b.PDF".into())), SourceKind::Pdf);
        assert_eq!(detect(&IngestionSource::File("a/b.mp3".into())), SourceKind::Media);
        assert_eq!(detect(&IngestionSource::File("a/b.mp4".into())), SourceKind::Media);
        assert_eq!(detect(&IngestionSource::File("a/b.xyz".into())), SourceKind::Unsupported);
        assert_eq!(detect(&IngestionSource::Url("https://x".into())), SourceKind::Url);
    }
}
```

- [ ] **Step 2: 写 `sources/text.rs`**

```rust
//! md/txt 直读。

use crate::ingestion::job::IngestError;

pub fn read_text_file(path: &str) -> Result<String, IngestError> {
    std::fs::read_to_string(path).map_err(|e| IngestError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_temp_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "# Hello\n刘磊").unwrap();
        let txt = read_text_file(f.path().to_str().unwrap()).unwrap();
        assert!(txt.contains("Hello"));
        assert!(txt.contains("刘磊"));
    }
}
```

> 注:`tempfile` 是否在 dev-deps?若不在,本测试改为写到 `std::env::temp_dir()` 下一个随机名文件再删,或加 `tempfile = "3"` 到 `[dev-dependencies]`。**实现时先 `grep tempfile src-tauri/Cargo.toml`**;缺则加 dev-dep。

- [ ] **Step 3: 写 `sources/pdf.rs`**

```rust
//! PDF 文本层抽取。扫描件无文本层 → 返回空串(上层标 Partial)。

use crate::ingestion::job::IngestError;

pub fn extract_pdf(path: &str) -> Result<String, IngestError> {
    let bytes = std::fs::read(path).map_err(|e| IngestError::Io(e.to_string()))?;
    pdf_extract::extract_text_from_mem(&bytes).map_err(|e| IngestError::Parse(e.to_string()))
}
```

- [ ] **Step 4: 写 `sources/url.rs`**

```rust
//! URL 抓取 + 正文抽取(reqwest + scraper)。

use crate::ingestion::job::IngestError;

pub async fn fetch_readable(url: &str) -> Result<String, IngestError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| IngestError::Io(e.to_string()))?;
    let html = resp.text().await.map_err(|e| IngestError::Io(e.to_string()))?;
    Ok(extract_main_text(&html))
}

/// 取 article/main/body 的可读文本,剥 script/style/nav。
fn extract_main_text(html: &str) -> String {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    // 优先 article/main,退回 body
    for sel in ["article", "main", "body"] {
        if let Ok(s) = Selector::parse(sel) {
            if let Some(el) = doc.select(&s).next() {
                let text: String = el
                    .text()
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.trim().is_empty() {
                    return text;
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_text() {
        let html = "<html><body><nav>menu</nav><article><h1>Title</h1><p>正文段落</p></article></body></html>";
        let txt = extract_main_text(html);
        assert!(txt.contains("Title"));
        assert!(txt.contains("正文段落"));
    }
}
```

- [ ] **Step 5: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/b2.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b2.txt | head`(EXIT=0)
  - `cargo test --lib ingestion::sources > /tmp/t2.txt 2>&1; grep "test result" /tmp/t2.txt`(detect + text + url 测试通过)

- [ ] **Step 6: 提交**
```bash
git add src-tauri/src/ingestion/sources/ src-tauri/Cargo.toml
git commit -m "feat(ingestion): source detection + text/pdf/url text extraction"
```

---

## Task 3: sources/media — symphonia 解码 + STT 桥接

**Files:** Modify `src-tauri/src/stt/commands.rs`, `src-tauri/src/stt/mod.rs`; Create `src-tauri/src/ingestion/sources/media.rs`; Modify `src-tauri/src/ingestion/sources/mod.rs`.

- [ ] **Step 1: 在 `stt/commands.rs` 加 public 包装**(复用已有私有 `ensure_openflow_engine`)

```rust
/// 后台管线用:直接喂 PCM f32 → 文本(摄入子项目 B 复用)。
pub async fn transcribe_samples(
    audio: Vec<f32>,
    sample_rate: u32,
    language: Option<&str>,
) -> Result<crate::stt::TranscribeResult, String> {
    let engine = ensure_openflow_engine().await?;
    engine.transcribe(audio, sample_rate, language).await
}
```

- [ ] **Step 2: 在 `stt/mod.rs` re-export**(与已有导出同区)

```rust
pub use commands::transcribe_samples;
```

> 确认 `stt/mod.rs` 有 `pub mod commands;` 或 `mod commands;`。若是私有 `mod commands;`,`pub use commands::transcribe_samples;` 仍能把符号提到 `crate::stt::transcribe_samples`(re-export 合法)。

- [ ] **Step 3: 写 `sources/media.rs`**(symphonia 解码 → mono f32 + sample_rate → STT)

```rust
//! 音视频文件 → symphonia 解码为 mono PCM f32 → STT → 文本。

use crate::ingestion::job::IngestError;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// 解码任意支持的容器/编码为 (mono f32 samples, sample_rate)。
pub fn decode_to_pcm(path: &str) -> Result<(Vec<f32>, u32), IngestError> {
    let file = std::fs::File::open(path).map_err(|e| IngestError::Io(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.rsplit('.').next() {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| IngestError::Decode(format!("probe: {e}")))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| IngestError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(16_000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| IngestError::Decode(format!("make decoder: {e}")))?;

    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break, // EOF 或读尽
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_mono_f32(&decoded, &mut samples),
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // 跳过坏帧
            Err(e) => return Err(IngestError::Decode(e.to_string())),
        }
    }
    if samples.is_empty() {
        return Err(IngestError::Decode("decoded 0 samples".into()));
    }
    Ok((samples, sample_rate))
}

/// 把一个解码缓冲(可能多声道)平均成 mono f32 追加进 out。
fn append_mono_f32(decoded: &AudioBufferRef, out: &mut Vec<f32>) {
    match decoded {
        AudioBufferRef::F32(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr]),
        AudioBufferRef::S16(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr] as f32 / 32768.0),
        AudioBufferRef::S32(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| buf.chan(ch)[fr] as f32 / 2147483648.0),
        AudioBufferRef::U8(buf) => mix_planar(buf.spec().channels.count(), buf.frames(), out, |ch, fr| (buf.chan(ch)[fr] as f32 - 128.0) / 128.0),
        _ => { /* 其余少见格式:忽略该缓冲(不致命) */ }
    }
}

fn mix_planar<F: Fn(usize, usize) -> f32>(channels: usize, frames: usize, out: &mut Vec<f32>, get: F) {
    if channels == 0 { return; }
    for fr in 0..frames {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            acc += get(ch, fr);
        }
        out.push(acc / channels as f32);
    }
}

/// 文件 → 文本(解码 + STT)。
pub async fn transcribe_media(path: &str) -> Result<String, IngestError> {
    let (pcm, sample_rate) = decode_to_pcm(path)?;
    let res = crate::stt::transcribe_samples(pcm, sample_rate, None)
        .await
        .map_err(IngestError::Stt)?;
    Ok(res.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 生成一个最小 16-bit PCM mono WAV(8000Hz, 几帧)写到临时文件,返回路径。
    fn write_tiny_wav() -> tempfile::NamedTempFile {
        let sample_rate: u32 = 8000;
        let samples: [i16; 4] = [0, 16000, -16000, 8000];
        let data_len = (samples.len() * 2) as u32;
        let mut f = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let w = f.as_file_mut();
        w.write_all(b"RIFF").unwrap();
        w.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        w.write_all(b"WAVE").unwrap();
        w.write_all(b"fmt ").unwrap();
        w.write_all(&16u32.to_le_bytes()).unwrap();          // fmt chunk size
        w.write_all(&1u16.to_le_bytes()).unwrap();            // PCM
        w.write_all(&1u16.to_le_bytes()).unwrap();            // mono
        w.write_all(&sample_rate.to_le_bytes()).unwrap();
        w.write_all(&(sample_rate * 2).to_le_bytes()).unwrap(); // byte rate
        w.write_all(&2u16.to_le_bytes()).unwrap();            // block align
        w.write_all(&16u16.to_le_bytes()).unwrap();           // bits/sample
        w.write_all(b"data").unwrap();
        w.write_all(&data_len.to_le_bytes()).unwrap();
        for s in samples { w.write_all(&s.to_le_bytes()).unwrap(); }
        w.flush().unwrap();
        f
    }

    #[test]
    fn decodes_tiny_wav_to_pcm() {
        let f = write_tiny_wav();
        let (pcm, sr) = decode_to_pcm(f.path().to_str().unwrap()).unwrap();
        assert_eq!(sr, 8000);
        assert_eq!(pcm.len(), 4);
        assert!(pcm.iter().any(|&x| x != 0.0));
    }
}
```

> `tempfile` dev-dep:同 Task 2 Step 2 注 —— 缺则加 `tempfile = "3"` 到 `[dev-dependencies]`。

- [ ] **Step 4: 在 `sources/mod.rs` 接入 media**。把 `extract_text` 里的 Media 分支改为真实调用:

```rust
        (SourceKind::Media, IngestionSource::File(p)) => {
            Ok(ExtractedDoc { text: media::transcribe_media(p).await?, source_label: label })
        }
```

并在 `sources/mod.rs` 顶部 `pub mod` 区加 `pub mod media;`。

- [ ] **Step 5: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/b3.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b3.txt | head`(EXIT=0)
  - `cargo test --lib ingestion::sources::media > /tmp/t3.txt 2>&1; grep "test result" /tmp/t3.txt`(decode wav 测试通过)
  > 若 symphonia API 细节(`AudioBufferRef` 变体名/`chan`/`spec().channels.count()`)与 0.5 实际不符,按编译器报错最小调整;核心契约是「probe→make decoder→循环 decode→mono f32」。

- [ ] **Step 6: 提交**
```bash
git add src-tauri/src/ingestion/sources/ src-tauri/src/stt/ src-tauri/Cargo.toml
git commit -m "feat(ingestion): media decode via symphonia + STT bridge (transcribe_samples)"
```

---

## Task 4: chunk.rs — token 预算切块

**Files:** Replace `src-tauri/src/ingestion/chunk.rs`.

- [ ] **Step 1: 写 `chunk.rs`**

```rust
//! 文本按 token 预算切块。token≈字符/4 粗估;按段落边界优先;块数上限截断。

/// 每块目标 token 预算。
pub const CHUNK_TOKEN_BUDGET: usize = 2500;
/// 每文档最大块数(成本兜底)。
pub const MAX_CHUNKS_PER_DOC: usize = 20;

/// 粗略 token 估算(字符数/4,CJK 偏保守)。
fn est_tokens(s: &str) -> usize {
    (s.chars().count() / 4).max(1)
}

/// 切块:按段落(空行分隔)累积到预算;超 MAX_CHUNKS 截断,返回 (chunks, truncated)。
pub fn split_chunks(text: &str, budget_tokens: usize, max_chunks: usize) -> (Vec<String>, bool) {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_tokens = 0usize;

    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let pt = est_tokens(para);
        if cur_tokens + pt > budget_tokens && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_tokens = 0;
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
        cur_tokens += pt;
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }

    let truncated = chunks.len() > max_chunks;
    if truncated {
        chunks.truncate(max_chunks);
    }
    (chunks, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_budget() {
        // 每段约 5 token(20 字符),预算 6 → 每段一块
        let text = "aaaaaaaaaaaaaaaaaaaa\n\nbbbbbbbbbbbbbbbbbbbb\n\ncccccccccccccccccccc";
        let (chunks, truncated) = split_chunks(text, 6, 100);
        assert_eq!(chunks.len(), 3);
        assert!(!truncated);
    }

    #[test]
    fn caps_at_max_chunks() {
        let text = (0..50).map(|i| format!("para{}aaaaaaaaaaaaaaaaa", i)).collect::<Vec<_>>().join("\n\n");
        let (chunks, truncated) = split_chunks(&text, 6, 10);
        assert_eq!(chunks.len(), 10);
        assert!(truncated);
    }

    #[test]
    fn empty_text_no_chunks() {
        let (chunks, truncated) = split_chunks("   \n\n  ", CHUNK_TOKEN_BUDGET, MAX_CHUNKS_PER_DOC);
        assert!(chunks.is_empty());
        assert!(!truncated);
    }
}
```

- [ ] **Step 2: 测试**
  `cd src-tauri && cargo test --lib ingestion::chunk > /tmp/t4.txt 2>&1; grep "test result" /tmp/t4.txt`(3 passed)

- [ ] **Step 3: 提交**
```bash
git add src-tauri/src/ingestion/chunk.rs
git commit -m "feat(ingestion): token-budget chunking with max-chunk cap"
```

---

## Task 5: extract.rs — 实体 JSON 解析 + slug 规范化 + LLM 抽取 + 累积

**Files:** Replace `src-tauri/src/ingestion/extract.rs`; Modify `src-tauri/src/providers/service.rs`.

- [ ] **Step 1: 在 `providers/service.rs` 加 `get_ingestion_llm_config`**(紧挨 `get_chat_llm_config`,复制其 role lookup,role 名换 `"ingestion"`,未配回退 `get_active_llm_config`)

```rust
    /// Resolve the ingestion-role model → active_model fallback chain.
    pub async fn get_ingestion_llm_config(&self) -> Option<(String, String, String, String)> {
        let configs = self.configs.read().await;
        if let Some(role_cfg) = configs.role_models.iter().find(|r| r.role == "ingestion") {
            if let Some(model_ref) = &role_cfg.model_ref {
                let parts: Vec<&str> = model_ref.splitn(2, '/').collect();
                if parts.len() == 2 {
                    let (pid, mid) = (parts[0], parts[1]);
                    if let Some(provider) = configs.find_provider(pid) {
                        return Some((
                            pid.to_string(),
                            mid.to_string(),
                            provider.api_key.clone().unwrap_or_default(),
                            provider.base_url.clone().unwrap_or_default(),
                        ));
                    }
                }
            }
        }
        drop(configs);
        self.get_active_llm_config().await
    }
```

> 确认 `self.configs` 字段名与锁类型:`get_chat_llm_config` 用 `let configs = self.configs.read().await;` —— 直接照抄它的开头。若字段名不同,以 `get_chat_llm_config` 实际写法为准。

- [ ] **Step 2: 写 `extract.rs`**

```rust
//! 文本块 → LLM 抽实体 → JSON 解析 → 跨块按 slug 累积。

use crate::ingestion::job::IngestError;
use serde::Deserialize;

/// LLM 抽出的一个知识实体。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExtractedEntity {
    pub slug: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "compiled_truth_md", alias = "compiled_truth", default)]
    pub compiled_truth: String,
    #[serde(default)]
    pub links: Vec<String>,
}

/// 抽取 system prompt(返回 JSON 数组)。
pub const EXTRACT_SYSTEM: &str = "You extract distinct knowledge entities from a text chunk for a personal knowledge base. \
Return ONLY a JSON array, no prose. Each item: {\"slug\":\"domain/kebab-name\",\"type\":\"person|concept|org|reference|note\",\"title\":\"...\",\"compiled_truth_md\":\"a self-contained markdown summary of this entity, may use ## sections and [[wikilinks]]\",\"links\":[\"other-slug\"]}. \
Use domain-prefixed kebab slugs (e.g. people/jane-doe, concept/vector-search, org/acme). If nothing substantive, return [].";

/// 规范化 slug:小写、空格→`-`、去非法字符、折叠多重 `-`、限长。保留首个 `/`(域前缀)。
pub fn normalize_slug(raw: &str) -> String {
    let raw = raw.trim().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in raw.chars() {
        let mapped = match c {
            'a'..='z' | '0'..='9' | '/' => Some(c),
            ' ' | '_' | '-' => Some('-'),
            c if c as u32 > 127 => Some(c), // 保留 CJK 等
            _ => None,
        };
        match mapped {
            Some('-') => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
            Some(ch) => {
                out.push(ch);
                prev_dash = false;
            }
            None => {}
        }
    }
    out.trim_matches('-').chars().take(120).collect()
}

/// 从 LLM 原始输出解析实体数组。鲁棒:剥 ```json fence、取首个 `[` 到末个 `]`。
pub fn parse_entities(raw: &str) -> Result<Vec<ExtractedEntity>, IngestError> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let start = cleaned.find('[');
    let end = cleaned.rfind(']');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &cleaned[s..=e],
        _ => return Err(IngestError::Parse("no JSON array in LLM output".into())),
    };
    let mut entities: Vec<ExtractedEntity> =
        serde_json::from_str(json).map_err(|e| IngestError::Parse(e.to_string()))?;
    for ent in entities.iter_mut() {
        ent.slug = normalize_slug(&ent.slug);
    }
    entities.retain(|e| !e.slug.is_empty());
    Ok(entities)
}

/// 跨块累积:同 slug 合并(标题取首个非空,compiled_truth 拼接,links 去重并集)。
pub fn accumulate(acc: &mut Vec<ExtractedEntity>, new: Vec<ExtractedEntity>) {
    for ent in new {
        if let Some(existing) = acc.iter_mut().find(|e| e.slug == ent.slug) {
            if existing.title.is_empty() {
                existing.title = ent.title;
            }
            if !ent.compiled_truth.trim().is_empty() {
                if !existing.compiled_truth.is_empty() {
                    existing.compiled_truth.push_str("\n\n");
                }
                existing.compiled_truth.push_str(&ent.compiled_truth);
            }
            for l in ent.links {
                if !existing.links.contains(&l) {
                    existing.links.push(l);
                }
            }
        } else {
            acc.push(ent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let raw = r#"[{"slug":"people/jane-doe","type":"person","title":"Jane","compiled_truth_md":"# Jane","links":["org/acme"]}]"#;
        let ents = parse_entities(raw).unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].slug, "people/jane-doe");
        assert_eq!(ents[0].page_type, "person");
        assert!(ents[0].compiled_truth.contains("Jane"));
        assert_eq!(ents[0].links, vec!["org/acme"]);
    }

    #[test]
    fn parses_fenced_json_with_prose() {
        let raw = "Here you go:\n```json\n[{\"slug\":\"Concept/Vector Search\",\"title\":\"VS\"}]\n```\nDone.";
        let ents = parse_entities(raw).unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].slug, "concept/vector-search"); // 规范化生效
    }

    #[test]
    fn malformed_json_errs_not_panics() {
        assert!(parse_entities("not json at all").is_err());
        assert!(parse_entities("[ {bad } ]").is_err());
    }

    #[test]
    fn normalize_slug_rules() {
        assert_eq!(normalize_slug("People/Jane  Doe!!"), "people/jane-doe");
        assert_eq!(normalize_slug("  __weird__  "), "weird");
        assert_eq!(normalize_slug("人物/刘磊"), "人物/刘磊");
    }

    #[test]
    fn accumulate_merges_by_slug() {
        let mut acc = vec![ExtractedEntity {
            slug: "concept/x".into(), page_type: "concept".into(), title: "X".into(),
            compiled_truth: "first".into(), links: vec!["a".into()],
        }];
        accumulate(&mut acc, vec![ExtractedEntity {
            slug: "concept/x".into(), page_type: "concept".into(), title: "".into(),
            compiled_truth: "second".into(), links: vec!["a".into(), "b".into()],
        }]);
        assert_eq!(acc.len(), 1);
        assert!(acc[0].compiled_truth.contains("first") && acc[0].compiled_truth.contains("second"));
        assert_eq!(acc[0].links, vec!["a", "b"]);
    }
}
```

- [ ] **Step 3: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/b5.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b5.txt | head`(EXIT=0)
  - `cargo test --lib ingestion::extract > /tmp/t5.txt 2>&1; grep "test result" /tmp/t5.txt`(5 passed)

- [ ] **Step 4: 提交**
```bash
git add src-tauri/src/ingestion/extract.rs src-tauri/src/providers/service.rs
git commit -m "feat(ingestion): entity JSON parse + slug normalize + cross-chunk accumulate + ingestion role"
```

---

## Task 6: merge.rs — 合并判定 + 写入

**Files:** Replace `src-tauri/src/ingestion/merge.rs`.

- [ ] **Step 1: 写 `merge.rs`**

```rust
//! 实体 → gbrain 页:get_page 命中则 LLM 合并,未命中则新建,统一 put_page。

use crate::gbrain::browse::{self, PageDetail};
use crate::ingestion::extract::ExtractedEntity;
use crate::ingestion::job::IngestError;
use crate::llm::provider::LlmProvider;
use crate::mcp::SharedMcpManager;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    Create,
    Merge,
}

/// 据现有页是否存在决定动作。纯函数,易测。
pub fn decide(existing: Option<&PageDetail>) -> MergeAction {
    match existing {
        Some(_) => MergeAction::Merge,
        None => MergeAction::Create,
    }
}

const MERGE_SYSTEM: &str = "You merge NEW information into an EXISTING knowledge page. \
Preserve the existing structure and all existing facts; add or update with the new info, do not delete. \
Return ONLY the full merged markdown body (no code fences, no commentary).";

/// 写一个实体。返回写入的 slug。`complete` 是注入的 LLM 闭包(便于测试/复用)。
pub async fn write_entity(
    mcp: &SharedMcpManager,
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    entity: &ExtractedEntity,
) -> Result<String, IngestError> {
    let existing = match browse::get_page(mcp, &entity.slug).await {
        Ok(p) => Some(p),
        Err(e) => {
            let msg = e_to_string(&e);
            if msg.contains("page_not_found") {
                None
            } else {
                return Err(IngestError::Gbrain(msg));
            }
        }
    };

    let content = match decide(existing.as_ref()) {
        MergeAction::Create => entity.compiled_truth.clone(),
        MergeAction::Merge => {
            let existing = existing.unwrap();
            let user = format!(
                "EXISTING PAGE (slug {}):\n\n{}\n\n---\nNEW INFORMATION:\n\n{}",
                entity.slug, existing.compiled_truth, entity.compiled_truth
            );
            complete_text(provider, model, MERGE_SYSTEM, &user).await?
        }
    };

    browse::put_page(mcp, &entity.slug, &content)
        .await
        .map_err(|e| IngestError::Gbrain(e_to_string(&e)))?;
    Ok(entity.slug.clone())
}

/// LLM 一次性文本补全(摄入复用)。
pub async fn complete_text(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String, IngestError> {
    use crate::agent::types::{ChatMessage, RespondOutput};
    use crate::llm::provider::CompletionConfig;
    let messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
    let config = CompletionConfig {
        model: model.to_string(),
        max_tokens: 4096,
        temperature: 0.3,
        thinking_enabled: false,
    };
    let out = provider
        .complete(messages, vec![], &config)
        .await
        .map_err(|e| IngestError::Llm(e.to_string()))?;
    Ok(match out {
        RespondOutput::Text { text, .. } => text,
        RespondOutput::ToolCalls { text, .. } => text.unwrap_or_default(),
    })
}

fn e_to_string(e: &crate::gbrain::browse::GbrainError) -> String {
    format!("{:?}", e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_create_when_absent() {
        assert_eq!(decide(None), MergeAction::Create);
    }

    #[test]
    fn decide_merge_when_present() {
        let p = PageDetail {
            slug: "concept/x".into(), title: "X".into(), page_type: "concept".into(),
            compiled_truth: "body".into(), frontmatter: serde_json::json!({}),
            created_at: None, updated_at: None, tags: vec![], raw_markdown: String::new(),
        };
        assert_eq!(decide(Some(&p)), MergeAction::Merge);
    }
}
```

> `e_to_string` 用 `{:?}`(GbrainError 仅 derive Debug);判 `page_not_found` 靠子串匹配(CallFailed 内含该串)。若 `GbrainError` 实现了 `Display`(`thiserror`),可改 `e.to_string()` —— 实现时看实际 derive。

- [ ] **Step 2: 编译 + 测试**
  - `cd src-tauri && cargo build > /tmp/b6.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b6.txt | head`(EXIT=0)
  - `cargo test --lib ingestion::merge > /tmp/t6.txt 2>&1; grep "test result" /tmp/t6.txt`(2 passed)

- [ ] **Step 3: 提交**
```bash
git add src-tauri/src/ingestion/merge.rs
git commit -m "feat(ingestion): merge decision + write_entity (get_page → LLM merge/create → put_page)"
```

---

## Task 7: IngestionService — submit + 管线编排 + 进度 emit

**Files:** Modify `src-tauri/src/ingestion/mod.rs`.

- [ ] **Step 1: 在 `mod.rs` 实现 `IngestionService`**(替换 Task 1 的骨架,保留 `pub mod`/`pub use`)

```rust
use crate::mcp::SharedMcpManager;
use crate::providers::ProviderService;
use job::{IngestionJob, IngestionSource, IngestionStatus, JobId, Progress};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;
use uuid::Uuid;

type JobMap = Arc<Mutex<HashMap<JobId, IngestionJob>>>;

/// 摄入服务:内存 job 注册表 + 管线编排。存于 AppState,命令直接调。
pub struct IngestionService {
    jobs: JobMap,
    provider_service: Arc<ProviderService>,
    mcp: SharedMcpManager,
}

impl IngestionService {
    pub fn new(provider_service: Arc<ProviderService>, mcp: SharedMcpManager) -> Self {
        Self { jobs: Arc::new(Mutex::new(HashMap::new())), provider_service, mcp }
    }

    pub async fn status(&self, id: &str) -> Option<IngestionJob> {
        self.jobs.lock().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<IngestionJob> {
        let mut v: Vec<IngestionJob> = self.jobs.lock().await.values().cloned().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// 提交一个源 → 建 job → spawn 管线。返回 job id。
    pub async fn submit(&self, source: IngestionSource, app: tauri::AppHandle) -> JobId {
        let id = Uuid::new_v4().to_string();
        let job = IngestionJob::new(id.clone(), source.label());
        self.jobs.lock().await.insert(id.clone(), job.clone());
        emit(&app, &job);

        let jobs = self.jobs.clone();
        let provider_service = self.provider_service.clone();
        let mcp = self.mcp.clone();
        let id2 = id.clone();
        tokio::spawn(async move {
            run_pipeline(jobs, provider_service, mcp, app, id2, source).await;
        });
        id
    }
}

fn emit(app: &tauri::AppHandle, job: &IngestionJob) {
    let _ = app.emit("ingestion:progress", job);
}

/// 更新 job 并 emit。
async fn update<F: FnOnce(&mut IngestionJob)>(
    jobs: &JobMap,
    app: &tauri::AppHandle,
    id: &str,
    f: F,
) {
    let mut guard = jobs.lock().await;
    if let Some(job) = guard.get_mut(id) {
        f(job);
        let snap = job.clone();
        drop(guard);
        emit(app, &snap);
    }
}

async fn run_pipeline(
    jobs: JobMap,
    provider_service: Arc<ProviderService>,
    mcp: SharedMcpManager,
    app: tauri::AppHandle,
    id: JobId,
    source: IngestionSource,
) {
    // 1. 抽文本
    update(&jobs, &app, &id, |j| {
        j.status = IngestionStatus::Parsing;
        j.progress = Progress { stage: "parsing".into(), done: 0, total: 0 };
    })
    .await;
    let doc = match sources::extract_text(&source).await {
        Ok(d) => d,
        Err(e) => return fail(&jobs, &app, &id, e.to_string()).await,
    };
    if doc.text.trim().is_empty() {
        return finish(&jobs, &app, &id, IngestionStatus::Partial, Some("no extractable text".into())).await;
    }

    // 2. 切块
    let (chunks, truncated) =
        chunk::split_chunks(&doc.text, chunk::CHUNK_TOKEN_BUDGET, chunk::MAX_CHUNKS_PER_DOC);

    // 3. 解析 LLM(utility role)
    let llm = match provider_service.get_ingestion_llm_config().await {
        Some((pid, model, key, url)) => {
            let cfg = crate::llm::llm_config_from_provider(&pid, &model, &key, &url, 4096, 0.3);
            match crate::llm::create_provider(&cfg) {
                Ok(p) => (p, model),
                Err(e) => return fail(&jobs, &app, &id, format!("provider: {e}")).await,
            }
        }
        None => return fail(&jobs, &app, &id, "no LLM provider configured".into()).await,
    };
    let (provider, model) = llm;

    // 4. 逐块抽实体 → 累积
    update(&jobs, &app, &id, |j| {
        j.status = IngestionStatus::Extracting;
        j.progress = Progress { stage: "extracting".into(), done: 0, total: chunks.len() as u32 };
    })
    .await;
    let mut acc: Vec<extract::ExtractedEntity> = Vec::new();
    let mut had_chunk_error = false;
    for (i, ch) in chunks.iter().enumerate() {
        match merge::complete_text(&provider, &model, extract::EXTRACT_SYSTEM, ch).await {
            Ok(raw) => match extract::parse_entities(&raw) {
                Ok(ents) => extract::accumulate(&mut acc, ents),
                Err(_) => had_chunk_error = true,
            },
            Err(_) => had_chunk_error = true,
        }
        let done = (i + 1) as u32;
        update(&jobs, &app, &id, |j| j.progress.done = done).await;
    }

    // 5. 逐实体写入
    update(&jobs, &app, &id, |j| {
        j.status = IngestionStatus::Writing;
        j.progress = Progress { stage: "writing".into(), done: 0, total: acc.len() as u32 };
    })
    .await;
    let mut written: Vec<String> = Vec::new();
    let mut had_write_error = false;
    for (i, ent) in acc.iter().enumerate() {
        match merge::write_entity(&mcp, &provider, &model, ent).await {
            Ok(slug) => written.push(slug),
            Err(_) => had_write_error = true,
        }
        let done = (i + 1) as u32;
        update(&jobs, &app, &id, |j| j.progress.done = done).await;
    }

    let partial = truncated || had_chunk_error || had_write_error || (written.is_empty() && !acc.is_empty());
    let status = if partial { IngestionStatus::Partial } else { IngestionStatus::Done };
    let note = if truncated { Some(format!("doc truncated to {} chunks", chunk::MAX_CHUNKS_PER_DOC)) } else { None };
    update(&jobs, &app, &id, |j| {
        j.pages_written = written;
    })
    .await;
    finish(&jobs, &app, &id, status, note).await;
}

async fn fail(jobs: &JobMap, app: &tauri::AppHandle, id: &str, err: String) {
    update(jobs, app, id, |j| {
        j.status = IngestionStatus::Failed;
        j.error = Some(err);
    })
    .await;
}

async fn finish(
    jobs: &JobMap,
    app: &tauri::AppHandle,
    id: &str,
    status: IngestionStatus,
    note: Option<String>,
) {
    update(jobs, app, id, |j| {
        j.status = status;
        if note.is_some() {
            j.error = note;
        }
    })
    .await;
}
```

> 注:`use crate::providers::ProviderService;` — 确认 ProviderService 的真实路径(`crate::providers::service::ProviderService` 或 re-export `crate::providers::ProviderService`);以编译为准。`tauri::Emitter` trait 提供 `.emit`(Tauri v2)。

- [ ] **Step 2: 编译**(本 Task 无新单测——管线是异步编排,纯逻辑已在 Task 4/5/6 测过;管线靠 Task 11 手动 E2E)
  - `cd src-tauri && cargo build > /tmp/b7.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b7.txt | head`(EXIT=0)
  - `cargo test --lib ingestion > /tmp/t7.txt 2>&1; grep "test result" /tmp/t7.txt`(此前所有 ingestion 单测仍过)

- [ ] **Step 3: 提交**
```bash
git add src-tauri/src/ingestion/mod.rs
git commit -m "feat(ingestion): IngestionService submit + pipeline orchestration + progress events"
```

---

## Task 8: Tauri 命令 + AppState 接线

**Files:** Modify `src-tauri/src/tauri_commands.rs`, `src-tauri/src/app.rs`, `src-tauri/src/main.rs`.

- [ ] **Step 1: AppState 加字段 + 构造**(`src-tauri/src/app.rs`)
  - 在 AppState struct 加:`pub ingestion: Arc<crate::ingestion::IngestionService>,`
  - 在构造 AppState 处(`provider_service`、`mcp_manager` 已就绪后)加:
    ```rust
    let ingestion = Arc::new(crate::ingestion::IngestionService::new(
        provider_service.clone(),
        mcp_manager.clone(),
    ));
    ```
    并把 `ingestion` 填入 AppState 字面量。
  > 需要 `IngestionService` 可见:`crate::ingestion` 已在 lib.rs 导出。`IngestionService` 在 `ingestion/mod.rs` 是 `pub struct`,加 `pub use mod 内` 自动可达 `crate::ingestion::IngestionService`(已在 Step 1 的 mod.rs 中通过 `pub struct` 暴露)。

- [ ] **Step 2: 写命令**(`src-tauri/src/tauri_commands.rs` 末尾)

```rust
#[tauri::command]
pub async fn ingest_files(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for p in paths {
        let id = state
            .ingestion
            .submit(crate::ingestion::IngestionSource::File(p), app.clone())
            .await;
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
pub async fn ingest_url(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    url: String,
) -> Result<String, String> {
    Ok(state
        .ingestion
        .submit(crate::ingestion::IngestionSource::Url(url), app)
        .await)
}

#[tauri::command]
pub async fn ingest_job_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<crate::ingestion::IngestionJob>, String> {
    Ok(state.ingestion.status(&id).await)
}

#[tauri::command]
pub async fn ingest_list_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::ingestion::IngestionJob>, String> {
    Ok(state.ingestion.list().await)
}
```

- [ ] **Step 3: 注册命令**(`src-tauri/src/main.rs` 的 `invoke_handler!`,gbrain_* 命令区附近)

```rust
            uclaw_core::tauri_commands::ingest_files,
            uclaw_core::tauri_commands::ingest_url,
            uclaw_core::tauri_commands::ingest_job_status,
            uclaw_core::tauri_commands::ingest_list_jobs,
```

- [ ] **Step 4: 编译**
  - `cd src-tauri && cargo build > /tmp/b8.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b8.txt | head`(EXIT=0)
  - `cargo test --lib > /tmp/t8.txt 2>&1; grep "test result" /tmp/t8.txt | tail -1`(全 lib 测试通过)

- [ ] **Step 5: 提交**
```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/app.rs src-tauri/src/main.rs
git commit -m "feat(ingestion): ingest_* Tauri commands + AppState wiring + invoke_handler registration"
```

---

## Task 9: 前端 — IPC 封装 + QuickCaptureDialog 喂资料模式

**Files:** Create `ui/src/lib/ingestion.ts`; Modify `ui/src/components/memory/QuickCaptureDialog.tsx`.

- [ ] **Step 1: 写 `ui/src/lib/ingestion.ts`**

```ts
import { invoke } from '@tauri-apps/api/core'

export type IngestionStatus =
  | 'queued' | 'parsing' | 'extracting' | 'writing' | 'done' | 'partial' | 'failed'

export interface IngestionProgress { stage: string; done: number; total: number }

export interface IngestionJob {
  id: string
  source_label: string
  status: IngestionStatus
  progress: IngestionProgress
  pages_written: string[]
  error: string | null
}

export function ingestFiles(paths: string[]): Promise<string[]> {
  return invoke<string[]>('ingest_files', { paths })
}

export function ingestUrl(url: string): Promise<string> {
  return invoke<string>('ingest_url', { url })
}

export function ingestListJobs(): Promise<IngestionJob[]> {
  return invoke<IngestionJob[]>('ingest_list_jobs', {})
}

export function ingestJobStatus(id: string): Promise<IngestionJob | null> {
  return invoke<IngestionJob | null>('ingest_job_status', { id })
}
```

- [ ] **Step 2: 在 `QuickCaptureDialog.tsx` 加 'feed' 模式**
  - 把 `type CaptureMode = 'fragment' | 'entity_page'` 改为加 `| 'feed'`。
  - 在模式切换按钮区(现有两个 `<button>` 旁)加第三个按钮:
    ```tsx
    <button
      type="button"
      onClick={() => setMode('feed')}
      className={mode === 'feed' ? activeBtnClass : inactiveBtnClass}
    >
      喂资料
    </button>
    ```
    (`activeBtnClass`/`inactiveBtnClass` 用现有两个按钮相同的 className 表达式,照抄。)
  - 在主体区,当 `mode === 'feed'` 渲染拖放区 + URL 输入(放在现有 fragment/entity 表单的同级条件分支):
    ```tsx
    {mode === 'feed' && (
      <FeedPanel onClose={() => setQuickCaptureOpen(false)} />
    )}
    ```

- [ ] **Step 3: 在同文件(或 `ui/src/components/memory/FeedPanel.tsx` 新建)实现 `FeedPanel`**

```tsx
import React from 'react'
import { getPathForFile } from '@tauri-apps/api'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { ingestFiles, ingestUrl, type IngestionJob } from '@/lib/ingestion'

export function FeedPanel({ onClose }: { onClose: () => void }): React.ReactElement {
  const [url, setUrl] = React.useState('')
  const [dragOver, setDragOver] = React.useState(false)
  const [jobs, setJobs] = React.useState<Record<string, IngestionJob>>({})

  React.useEffect(() => {
    const un = listen<IngestionJob>('ingestion:progress', (e) => {
      const job = e.payload
      setJobs((prev) => ({ ...prev, [job.id]: job }))
      if (job.status === 'done' || job.status === 'partial') {
        toast.success(`从 ${job.source_label} 写入 ${job.pages_written.length} 页`)
      } else if (job.status === 'failed') {
        toast.error(`摄入失败: ${job.source_label}${job.error ? ` (${job.error})` : ''}`)
      }
    })
    return () => { un.then((f) => f()) }
  }, [])

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    const files = Array.from(e.dataTransfer.files)
    const paths: string[] = []
    for (const f of files) {
      try { const p = getPathForFile(f); if (p) paths.push(p) } catch { /* skip */ }
    }
    if (paths.length === 0) { toast.error('无法获取文件路径'); return }
    await ingestFiles(paths)
    toast.message(`已开始摄入 ${paths.length} 个文件`)
  }

  const submitUrl = async () => {
    const u = url.trim()
    if (!u) return
    await ingestUrl(u)
    toast.message(`已开始摄入 ${u}`)
    setUrl('')
  }

  const active = Object.values(jobs)

  return (
    <div className="flex flex-col gap-3">
      <div
        onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
        className={`rounded-lg border-2 border-dashed p-6 text-center text-sm ${
          dragOver ? 'border-accent bg-accent/10' : 'border-border text-muted-foreground'
        }`}
      >
        拖放 PDF / md / 音视频文件到这里
      </div>
      <div className="flex gap-2">
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void submitUrl() }}
          placeholder="或粘贴一个 URL"
          className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-sm"
        />
        <button type="button" onClick={() => void submitUrl()} className="rounded-md bg-accent px-3 py-1 text-sm text-accent-foreground">
          摄入
        </button>
      </div>
      {active.length > 0 && (
        <ul className="max-h-40 overflow-auto text-xs text-muted-foreground">
          {active.map((j) => (
            <li key={j.id} className="flex justify-between py-0.5">
              <span className="truncate">{j.source_label}</span>
              <span>{j.status === 'extracting' || j.status === 'writing'
                ? `${j.progress.stage} ${j.progress.done}/${j.progress.total}`
                : j.status}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
```

> 导入路径核对:`getPathForFile` 在本仓库由 `@tauri-apps/api` 导出(见 `AgentView.tsx:120`);若实际从别处导入(如 `@tauri-apps/api/webviewWindow`),照 `AgentView.tsx` 的真实 import 写。`setQuickCaptureOpen` 来自 `quickCaptureOpenAtom`(`useSetAtom`)。主题色用 token(`bg-accent`/`text-muted-foreground`/`border-border`),勿硬编码。

- [ ] **Step 4: 写 vitest**(`ui/src/components/memory/FeedPanel.test.tsx`)

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { FeedPanel } from './FeedPanel'

vi.mock('@tauri-apps/api', () => ({ getPathForFile: (f: File) => `/tmp/${f.name}` }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
const ingestUrl = vi.fn().mockResolvedValue('job-1')
const ingestFiles = vi.fn().mockResolvedValue(['job-2'])
vi.mock('@/lib/ingestion', () => ({ ingestUrl: (u: string) => ingestUrl(u), ingestFiles: (p: string[]) => ingestFiles(p) }))
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), message: vi.fn() } }))

describe('FeedPanel', () => {
  beforeEach(() => { ingestUrl.mockClear(); ingestFiles.mockClear() })

  it('submits a URL', async () => {
    render(<FeedPanel onClose={() => {}} />)
    fireEvent.change(screen.getByPlaceholderText('或粘贴一个 URL'), { target: { value: 'https://x.com' } })
    fireEvent.click(screen.getByText('摄入'))
    await waitFor(() => expect(ingestUrl).toHaveBeenCalledWith('https://x.com'))
  })

  it('renders the drop zone', () => {
    render(<FeedPanel onClose={() => {}} />)
    expect(screen.getByText(/拖放/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 5: 类型检查 + 测试**
  - `cd ui && npx tsc --noEmit 2>&1 | head`(无新错误)
  - `npm test -- --run FeedPanel 2>&1 | tail -15`(2 passed)

- [ ] **Step 6: 提交**
```bash
git add ui/src/lib/ingestion.ts ui/src/components/memory/QuickCaptureDialog.tsx ui/src/components/memory/FeedPanel.tsx ui/src/components/memory/FeedPanel.test.tsx
git commit -m "feat(ingestion): QuickCaptureDialog feed mode + FeedPanel drop/url + progress subscription"
```

---

## Task 10: 集成验证 + 手动 E2E

**Files:** 无(验证 Task)。

- [ ] **Step 1: 全量构建 + 全测试**
  - `cd src-tauri && cargo build > /tmp/bf.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/bf.txt | head`(EXIT=0)
  - `cargo test --lib > /tmp/tf.txt 2>&1; grep "test result" /tmp/tf.txt | tail -1`(全过)
  - `cd ui && npx tsc --noEmit 2>&1 | head; npm test -- --run 2>&1 | tail -10`

- [ ] **Step 2: 手动 E2E**(`cargo tauri dev`,gbrain 已连)。写进 PR 验证清单:
  1. 打开 QuickCaptureDialog(全局快捷键)→ 切到"喂资料"。
  2. 拖一个 **md/txt** → 轻提示"已开始" → 完成提示"写入 N 页" → 去 WikiView(A)/双星云(C)看到新实体页、内容合理。
  3. 拖一个 **PDF**(有文本层)→ 同上;扫描件 PDF → Partial + 提示(不崩)。
  4. 贴一个 **URL** → 抓正文 → 出页。
  5. 拖一个 **mp3/mp4** → STT → 抽实体 → 出页(需 SenseVoice 模型已下载;未下载 → Failed + 清晰原因)。
  6. 喂一个与已有实体重叠的资料 → 该页被**合并更新**(版本史多一版,可回滚)。
  7. 不支持格式(如 .zip)→ 即时错误提示,不建 job。
  8. Settings 配 `ingestion` role 为廉价模型 → 抽取走该模型(成本可控)。

- [ ] **Step 3: 更新 CLAUDE.md 依赖说明**(Part 2「新依赖」语境):记 `pdf-extract` + `symphonia` 的用途(子项目 B 文档/音视频摄入)。提交:
```bash
git add CLAUDE.md
git commit -m "docs(claude): note pdf-extract + symphonia deps (ingestion pipeline B)"
```

---

## 自检(对照 spec)

- **Spec §2 决策 1(全量格式)** → Task 2(text/pdf/url)+ Task 3(media)。**决策 2(抽实体+合并)** → Task 5(抽取/累积)+ Task 6(合并/新建)。**决策 3(全静默+事后审)** → Task 7 管线无审核门、仅 emit 进度 + Task 9 仅轻提示;事后看 C/A。**决策 4(廉价 utility 模型+块上限)** → Task 5(`get_ingestion_llm_config`)+ Task 4(`MAX_CHUNKS_PER_DOC`)。**决策 5(symphonia)** → Task 3。**决策 6(内存态 job)** → Task 1/7(HashMap,无 migration)。**决策 7(QuickCaptureDialog)** → Task 9。
- **Spec §4 模块拆分** → Task 1–7 逐文件对应。**§6 成本** → Task 5/4。**§7 前端** → Task 9。**§8 错误处理** → IngestError(Task 1)+ 管线 fail/Partial 分支(Task 7)+ 命令 Result(Task 8)。**§9 测试** → 各 Task 单测 + Task 10 E2E。**§10 新依赖标注** → Task 1 + Task 10 Step 3。
- **有意偏离**:spec §3/§4 说 IngestionService「impl ManagedService 注册 Stage 3」;本计划改为**存 AppState、不实现 ManagedService**(无生命周期工作,YAGNI;命令直接调,进度靠命令传入的 AppHandle emit)。已在计划头与 Task 8 注明。
- **占位符扫描**:无 TBD;`tempfile` dev-dep、ProviderService 路径、symphonia 0.5 API 细节、`getPathForFile` 导入处标了「按编译器/真实 import 为准」的核对点(非占位,是已知需现场确认的真实 API 名)。
- **类型一致**:`IngestionSource`/`IngestionJob`/`IngestionStatus`/`ExtractedEntity`/`MergeAction` 跨 Task 同名同形;`split_chunks(text,budget,max)->(Vec<String>,bool)`、`parse_entities(&str)->Result<Vec<ExtractedEntity>,IngestError>`、`write_entity(mcp,provider,model,entity)`、`complete_text(provider,model,system,user)`、`get_ingestion_llm_config()->Option<(String,String,String,String)>` 全任务一致;前端 `IngestionJob` 字段(snake_case)对齐后端 serde。
