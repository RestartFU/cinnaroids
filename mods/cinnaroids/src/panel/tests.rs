use serde_json::Value;

/// Collect native action routes throughout the authored surface.
fn actions(node: &Value, result: &mut Vec<String>) {
    if let Some(mappings) = node["button_mappings"].as_array() {
        for mapping in mappings {
            result.push(mapping["to_button_id"].as_str().unwrap().to_owned());
        }
    }
    if let Some(controls) = node["controls"].as_array() {
        for entry in controls {
            for child in entry.as_object().unwrap().values() {
                actions(child, result);
            }
        }
    }
}

/// Resolve a named child without coupling assertions to draw order.
fn child<'a>(parent: &'a Value, name: &str) -> &'a Value {
    parent["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|entry| {
            entry
                .as_object()
                .unwrap()
                .iter()
                .find_map(|(key, value)| (key.split('@').next() == Some(name)).then_some(value))
        })
        .unwrap()
}

#[test]
fn fold_keeps_every_control_reachable_on_one_retained_scrollable_surface() {
    for dark in [true, false] {
        let preferences = crate::state::Preferences {
            dark_mode: dark,
            ..Default::default()
        };
        let state = crate::state::State::new(preferences);
        let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
        let surface = &panel["surface"];
        let document: Value = serde_json::from_str(surface["document"].as_str().unwrap()).unwrap();
        let root = &document["panel"];
        let dialog = child(root, "dialog");
        let view = child(dialog, "modules");
        assert_eq!(view["scroll_content"], "module_rows");
        let viewport = child(view, "viewport");
        assert_eq!(viewport["clips_children"], true);
        let modules = child(viewport, "module_rows");
        assert_eq!(modules["controls"].as_array().unwrap().len(), 7);
        let fullbright = child(modules, "fullbright");
        assert_eq!(fullbright["size"], serde_json::json!([370, 32]));
        assert!(
            fullbright["controls"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry.get("details").is_none())
        );
        for (name, _) in super::MODULES.iter().take(6) {
            assert_eq!(
                child(child(modules, name), "details")["clips_children"],
                true
            );
        }
        let mut routes = Vec::new();
        actions(root, &mut routes);
        for index in 0..29 {
            assert!(
                routes.contains(&format!("mod.control:{index}")),
                "missing native control {index}"
            );
        }
        for index in [2, 5, 9, 12, 15, 19] {
            assert!(routes.contains(&format!("mod.edit:{index}")));
        }
        assert!(
            !routes.iter().any(|route| route.starts_with("mod.category:")
                || matches!(route.as_str(), "mod.next" | "mod.prev"))
        );
        assert!(routes.contains(&"mod.close".to_owned()));
        assert_eq!(panel["motion"], true);
        assert_eq!(surface["transitions"]["#menu_scale"]["exit"], 0.975);
        assert_eq!(surface["transitions"]["#menu_blur"]["from"], 5);
        assert_eq!(surface["bindings"]["#fold_aim_height"], 80.0);
        assert_eq!(surface["bindings"]["#fold_reach_height"], 0.0);
    }
}

#[test]
fn disclosure_changes_only_bindings_and_keeps_summaries_current() {
    use crate::state::{Controls, Event, Preferences, State};
    let mut state = State::new(Preferences::default());
    let initial: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    state.controls(Controls {
        focused: true,
        panel_open: true,
        keys: &[],
        events: &[
            Event {
                id: "expand_reach".into(),
                value: 1.0,
            },
            Event {
                id: "reach_blocks".into(),
                value: 4.2,
            },
        ],
    });
    let updated: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(
        initial["surface"]["document"],
        updated["surface"]["document"]
    );
    assert_eq!(updated["surface"]["bindings"]["#fold_aim_height"], 0.0);
    assert_eq!(updated["surface"]["bindings"]["#fold_reach_height"], 48.0);
    assert_eq!(
        updated["surface"]["bindings"]["#summary_reach"],
        "4.2 blocks"
    );
    assert_eq!(
        updated["surface"]["bindings"]["#enabled_count"],
        "0 of 7 enabled"
    );
}
