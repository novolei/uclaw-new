use super::*;

#[test]
fn jcode_inspired_catalog_contains_required_families() {
    let ids: Vec<&str> = jcode_inspired_tool_family_cards()
        .iter()
        .map(|card| card.family_id)
        .collect();

    assert_eq!(
        ids,
        vec![
            "filesystem.search",
            "filesystem.read",
            "filesystem.write",
            "filesystem.patch",
            "shell.command",
            "runtime.background",
            "context.session_search",
        ]
    );
}

#[test]
fn write_patch_shell_and_background_are_permissioned() {
    for family_id in [
        "filesystem.write",
        "filesystem.patch",
        "shell.command",
        "runtime.background",
    ] {
        let card = tool_family_card(family_id).expect("card exists");
        assert!(card.requires_permission, "{family_id} must stay gated");
        assert!(
            card.policy_tags.contains(&"permission.required"),
            "{family_id} should advertise permission.required"
        );
    }
}

#[test]
fn cards_map_to_existing_or_future_tool_ids() {
    let search = tool_family_card("filesystem.search").unwrap();
    assert_eq!(search.tool_ids, &["search"]);
    assert!(search.capability_tags.contains(&"search"));
    assert!(search.capability_tags.contains(&"filesystem"));

    let background = tool_family_card("runtime.background").unwrap();
    assert!(background.tool_ids.is_empty());
    assert!(background.capability_tags.contains(&"background"));
    assert!(background.event_profile.contains(&"checkpoint"));
    assert_eq!(background.execution_status, "planned");

    let session = tool_family_card("context.session_search").unwrap();
    assert!(session.tool_ids.is_empty());
    assert!(session.capability_tags.contains(&"session_search"));
    assert_eq!(session.execution_status, "planned");
}

#[test]
fn family_tags_project_to_registry_tags() {
    let tags = registry_tags_for_tool("shell");
    assert_eq!(tags.get("family:shell.command"), Some(&"1".to_string()));
    assert!(tags.get("family:runtime.background").is_none());
    assert!(tags.get("tag:background").is_none());
    assert_eq!(
        tags.get("policy:permission.required"),
        Some(&"1".to_string())
    );
}

#[test]
fn unknown_tools_get_no_family_tags() {
    let tags = registry_tags_for_tool("not-a-tool");
    assert!(tags.is_empty());
}

#[tokio::test]
async fn registry_hub_resolves_patch_family_to_edit_tool() {
    let hub = crate::registries::RegistryHub::new();
    crate::registries::register_builtin_tools(&hub)
        .await
        .unwrap();

    let q = crate::runtime::contracts::CapabilityQuery {
        name: None,
        kind: "filesystem".into(),
        tags: {
            let mut tags = std::collections::BTreeMap::new();
            tags.insert("family:filesystem.patch".into(), "1".into());
            tags
        },
    };

    let result = crate::registries::resolve(&*hub.tools.read().await, &q);
    assert_eq!(result.best(), Some("edit"));
}

#[tokio::test]
async fn planned_families_do_not_resolve_to_live_tools() {
    let hub = crate::registries::RegistryHub::new();
    crate::registries::register_builtin_tools(&hub)
        .await
        .unwrap();

    for family in ["runtime.background", "context.session_search"] {
        let q = crate::runtime::contracts::CapabilityQuery {
            name: None,
            kind: String::new(),
            tags: {
                let mut tags = std::collections::BTreeMap::new();
                tags.insert(format!("family:{family}"), "1".into());
                tags
            },
        };

        let result = crate::registries::resolve(&*hub.tools.read().await, &q);
        assert!(result.is_empty(), "{family} must stay catalog-only");
    }
}
