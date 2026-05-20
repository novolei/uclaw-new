#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationAppStatus {
    Active,
    Paused,
    Error,
    NeedsLogin,
    WaitingUser,
    Uninstalled,
}

impl AutomationAppStatus {
    pub fn from_enabled_error(enabled: bool, error: Option<&str>) -> Self {
        if let Some(error) = error {
            if error.to_ascii_lowercase().contains("login") {
                return Self::NeedsLogin;
            }
            return Self::Error;
        }

        if enabled {
            Self::Active
        } else {
            Self::Paused
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutomationUserOverrides {
    pub frequency: Option<String>,
    pub notification_level: Option<String>,
    pub model_source_id: Option<String>,
    pub model_id: Option<String>,
    pub login_notice_dismissed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionResolutionInput {
    pub spec_permissions: Vec<String>,
    pub granted: Vec<String>,
    pub denied: Vec<String>,
    pub default_allowed: bool,
}

pub fn resolve_automation_permission(permission: &str, input: &PermissionResolutionInput) -> bool {
    if input.denied.iter().any(|item| item == permission) {
        return false;
    }

    if input.granted.iter().any(|item| item == permission) {
        return true;
    }

    if input.spec_permissions.iter().any(|item| item == permission) {
        return true;
    }

    input.default_allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_permission_denied_wins() {
        let row = PermissionResolutionInput {
            spec_permissions: vec!["ai-browser".into()],
            granted: vec!["ai-browser".into()],
            denied: vec!["ai-browser".into()],
            default_allowed: false,
        };
        assert!(!resolve_automation_permission("ai-browser", &row));
    }

    #[test]
    fn resolve_permission_spec_default_allows_declared_permission() {
        let row = PermissionResolutionInput {
            spec_permissions: vec!["ai-browser".into()],
            granted: vec![],
            denied: vec![],
            default_allowed: false,
        };
        assert!(resolve_automation_permission("ai-browser", &row));
    }

    #[test]
    fn app_status_maps_enabled_to_active_or_paused() {
        assert_eq!(
            AutomationAppStatus::from_enabled_error(true, None),
            AutomationAppStatus::Active
        );
        assert_eq!(
            AutomationAppStatus::from_enabled_error(false, None),
            AutomationAppStatus::Paused
        );
        assert_eq!(
            AutomationAppStatus::from_enabled_error(true, Some("login required")),
            AutomationAppStatus::NeedsLogin
        );
    }
}
