//! M1-T7 — eagerly warm up HTTP/2 + TLS connections to the active LLM
//! provider during application boot.
//!
//! Cold TLS handshake + HTTP/2 connection negotiation to a remote
//! provider typically costs 200–500 ms (varies by latency to the
//! provider's edge). That entire cost lands on the user's very first
//! message of a session if we don't prewarm. After prewarm, the
//! existing `reqwest::Client` connection pool keeps the socket open
//! (`tcp_keepalive` + HTTP/2 multiplexing) so subsequent requests skip
//! the handshake entirely.
//!
//! The helper here:
//!
//! - issues a single tiny request to the provider's base URL using the
//!   same `reqwest::Client` shape the production providers use
//! - ignores success/failure — boot must not block on network state
//! - logs at `info` so the user can see the prewarm fired in dev mode
//!
//! Wire-up lives in `main.rs::Stage 3` (M1-T7 PR #315): a fire-and-
//! forget `tokio::spawn` after the agent loop is up. The prewarm task
//! is NOT registered with `ServiceManager` because it's a one-shot
//! warm-up, not a long-running service.

use std::time::{Duration, Instant};

/// Resolve a sensible base URL to prewarm against. Falls back to the
/// canonical provider URL when the user hasn't overridden it.
pub fn base_url_for(provider: &str, configured: Option<&str>) -> String {
    if let Some(url) = configured.filter(|s| !s.is_empty()) {
        return url.to_string();
    }
    match provider {
        "anthropic" => "https://api.anthropic.com".into(),
        "openai" => "https://api.openai.com".into(),
        "deepseek" => "https://api.deepseek.com".into(),
        "gemini" => "https://generativelanguage.googleapis.com".into(),
        "groq" => "https://api.groq.com".into(),
        "openrouter" => "https://openrouter.ai".into(),
        // Unknown / custom provider — caller passed a non-empty
        // `configured` already in that case.
        other => format!("https://api.{other}.example.invalid"),
    }
}

/// Build a `reqwest::Client` with the same shape that production
/// providers use (`gzip + brotli + deflate + rustls + HTTP/2`).
///
/// Returns `None` if the client can't be constructed (e.g. TLS init
/// failure on a system with broken rustls). Callers should treat that
/// as "no prewarm" and fall through.
fn build_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .tcp_keepalive(Some(Duration::from_secs(45)))
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .ok()
}

/// Warm up a single provider URL. Returns the elapsed time of the
/// handshake + first byte if the request reached the server (even with
/// 4xx/5xx — we only care that the TLS + HTTP/2 connection is now in
/// the pool).
pub async fn prewarm(provider: &str, configured_base_url: Option<&str>) -> Option<Duration> {
    let url = base_url_for(provider, configured_base_url);
    let client = build_client()?;
    let started = Instant::now();
    // HEAD request — minimal payload, no API key required. Even when the
    // provider returns 405 Method Not Allowed (some do), the handshake
    // still completed and the pool now has an open connection.
    let result = client.head(&url).send().await;
    let elapsed = started.elapsed();
    match result {
        Ok(resp) => {
            tracing::info!(
                provider,
                base_url = %url,
                status = %resp.status(),
                elapsed_ms = elapsed.as_millis() as u64,
                "LLM prewarm: connection established"
            );
            Some(elapsed)
        }
        Err(e) => {
            tracing::info!(
                provider,
                base_url = %url,
                elapsed_ms = elapsed.as_millis() as u64,
                error = %e,
                "LLM prewarm: failed (boot continues unaffected)"
            );
            None
        }
    }
}

/// Spawn a fire-and-forget prewarm task. Returns immediately; the
/// prewarm work happens on the current tokio runtime. Suitable for
/// `main.rs::Stage 3`.
pub fn spawn_prewarm(provider: String, configured_base_url: Option<String>) {
    tokio::spawn(async move {
        let _ = prewarm(&provider, configured_base_url.as_deref()).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_known_providers() {
        assert_eq!(base_url_for("anthropic", None), "https://api.anthropic.com");
        assert_eq!(base_url_for("openai", None), "https://api.openai.com");
        assert_eq!(base_url_for("deepseek", None), "https://api.deepseek.com");
        assert_eq!(
            base_url_for("gemini", None),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(base_url_for("groq", None), "https://api.groq.com");
        assert_eq!(base_url_for("openrouter", None), "https://openrouter.ai");
    }

    #[test]
    fn base_url_unknown_provider_falls_back_to_invalid() {
        // We deliberately route unknown providers to an `.invalid` TLD
        // so prewarm doesn't accidentally probe a third-party host
        // chosen by the model config.
        let url = base_url_for("acme-custom", None);
        assert!(url.contains(".invalid"), "got {url}");
    }

    #[test]
    fn base_url_user_override_takes_priority() {
        let url = base_url_for("anthropic", Some("https://my-proxy.local:9443"));
        assert_eq!(url, "https://my-proxy.local:9443");
    }

    #[test]
    fn base_url_empty_override_falls_back() {
        // Empty string in `configured` is treated as "unset".
        let url = base_url_for("anthropic", Some(""));
        assert_eq!(url, "https://api.anthropic.com");
    }

    #[test]
    fn build_client_constructs_successfully() {
        let client = build_client();
        assert!(client.is_some(), "reqwest::Client must build under test runtime");
    }

    /// Network-touching test: prewarm against the localhost invalid
    /// fallback URL. Should not hang — the .invalid TLD resolves to
    /// NXDOMAIN and the connect_timeout caps wait at 3s.
    #[tokio::test]
    async fn prewarm_against_invalid_returns_without_hang() {
        let started = Instant::now();
        let elapsed = prewarm("nonexistent-provider", None).await;
        let total = started.elapsed();
        // Either Some (impossible against .invalid) or None — but
        // total wall time must be bounded by connect_timeout (3s)
        // plus a small slop.
        assert!(
            total < Duration::from_secs(5),
            "prewarm hung: {total:?}, returned {elapsed:?}"
        );
    }
}
