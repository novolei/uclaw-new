use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use std::collections::HashMap;
use std::time::Duration;

/// Default timeout for JSON-RPC requests (30 seconds).
/// Suitable for fast operations: retrieve, list, delete, get_*.
/// LLM-backed operations (memorize*) override this via
/// `send_request_with_timeout` because end-to-end LLM extraction of
/// many items can legitimately take a minute.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for LLM-backed memorize operations (2 minutes).
/// MemU's memorize pipeline runs an LLM extraction over the input that
/// can yield 8-10+ memory items per call, each requiring a model
/// invocation. 30s default produces "Request timed out" + orphan
/// "Received response for unknown id" warnings — both indicate the
/// Python side completed slightly after the Rust side gave up.
pub const MEMORIZE_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum number of automatic restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 3;

/// Delay between restart attempts.
const RESTART_DELAY: Duration = Duration::from_secs(2);

/// Error type for MemU bridge operations.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Python subprocess not running")]
    NotRunning,

    #[error("Failed to start Python subprocess: {0}")]
    StartFailed(String),

    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    #[error("Subprocess communication error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Python error: {0}")]
    PythonError(String),

    #[error("Bridge shutting down")]
    ShuttingDown,

    #[error("Request cancelled: subprocess restarted")]
    RequestCancelled,
}

/// A pending request waiting for a response from the Python subprocess.
struct PendingRequest {
    tx: oneshot::Sender<Result<serde_json::Value, BridgeError>>,
}

/// Internal state shared between the bridge and the response reader task.
struct BridgeInner {
    child: Option<Child>,
    stdin: Option<tokio::process::ChildStdin>,
    pending: HashMap<u64, PendingRequest>,
}

/// Manages the lifecycle of a memU Python subprocess.
///
/// Communication uses a JSON-RPC style protocol over stdio:
/// - Requests are written to the child's stdin as single-line JSON
/// - Responses are read from the child's stdout as single-line JSON
pub struct MemUBridge {
    inner: Arc<Mutex<BridgeInner>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    python_path: String,
    script_path: PathBuf,
    data_dir: PathBuf,
    llm_env: Vec<(String, String)>,
    shutdown: AtomicBool,
    /// Prevent concurrent restart attempts.
    /// When one caller is restarting the subprocess, other callers
    /// must wait — otherwise the second spawn_subprocess overwrites
    /// inner.child (which holds kill_on_drop) and kills the first
    /// freshly-started process.
    restart_lock: Mutex<()>,
}

impl MemUBridge {
    /// Create a new MemUBridge.
    ///
    /// # Arguments
    /// * `python_path` - Path to the Python 3.13+ interpreter (e.g. "python3")
    /// * `script_path` - Path to the `memu_bridge.py` script
    /// * `data_dir` - uClaw data directory (e.g. `~/.uclaw`)
    pub fn new(python_path: impl Into<String>, script_path: impl Into<PathBuf>, data_dir: impl Into<PathBuf>, llm_env: Vec<(String, String)>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BridgeInner {
                child: None,
                stdin: None,
                pending: HashMap::new(),
            })),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(false)),
            python_path: python_path.into(),
            script_path: script_path.into(),
            data_dir: data_dir.into(),
            llm_env,
            shutdown: AtomicBool::new(false),
            restart_lock: Mutex::new(()),
        }
    }

    /// Start the memU Python subprocess.
    ///
    /// If already running, this is a no-op.
    pub async fn start(&self) -> Result<(), BridgeError> {
        if self.alive.load(Ordering::SeqCst) {
            tracing::debug!("MemU bridge already running");
            return Ok(());
        }

        self.spawn_subprocess().await
    }

    /// Spawn (or respawn) the Python subprocess.
    ///
    /// IMPORTANT: Caller must hold `restart_lock` to prevent concurrent
    /// spawns from overwriting `inner.child` and triggering kill_on_drop
    /// on a freshly-started process.
    async fn spawn_subprocess(&self) -> Result<(), BridgeError> {
        let db_path = self.data_dir.join("memory").join("memu.db");

        // Ensure the memory directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        tracing::info!(
            python = %self.python_path,
            script = %self.script_path.display(),
            db_path = %db_path.display(),
            "Starting memU Python subprocess"
        );

        let mut cmd = Command::new(&self.python_path);
        cmd.arg(&self.script_path)
            .env("MEMU_DB_PATH", db_path.to_string_lossy().as_ref())
            .env("MEMU_DATA_DIR", self.data_dir.to_string_lossy().as_ref());

        // 传递 LLM 配置环境变量
        for (key, value) in &self.llm_env {
            if !value.is_empty() {
                cmd.env(key, value);
            }
        }

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| BridgeError::StartFailed(format!("Failed to spawn python: {e}")))?;

        let stdout = child.stdout.take()
            .ok_or_else(|| BridgeError::StartFailed("Failed to capture stdout".into()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| BridgeError::StartFailed("Failed to capture stderr".into()))?;
        let stdin = child.stdin.take()
            .ok_or_else(|| BridgeError::StartFailed("Failed to capture stdin".into()))?;

        // Before storing the new child, explicitly drop any previous child
        // to prevent accidental kill_on_drop from a race.
        // (The restart_lock in the caller should prevent this, but this
        // is defense-in-depth.)
        let mut inner = self.inner.lock().await;
        if let Some(mut old_child) = inner.child.take() {
            drop(old_child); // kill_on_drop triggers here
            // Give the OS a moment to release resources
        }
        inner.child = Some(child);
        inner.stdin = Some(stdin);
        drop(inner);

        self.alive.store(true, Ordering::SeqCst);

        // Spawn stdout reader task
        let inner_clone = Arc::clone(&self.inner);
        let alive_flag = Arc::clone(&self.alive);
        tokio::spawn(async move {
            Self::read_responses(inner_clone, stdout, alive_flag).await;
        });

        // Spawn stderr reader task (for logging)
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "memu_bridge::stderr", "{}", line);
            }
        });

        // Brief delay to detect immediate startup crashes (e.g. missing deps).
        // If the process exits within 500ms, report a clear error instead of
        // letting subsequent requests fail with cryptic "RequestCancelled".
        tokio::time::sleep(Duration::from_millis(500)).await;
        if !self.alive.load(Ordering::SeqCst) {
            return Err(BridgeError::StartFailed(
                "Python subprocess exited immediately after spawn \
                 (likely missing dependencies — check stderr logs)"
                    .to_string(),
            ));
        }

        tracing::info!("memU Python subprocess started successfully");
        Ok(())
    }

    /// Background task that reads JSON responses from the subprocess stdout.
    async fn read_responses(
        inner: Arc<Mutex<BridgeInner>>,
        stdout: tokio::process::ChildStdout,
        alive: Arc<AtomicBool>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }

                    // Parse the JSON response
                    let response: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!("Failed to parse response JSON: {e}, line: {line}");
                            continue;
                        }
                    };

                    // Extract the request ID
                    let id = match response.get("id").and_then(|v| v.as_u64()) {
                        Some(id) => id,
                        None => {
                            tracing::error!("Response missing 'id' field: {line}");
                            continue;
                        }
                    };

                    // Resolve the pending request
                    let mut guard = inner.lock().await;
                    if let Some(pending) = guard.pending.remove(&id) {
                        let result = if let Some(error) = response.get("error") {
                            let msg = error
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown Python error")
                                .to_string();
                            Err(BridgeError::PythonError(msg))
                        } else if let Some(result) = response.get("result") {
                            Ok(result.clone())
                        } else {
                            Ok(serde_json::Value::Null)
                        };

                        let _ = pending.tx.send(result);
                    } else {
                        // Late response (caller already timed out and removed
                        // the pending entry) or duplicate. Common with the
                        // 30s default timeout vs slow memorize LLM calls.
                        // Demoted to debug — not actionable, just noise.
                        tracing::debug!(id, "memU: response for unknown/timed-out request id");
                    }
                }
                Ok(None) => {
                    // EOF — subprocess has closed stdout
                    tracing::warn!("memU subprocess stdout closed (EOF)");
                    alive.store(false, Ordering::SeqCst);

                    // Cancel all pending requests
                    let mut guard = inner.lock().await;
                    for (_, pending) in guard.pending.drain() {
                        let _ = pending.tx.send(Err(BridgeError::RequestCancelled));
                    }
                    break;
                }
                Err(e) => {
                    tracing::error!("Error reading from memU subprocess: {e}");
                    alive.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    }

    /// Stop the Python subprocess gracefully.
    pub async fn stop(&self) -> Result<(), BridgeError> {
        self.shutdown.store(true, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);

        let mut inner = self.inner.lock().await;

        // Close stdin to signal the subprocess to exit
        inner.stdin.take();

        // Cancel all pending requests
        for (_, pending) in inner.pending.drain() {
            let _ = pending.tx.send(Err(BridgeError::ShuttingDown));
        }

        // Wait briefly for graceful exit, then kill
        if let Some(mut child) = inner.child.take() {
            let kill_timeout = Duration::from_secs(5);
            match tokio::time::timeout(kill_timeout, child.wait()).await {
                Ok(Ok(status)) => {
                    tracing::info!("memU subprocess exited with status: {status}");
                }
                Ok(Err(e)) => {
                    tracing::warn!("Error waiting for memU subprocess: {e}");
                }
                Err(_) => {
                    tracing::warn!("memU subprocess did not exit in time, killing");
                    let _ = child.kill().await;
                }
            }
        }

        tracing::info!("memU bridge stopped");
        Ok(())
    }

    /// Check if the subprocess is alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Send a JSON-RPC style request to the Python subprocess and await the response.
    ///
    /// # Arguments
    /// * `method` - The method name (e.g. "memorize", "retrieve")
    /// * `params` - The method parameters as a JSON value
    pub async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, BridgeError> {
        self.send_request_with_timeout(method, params, DEFAULT_TIMEOUT).await
    }

    /// Send a request with a custom timeout.
    pub async fn send_request_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, BridgeError> {
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(BridgeError::ShuttingDown);
        }

        // Auto-restart if not alive.
        // Use a restart lock to prevent concurrent try_restart calls from
        // overwriting inner.child (kill_on_drop → kills freshly-started process).
        if !self.alive.load(Ordering::SeqCst) {
            let _restart_guard = self.restart_lock.lock().await;
            // Re-check: another caller may have completed a restart while
            // we waited for the lock.
            if !self.alive.load(Ordering::SeqCst) {
                tracing::info!("memU subprocess not running, attempting restart...");
                self.try_restart().await?;
            }
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        let mut request_line = serde_json::to_string(&request)?;
        request_line.push('\n');

        // Register the pending request
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.lock().await;
            inner.pending.insert(id, PendingRequest { tx });
        }

        // Write to stdin (separate lock scope to avoid borrow conflicts)
        {
            let mut inner = self.inner.lock().await;
            let has_stdin = inner.stdin.is_some();
            if !has_stdin {
                inner.pending.remove(&id);
                return Err(BridgeError::NotRunning);
            }
            let stdin = inner.stdin.as_mut().unwrap();
            if let Err(e) = stdin.write_all(request_line.as_bytes()).await {
                inner.pending.remove(&id);
                return Err(BridgeError::IoError(e));
            }
            if let Err(e) = stdin.flush().await {
                inner.pending.remove(&id);
                return Err(BridgeError::IoError(e));
            }
        }

        // Wait for response with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BridgeError::RequestCancelled),
            Err(_) => {
                // Timeout — remove the pending request
                let mut inner = self.inner.lock().await;
                inner.pending.remove(&id);
                Err(BridgeError::Timeout(timeout))
            }
        }
    }

    /// Attempt to restart the subprocess with exponential backoff.
    async fn try_restart(&self) -> Result<(), BridgeError> {
        for attempt in 1..=MAX_RESTART_ATTEMPTS {
            tracing::info!("memU restart attempt {attempt}/{MAX_RESTART_ATTEMPTS}");

            // Clean up old process
            {
                let mut inner = self.inner.lock().await;
                inner.stdin.take();
                if let Some(mut child) = inner.child.take() {
                    let _ = child.kill().await;
                }
            }

            tokio::time::sleep(RESTART_DELAY).await;

            match self.spawn_subprocess().await {
                Ok(()) => {
                    tracing::info!("memU subprocess restarted successfully on attempt {attempt}");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!("memU restart attempt {attempt} failed: {e}");
                    if attempt == MAX_RESTART_ATTEMPTS {
                        return Err(e);
                    }
                }
            }
        }

        Err(BridgeError::StartFailed("Max restart attempts exceeded".into()))
    }
}

impl Drop for MemUBridge {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.alive.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_new() {
        let bridge = MemUBridge::new("python3", "/tmp/test.py", "/tmp/data", vec![]);
        assert!(!bridge.is_alive());
    }
}
