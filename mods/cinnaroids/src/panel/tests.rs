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
        .unwrap_or_else(|| panic!("missing child {name}"))
}

fn panel(state: &crate::state::State) -> (Value, Value) {
    let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    let document = serde_json::from_str(panel["surface"]["document"].as_str().unwrap()).unwrap();
    (panel, document)
}

fn column<'a>(document: &'a Value, id: &str) -> &'a Value {
    child(child(&document["panel"], "desk"), id)
}

#[test]
fn every_control_is_reachable_from_the_floating_columns() {
    let state = crate::state::State::new(Default::default());
    let (panel, document) = panel(&state);
    let controls = panel["controls"].as_array().unwrap();
    let mut routes = Vec::new();
    actions(&document["panel"], &mut routes);
    for (index, control) in controls.iter().enumerate().take(27) {
        assert!(
            routes.contains(&format!("mod.control:{index}")),
            "missing native control {index} ({})",
            control["id"]
        );
    }
    for index in [2, 5, 9, 12, 15, 18] {
        assert_eq!(controls[index]["kind"], "slider");
        assert!(routes.contains(&format!("mod.edit:{index}")));
    }
    assert_eq!(controls[27]["id"], "drag_x");
    assert_eq!(controls[28]["id"], "drag_y");
    assert_eq!(controls.len(), 29);
    let pads = routes
        .iter()
        .filter(|route| route.starts_with("mod.vector:"))
        .count();
    assert_eq!(pads, 4);
    assert!(routes.contains(&"mod.vector:27:28".to_owned()));
    for id in super::COLUMNS {
        let header = child(child(column(&document, id), "entrance"), "grip");
        assert_eq!(
            header["clips_children"], true,
            "pads only take presses on headers"
        );
    }
    assert!(!routes.iter().any(|route| route == "mod.close"));
    assert_eq!(panel["motion"], true);
    assert_eq!(panel["dark"], true);
}

#[test]
fn generated_settings_heights_match_the_row_layout() {
    let state = crate::state::State::new(Default::default());
    let (_, document) = panel(&state);
    for (module, owner, height) in super::MODULES {
        let body = child(
            child(column(&document, super::COLUMNS[owner]), "entrance"),
            "body",
        );
        let row = child(body, module);
        if height == 0 {
            assert!(row["controls"].as_array().unwrap().iter().all(|entry| {
                !entry
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|key| key.starts_with("details"))
            }));
            continue;
        }
        let details = child(row, "details");
        assert_eq!(details["clips_children"], true);
        assert_eq!(child(details, "content")["size"][1], height, "{module}");
    }
}

#[test]
fn open_and_close_motion_is_reversible_and_dims_the_world() {
    let state = crate::state::State::new(Default::default());
    let (panel, document) = panel(&state);
    let surface = &panel["surface"];
    let transitions = &surface["transitions"];
    assert_eq!(transitions["#world_blur"]["from"], 0);
    assert_eq!(transitions["#world_blur"]["exit"], 0);
    assert_eq!(
        transitions["#col_combat_in"]["from"],
        serde_json::json!([0, 10])
    );
    assert!(
        transitions.get("#col_combat_pos").is_none(),
        "drags follow the pointer directly"
    );
    assert!(transitions.as_object().unwrap().len() <= 128);
    let bindings = surface["bindings"].as_object().unwrap();
    assert!(bindings.len() <= 128, "{} bindings", bindings.len());
    assert_eq!(bindings["#world_blur"], 4);
    assert_eq!(bindings["#col_menu_alpha"], 1);
    assert_eq!(bindings["#col_menu_h"], 52);
    let desk = child(&document["panel"], "desk");
    assert_eq!(desk["controls"].as_array().unwrap().len(), 4);
    let settings = column(&document, "menu");
    assert_eq!(settings["bindings"].as_array().unwrap().len(), 1);
    assert_eq!(
        child(child(settings, "entrance"), "title")["text"],
        "Settings"
    );
    assert_eq!(bindings["#menu_capture"], 0);
    assert_eq!(transitions["#menu_capture"]["duration"], 0.14);
}

#[test]
fn disclosure_and_toggles_change_only_bindings() {
    use crate::state::{Controls, Event, Preferences, State};
    let mut state = State::new(Preferences::default());
    let (initial, _) = panel(&state);
    state.controls(Controls {
        focused: true,
        panel_open: true,
        keys: &[],
        held: &[],
        seconds: 0.0,
        events: &[
            Event {
                id: "expand_reach".into(),
                value: 1.0,
            },
            Event {
                id: "reach_blocks".into(),
                value: 4.2,
            },
            Event {
                id: "fake_lag".into(),
                value: 1.0,
            },
        ],
    });
    let (updated, _) = panel(&state);
    assert_eq!(
        initial["surface"]["document"],
        updated["surface"]["document"]
    );
    let bindings = &updated["surface"]["bindings"];
    assert_eq!(bindings["#reach_h"], 60);
    assert_eq!(bindings["#reach_angle"], 0);
    assert_eq!(bindings["#aim_h"], 0);
    assert_eq!(bindings["#jump_reset_y"], serde_json::json!([0, 120]));
    assert_eq!(bindings["#col_combat_h"], 140);
    assert_eq!(bindings["#col_combat_bg"], 167);
    assert_eq!(bindings["#reach_value"], "4.2 blocks");
    assert_eq!(bindings["#fake_lag_bar"], serde_json::json!([1, 1]));
    assert_eq!(bindings["#col_network_count"], "1/1");
}

#[test]
fn rows_keep_values_and_show_bound_keys_as_keycaps() {
    use crate::state::{Preferences, State};
    let mut state = State::new(Preferences {
        clicker_key: "KeyR".into(),
        aim_key: "Digit4".into(),
        ..Preferences::default()
    });
    state.modules.clicker = true;
    state.capturing_key = Some(crate::state::KeybindTarget::Aim);
    let (panel, _) = panel(&state);
    let bindings = &panel["surface"]["bindings"];
    assert_eq!(bindings["#clicker_bind"], "R");
    assert_eq!(bindings["#aim_bind"], "4");
    assert_eq!(bindings["#reach_bind"], "");
    assert_eq!(bindings["#reach_keycap"], 0);
    assert_eq!(bindings["#clicker_keycap"], 1);
    assert_eq!(
        bindings["#clicker_value"], "20 CPS",
        "values stay visible when bound"
    );
    assert_eq!(bindings["#reach_value"], "3.0 blocks");
    assert_eq!(bindings["#clicker_key_color"][0], 1);
    assert_eq!(bindings["#aim_capture"], 1);
    assert_eq!(bindings["#clicker_capture"], 0);
    assert_eq!(super::key_legend(""), "None");
    assert_eq!(super::key_legend("F7"), "F7");
    assert_eq!(super::key_legend("Key"), "Key");
}

#[test]
fn collapsed_columns_hide_their_body_and_pads_track_column_positions() {
    use crate::state::{Preferences, State};
    let mut preferences = Preferences::default();
    preferences.collapsed[2] = true;
    preferences.collapsed[3] = true;
    preferences.columns[2] = [12.5, -40.0];
    let state = State::new(preferences);
    let (panel, _) = panel(&state);
    let bindings = &panel["surface"]["bindings"];
    assert_eq!(bindings["#col_render_h"], 0);
    assert_eq!(bindings["#col_render_bg"], 22);
    assert_eq!(bindings["#col_render_angle"], -90);
    assert_eq!(bindings["#col_render_pos"], serde_json::json!([12.5, -40]));
    assert_eq!(
        bindings["#col_render_pad"],
        serde_json::json!([-2060.5, -2008])
    );
    assert_eq!(
        bindings["#col_render_tint_at"],
        serde_json::json!([2060.5, 2008])
    );
    assert_eq!(
        bindings["#col_render_tint_h"], 22,
        "collapsed headers keep rounded corners"
    );
    assert_eq!(bindings["#col_combat_tint_h"], 27);
    assert_eq!(bindings["#col_menu_h"], 0);
    assert_eq!(bindings["#col_menu_bg"], 22);
    assert_eq!(bindings["#col_menu_alpha"], 1);
    assert_eq!(bindings["#col_menu_angle"], -90);
}

/// Numbers compare by value: the shell writes `1`, serde writes `1.0`.
fn normalize(value: Value) -> Value {
    match value {
        Value::Number(number) => serde_json::json!(number.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalize(v))).collect())
        }
        other => other,
    }
}

#[test]
fn hand_built_shell_matches_the_serde_schema_in_every_state() {
    use crate::state::{KeybindTarget, Preferences, State};
    let mut states = vec![State::new(Preferences::default())];
    let mut busy = State::new(Preferences {
        clicker_key: "KeyR".into(),
        aim_key: "Digit4".into(),
        reach_key: "F9".into(),
        reach_blocks: 4.2,
        aim_mode: crate::state::AimMode::Continuous,
        menu_key: "F6".into(),
        ..Preferences::default()
    });
    busy.modules.clicker = true;
    busy.modules.fullbright = true;
    busy.capturing_key = Some(KeybindTarget::Reach);
    states.push(busy);
    for state in states {
        let fast: Value = serde_json::from_str(&state.panel_shell_json().unwrap()).unwrap();
        let reference: Value =
            serde_json::from_str(&state.panel_reference_json().unwrap()).unwrap();
        assert_eq!(normalize(fast), normalize(reference));
    }
}
