use serde_json::Value;

/// Bindings, and separately transitions, one Cinnabar panel surface retains.
const MAX_PANEL_BINDINGS: usize = 256;

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

/// The same as `panel`, for tests whose local `panel` value shadows it.
fn panel_of(state: &crate::state::State) -> (Value, Value) {
    panel(state)
}

fn column<'a>(document: &'a Value, id: &str) -> &'a Value {
    child(child(&document["panel"], "desk"), id)
}

#[test]
fn every_control_is_reachable_from_the_floating_columns() {
    let state = crate::state::State::permitted(Default::default());
    let (panel, document) = panel(&state);
    let controls = panel["controls"].as_array().unwrap();
    let mut routes = Vec::new();
    actions(&document["panel"], &mut routes);
    for (index, control) in controls
        .iter()
        .enumerate()
        .filter(|(_, control)| control["id"] != "drag_x" && control["id"] != "drag_y")
    {
        assert!(
            routes.contains(&format!("mod.control:{index}")),
            "missing native control {index} ({})",
            control["id"]
        );
    }
    for (index, _) in controls.iter().enumerate().filter(|(_, control)| {
        control["kind"] == "slider" && !control["id"].as_str().unwrap().starts_with("drag_")
    }) {
        assert!(routes.contains(&format!("mod.edit:{index}")));
    }
    assert_eq!(controls[27]["id"], "drag_x");
    assert_eq!(controls[28]["id"], "drag_y");
    assert_eq!(controls.len(), 41);
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
    for removed in ["title", "dark", "style", "theme", "sections"] {
        assert!(panel.get(removed).is_none(), "{removed}");
    }
    // Host popups keep the dark look the removed `dark` flag used to give them.
    let colors = &panel["widget_colors"];
    assert_eq!(colors.as_object().unwrap().len(), 5);
    for (slot, color) in ["background", "surface", "text", "accent", "border"]
        .into_iter()
        .zip(super::WIDGET_COLORS)
    {
        let channels: Vec<f32> = colors[slot]
            .as_array()
            .unwrap()
            .iter()
            .map(|channel| channel.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(channels, color, "{slot}");
    }
}

/// Every binding name a document node reads; bound label text must keep its own binding.
fn bound(node: &Value, result: &mut Vec<String>) {
    let names: Vec<_> = node["bindings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|binding| binding["binding_name"].as_str().unwrap().to_owned())
        .collect();
    if let Some(text) = node["text"].as_str().filter(|text| text.starts_with('#')) {
        assert!(
            names.iter().any(|name| name == text),
            "{text} label is unbound"
        );
    }
    result.extend(names);
    for entry in node["controls"].as_array().into_iter().flatten() {
        for child in entry.as_object().unwrap().values() {
            bound(child, result);
        }
    }
}

#[test]
fn the_surface_reads_only_values_this_component_publishes() {
    use crate::state::{Preferences, State};
    let mut state = State::permitted(Preferences::default());
    state.drag = None;
    let (panel, document) = panel(&state);
    let bindings = panel["surface"]["bindings"].as_object().unwrap();
    let mut names = Vec::new();
    bound(&document["panel"], &mut names);
    for definition in document.as_object().unwrap().values() {
        bound(definition, &mut names);
    }
    assert!(!names.is_empty());
    for name in &names {
        assert!(
            !name.starts_with("#row_"),
            "{name}: Cinnabar no longer publishes native row values"
        );
        assert!(bindings.contains_key(name), "{name} is never published");
    }
    assert!(
        bindings.len() <= MAX_PANEL_BINDINGS,
        "{} bindings",
        bindings.len()
    );
}

#[test]
fn every_value_control_publishes_its_own_feedback() {
    use crate::state::{AimMode, KeybindTarget, Preferences, State};
    let mut state = State::permitted(Preferences {
        clicker_key: "ShiftRight".into(),
        aim_key: "Numpad7".into(),
        reach_blocks: 4.2,
        aim_mode: AimMode::Continuous,
        esp_color: 2,
        fake_lag_ms: 250,
        show_real_position: true,
        ..Preferences::default()
    });
    state.modules.aim = true;
    state.capturing_key = Some(KeybindTarget::Clicker);
    let (panel, _) = panel(&state);
    let bindings = &panel["surface"]["bindings"];
    let mut checked = 0;
    for (index, control) in panel["controls"].as_array().unwrap().iter().enumerate() {
        let id = control["id"].as_str().unwrap();
        let name = |suffix: &str| format!("#c{index}_{suffix}");
        match control["kind"].as_str().unwrap() {
            "toggle" => {
                let on = control["value"].as_bool().unwrap();
                assert_eq!(bindings[name("on")], u8::from(on), "{id}");
            }
            "slider" if !id.starts_with("drag_") => {
                let value = control["value"].as_f64().unwrap();
                let (min, max) = (
                    control["min"].as_f64().unwrap(),
                    control["max"].as_f64().unwrap(),
                );
                let text = format!("{value:.2}");
                let text = text.trim_end_matches('0').trim_end_matches('.');
                assert_eq!(bindings[name("text")], text, "{id}");
                let fill = bindings[name("fill")].as_f64().unwrap();
                assert!((fill - (value - min) / (max - min)).abs() < 1e-4, "{id}");
            }
            "keybind" => {
                let key = control["key"].as_str().unwrap();
                assert_eq!(bindings[name("text")], super::key_label(key), "{id}");
            }
            "choice" => {
                let option = &control["options"][control["index"].as_u64().unwrap() as usize];
                assert_eq!(&bindings[name("text")], option, "{id}");
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, 15 + 7 + 6 + 2);
    assert_eq!(bindings["#c3_text"], "RShift");
    assert_eq!(bindings["#c7_text"], "Num7");
    assert_eq!(bindings["#c9_text"], "4.2");
    assert_eq!(bindings["#c6_text"], "Continuous");
    assert_eq!(bindings["#c32_text"], "Cyan");
    assert_eq!(bindings["#c4_on"], 1);
    assert_eq!(bindings["#c16_on"], 1);
}

#[test]
fn feedback_formatting_matches_the_earlier_native_controls() {
    let text = |value: f32| {
        let mut json = String::new();
        super::slider_text(&mut json, value);
        serde_json::from_str::<String>(&json).unwrap()
    };
    assert_eq!(text(3.0), "3");
    assert_eq!(text(4.2), "4.2");
    assert_eq!(text(4.25), "4.25");
    assert_eq!(text(1000.0), "1000");
    assert_eq!(text(0.0), "0");
    let fraction = |value: f32| {
        let mut json = String::new();
        super::fraction(&mut json, value);
        json.parse::<f64>().unwrap()
    };
    assert_eq!(fraction(0.0), 0.0);
    assert_eq!(fraction(1.0), 1.0);
    assert_eq!(fraction(2.0), 1.0);
    assert_eq!(fraction(0.5), 0.5);
    assert_eq!(fraction(1.0 / 29.0), 0.0345);
    for (key, label) in [
        ("", "None"),
        ("KeyR", "R"),
        ("Digit4", "4"),
        ("F7", "F7"),
        ("ShiftRight", "RShift"),
        ("ArrowLeft", "Left"),
        ("Numpad0", "Num0"),
        ("NumpadAdd", "Num+"),
        ("Backslash", "\\"),
    ] {
        assert_eq!(super::key_label(key), label);
        let mut json = String::new();
        super::escaped(&mut json, super::key_label(key));
        assert_eq!(serde_json::from_str::<String>(&json).unwrap(), label);
    }
}

/// Finds a named descendant anywhere below a node.
fn find<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node["controls"].as_array()?.iter().find_map(|entry| {
        entry.as_object().unwrap().iter().find_map(|(key, child)| {
            if key.split('@').next() == Some(name) {
                Some(child)
            } else {
                find(child, name)
            }
        })
    })
}

#[test]
fn hover_and_press_are_native_button_states() {
    let state = crate::state::State::permitted(Default::default());
    let (_, document) = panel(&state);
    fn visit(node: &Value, count: &mut usize) {
        if node["hover_control"] == "hover" {
            assert_eq!(node["pressed_control"], "press");
            for state in ["hover", "press"] {
                assert_eq!(
                    child(node, state)["visible"],
                    false,
                    "{state} starts hidden"
                );
            }
            *count += 1;
        }
        for entry in node["controls"].as_array().into_iter().flatten() {
            for value in entry.as_object().unwrap().values() {
                visit(value, count);
            }
        }
    }
    let mut count = 0;
    visit(&document["panel"], &mut count);
    // Ten rows, eight disclosure arrows, six keycaps, two dropdowns and Reset layout.
    assert_eq!(count, 10 + 8 + 6 + 2 + 1);
}

#[test]
fn trailing_values_reuse_slider_text_with_a_fixed_unit() {
    let state = crate::state::State::permitted(Default::default());
    let (panel, document) = panel(&state);
    let clicker = find(&document["panel"], "clicker").unwrap();
    let value = find(clicker, "value").unwrap();
    let number = find(value, "number").unwrap();
    assert_eq!(number["text"], "#c2_text");
    assert_eq!(find(value, "unit").unwrap()["text"], " CPS");
    let aim = find(&document["panel"], "aim").unwrap();
    assert_eq!(
        find(find(aim, "value").unwrap(), "unit").unwrap()["text"],
        "%"
    );
    let bindings = &panel["surface"]["bindings"];
    assert_eq!(bindings["#c2_text"], "20");
    assert_eq!(bindings["#reach_value"], "3.0 blocks");
}

#[test]
fn generated_settings_heights_match_the_row_layout() {
    let state = crate::state::State::permitted(Default::default());
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
    let state = crate::state::State::permitted(Default::default());
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
    assert!(transitions.as_object().unwrap().len() <= MAX_PANEL_BINDINGS);
    let bindings = surface["bindings"].as_object().unwrap();
    assert!(
        bindings.len() <= MAX_PANEL_BINDINGS,
        "{} bindings",
        bindings.len()
    );
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
    // The header divider fades with the column's open state.
    let divider = child(child(column(&document, "combat"), "entrance"), "divider");
    assert_eq!(divider["bindings"][0]["binding_name"], "#col_combat_open");
    assert_eq!(divider["bindings"][0]["binding_name_override"], "#alpha");
    assert_eq!(transitions["#col_combat_open"]["duration"], 0.16);
    assert_eq!(transitions["#col_combat_open"]["easing"], "out_cubic");
    assert_eq!(bindings["#col_combat_open"], 1);
    // A row keycap fades quickly while its value slides with the settle curve.
    assert_eq!(transitions["#clicker_keycap"]["duration"], 0.16);
    assert_eq!(transitions["#clicker_keycap"]["easing"], "out_cubic");
    assert_eq!(transitions["#clicker_value_at"]["duration"], 0.22);
}

#[test]
fn disclosure_and_toggles_change_only_bindings() {
    use crate::state::{Controls, Event, Preferences, State};
    let mut state = State::permitted(Preferences::default());
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
    assert!(bindings.get("#clicker_y").is_none(), "first rows stay put");
    assert_eq!(bindings["#col_combat_h"], 180);
    assert_eq!(bindings["#col_combat_bg"], 207);
    assert_eq!(bindings["#reach_value"], "4.2 blocks");
    assert_eq!(bindings["#c9_text"], "4.2");
    assert_eq!(bindings["#fake_lag_bar"], serde_json::json!([1, 1]));
    assert_eq!(bindings["#c14_on"], 1);
    assert_eq!(bindings["#col_network_count"], "1/1");
}

#[test]
fn rows_keep_values_and_show_bound_keys_as_keycaps() {
    use crate::state::{Preferences, State};
    let mut state = State::permitted(Preferences {
        clicker_key: "KeyR".into(),
        aim_key: "Digit4".into(),
        ..Preferences::default()
    });
    state.modules.clicker = true;
    state.capturing_key = Some(crate::state::KeybindTarget::Aim);
    let (panel, _) = panel(&state);
    let bindings = &panel["surface"]["bindings"];
    // Row keycaps and the settings keycaps share one legend.
    assert_eq!(bindings["#c3_text"], "R");
    assert_eq!(bindings["#c7_text"], "4");
    assert_eq!(bindings["#c10_text"], "None");
    assert_eq!(bindings["#reach_keycap"], 0);
    assert_eq!(bindings["#clicker_keycap"], 1);
    assert_eq!(bindings["#reach_value_at"], serde_json::json!([72, 6]));
    assert_eq!(bindings["#clicker_value_at"], serde_json::json!([50, 6]));
    assert_eq!(bindings["#c2_text"], "20", "values stay visible when bound");
    assert_eq!(bindings["#reach_value"], "3.0 blocks");
    // Enabled rows light the coral legend with the row's own progress.
    assert_eq!(bindings["#c1_on"], 1);
    assert_eq!(bindings["#c4_on"], 0);
    assert_eq!(bindings["#aim_capture"], 1);
    assert_eq!(bindings["#clicker_capture"], 0);
    let (_, document) = panel_of(&state);
    let clicker = find(&document["panel"], "clicker").unwrap();
    let value = find(clicker, "value").unwrap();
    assert_eq!(value["bindings"][0]["binding_name"], "#clicker_value_at");
    assert_eq!(value["bindings"][0]["binding_name_override"], "#offset");
    let keycap = find(clicker, "keycap").unwrap();
    assert_eq!(find(keycap, "legend").unwrap()["text"], "#c3_text");
    assert_eq!(
        find(keycap, "lit").unwrap()["bindings"][1]["binding_name"],
        "#c1_on"
    );
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
    let state = State::permitted(preferences);
    let (panel, _) = panel(&state);
    let bindings = &panel["surface"]["bindings"];
    assert_eq!(bindings["#col_render_h"], 0);
    assert_eq!(bindings["#col_render_bg"], 22);
    assert_eq!(bindings["#col_render_open"], 0);
    assert_eq!(bindings["#col_render_angle"], -90);
    assert_eq!(bindings["#col_render_pos"], serde_json::json!([12.5, -40]));
    // The pad's centre cancels the column offset; its tints sit back at the column.
    assert_eq!(bindings["#col_render_pad"], serde_json::json!([-12.5, 40]));
    let (_, document) = panel_of(&state);
    let grip = child(child(column(&document, "render"), "entrance"), "grip");
    let pad = child(grip, "grip");
    assert_eq!(pad["anchor_to"], "center");
    let over = child(pad, "over");
    assert_eq!(over["anchor_from"], "center");
    assert_eq!(over["bindings"][0]["binding_name"], "#col_render_pos");
    // Tints share the plate height: exactly the header while collapsed keeps corners round.
    assert_eq!(
        child(over, "tint")["bindings"][0]["binding_name"],
        "#col_render_bg"
    );
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
    let mut states = vec![State::permitted(Preferences::default())];
    let mut busy = State::permitted(Preferences {
        clicker_key: "KeyR".into(),
        aim_key: "Digit4".into(),
        reach_key: "F9".into(),
        reach_blocks: 4.2,
        aim_mode: crate::state::AimMode::Continuous,
        aim_horizontal_only: true,
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

#[test]
fn retained_patches_match_all_live_controls_without_replacing_the_surface() {
    use crate::state::{AimMode, KeybindTarget, Preferences, State};
    for mode in [AimMode::Continuous, AimMode::WhileClicking] {
        for capture in [None, Some(KeybindTarget::Menu), Some(KeybindTarget::Aim)] {
            let mut state = State::permitted(Preferences {
                clicker_key: "KeyR".into(),
                aim_key: "Numpad7".into(),
                reach_blocks: 4.2,
                cps: 27,
                aim_mode: mode,
                esp_color: 2,
                esp_names: false,
                aim_horizontal_only: true,
                ..Preferences::default()
            });
            state.modules.clicker = true;
            state.modules.player_esp = true;
            state.capturing_key = capture;
            let patch: Value = serde_json::from_str(&super::control_patch(&state)).unwrap();
            let (expected, _) = panel(&state);
            assert_eq!(patch["capture_key"], expected["capture_key"]);
            assert!(patch.get("surface").is_none());
            assert!(patch.get("document").is_none());
            let mut checked = std::collections::BTreeSet::new();
            for entry in patch["control_values"].as_array().unwrap() {
                let index = entry[0].as_u64().unwrap() as usize;
                let control = &expected["controls"][index];
                let field = if control["kind"] == "choice" {
                    "index"
                } else {
                    "value"
                };
                assert_eq!(
                    normalize(entry[1].clone()),
                    normalize(control[field].clone()),
                    "{}",
                    control["id"]
                );
                assert!(checked.insert(control["id"].as_str().unwrap()));
            }
            for edit in patch["controls"].as_array().unwrap() {
                let control = expected["controls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|control| control["id"] == edit["id"])
                    .unwrap();
                assert_eq!(edit["key"], control["key"]);
                assert_eq!(edit["capturing"], control["capturing"]);
                assert!(checked.insert(control["id"].as_str().unwrap()));
            }
            assert_eq!(
                checked.len(),
                expected["controls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|control| control["kind"] != "button")
                    .count()
            );
        }
    }
}

#[test]
fn completed_binding_updates_are_valid_for_initial_and_retained_publication() {
    let state = crate::state::State::permitted(Default::default());
    let bindings = super::bindings_json(&state);
    for prefix in ["{}", r#"{"surface":"clickgui"}"#] {
        let patch: Value =
            serde_json::from_str(&super::with_bindings(prefix.into(), &bindings)).unwrap();
        assert_eq!(
            patch["bindings"],
            serde_json::from_str::<Value>(&bindings).unwrap()
        );
        assert_eq!(
            patch.get("surface"),
            serde_json::from_str::<Value>(prefix)
                .unwrap()
                .get("surface")
        );
    }
}

#[test]
fn final_binding_step_reserves_publication_work() {
    let state = crate::state::State::permitted(Default::default());
    let mut builder = super::Bindings::new(&state);
    for _ in 0..46 {
        assert!(!builder.finishing());
        assert!(!builder.advance());
    }
    assert!(builder.finishing());
    assert!(builder.advance());
    assert_eq!(builder.finish(), super::bindings_json(&state));
}

#[test]
fn reach_row_preserves_every_supported_decimal_distance() {
    let mut state = crate::state::State::permitted(Default::default());
    for tenths in 30..=60 {
        state.preferences.reach_blocks = tenths as f32 / 10.;
        let bindings: Value = serde_json::from_str(&super::bindings_json(&state)).unwrap();
        assert_eq!(
            bindings["#reach_value"],
            format!("{}.{:01} blocks", tenths / 10, tenths % 10)
        );
    }
}

#[test]
fn denied_module_rows_expose_disabled_state_and_missing_permission_explanations() {
    let mut state = crate::state::State::new(Default::default());
    state.set_permissions(&[]);
    let bindings: serde_json::Value = serde_json::from_str(&super::bindings_json(&state)).unwrap();
    for (index, requirement) in crate::permissions::REQUIREMENTS.iter().enumerate() {
        assert_eq!(super::MODULES[index].0, requirement.id);
        assert_eq!(bindings[format!("#{}_available", requirement.id)], false);
        assert_eq!(bindings[format!("#{}_opacity", requirement.id)], 0.4);
        assert!(
            bindings[format!("#{}_permission", requirement.id)]
                .as_str()
                .unwrap()
                .starts_with("Requires: ")
        );
    }
}
