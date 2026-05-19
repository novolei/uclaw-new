#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRecoveryKind {
    RefreshTabsAndRetry,
    RefreshDomAndRetry,
    WaitAndRetry,
    Stop,
}

pub fn classify_browser_error(error: &str) -> BrowserRecoveryKind {
    let lower = error.to_ascii_lowercase();
    if error.contains("Tab '") && lower.contains("not found") {
        return BrowserRecoveryKind::RefreshTabsAndRetry;
    }
    if error.contains("Element [") && lower.contains("not found") {
        return BrowserRecoveryKind::RefreshDomAndRetry;
    }
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("detached")
        || lower.contains("target closed")
        || lower.contains("execution context was destroyed")
    {
        return BrowserRecoveryKind::WaitAndRetry;
    }
    BrowserRecoveryKind::Stop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_stale_tab() {
        assert_eq!(
            classify_browser_error("Tab 'abc' not found"),
            BrowserRecoveryKind::RefreshTabsAndRetry
        );
    }

    #[test]
    fn classifies_stale_index() {
        assert_eq!(
            classify_browser_error("Element [3] not found"),
            BrowserRecoveryKind::RefreshDomAndRetry
        );
    }

    #[test]
    fn classifies_navigation_timeout() {
        assert_eq!(
            classify_browser_error("Timeout while waiting for navigation"),
            BrowserRecoveryKind::WaitAndRetry
        );
    }

    #[test]
    fn classifies_unknown_as_stop() {
        assert_eq!(
            classify_browser_error("permission denied by runtime policy"),
            BrowserRecoveryKind::Stop
        );
    }
}
