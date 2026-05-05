use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::memubot_config::LocalApiConfig;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::routes::{self, ApiState};

// ─── LocalApiService ─────────────────────────────────────────────────

/// 本地 API 服务
///
/// 提供 HTTP API 端点供外部工具和服务访问 uClaw 的记忆和服务状态。
/// 仅监听 127.0.0.1（localhost），不暴露到外网。
/// 对标 memUBot 的 local-api.ts，默认端口 7337。
pub struct LocalApiService {
    /// 本地 API 配置（包含 enabled 开关和端口号）
    config: LocalApiConfig,
    /// HTTP 服务器的 tokio 任务句柄
    handle: RwLock<Option<JoinHandle<()>>>,
    /// 标识服务是否正在运行
    is_running: AtomicBool,
    /// 服务启动时间（用于计算 uptime）
    start_time: RwLock<Option<std::time::Instant>>,
}

impl LocalApiService {
    /// 创建 LocalApiService 实例
    ///
    /// - `config`: 从 MemubotConfig 中读取的 LocalApiConfig
    pub fn new(config: LocalApiConfig) -> Self {
        Self {
            config,
            handle: RwLock::new(None),
            is_running: AtomicBool::new(false),
            start_time: RwLock::new(None),
        }
    }

    /// 获取监听地址（仅 localhost）
    fn listen_addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.config.port))
    }
}

// ─── ManagedService 实现 ─────────────────────────────────────────────

#[async_trait]
impl ManagedService for LocalApiService {
    fn name(&self) -> &str {
        "local_api"
    }

    async fn start(&self) -> anyhow::Result<()> {
        // 如果配置中未启用，则跳过启动
        if !self.config.enabled {
            tracing::info!("[LocalAPI] 本地 API 已禁用，跳过启动");
            return Ok(());
        }

        let addr = self.listen_addr();
        let state = Arc::new(ApiState {
            start_time: std::time::Instant::now(),
        });
        let router = routes::create_router(state);

        tracing::info!("[LocalAPI] 启动 HTTP 服务，监听: {}", addr);

        // 绑定 TCP 监听器
        let listener = tokio::net::TcpListener::bind(addr).await?;

        // 在后台 tokio 任务中运行 axum HTTP 服务器
        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!("[LocalAPI] HTTP 服务错误: {}", e);
            }
        });

        // 记录任务句柄和启动时间
        *self.handle.write().await = Some(handle);
        *self.start_time.write().await = Some(std::time::Instant::now());
        self.is_running.store(true, Ordering::SeqCst);

        tracing::info!("[LocalAPI] HTTP 服务已启动 → http://{}", addr);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        // 取消后台任务
        if let Some(handle) = self.handle.write().await.take() {
            handle.abort();
        }
        self.is_running.store(false, Ordering::SeqCst);
        *self.start_time.write().await = None;

        tracing::info!("[LocalAPI] HTTP 服务已停止");
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        if self.is_running.load(Ordering::SeqCst) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    fn health(&self) -> ServiceHealth {
        // 注意：health() 是同步方法，无法 await RwLock
        // 使用 try_read 获取启动时间
        let uptime_secs = self
            .start_time
            .try_read()
            .ok()
            .and_then(|guard| guard.map(|t| t.elapsed().as_secs()));

        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs,
            last_error: None,
            metrics: serde_json::json!({
                "port": self.config.port,
                "enabled": self.config.enabled,
            }),
        }
    }
}
