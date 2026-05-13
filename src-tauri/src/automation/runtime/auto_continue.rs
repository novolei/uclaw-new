/// Configuration for the auto-continue retry loop.
#[derive(Debug, Clone)]
pub struct AutoContinueConfig {
    pub max_retries: u32,
}

impl Default for AutoContinueConfig {
    fn default() -> Self {
        Self { max_retries: 10 }
    }
}

/// Terminal state of an automation run.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionGate {
    /// The agent called report_to_user successfully.
    Reported { text: String, outcome: String },
    /// The agent called request_escalation — a user decision is needed.
    Escalated { escalation_id: String },
    /// The agentic loop exhausted its iteration budget without reporting.
    LoopExhausted,
    /// An unrecoverable error terminated the run early.
    ErrorTerminal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_max_retries_is_10() {
        assert_eq!(AutoContinueConfig::default().max_retries, 10);
    }

    #[test]
    fn reported_gate_equality() {
        let a = CompletionGate::Reported { text: "x".into(), outcome: "useful".into() };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn escalated_gate_equality() {
        let a = CompletionGate::Escalated { escalation_id: "esc-1".into() };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn error_terminal_equality() {
        let a = CompletionGate::ErrorTerminal("oops".into());
        assert_eq!(a, CompletionGate::ErrorTerminal("oops".into()));
        assert_ne!(a, CompletionGate::ErrorTerminal("other".into()));
    }
}
