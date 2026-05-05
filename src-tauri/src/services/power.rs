use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use super::{ManagedService, ServiceHealth, ServiceStatus};

// ─── PowerService ─────────────────────────────────────────────────────

/// 防休眠服务
///
/// 阻止系统进入空闲休眠，确保 uClaw 24/7 持续运行。
///
/// 平台实现策略：
/// - **macOS**: 启动内置 `caffeinate -di` 子进程（阻止 idle sleep + display sleep）
/// - **Windows**: 调用 `SetThreadExecutionState` （当前为 stub，后续补充）
/// - **Linux**: 暂不支持，仅打印日志
pub struct PowerService {
    /// 当前是否正在阻止休眠
    is_active: AtomicBool,
    /// 防休眠原因描述
    reason: Mutex<String>,
    /// macOS: 存储 caffeinate 子进程，用于后续终止
    #[cfg(target_os = "macos")]
    child: Mutex<Option<std::process::Child>>,
}

impl PowerService {
    /// 创建 PowerService 实例（初始状态：允许休眠）
    pub fn new() -> Self {
        Self {
            is_active: AtomicBool::new(false),
            reason: Mutex::new(String::new()),
            #[cfg(target_os = "macos")]
            child: Mutex::new(None),
        }
    }

    /// 阻止系统休眠
    ///
    /// 在不同平台上执行对应的防休眠操作，并将状态标记为 active。
    pub fn prevent_sleep(&self, reason: &str) -> anyhow::Result<()> {
        // 如果已经在阻止休眠，先释放再重新获取
        if self.is_active.load(Ordering::SeqCst) {
            tracing::debug!("[PowerService] 已处于防休眠状态，先释放再重新获取");
            let _ = self.allow_sleep();
        }

        #[cfg(target_os = "macos")]
        {
            self.prevent_sleep_macos(reason)?;
        }

        #[cfg(target_os = "windows")]
        {
            self.prevent_sleep_windows()?;
        }

        #[cfg(target_os = "linux")]
        {
            tracing::info!("[PowerService] Linux 防休眠暂不支持，跳过");
        }

        // 更新状态
        if let Ok(mut r) = self.reason.lock() {
            *r = reason.to_string();
        }
        self.is_active.store(true, Ordering::SeqCst);
        tracing::info!("[PowerService] 已阻止系统休眠: {}", reason);
        Ok(())
    }

    /// 允许系统休眠
    ///
    /// 释放平台层面的防休眠锁/子进程，并将状态标记为 inactive。
    pub fn allow_sleep(&self) -> anyhow::Result<()> {
        if !self.is_active.load(Ordering::SeqCst) {
            tracing::debug!("[PowerService] 当前未处于防休眠状态，无需操作");
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            self.allow_sleep_macos()?;
        }

        #[cfg(target_os = "windows")]
        {
            self.allow_sleep_windows()?;
        }

        self.is_active.store(false, Ordering::SeqCst);
        tracing::info!("[PowerService] 已允许系统休眠");
        Ok(())
    }

    /// 查询当前是否正在阻止系统休眠
    pub fn is_preventing_sleep(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    // ===== macOS 实现：使用 caffeinate 子进程 =====

    /// macOS: 启动 `caffeinate -di` 阻止 idle sleep 和 display sleep
    ///
    /// caffeinate 是 macOS 内置工具，无需额外依赖，最简单可靠。
    /// - `-d`: 阻止显示器休眠（prevent the display from sleeping）
    /// - `-i`: 阻止系统空闲休眠（prevent the system from idle sleeping）
    #[cfg(target_os = "macos")]
    fn prevent_sleep_macos(&self, reason: &str) -> anyhow::Result<()> {
        use std::process::Command;

        let child = Command::new("caffeinate")
            .args(["-di"])
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!("启动 caffeinate 失败: {e}")
            })?;

        let pid = child.id();
        tracing::info!(
            "[PowerService] macOS caffeinate 已启动 (PID: {}, 原因: {})",
            pid,
            reason,
        );

        if let Ok(mut guard) = self.child.lock() {
            *guard = Some(child);
        }
        Ok(())
    }

    /// macOS: 终止 caffeinate 子进程，恢复系统正常休眠行为
    #[cfg(target_os = "macos")]
    fn allow_sleep_macos(&self) -> anyhow::Result<()> {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let pid = child.id();
                match child.kill() {
                    Ok(()) => {
                        // 等待子进程退出，防止成为僵尸进程
                        let _ = child.wait();
                        tracing::info!(
                            "[PowerService] macOS caffeinate 已终止 (PID: {})",
                            pid,
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[PowerService] 终止 caffeinate (PID: {}) 失败: {}",
                            pid,
                            e,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    // ===== Windows 实现（stub） =====

    /// Windows: 调用 SetThreadExecutionState 阻止休眠
    ///
    /// TODO: 使用 winapi 或 windows crate 实现
    /// SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
    #[cfg(target_os = "windows")]
    fn prevent_sleep_windows(&self) -> anyhow::Result<()> {
        tracing::warn!("[PowerService] Windows 防休眠尚未实现，仅记录日志");
        Ok(())
    }

    /// Windows: 恢复正常休眠行为
    ///
    /// TODO: SetThreadExecutionState(ES_CONTINUOUS)
    #[cfg(target_os = "windows")]
    fn allow_sleep_windows(&self) -> anyhow::Result<()> {
        tracing::warn!("[PowerService] Windows 恢复休眠尚未实现，仅记录日志");
        Ok(())
    }
}

// ─── Drop: 确保进程退出时释放防休眠锁 ──────────────────────────────────

impl Drop for PowerService {
    fn drop(&mut self) {
        if self.is_active.load(Ordering::SeqCst) {
            tracing::info!("[PowerService] Drop: 正在释放防休眠锁...");
            if let Err(e) = self.allow_sleep() {
                tracing::error!("[PowerService] Drop: 释放防休眠锁失败: {}", e);
            }
        }
    }
}

// ─── ManagedService trait 实现 ────────────────────────────────────────

#[async_trait]
impl ManagedService for PowerService {
    fn name(&self) -> &str {
        "power"
    }

    /// 启动服务：阻止系统休眠
    async fn start(&self) -> anyhow::Result<()> {
        self.prevent_sleep("uClaw 24/7 主动记忆代理")
    }

    /// 停止服务：允许系统休眠
    async fn stop(&self) -> anyhow::Result<()> {
        self.allow_sleep()
    }

    fn status(&self) -> ServiceStatus {
        if self.is_preventing_sleep() {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    fn health(&self) -> ServiceHealth {
        let reason = self
            .reason
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default();

        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None,
            last_error: None,
            metrics: serde_json::json!({
                "preventing_sleep": self.is_preventing_sleep(),
                "reason": reason,
            }),
        }
    }
}

// ─── 单元测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_service_is_inactive() {
        let svc = PowerService::new();
        assert!(!svc.is_preventing_sleep());
        assert_eq!(svc.status(), ServiceStatus::Stopped);
    }

    #[test]
    fn test_health_when_stopped() {
        let svc = PowerService::new();
        let health = svc.health();
        assert_eq!(health.name, "power");
        assert_eq!(health.status, ServiceStatus::Stopped);
        assert_eq!(health.metrics["preventing_sleep"], false);
    }

    /// macOS 专属测试：验证 caffeinate 子进程的启动与终止
    #[cfg(target_os = "macos")]
    #[test]
    fn test_prevent_and_allow_sleep_macos() {
        let svc = PowerService::new();

        // 阻止休眠
        svc.prevent_sleep("测试防休眠").unwrap();
        assert!(svc.is_preventing_sleep());
        assert_eq!(svc.status(), ServiceStatus::Running);

        // 验证 caffeinate 子进程存在
        {
            let guard = svc.child.lock().unwrap();
            assert!(guard.is_some());
        }

        // 允许休眠
        svc.allow_sleep().unwrap();
        assert!(!svc.is_preventing_sleep());
        assert_eq!(svc.status(), ServiceStatus::Stopped);

        // 验证子进程已被清理
        {
            let guard = svc.child.lock().unwrap();
            assert!(guard.is_none());
        }
    }

    /// 测试重复调用 allow_sleep 不会出错
    #[test]
    fn test_allow_sleep_when_not_active() {
        let svc = PowerService::new();
        // 未激活时调用 allow_sleep 应直接返回 Ok
        assert!(svc.allow_sleep().is_ok());
    }

    /// macOS: 测试 Drop 时自动释放
    #[cfg(target_os = "macos")]
    #[test]
    fn test_drop_cleans_up() {
        let svc = PowerService::new();
        svc.prevent_sleep("Drop 测试").unwrap();
        assert!(svc.is_preventing_sleep());
        // svc 在此处 drop，应自动终止 caffeinate
        drop(svc);
        // 无法直接验证进程已终止，但确保不会 panic
    }

    /// 测试 ManagedService trait 的 name()
    #[test]
    fn test_service_name() {
        let svc = PowerService::new();
        assert_eq!(svc.name(), "power");
    }
}
