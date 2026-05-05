use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::types::*;

// ─── ManagedService trait ──────────────────────────────────────────────

/// 受管服务 trait。
/// 所有需要 ServiceManager 统一管理的后台服务都必须实现此 trait。
#[async_trait]
pub trait ManagedService: Send + Sync {
    /// 服务名称（唯一标识，用于查找和日志）
    fn name(&self) -> &str;

    /// 启动服务。实现方应在内部将状态切换为 Starting → Running。
    async fn start(&self) -> anyhow::Result<()>;

    /// 优雅停止服务。实现方应在内部将状态切换为 Stopping → Stopped。
    async fn stop(&self) -> anyhow::Result<()>;

    /// 获取当前服务状态（轻量同步调用）
    fn status(&self) -> ServiceStatus;

    /// 获取完整健康信息快照
    fn health(&self) -> ServiceHealth;
}

// ─── ServiceManager ────────────────────────────────────────────────────

/// 每个服务停止的超时时间（秒）
const STOP_TIMEOUT_SECS: u64 = 5;

/// 服务管理器：统一管理所有后台服务的注册、启停和健康监控。
///
/// 设计要点：
/// - 服务按注册顺序启动，按注册逆序停止（类似栈结构）
/// - 单个服务的启动/停止失败不影响其它服务
/// - stop_all 对每个服务设置 5 秒超时
pub struct ServiceManager {
    /// 已注册的服务列表（保持注册顺序）
    services: RwLock<Vec<Arc<dyn ManagedService>>>,
}

impl ServiceManager {
    /// 创建一个空的服务管理器
    pub fn new() -> Self {
        Self {
            services: RwLock::new(Vec::new()),
        }
    }

    /// 注册一个受管服务。
    /// 重复名称的服务不会被重复注册，会记录警告日志。
    pub async fn register(&self, service: Arc<dyn ManagedService>) {
        let mut services = self.services.write().await;
        let name = service.name().to_string();

        // 检查是否已存在同名服务
        if services.iter().any(|s| s.name() == name) {
            tracing::warn!("服务 '{}' 已注册，跳过重复注册", name);
            return;
        }

        tracing::info!("注册服务: {}", name);
        services.push(service);
    }

    /// 按注册顺序逐个启动所有服务。
    ///
    /// 返回每个服务的名称和启动结果。
    /// 某个服务启动失败不会阻断后续服务的启动。
    pub async fn start_all(&self) -> Vec<(String, anyhow::Result<()>)> {
        let services = self.services.read().await;
        let mut results = Vec::with_capacity(services.len());

        for service in services.iter() {
            let name = service.name().to_string();
            tracing::info!("正在启动服务: {}", name);

            match service.start().await {
                Ok(()) => {
                    tracing::info!("服务 '{}' 启动成功", name);
                    results.push((name, Ok(())));
                }
                Err(e) => {
                    tracing::error!("服务 '{}' 启动失败: {}", name, e);
                    results.push((name, Err(e)));
                }
            }
        }

        results
    }

    /// 按注册逆序逐个停止所有服务（优雅关闭）。
    ///
    /// 每个服务有 5 秒超时限制；超时后记录错误并继续停止下一个服务。
    /// 返回每个服务的名称和停止结果。
    pub async fn stop_all(&self) -> Vec<(String, anyhow::Result<()>)> {
        let services = self.services.read().await;
        let mut results = Vec::with_capacity(services.len());

        // 按注册逆序停止
        for service in services.iter().rev() {
            let name = service.name().to_string();

            // 仅尝试停止处于 Running / Starting / Failed 状态的服务
            let status = service.status();
            if status == ServiceStatus::Stopped {
                tracing::debug!("服务 '{}' 已处于 Stopped 状态，跳过", name);
                results.push((name, Ok(())));
                continue;
            }

            tracing::info!("正在停止服务: {}", name);

            // 带超时的停止操作
            let stop_result = tokio::time::timeout(
                Duration::from_secs(STOP_TIMEOUT_SECS),
                service.stop(),
            )
            .await;

            match stop_result {
                Ok(Ok(())) => {
                    tracing::info!("服务 '{}' 已停止", name);
                    results.push((name, Ok(())));
                }
                Ok(Err(e)) => {
                    tracing::error!("服务 '{}' 停止时出错: {}", name, e);
                    results.push((name, Err(e)));
                }
                Err(_) => {
                    let msg = format!("服务 '{}' 停止超时 ({}s)", name, STOP_TIMEOUT_SECS);
                    tracing::error!("{}", msg);
                    results.push((name, Err(anyhow::anyhow!(msg))));
                }
            }
        }

        results
    }

    /// 停止指定名称的服务。
    ///
    /// 如果找不到该服务，返回错误。带 5 秒超时保护。
    pub async fn stop_service(&self, name: &str) -> anyhow::Result<()> {
        let services = self.services.read().await;
        let service = services
            .iter()
            .find(|s| s.name() == name)
            .ok_or_else(|| anyhow::anyhow!("未找到服务: {}", name))?
            .clone();

        // 释放读锁后再操作
        drop(services);

        let status = service.status();
        if status == ServiceStatus::Stopped {
            tracing::debug!("服务 '{}' 已处于 Stopped 状态，无需停止", name);
            return Ok(());
        }

        tracing::info!("正在停止服务: {}", name);

        let stop_result = tokio::time::timeout(
            Duration::from_secs(STOP_TIMEOUT_SECS),
            service.stop(),
        )
        .await;

        match stop_result {
            Ok(Ok(())) => {
                tracing::info!("服务 '{}' 已停止", name);
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!("服务 '{}' 停止时出错: {}", name, e);
                Err(e)
            }
            Err(_) => {
                let msg = format!("服务 '{}' 停止超时 ({}s)", name, STOP_TIMEOUT_SECS);
                tracing::error!("{}", msg);
                Err(anyhow::anyhow!(msg))
            }
        }
    }

    /// 重启指定名称的服务（先停后启）。
    ///
    /// 如果找不到该服务，返回错误。
    pub async fn restart_service(&self, name: &str) -> anyhow::Result<()> {
        let services = self.services.read().await;
        let service = services
            .iter()
            .find(|s| s.name() == name)
            .ok_or_else(|| anyhow::anyhow!("未找到服务: {}", name))?
            .clone();

        // 释放读锁后再操作，避免持锁时间过长
        drop(services);

        tracing::info!("正在重启服务: {}", name);

        // 先停止（带超时）
        let stop_result = tokio::time::timeout(
            Duration::from_secs(STOP_TIMEOUT_SECS),
            service.stop(),
        )
        .await;

        match stop_result {
            Ok(Ok(())) => tracing::info!("服务 '{}' 已停止，准备重新启动", name),
            Ok(Err(e)) => tracing::warn!("服务 '{}' 停止时出错: {}，仍将尝试重启", name, e),
            Err(_) => tracing::warn!("服务 '{}' 停止超时，仍将尝试重启", name),
        }

        // 再启动
        service.start().await?;
        tracing::info!("服务 '{}' 重启完成", name);

        Ok(())
    }

    /// 获取指定服务的当前状态。
    /// 找不到时返回 None。
    pub async fn get_status(&self, name: &str) -> Option<ServiceStatus> {
        let services = self.services.read().await;
        services.iter().find(|s| s.name() == name).map(|s| s.status())
    }

    /// 获取所有服务的健康摘要，包含聚合计数和每个服务的详细信息。
    pub async fn get_all_health(&self) -> ServicesSummary {
        let services = self.services.read().await;
        let mut healths = Vec::with_capacity(services.len());
        let (mut running, mut stopped, mut failed) = (0usize, 0usize, 0usize);

        for service in services.iter() {
            let h = service.health();
            match &h.status {
                ServiceStatus::Running => running += 1,
                ServiceStatus::Stopped => stopped += 1,
                ServiceStatus::Failed { .. } => failed += 1,
                _ => {} // Starting / Stopping 不计入固定分类
            }
            healths.push(h);
        }

        ServicesSummary {
            total: healths.len(),
            running,
            stopped,
            failed,
            services: healths,
        }
    }

    /// 获取指定服务的健康信息。
    /// 找不到时返回 None。
    pub async fn get_health(&self, name: &str) -> Option<ServiceHealth> {
        let services = self.services.read().await;
        services.iter().find(|s| s.name() == name).map(|s| s.health())
    }
}

// ─── 单元测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    /// 模拟服务，用于测试 ServiceManager 的各项功能
    struct MockService {
        svc_name: String,
        running: AtomicBool,
        /// 如果设为 true，则 start() 会返回错误
        fail_on_start: bool,
        /// 如果设为 true，则 stop() 会模拟长时间阻塞（超时测试）
        slow_stop: bool,
    }

    impl MockService {
        fn new(name: &str) -> Arc<Self> {
            Arc::new(Self {
                svc_name: name.to_string(),
                running: AtomicBool::new(false),
                fail_on_start: false,
                slow_stop: false,
            })
        }

        fn with_fail_on_start(name: &str) -> Arc<Self> {
            Arc::new(Self {
                svc_name: name.to_string(),
                running: AtomicBool::new(false),
                fail_on_start: true,
                slow_stop: false,
            })
        }

        fn with_slow_stop(name: &str) -> Arc<Self> {
            Arc::new(Self {
                svc_name: name.to_string(),
                running: AtomicBool::new(false),
                fail_on_start: false,
                slow_stop: true,
            })
        }
    }

    #[async_trait]
    impl ManagedService for MockService {
        fn name(&self) -> &str {
            &self.svc_name
        }

        async fn start(&self) -> anyhow::Result<()> {
            if self.fail_on_start {
                return Err(anyhow::anyhow!("模拟启动失败"));
            }
            self.running.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            if self.slow_stop {
                // 模拟超长停止时间（超过 5 秒超时阈值）
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            self.running.store(false, Ordering::SeqCst);
            Ok(())
        }

        fn status(&self) -> ServiceStatus {
            if self.running.load(Ordering::SeqCst) {
                ServiceStatus::Running
            } else {
                ServiceStatus::Stopped
            }
        }

        fn health(&self) -> ServiceHealth {
            ServiceHealth {
                name: self.svc_name.clone(),
                status: self.status(),
                uptime_secs: None,
                last_error: None,
                metrics: serde_json::json!({}),
            }
        }
    }

    #[tokio::test]
    async fn test_register_and_start_all() {
        let manager = ServiceManager::new();
        let svc_a = MockService::new("service-a");
        let svc_b = MockService::new("service-b");

        manager.register(svc_a.clone()).await;
        manager.register(svc_b.clone()).await;

        let results = manager.start_all().await;
        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());

        // 验证服务确实处于运行状态
        assert_eq!(svc_a.status(), ServiceStatus::Running);
        assert_eq!(svc_b.status(), ServiceStatus::Running);
    }

    #[tokio::test]
    async fn test_duplicate_register_is_ignored() {
        let manager = ServiceManager::new();
        let svc = MockService::new("dup");

        manager.register(svc.clone()).await;
        manager.register(svc.clone()).await;

        let summary = manager.get_all_health().await;
        assert_eq!(summary.total, 1);
    }

    #[tokio::test]
    async fn test_start_failure_does_not_block_others() {
        let manager = ServiceManager::new();
        let svc_fail = MockService::with_fail_on_start("fail-svc");
        let svc_ok = MockService::new("ok-svc");

        manager.register(svc_fail.clone()).await;
        manager.register(svc_ok.clone()).await;

        let results = manager.start_all().await;
        assert!(results[0].1.is_err()); // fail-svc 失败
        assert!(results[1].1.is_ok()); // ok-svc 正常
        assert_eq!(svc_ok.status(), ServiceStatus::Running);
    }

    #[tokio::test]
    async fn test_stop_all_reverse_order() {
        let manager = ServiceManager::new();
        let svc_a = MockService::new("a");
        let svc_b = MockService::new("b");

        manager.register(svc_a.clone()).await;
        manager.register(svc_b.clone()).await;
        manager.start_all().await;

        let results = manager.stop_all().await;
        // 验证逆序：第一个停止的应该是 b
        assert_eq!(results[0].0, "b");
        assert_eq!(results[1].0, "a");
        assert_eq!(svc_a.status(), ServiceStatus::Stopped);
        assert_eq!(svc_b.status(), ServiceStatus::Stopped);
    }

    #[tokio::test]
    async fn test_stop_timeout() {
        let manager = ServiceManager::new();
        let slow = MockService::with_slow_stop("slow");

        manager.register(slow.clone()).await;
        manager.start_all().await;

        let start = Instant::now();
        let results = manager.stop_all().await;
        let elapsed = start.elapsed();

        // 应该在大约 5 秒超时后返回错误（而非等待 10 秒）
        assert!(elapsed.as_secs() < 8);
        assert!(results[0].1.is_err());
    }

    #[tokio::test]
    async fn test_restart_service() {
        let manager = ServiceManager::new();
        let svc = MockService::new("restartable");

        manager.register(svc.clone()).await;
        manager.start_all().await;
        assert_eq!(svc.status(), ServiceStatus::Running);

        manager.restart_service("restartable").await.unwrap();
        assert_eq!(svc.status(), ServiceStatus::Running);
    }

    #[tokio::test]
    async fn test_restart_unknown_service() {
        let manager = ServiceManager::new();
        let result = manager.restart_service("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_status_and_health() {
        let manager = ServiceManager::new();
        let svc = MockService::new("status-test");

        manager.register(svc.clone()).await;

        // 启动前
        let status = manager.get_status("status-test").await;
        assert_eq!(status, Some(ServiceStatus::Stopped));

        // 启动后
        manager.start_all().await;
        let status = manager.get_status("status-test").await;
        assert_eq!(status, Some(ServiceStatus::Running));

        // 健康信息
        let health = manager.get_health("status-test").await.unwrap();
        assert_eq!(health.name, "status-test");
        assert_eq!(health.status, ServiceStatus::Running);

        // 不存在的服务
        assert!(manager.get_status("nope").await.is_none());
        assert!(manager.get_health("nope").await.is_none());
    }

    #[tokio::test]
    async fn test_get_all_health_summary() {
        let manager = ServiceManager::new();
        let svc_a = MockService::new("running-svc");
        let svc_b = MockService::new("stopped-svc");

        manager.register(svc_a.clone()).await;
        manager.register(svc_b.clone()).await;

        // 只启动 svc_a
        svc_a.start().await.unwrap();

        let summary = manager.get_all_health().await;
        assert_eq!(summary.total, 2);
        assert_eq!(summary.running, 1);
        assert_eq!(summary.stopped, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.services.len(), 2);
    }
}
