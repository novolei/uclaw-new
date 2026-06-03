// SPDX-License-Identifier: Apache-2.0
//! Source probing: measure host reachability + first-byte latency with a
//! lightweight ranged GET (first few KB), then rank hosts fastest-first.
//! The probe is behind a trait so ranking is unit-testable with injected
//! latencies (no network).

use std::time::Duration;

use async_trait::async_trait;

use super::manifest::Host;

/// Outcome of probing one host.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub host: Host,
    /// `None` = unreachable / errored; `Some(latency)` = first bytes received.
    pub latency: Option<Duration>,
}

/// Abstracts the network probe so `rank_hosts` can be tested deterministically.
#[async_trait]
pub trait SourceProbe: Send + Sync {
    /// Probe one URL representing a host; return measured first-byte latency.
    async fn probe(&self, host: Host, url: &str) -> ProbeResult;
}

/// Rank probe results fastest-first; unreachable hosts (latency `None`) drop
/// to the end in their original relative order. Pure — unit-testable.
pub fn rank_hosts(mut results: Vec<ProbeResult>) -> Vec<Host> {
    results.sort_by(|a, b| match (a.latency, b.latency) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    results.into_iter().map(|r| r.host).collect()
}

/// Real HTTP probe: a ranged GET for the first `PROBE_BYTES` bytes, timing
/// how long until the response starts. Uses a short timeout so a dead mirror
/// fails fast.
pub struct HttpProbe {
    client: reqwest::Client,
}

const PROBE_BYTES: u64 = 16 * 1024;
const PROBE_TIMEOUT_SECS: u64 = 6;

impl HttpProbe {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("uclaw-backend/model-probe")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for HttpProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SourceProbe for HttpProbe {
    async fn probe(&self, host: Host, url: &str) -> ProbeResult {
        let start = std::time::Instant::now();
        let req = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes=0-{}", PROBE_BYTES - 1));
        match req.send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 206 => {
                ProbeResult { host, latency: Some(start.elapsed()) }
            }
            _ => ProbeResult { host, latency: None },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(host: Host, ms: Option<u64>) -> ProbeResult {
        ProbeResult { host, latency: ms.map(Duration::from_millis) }
    }

    #[test]
    fn ranks_fastest_first() {
        let ranked = rank_hosts(vec![
            r(Host::HuggingFace, Some(800)),
            r(Host::ModelScope, Some(120)),
            r(Host::HfMirror, Some(300)),
        ]);
        assert_eq!(ranked, vec![Host::ModelScope, Host::HfMirror, Host::HuggingFace]);
    }

    #[test]
    fn unreachable_hosts_sink_to_end() {
        let ranked = rank_hosts(vec![
            r(Host::HuggingFace, None),
            r(Host::ModelScope, Some(120)),
            r(Host::HfMirror, None),
        ]);
        assert_eq!(ranked[0], Host::ModelScope);
        assert!(ranked[1..].contains(&Host::HuggingFace));
        assert!(ranked[1..].contains(&Host::HfMirror));
    }

    #[test]
    fn all_unreachable_preserves_all() {
        let ranked = rank_hosts(vec![r(Host::HuggingFace, None), r(Host::ModelScope, None)]);
        assert_eq!(ranked.len(), 2);
    }
}
