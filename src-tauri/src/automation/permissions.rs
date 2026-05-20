use crate::automation::protocol::humane_v1::Permission;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum PermissionError {
    #[error("permission denied by user")]
    Denied,
    #[error("permission not granted")]
    NotGranted,
    #[error("tool has no permission mapping")]
    Unmapped,
}

pub fn check(
    spec_perms: &[Permission],
    granted: &[Permission],
    denied: &[Permission],
    tool_name: &str,
) -> Result<(), PermissionError> {
    let required = match required_for(tool_name) {
        None => return Ok(()),                      // ungated tools
        Some(p) => p,
    };
    if denied.contains(&required) { return Err(PermissionError::Denied); }
    if granted.contains(&required) || spec_perms.contains(&required) { return Ok(()); }
    Err(PermissionError::NotGranted)
}

fn required_for(tool: &str) -> Option<Permission> {
    match tool {
        "shell" | "bash"          => Some(Permission::Shell),
        "edit" | "file"           => Some(Permission::Filesystem),
        "web" | "web_fetch" | "http_request" => Some(Permission::Network),
        "notify_user"             => Some(Permission::Notification),
        // Live-room gbrain tools are exposed only with the browser-capable built-in spec.
        // A later protocol revision can split this into a dedicated Knowledge permission.
        "gbrain_room_search" | "gbrain_room_get_page" | "gbrain_room_put_page" => Some(Permission::AiBrowser),
        t if t.starts_with("browser_") => Some(Permission::AiBrowser),
        "memory" | "report_to_user" | "request_escalation" => None,
        _ => None,    // unknown tools pass through (Phase 1 conservative; Phase 2 may flip to Unmapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn denied_overrides_granted() {
        let r = check(&[Permission::Notification], &[Permission::Notification], &[Permission::Notification], "notify_user");
        assert!(matches!(r, Err(PermissionError::Denied)));
    }
    #[test]
    fn granted_unlocks_tool() {
        let r = check(&[], &[Permission::Shell, Permission::Filesystem], &[], "shell");
        assert!(r.is_ok());
    }
    #[test]
    fn spec_perm_acts_as_implicit_grant() {
        let r = check(&[Permission::Network], &[], &[], "web");
        assert!(r.is_ok());
    }
    #[test]
    fn missing_perm_rejects() {
        let r = check(&[], &[], &[], "shell");
        assert!(matches!(r, Err(PermissionError::NotGranted)));
    }
    #[test]
    fn memory_never_gated() {
        assert!(check(&[], &[], &[], "memory").is_ok());
        assert!(check(&[], &[], &[], "report_to_user").is_ok());
        assert!(check(&[], &[], &[], "request_escalation").is_ok());
    }

    #[test]
    fn room_scoped_gbrain_tools_require_ai_browser_permission() {
        let missing = check(&[], &[], &[], "gbrain_room_search");
        assert!(matches!(missing, Err(PermissionError::NotGranted)));

        let granted = check(&[Permission::AiBrowser], &[], &[], "gbrain_room_search");
        assert!(granted.is_ok());
        assert!(check(&[Permission::AiBrowser], &[], &[], "gbrain_room_get_page").is_ok());
        assert!(check(&[Permission::AiBrowser], &[], &[], "gbrain_room_put_page").is_ok());
    }
}
