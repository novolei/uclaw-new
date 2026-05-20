//! 知识摄入管线(子项目 B):拖放文件 → 后台静默解析 → LLM 抽实体 →
//! 智能合并 put_page 写入 gbrain。job 内存态,事后用户在双星云(C)看/改/回滚。

pub mod job;
pub mod sources;
pub mod chunk;
pub mod extract;
pub mod merge;

pub use job::{IngestError, IngestionJob, IngestionSource, IngestionStatus, JobId, Progress};

use crate::mcp::SharedMcpManager;
use crate::providers::service::ProviderService;
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
    let (provider, model) = match provider_service.get_ingestion_llm_config().await {
        Some((pid, model, key, url)) => {
            let cfg = crate::llm::llm_config_from_provider(&pid, &model, &key, &url, 4096, 0.3);
            match crate::llm::create_provider(&cfg) {
                Ok(p) => (p, model),
                Err(e) => return fail(&jobs, &app, &id, format!("provider: {e}")).await,
            }
        }
        None => return fail(&jobs, &app, &id, "no LLM provider configured".into()).await,
    };

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
