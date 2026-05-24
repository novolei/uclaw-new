use super::*;
use crate::browser::provider::BrowserProviderRouteDecisionStatus;

#[test]
fn live_provider_route_selects_local_chromium_for_click() {
    let action = BrowserAction::Click {
        tab_id: "tab-1".to_string(),
        index: 3,
    };

    let decision = route_live_browser_action_provider(&action);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
    assert!(decision.event_intents.iter().any(|intent| {
        intent.event_name.as_str() == "browser.provider.selected"
            && intent.provider_id.as_deref() == Some(LOCAL_CHROMIUM_PROVIDER_ID)
    }));
    assert!(!provider_route_blocks_local_action(&decision));
}

#[test]
fn provider_selection_maps_get_state_to_snapshot_request() {
    let action = BrowserAction::GetState {
        tab_id: "tab-1".to_string(),
        include_screenshot: true,
        include_visual: true,
    };

    let selection = provider_selection_request_for_action(&action);

    assert_eq!(selection.action.as_deref(), Some("dom_snapshot"));
    assert_eq!(selection.observation_mode.as_deref(), Some("screenshot"));
    assert!(!selection.requires_mcp_specific_capability);
}

#[test]
fn evaluate_route_preserves_local_registry_without_raw_provider_promotion() {
    let action = BrowserAction::Evaluate {
        tab_id: "tab-1".to_string(),
        script: "document.title".to_string(),
    };

    let selection = provider_selection_request_for_action(&action);
    let decision = route_live_browser_action_provider(&action);

    assert!(selection.action.is_none());
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
}

#[test]
fn non_local_provider_route_blocks_local_action_registry() {
    let decision = BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::Selected,
        selected_provider_id: Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
        candidates: Vec::new(),
        event_intents: Vec::new(),
    };

    assert!(provider_route_blocks_local_action(&decision));
}
