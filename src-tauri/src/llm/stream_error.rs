//! Classification of stream errors so the dispatcher can decide
//! whether to retry the stream, fail loudly, or treat as transient.
//!
//! See docs/superpowers/specs/2026-05-09-llm-timeout-rca.md §3.2.

use crate::error::Error;

/// Categorizes a stream error from an LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorKind {
    /// No bytes received within the stall window — connection healthy
    /// in some abstract sense but server stopped emitting. Always retry.
    Stalled,
    /// Connection reset, broken pipe, body decode error mid-stream, etc.
    /// Almost always recoverable on a fresh attempt. Retry up to N times.
    TransientNetwork,
    /// HTTP 4xx, model-not-found, auth failures, malformed requests.
    /// Will not succeed on retry. Surface immediately.
    Fatal,
}

/// Look at an `Error` and decide what kind of recovery (if any) makes sense.
pub fn classify_stream_error(err: &Error) -> StreamErrorKind {
    match err {
        Error::StreamStalled { .. } => StreamErrorKind::Stalled,
        Error::Internal(msg) => {
            let lower = msg.to_ascii_lowercase();
            // 4xx and explicit auth failures are fatal
            if lower.contains("status 400")
                || lower.contains("status 401")
                || lower.contains("status 403")
                || lower.contains("status 404")
                || lower.contains("status 422")
                || lower.contains("api error:") // OpenAI/Anthropic-side message
                || lower.contains("invalid api key")
                || lower.contains("model_not_found")
                || lower.contains("invalid_request_error")
            {
                return StreamErrorKind::Fatal;
            }
            // Body / connection errors → transient
            if lower.contains("error decoding response body")
                || lower.contains("connection reset")
                || lower.contains("broken pipe")
                || lower.contains("connection closed")
                || lower.contains("stream read error")
                || lower.contains("connection error")
                || lower.contains("timed out")
            {
                return StreamErrorKind::TransientNetwork;
            }
            // Default: treat unknowns as transient. We'd rather retry once
            // and lose a few seconds than surface an opaque failure to the
            // user that a retry would have cured.
            StreamErrorKind::TransientNetwork
        }
        Error::Llm(_) => StreamErrorKind::Fatal,
        // Default: anything else is transient.
        _ => StreamErrorKind::TransientNetwork,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stalled_is_stalled() {
        let err = Error::StreamStalled { duration: Duration::from_secs(45) };
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Stalled);
    }

    #[test]
    fn body_decode_is_transient() {
        let err = Error::Internal("OpenAI stream read error: error decoding response body".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn connection_reset_is_transient() {
        let err = Error::Internal("Anthropic connection reset by peer".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn auth_is_fatal() {
        let err = Error::Internal("OpenAI API error: invalid api key".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }

    #[test]
    fn status_401_is_fatal() {
        let err = Error::Internal("OpenAI API returned status 401".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }

    #[test]
    fn status_500_is_transient_via_default() {
        // 5xx isn't explicitly fatal; default fallthrough → transient.
        let err = Error::Internal("OpenAI API returned status 500".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn unknown_is_transient_default() {
        let err = Error::Internal("some other weird thing happened".into());
        assert_eq!(classify_stream_error(&err), StreamErrorKind::TransientNetwork);
    }

    #[test]
    fn llm_subtype_is_fatal() {
        let err = Error::Llm(crate::error::LlmError::ProviderNotConfigured("openai".into()));
        assert_eq!(classify_stream_error(&err), StreamErrorKind::Fatal);
    }
}
