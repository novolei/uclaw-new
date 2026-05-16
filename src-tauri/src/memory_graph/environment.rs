//! Environment Memory — 机器/项目环境上下文
//!
//! 在 workspace 初始化时扫描一次，将机器身份、项目结构、工具配置
//! 写入 memory_nodes (kind=Reference)，供后续召回和上下文注入使用。
//!
//! ## 持久化策略
//! - 每个 space 一个固定 ID: `env:{space_id}`
//! - 使用 upsert（先查后 insert/update），确保幂等
//! - 内容以结构化 Markdown 格式存储（便于 LLM 阅读）


use std::path::Path;
use tracing::{info, warn};

use super::models::*;
use super::store::MemoryGraphStore;

// ─── EnvironmentMemory ───────────────────────────────────────────────

/// 机器 + 项目环境信息
#[derive(Debug, Clone)]
pub struct EnvironmentMemory {
    /// 机器身份：OS、hostname、shell、CPU/内存
    pub machine_identity: String,
    /// 项目结构概览：主要语言、框架、目录树摘要
    pub project_structure: String,
    /// 已检测到的工具链：版本信息
    pub tool_configs: String,
}

impl EnvironmentMemory {
    /// 扫描当前环境，收集三类信息
    pub fn scan(workspace_root: &Path) -> Self {
        let machine = Self::scan_machine_identity();
        let project = Self::scan_project_structure(workspace_root);
        let tools = Self::scan_tool_configs();
        Self {
            machine_identity: machine,
            project_structure: project,
            tool_configs: tools,
        }
    }

    /// 格式化为 LLM 可读的 Markdown 片段
    pub fn to_markdown(&self) -> String {
        let mut md = String::from("## 环境信息 (Environment)\n\n");
        md.push_str("### 机器身份\n");
        md.push_str(&self.machine_identity);
        md.push_str("\n\n### 项目结构\n");
        md.push_str(&self.project_structure);
        md.push_str("\n\n### 工具链\n");
        md.push_str(&self.tool_configs);
        md.push('\n');
        md
    }

    // ── 私有扫描方法 ──────────────────────────────────────────────

    fn scan_machine_identity() -> String {
        let hostname = std::env::var("HOSTNAME")
            .unwrap_or_else(|_| "unknown".to_string());

        let os = format!(
            "{} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string());

        let home = dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~".to_string());

        // CPU 核心数
        let cpu_count = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);

        // 内存（macOS 通过 sysctl，其他平台用 /proc/meminfo）
        let mem_gb = Self::detect_memory_gb();

        format!(
            "- **OS**: {os}\n\
             - **Hostname**: {hostname}\n\
             - **Shell**: {shell}\n\
             - **Home**: {home}\n\
             - **CPU Cores**: {cpu_count}\n\
             - **Memory**: ~{mem_gb} GB"
        )
    }

    fn scan_project_structure(workspace_root: &Path) -> String {
        if !workspace_root.exists() {
            return "- *(项目目录不存在)*".to_string();
        }

        let mut lines: Vec<String> = Vec::new();

        // 检测包管理文件 → 推断语言/框架
        let detectors: &[(&str, &str)] = &[
            ("package.json", "Node.js / TypeScript"),
            ("Cargo.toml", "Rust"),
            ("go.mod", "Go"),
            ("requirements.txt", "Python (pip)"),
            ("pyproject.toml", "Python (modern)"),
            ("Pipfile", "Python (pipenv)"),
            ("Gemfile", "Ruby"),
            ("CMakeLists.txt", "C/C++ (CMake)"),
            ("Makefile", "C/C++ (Make)"),
            ("pom.xml", "Java (Maven)"),
            ("build.gradle", "Java (Gradle)"),
            ("build.gradle.kts", "Java/Kotlin (Gradle)"),
            ("composer.json", "PHP"),
            ("mix.exs", "Elixir"),
            ("stack.yaml", "Haskell"),
            ("pubspec.yaml", "Dart/Flutter"),
            ("Dockerfile", "Docker"),
            ("docker-compose.yml", "Docker Compose"),
            (".git", "Git repository"),
        ];

        let mut found: Vec<&str> = Vec::new();
        for (file, label) in detectors {
            if workspace_root.join(file).exists() {
                found.push(label);
            }
        }

        if found.is_empty() {
            lines.push("- *(未检测到已知项目类型)*".to_string());
        } else {
            for label in &found {
                lines.push(format!("- {}", label));
            }
        }

        // 项目路径
        let path_str = workspace_root.display().to_string();
        // 截断过长的路径
        let short_path = if path_str.len() > 80 {
            format!("...{}", &path_str[path_str.len().saturating_sub(77)..])
        } else {
            path_str
        };
        lines.insert(0, format!("- **Workspace**: {short_path}"));

        lines.join("\n")
    }

    fn scan_tool_configs() -> String {
        let mut lines: Vec<String> = Vec::new();

        // 检测常见 CLI 工具版本
        let tools_to_check: &[(&str, &[&str])] = &[
            ("git", &["--version"]),
            ("node", &["--version"]),
            ("npm", &["--version"]),
            ("pnpm", &["--version"]),
            ("yarn", &["--version"]),
            ("rustc", &["--version"]),
            ("cargo", &["--version"]),
            ("python3", &["--version"]),
            ("python", &["--version"]),
            ("go", &["version"]),
            ("docker", &["--version"]),
            ("gcc", &["--version"]),
            ("make", &["--version"]),
            ("cmake", &["--version"]),
        ];

        for (tool, args) in tools_to_check {
            match std::process::Command::new(tool).args(*args).output() {
                Ok(output) => {
                    let version = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let version_short = if version.len() > 100 {
                        format!("{}...", &version[..97])
                    } else {
                        version
                    };
                    if !version_short.is_empty() {
                        lines.push(format!("- **{}**: {}", tool, version_short));
                    }
                }
                Err(_) => {
                    // 工具未安装，静默跳过
                }
            }
        }

        if lines.is_empty() {
            "- *(未检测到已安装开发工具)*".to_string()
        } else {
            lines.join("\n")
        }
    }

    fn detect_memory_gb() -> u64 {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    if let Ok(bytes) = s.trim().parse::<u64>() {
                        return bytes / (1024 * 1024 * 1024);
                    }
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        let kb: u64 = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        return kb / (1024 * 1024);
                    }
                }
            }
        }
        8 // fallback
    }
}

// ─── 持久化 ─────────────────────────────────────────────────────────

/// 为指定 space 持久化环境信息到 memory_nodes。
///
/// 使用稳定 ID `env:{space_id}`，如果已存在则更新内容。
/// 失败时静默 warn，不阻塞启动流程。
pub fn persist_environment(
    store: &MemoryGraphStore,
    space_id: &str,
    workspace_root: &Path,
) {
    let env = EnvironmentMemory::scan(workspace_root);
    let content = env.to_markdown();
    let now = chrono::Utc::now().to_rfc3339();
    let node_id = format!("env:{}", space_id);

    // 检查是否已存在
    let exists = match store.get_node(&node_id) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => {
            warn!(error = %e, "environment: failed to check existing node");
            return;
        }
    };

    if exists {
        // 更新 metadata 中的 content 字段
        let metadata = serde_json::json!({
            "content": content,
            "machine_identity": env.machine_identity,
            "project_structure": env.project_structure,
            "tool_configs": env.tool_configs,
            "last_scanned_at": now,
        });
        if let Err(e) = store.update_node(&node_id, None, None, Some(&metadata)) {
            warn!(error = %e, "environment: failed to update node {}", node_id);
        } else {
            info!(node_id = %node_id, "environment: updated");
        }
    } else {
        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Reference,
            title: "环境信息".to_string(),
            metadata: Some(serde_json::json!({
                "content": content,
                "machine_identity": env.machine_identity,
                "project_structure": env.project_structure,
                "tool_configs": env.tool_configs,
                "last_scanned_at": now,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        if let Err(e) = store.create_node(&node) {
            warn!(error = %e, "environment: failed to create node {}", node_id);
        } else {
            info!(node_id = %node_id, "environment: persisted");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_does_not_panic() {
        let env = EnvironmentMemory::scan(Path::new("."));
        assert!(!env.machine_identity.is_empty());
        assert!(!env.project_structure.is_empty());
        // tool_configs may be empty in minimal test environments
    }

    #[test]
    fn test_to_markdown_includes_sections() {
        let env = EnvironmentMemory {
            machine_identity: "- OS: test".to_string(),
            project_structure: "- Rust project".to_string(),
            tool_configs: "- git: 2.40".to_string(),
        };
        let md = env.to_markdown();
        assert!(md.contains("环境信息"));
        assert!(md.contains("机器身份"));
        assert!(md.contains("项目结构"));
        assert!(md.contains("工具链"));
        assert!(md.contains("OS: test"));
        assert!(md.contains("Rust project"));
        assert!(md.contains("git: 2.40"));
    }
}
