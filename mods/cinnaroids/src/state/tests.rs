use super::*;

const BOUND_CLICKER_KEY: &str = "F8";
const BOUND_AIM_KEY: &str = "KeyR";
const BOUND_REACH_KEY: &str = "KeyV";
const BOUND_JUMP_RESET_KEY: &str = "KeyB";

fn controls(state: &mut State, keys: &[&str], events: &[(&str, f32)]) {
    state.controls(Controls {
        focused: true,
        panel_open: true,
        keys: &keys.iter().map(|key| (*key).into()).collect::<Vec<_>>(),
        held: &[],
        seconds: 0.0,
        events: &events
            .iter()
            .map(|(id, value)| Event {
                id: (*id).into(),
                value: *value,
            })
            .collect::<Vec<_>>(),
    });
}

fn state() -> State {
    State::new(Preferences::default())
}

/// Assign explicit shortcuts for tests of binding conflicts and keyboard toggles.
fn state_with_bindings() -> State {
    State::new(Preferences {
        clicker_key: BOUND_CLICKER_KEY.into(),
        aim_key: BOUND_AIM_KEY.into(),
        reach_key: BOUND_REACH_KEY.into(),
        jump_reset_key: BOUND_JUMP_RESET_KEY.into(),
        ..Preferences::default()
    })
}

#[test]
fn horizontal_only_defaults_off_and_persists_panel_changes() {
    assert!(!Preferences::from_json("{}").aim_horizontal_only);
    assert!(!Preferences::from_json(r#"{"aim_horizontal_only":"true"}"#).aim_horizontal_only);
    let mut state = state();
    assert!(!state.aim_config().horizontal_only);
    controls(&mut state, &[], &[("aim_horizontal_only", 1.0)]);
    assert!(state.aim_config().horizontal_only);
    assert!(state.preferences_dirty);
    let saved = serde_json::to_string(&state.preferences).unwrap();
    let restored = State::new(Preferences::from_json(&saved));
    assert!(restored.aim_config().horizontal_only);
    assert!(!restored.modules.aim);
    let panel: Value = serde_json::from_str(&restored.panel_json().unwrap()).unwrap();
    assert!(
        panel["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| { control["id"] == "aim_horizontal_only" && control["value"] == true })
    );
    controls(&mut state, &[], &[("aim_horizontal_only", 0.0)]);
    assert!(!state.aim_config().horizontal_only);
}

#[test]
fn preferences_restore_without_enabling_any_module() {
    let preferences = Preferences::from_json(
        r#"{"cps":18,"aim_strength":70,"aim_mode":"continuous","reach_blocks":5.2,"dark_mode":false,"columns":[[10,-20],[5000,0],[0,0],[900,0]],"collapsed":[true],"clicker_key":"KeyV","clicker":true,"aim":true,"reach":true,"cinnabar_path":"unused"}"#,
    );
    let state = State::new(preferences);
    assert_eq!(state.preferences.cps, 18);
    assert_eq!(state.preferences.aim_strength, 70);
    assert_eq!(state.preferences.aim_mode, AimMode::Continuous);
    assert_eq!(state.preferences.reach_blocks, 5.2);
    assert_eq!(state.preferences.columns[0], [10.0, -20.0]);
    assert_eq!(
        state.preferences.columns[1],
        [1898.0, 0.0],
        "clamped to the pointer pad"
    );
    assert_eq!(state.preferences.columns[2], [0.0, 0.0]);
    assert_eq!(
        state.preferences.columns[3],
        [230.0, 0.0],
        "Menu stays in the reference box"
    );
    assert_eq!(state.preferences.collapsed, [true, false, false, false]);
    assert_eq!(state.preferences.clicker_key, "KeyV");
    assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
    let json = serde_json::to_value(&state.preferences).unwrap();
    assert_eq!(json["netherite_range"], 64);
    assert!(json.get("aim").is_none());
    assert!(json.get("dark_mode").is_none());
}

#[test]
fn legacy_preferences_are_tolerated_and_bounds_are_restored() {
    let prefs = Preferences::from_json(
        r#"{"cps":400,"toggle_key":65,"aim_strength":255,"reach_blocks":90,"dark_mode":false,"hotkey_capture":true}"#,
    );
    assert_eq!(prefs.cps, MAX_CPS);
    assert_eq!(prefs.clicker_key, "KeyA");
    assert_eq!(prefs.aim_strength, 100);
    assert_eq!(prefs.reach_blocks, mod_api::MAX_ENTITY_REACH_BLOCKS);
    let prefs = Preferences::from_json(r#"{"clicker_key":"F10"}"#);
    assert_eq!(prefs.clicker_key, "F10");
    assert_eq!(Preferences::from_json("broken"), Preferences::default());
}

#[test]
fn enabling_clicker_keeps_aim_and_reach_independent() {
    let mut state = state_with_bindings();
    controls(&mut state, &[], &[("aim", 1.0), ("reach", 1.0)]);
    controls(&mut state, &[BOUND_CLICKER_KEY], &[]);
    assert!(state.modules.clicker && state.modules.aim && state.modules.reach);
    controls(&mut state, &[], &[("clicker", 0.0)]);
    assert!(!state.modules.clicker && state.modules.aim && state.modules.reach);
    assert!(!state.preferences_dirty);
}

#[test]
fn f10_has_no_builtin_action_and_can_be_assigned_to_one_module() {
    let mut state = state();
    controls(
        &mut state,
        &[],
        &[
            ("clicker", 1.0),
            ("aim", 1.0),
            ("reach", 1.0),
            ("jump_reset", 1.0),
            ("fake_lag", 1.0),
            ("netherite", 1.0),
            ("fullbright", 1.0),
        ],
    );
    controls(&mut state, &["F10"], &[("stop_all", 1.0)]);
    assert!(
        state.modules.clicker
            && state.modules.aim
            && state.modules.reach
            && state.modules.jump_reset
            && state.modules.fake_lag
            && state.modules.netherite
            && state.modules.fullbright
    );
    assert!(!state.reserved_keys().contains(&"F10".into()));
    controls(&mut state, &[], &[("clicker_key", 1.0)]);
    controls(&mut state, &["F10"], &[]);
    assert_eq!(state.preferences.clicker_key, "F10");
    assert!(state.modules.clicker);
    controls(&mut state, &["F10"], &[]);
    assert!(
        !state.modules.clicker
            && state.modules.aim
            && state.modules.reach
            && state.modules.jump_reset
            && state.modules.fake_lag
            && state.modules.netherite
            && state.modules.fullbright
    );
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&state.preferences).unwrap()),
        state.preferences
    );
    let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert!(
        panel["controls"]
            .as_array()
            .unwrap()
            .iter()
            .all(|control| control["id"] != "stop_all")
    );
}

#[test]
fn key_capture_changes_the_binding_without_toggling_a_module() {
    let mut state = state_with_bindings();
    controls(&mut state, &[], &[("clicker_key", 1.0)]);
    controls(&mut state, &[PANEL_KEY], &[]);
    assert!(state.capturing_key.is_some());
    controls(&mut state, &["ControlRight"], &[]);
    assert!(state.capturing_key.is_none());
    assert_eq!(state.preferences.clicker_key, "ControlRight");
    assert!(!state.modules.clicker);
    assert!(state.reservations_dirty && state.preferences_dirty);
    assert_eq!(
        state.reserved_keys(),
        [
            PANEL_KEY,
            "ControlRight",
            BOUND_AIM_KEY,
            BOUND_REACH_KEY,
            BOUND_JUMP_RESET_KEY
        ]
    );
    controls(&mut state, &["ControlRight"], &[]);
    assert!(state.modules.clicker);
}

#[test]
fn left_clicks_cannot_capture_any_binding_even_when_opening_the_keycap() {
    for (id, target) in [
        ("menu_key", KeybindTarget::Menu),
        ("clicker_key", KeybindTarget::Clicker),
        ("aim_key", KeybindTarget::Aim),
        ("reach_key", KeybindTarget::Reach),
        ("jump_reset_key", KeybindTarget::JumpReset),
    ] {
        let mut state = state();
        state.reservations_dirty = false;
        // The host can deliver the activating click and keycap event together.
        controls(&mut state, &["MouseLeft"], &[(id, 1.0)]);
        // A second click must leave capture waiting for a usable binding.
        controls(&mut state, &["MouseLeft"], &[(id, 1.0)]);
        assert_eq!(state.capturing_key, Some(target));
        assert_eq!(state.preferences, Preferences::default());
        assert!(!state.preferences_dirty && !state.reservations_dirty);
        assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
        assert!(!state.modules.jump_reset);

        controls(&mut state, &["MouseLeft", "KeyG"], &[]);
        assert!(state.capturing_key.is_none());
        assert_eq!(state.binding_mut(target).as_str(), "KeyG");
        assert!(state.preferences_dirty && state.reservations_dirty);
        assert!(!state.reserved_keys().contains(&"MouseLeft".into()));
    }
}

#[test]
fn saved_left_click_bindings_recover_without_resetting_other_preferences() {
    for id in [
        "menu_key",
        "clicker_key",
        "aim_key",
        "reach_key",
        "jump_reset_key",
        "toggle_key",
    ] {
        let preferences = Preferences::from_json(&format!(
            r#"{{"{id}":"MouseLeft","cps":17,"dark_mode":false}}"#
        ));
        let expected = Preferences {
            cps: 17,
            ..Preferences::default()
        };
        assert_eq!(preferences, expected, "{id}");
        let mut state = State::new(preferences);
        assert!(!state.reserved_keys().contains(&"MouseLeft".into()));
        controls(&mut state, &["MouseLeft"], &[("clicker_key", 1.0)]);
        controls(&mut state, &["KeyG"], &[]);
        let saved = serde_json::to_string(&state.preferences).unwrap();
        assert_eq!(Preferences::from_json(&saved), state.preferences);
        assert_eq!(state.preferences.clicker_key, "KeyG");
    }
}

#[test]
fn secondary_mouse_bindings_remain_available() {
    for key in ["MouseRight", "MouseMiddle"] {
        let mut state = state();
        controls(&mut state, &["MouseLeft"], &[("clicker_key", 1.0)]);
        controls(&mut state, &[key], &[]);
        assert!(state.capturing_key.is_none());
        assert_eq!(state.preferences.clicker_key, key);
        assert!(!state.modules.clicker);
        let saved = serde_json::to_string(&state.preferences).unwrap();
        assert_eq!(Preferences::from_json(&saved), state.preferences);
        controls(&mut state, &[key], &[]);
        assert!(state.modules.clicker);
    }
}

#[test]
fn escape_clears_each_module_binding_without_toggling_and_persists() {
    for (id, target, old_key) in [
        ("clicker_key", KeybindTarget::Clicker, BOUND_CLICKER_KEY),
        ("aim_key", KeybindTarget::Aim, BOUND_AIM_KEY),
        ("reach_key", KeybindTarget::Reach, BOUND_REACH_KEY),
        (
            "jump_reset_key",
            KeybindTarget::JumpReset,
            BOUND_JUMP_RESET_KEY,
        ),
    ] {
        let mut state = state_with_bindings();
        state.modules.clicker = true;
        state.modules.aim = true;
        state.modules.reach = true;
        state.modules.jump_reset = true;
        controls(&mut state, &[], &[(id, 1.0)]);
        let capturing: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
        assert_eq!(capturing["capture_key"], true);
        state.preferences_dirty = false;
        state.reservations_dirty = false;
        state.panel_dirty = false;
        // Escape takes precedence over other keys delivered in the same frame.
        controls(&mut state, &[old_key, "Escape"], &[]);
        assert!(state.binding_mut(target).is_empty());
        assert!(state.capturing_key.is_none());
        assert!(
            state.modules.clicker
                && state.modules.aim
                && state.modules.reach
                && state.modules.jump_reset
        );
        assert!(state.panel_dirty && state.preferences_dirty && state.reservations_dirty);
        assert!(!state.reserved_keys().contains(&old_key.into()));
        assert!(!state.reserved_keys().iter().any(String::is_empty));
        let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
        assert_eq!(panel["capture_key"], false);
        let keycap = panel["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|control| control["id"] == id)
            .unwrap();
        assert_eq!(keycap["key"], "None");
        let saved = serde_json::to_string(&state.preferences).unwrap();
        assert_eq!(Preferences::from_json(&saved), state.preferences);
        controls(&mut state, &[old_key, "Escape"], &[]);
        assert!(
            state.modules.clicker
                && state.modules.aim
                && state.modules.reach
                && state.modules.jump_reset
        );
        controls(&mut state, &[], &[(id, 1.0)]);
        controls(&mut state, &["KeyG"], &[]);
        assert_eq!(state.binding_mut(target), "KeyG");
    }
}

#[test]
fn all_module_bindings_can_remain_unbound_after_reload() {
    let prefs = Preferences::from_json(
        r#"{"clicker_key":"","aim_key":"","reach_key":"","jump_reset_key":""}"#,
    );
    assert!(
        prefs.clicker_key.is_empty()
            && prefs.aim_key.is_empty()
            && prefs.reach_key.is_empty()
            && prefs.jump_reset_key.is_empty()
    );
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&prefs).unwrap()),
        prefs
    );
    let mut state = State::new(prefs);
    assert_eq!(state.reserved_keys(), [PANEL_KEY]);
    controls(&mut state, &[""], &[]);
    assert!(
        !state.modules.clicker
            && !state.modules.aim
            && !state.modules.reach
            && !state.modules.jump_reset
    );
}

#[test]
fn escape_cancels_menu_key_capture_and_keeps_the_panel_reachable() {
    let mut state = state();
    state.preferences.menu_key = "F6".into();
    state.preferences_dirty = false;
    state.reservations_dirty = false;
    controls(&mut state, &[], &[("menu_key", 1.0)]);
    controls(&mut state, &["Escape"], &[]);
    assert_eq!(state.preferences.menu_key, "F6");
    assert!(state.capturing_key.is_none() && state.panel_dirty);
    assert!(!state.preferences_dirty && !state.reservations_dirty);
}

#[test]
fn absent_focus_ignores_panel_and_toggle_edges() {
    let mut state = state_with_bindings();
    state.controls(Controls {
        focused: false,
        panel_open: false,
        keys: &[BOUND_CLICKER_KEY.into()],
        held: &[],
        seconds: 0.0,
        events: &[Event {
            id: "aim".into(),
            value: 1.0,
        }],
    });
    assert!(!state.modules.clicker && !state.modules.aim);
}

#[test]
fn closing_panel_or_losing_focus_cancels_capture_before_gameplay_keys() {
    for (focused, panel_open) in [(true, false), (false, true)] {
        let mut state = state_with_bindings();
        state.reservations_dirty = false;
        controls(&mut state, &[], &[("clicker_key", 1.0)]);
        assert!(state.capturing_key.is_some());
        state.panel_dirty = false;
        state.controls(Controls {
            focused,
            panel_open,
            keys: &["KeyW".into()],
            held: &[],
            seconds: 0.0,
            events: &[],
        });
        assert!(state.capturing_key.is_none());
        assert!(state.panel_dirty);
        assert_eq!(state.preferences.clicker_key, BOUND_CLICKER_KEY);
        assert!(!state.reservations_dirty && !state.preferences_dirty);

        state.controls(Controls {
            focused: true,
            panel_open: false,
            keys: &["KeyW".into()],
            held: &[],
            seconds: 0.0,
            events: &[],
        });
        assert_eq!(state.preferences.clicker_key, BOUND_CLICKER_KEY);
        assert!(!state.modules.clicker && !state.reservations_dirty);
    }
}

#[test]
fn closed_panel_cannot_start_key_capture_and_preserves_normal_toggle() {
    let mut state = state_with_bindings();
    state.controls(Controls {
        focused: true,
        panel_open: false,
        keys: &[BOUND_CLICKER_KEY.into()],
        held: &[],
        seconds: 0.0,
        events: &[Event {
            id: "clicker_key".into(),
            value: 1.0,
        }],
    });
    assert!(state.capturing_key.is_none());
    assert!(state.modules.clicker);
    assert_eq!(state.preferences.clicker_key, BOUND_CLICKER_KEY);
}

#[test]
fn panel_has_every_control_without_native_chrome_and_only_rebuilds_after_change() {
    let mut state = state();
    let json: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    // Cinnabar panels carry only a surface, controls and input options.
    for removed in ["title", "dark", "style", "theme", "sections"] {
        assert!(json.get(removed).is_none(), "{removed}");
    }
    assert!(json["surface"]["document"].is_string());
    assert_eq!(json["toggle_key"], PANEL_KEY);
    assert!(
        json["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| control["id"] == "jump_reset")
    );
    assert!(state.panel_json().unwrap().len() < 128 * 1024);
    state.panel_dirty = false;
    controls(&mut state, &[], &[]);
    assert!(!state.panel_dirty);
    controls(&mut state, &[], &[("dark_mode", 0.0)]);
    assert!(
        !state.panel_dirty && !state.preferences_dirty,
        "themes are gone"
    );
    let json: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert!(json.get("dark").is_none());
    assert_eq!(json["reference_size"], serde_json::json!([760.0, 420.0]));
}

#[test]
fn netherite_finder_searches_its_saved_range_only_while_enabled() {
    let mut state = state();
    assert_eq!(state.finder_range(), None);
    state.preferences_dirty = false;
    controls(&mut state, &[], &[("netherite", 1.0)]);
    assert_eq!(state.finder_range(), Some(64));
    assert!(!state.preferences_dirty, "enablement is runtime state");
    controls(&mut state, &[], &[("netherite_range", 500.0)]);
    // Terrain searches reach the terrain distance limit and no further.
    assert_eq!(
        crate::finder::MAX_RANGE,
        mod_api::MAX_TERRAIN_DISTANCE as f32
    );
    assert_eq!(state.finder_range(), Some(128));
    assert!(state.preferences_dirty);
    let saved = serde_json::to_string(&state.preferences).unwrap();
    let restored = State::new(Preferences::from_json(&saved));
    assert_eq!(restored.preferences.netherite_range, 128);
    assert_eq!(restored.finder_range(), None);
    assert_eq!(
        Preferences::from_json(r#"{"netherite_range":999}"#).netherite_range,
        128
    );
    assert_eq!(
        Preferences::from_json(r#"{"netherite_range":0}"#).netherite_range,
        1
    );
    controls(&mut state, &[], &[("netherite", 0.0)]);
    assert_eq!(state.finder_range(), None);
}

#[test]
fn panel_exposes_all_controls_without_category_sections() {
    use std::collections::HashSet;

    let panel: Value = serde_json::from_str(&state().panel_json().unwrap()).unwrap();
    assert!(panel.get("sections").is_none());
    let ids: Vec<_> = panel["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|control| control["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
    assert_eq!(
        ids,
        [
            "menu_key",
            "clicker",
            "cps",
            "clicker_key",
            "aim",
            "aim_strength",
            "aim_mode",
            "aim_key",
            "reach",
            "reach_blocks",
            "reach_key",
            "jump_reset",
            "jump_reset_window_ticks",
            "jump_reset_key",
            "fake_lag",
            "fake_lag_ms",
            "show_real_position",
            "netherite",
            "netherite_range",
            "fullbright",
            "expand_clicker",
            "expand_aim",
            "expand_reach",
            "expand_jump_reset",
            "expand_fake_lag",
            "expand_netherite",
            "reset_layout",
            "drag_x",
            "drag_y",
            "aim_horizontal_only",
            "player_esp",
            "esp_range",
            "esp_color",
            "esp_names",
            "esp_distance",
            "esp_health",
            "expand_esp",
            "auto_totem",
        ]
    );
}

#[test]
fn reach_only_publishes_for_an_enabled_current_gameplay_frame() {
    let mut state = state();
    assert_eq!(state.attack_reach(true), None);
    controls(&mut state, &[], &[("reach", 1.0), ("reach_blocks", 4.6)]);
    assert_eq!(state.attack_reach(false), None);
    assert_eq!(state.attack_reach(true), Some(4.6));
    controls(&mut state, &[], &[("reach", 0.0)]);
    assert_eq!(state.attack_reach(true), None);
}

#[test]
fn clicker_uses_physical_first_press_and_never_catches_up_a_stall() {
    let mut state = state();
    state.modules.clicker = true;
    assert!(!state.attack_pulse(Some((1, 0, true, 0.05))));
    assert!(state.attack_pulse(Some((1, 0, true, 0.05))));
    assert!(state.attack_pulse(Some((1, 0, true, 3.0))));
    assert!(!state.attack_pulse(Some((1, 0, true, 0.01))));
    assert!(!state.attack_pulse(Some((1, 0, false, 0.05))));
    assert!(!state.attack_pulse(Some((1, 0, true, 0.05))));
}

#[test]
fn clicker_repeats_at_the_selected_rate_at_different_frame_rates() {
    for fps in [30, 60, 144] {
        let mut cadence = ClickCadence::default();
        assert!(!cadence.update(true, 20, Some((1, 0, true, 0.0))));
        let pulses = (0..fps)
            .filter(|_| cadence.update(true, 20, Some((1, 0, true, 1.0 / fps as f32))))
            .count();
        assert!((19..=20).contains(&pulses), "{fps} FPS: {pulses} pulses");
    }
}

#[test]
fn clicker_drops_repeat_state_on_every_authority_or_input_loss() {
    for loss in [
        None,
        Some((2, 0, true, 0.1)),
        Some((1, 1, true, 0.1)),
        Some((1, 0, false, 0.1)),
        Some((1, 0, true, f32::NAN)),
    ] {
        let mut cadence = ClickCadence::default();
        cadence.update(true, 20, Some((1, 0, true, 0.0)));
        assert!(cadence.update(true, 20, Some((1, 0, true, 0.05))));
        assert!(!cadence.update(true, 20, loss));
        assert!(!cadence.update(true, 20, Some((1, 0, true, 0.01))));
    }
}

#[test]
fn clicker_disabled_and_invalid_duration_never_produce_a_press() {
    let mut cadence = ClickCadence::default();
    assert!(!cadence.update(false, 20, Some((1, 0, true, 0.0))));
    assert!(!cadence.update(false, 20, Some((1, 0, true, 1.0))));
    assert!(!cadence.update(true, 20, Some((1, 0, true, -1.0))));
}

#[test]
fn every_module_has_a_persisted_independent_binding() {
    let old = Preferences::from_json(r#"{"clicker_key":"KeyV","cps":23}"#);
    assert_eq!(old.clicker_key, "KeyV");
    assert_eq!(old.cps, 23);
    assert!(old.aim_key.is_empty());
    assert_ne!(old.reach_key, old.clicker_key);
    let saved =
        Preferences::from_json(r#"{"clicker_key":"KeyC","aim_key":"KeyG","reach_key":"KeyH"}"#);
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&saved).unwrap()),
        saved
    );
    for field in ["aim_key", "reach_key", "jump_reset_key"] {
        for invalid in ["MouseLeft", "Escape", "Key:V"] {
            let prefs = Preferences::from_json(&format!(r#"{{"{field}":"{invalid}"}}"#));
            assert_eq!(prefs, Preferences::default());
        }
    }
}

#[test]
fn capture_each_module_consumes_the_edge_without_any_toggle() {
    for (id, target) in [
        ("clicker_key", KeybindTarget::Clicker),
        ("aim_key", KeybindTarget::Aim),
        ("reach_key", KeybindTarget::Reach),
        ("jump_reset_key", KeybindTarget::JumpReset),
    ] {
        let mut state = state();
        state.modules.aim = true;
        controls(&mut state, &[], &[(id, 1.0)]);
        assert_eq!(state.capturing_key, Some(target));
        controls(&mut state, &["KeyG"], &[]);
        assert_eq!(state.binding_mut(target).as_str(), "KeyG");
        assert!(state.capturing_key.is_none());
        assert!(!state.modules.clicker && state.modules.aim && !state.modules.reach);
        controls(&mut state, &["KeyG"], &[]);
        match target {
            KeybindTarget::Menu => unreachable!(),
            KeybindTarget::Clicker => {
                assert!(state.modules.clicker && state.modules.aim && !state.modules.reach)
            }
            KeybindTarget::Aim => {
                assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach)
            }
            KeybindTarget::Reach => {
                assert!(!state.modules.clicker && state.modules.aim && state.modules.reach)
            }
            KeybindTarget::JumpReset => {
                assert!(
                    state.modules.jump_reset
                        && !state.modules.clicker
                        && state.modules.aim
                        && !state.modules.reach
                )
            }
        }
    }
}

#[test]
fn conflicting_binding_stays_in_capture_until_a_free_key() {
    let mut state = state_with_bindings();
    controls(&mut state, &[], &[("aim_key", 1.0)]);
    controls(&mut state, &[BOUND_CLICKER_KEY], &[]);
    assert_eq!(state.capturing_key, Some(KeybindTarget::Aim));
    assert!(!state.modules.clicker && !state.modules.aim);
    controls(&mut state, &["KeyG"], &[]);
    assert!(state.capturing_key.is_none());
    assert_eq!(state.preferences.aim_key, "KeyG");
    assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
}

#[test]
fn reservations_are_unique_even_for_programmatically_shared_bindings() {
    let mut state = state_with_bindings();
    state.preferences.aim_key = state.preferences.clicker_key.clone();
    state.preferences.reach_key = state.preferences.clicker_key.clone();
    assert_eq!(
        state.reserved_keys(),
        [PANEL_KEY, BOUND_CLICKER_KEY, BOUND_JUMP_RESET_KEY]
    );
}

#[test]
fn capture_loss_consumes_module_edges_without_toggling() {
    for id in ["clicker_key", "aim_key", "reach_key"] {
        for (focused, panel_open) in [(true, false), (false, true)] {
            let mut state = state_with_bindings();
            controls(&mut state, &[], &[(id, 1.0)]);
            state.controls(Controls {
                focused,
                panel_open,
                keys: &[
                    BOUND_CLICKER_KEY.into(),
                    BOUND_AIM_KEY.into(),
                    BOUND_REACH_KEY.into(),
                ],
                held: &[],
                seconds: 0.0,
                events: &[],
            });
            assert!(state.capturing_key.is_none());
            assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
            assert_eq!(state.preferences, state_with_bindings().preferences);
        }
    }
    let mut state = state_with_bindings();
    state.modules.aim = true;
    controls(&mut state, &[], &[("reach_key", 1.0)]);
    state.controls(Controls {
        focused: true,
        panel_open: false,
        keys: &["F10".into()],
        held: &[],
        seconds: 0.0,
        events: &[],
    });
    assert!(state.modules.aim);
}

#[test]
fn fake_lag_delay_is_bounded_persisted_and_disabled_by_its_toggle() {
    let mut state = State::new(Preferences::from_json(
        r#"{"fake_lag_ms":5000,"fake_lag":true}"#,
    ));
    assert_eq!(
        state.preferences.fake_lag_ms,
        mod_api::MAX_PACKET_DELAY_MS as u16
    );
    assert_eq!(state.packet_delay_ms(), 0);
    controls(
        &mut state,
        &[],
        &[("fake_lag", 1.0), ("fake_lag_ms", 250.4)],
    );
    assert_eq!(state.packet_delay_ms(), 250);
    state.controls(Controls {
        focused: false,
        panel_open: false,
        keys: &[],
        held: &[],
        seconds: 0.0,
        events: &[],
    });
    assert_eq!(state.packet_delay_ms(), 250);
    controls(&mut state, &[], &[("fake_lag_ms", f32::NAN)]);
    assert_eq!(state.packet_delay_ms(), 250);
    controls(&mut state, &[], &[("fake_lag_ms", -5.0)]);
    assert_eq!(state.packet_delay_ms(), 0);
    controls(&mut state, &[], &[("fake_lag_ms", 800.0)]);
    controls(&mut state, &[], &[("fake_lag", 0.0)]);
    assert_eq!(state.packet_delay_ms(), 0);
    assert_eq!(state.preferences.fake_lag_ms, 800);
    let saved = serde_json::to_string(&state.preferences).unwrap();
    assert_eq!(Preferences::from_json(&saved).fake_lag_ms, 800);
    assert!(
        serde_json::from_str::<Value>(&saved)
            .unwrap()
            .get("fake_lag")
            .is_none()
    );
}

#[test]
fn fake_lag_panel_exposes_millisecond_slider_and_toggle() {
    let panel: Value = serde_json::from_str(&state().panel_json().unwrap()).unwrap();
    let controls = panel["controls"].as_array().unwrap();
    let delay = controls
        .iter()
        .find(|control| control["id"] == "fake_lag_ms")
        .unwrap();
    assert_eq!(delay["min"], 0.0);
    assert_eq!(delay["max"], mod_api::MAX_PACKET_DELAY_MS as f32);
    assert_eq!(delay["step"], 1.0);
    assert_eq!(delay["label"], "FakeLag delay (ms)");
    assert!(
        controls
            .iter()
            .any(|control| control["id"] == "fake_lag" && control["value"] == false)
    );
}

#[test]
fn real_position_preference_requires_live_nonzero_fake_lag() {
    let mut state = State::new(Preferences::from_json(r#"{"show_real_position":true}"#));
    assert!(!state.show_real_position());
    controls(&mut state, &[], &[("fake_lag", 1.0)]);
    assert!(state.show_real_position());
    controls(&mut state, &[], &[("fake_lag_ms", 0.0)]);
    assert!(!state.show_real_position());
    controls(
        &mut state,
        &[],
        &[("fake_lag_ms", 500.0), ("show_real_position", 0.0)],
    );
    assert!(!state.show_real_position());
    assert!(state.preferences_dirty);
    controls(&mut state, &[], &[("show_real_position", 1.0)]);
    assert!(state.show_real_position());
    let saved = serde_json::to_string(&state.preferences).unwrap();
    assert!(Preferences::from_json(&saved).show_real_position);
    controls(&mut state, &[], &[("fake_lag", 0.0)]);
    assert!(!state.show_real_position());
    assert!(state.preferences.show_real_position);
    let panel: serde_json::Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert!(
        panel["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| control["id"] == "show_real_position"
                && control["label"] == "Show real position")
    );
}

#[test]
fn fullbright_is_runtime_only_and_can_be_disabled_independently() {
    let mut state = State::new(Preferences::from_json(r#"{"fullbright":true}"#));
    assert!(!state.modules.fullbright);
    assert!(state.fullbright_dirty);
    state.fullbright_dirty = false;
    controls(&mut state, &[], &[("fullbright", 1.0)]);
    assert!(state.modules.fullbright && state.fullbright_dirty);
    assert!(!state.modules.netherite && !state.modules.clicker);
    assert!(!state.preferences_dirty);
    state.fullbright_dirty = false;
    controls(&mut state, &[], &[("netherite", 1.0)]);
    assert!(state.modules.fullbright && state.modules.netherite);
    assert!(!state.fullbright_dirty);
    controls(&mut state, &[], &[("fullbright", f32::NAN)]);
    assert!(state.modules.fullbright);
    controls(&mut state, &[], &[("fullbright", 0.0)]);
    assert!(!state.modules.fullbright && state.fullbright_dirty);
    assert!(state.modules.netherite);
    let saved = serde_json::to_value(&state.preferences).unwrap();
    assert!(saved.get("fullbright").is_none());
}

#[test]
fn fullbright_has_a_disabled_panel_toggle() {
    let panel: Value = serde_json::from_str(&state().panel_json().unwrap()).unwrap();
    assert!(
        panel["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| control["id"] == "fullbright"
                && control["value"] == false
                && control["label"] == "Fullbright")
    );
}

#[test]
fn jump_reset_is_off_by_default_and_can_be_disabled_in_the_panel() {
    let mut state = state_with_bindings();
    assert!(!state.modules.jump_reset);
    controls(&mut state, &[BOUND_JUMP_RESET_KEY], &[]);
    assert!(state.modules.jump_reset && !state.preferences_dirty);
    controls(&mut state, &[], &[("jump_reset", 0.0)]);
    assert!(!state.modules.jump_reset);
}

#[test]
fn jump_reset_preferences_are_bounded_and_binding_is_independent() {
    let restored = Preferences::from_json(
        r#"{"jump_reset":true,"jump_reset_window_ticks":999,"jump_reset_key":"KeyR"}"#,
    );
    assert_eq!(restored.jump_reset_window_ticks, MAX_WINDOW_TICKS);
    assert_ne!(restored.jump_reset_key, restored.aim_key);
    assert!(!State::new(restored).modules.jump_reset);
    let mut state = state_with_bindings();
    controls(
        &mut state,
        &[],
        &[("jump_reset_window_ticks", 0.0), ("jump_reset_key", 1.0)],
    );
    assert_eq!(state.preferences.jump_reset_window_ticks, MIN_WINDOW_TICKS);
    controls(&mut state, &[BOUND_AIM_KEY], &[]);
    assert!(state.capturing_key.is_some());
    controls(&mut state, &["KeyG"], &[]);
    assert_eq!(state.preferences.jump_reset_key, "KeyG");
    assert!(state.reserved_keys().contains(&"KeyG".into()));
    assert!(!state.modules.jump_reset);
    let json = serde_json::to_string(&state.preferences).unwrap();
    assert_eq!(Preferences::from_json(&json), state.preferences);
    controls(&mut state, &["KeyG"], &[]);
    assert!(state.modules.jump_reset);
}

#[test]
fn menu_binding_capture_updates_panel_reservations_and_saved_preferences() {
    let mut state = state_with_bindings();
    controls(&mut state, &[], &[("menu_key", 1.0)]);
    controls(&mut state, &[BOUND_CLICKER_KEY], &[]);
    assert_eq!(state.capturing_key, Some(KeybindTarget::Menu));
    controls(&mut state, &["F6"], &[]);
    assert!(state.capturing_key.is_none());
    assert_eq!(state.preferences.menu_key, "F6");
    assert!(state.preferences_dirty && state.reservations_dirty && state.panel_dirty);
    assert!(!state.reserved_keys().contains(&PANEL_KEY.into()));
    assert!(state.reserved_keys().contains(&"F6".into()));
    let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(panel["toggle_key"], "F6");
    let saved = serde_json::to_string(&state.preferences).unwrap();
    assert_eq!(Preferences::from_json(&saved), state.preferences);
}

#[test]
fn menu_defaults_to_f7_and_restores_invalid_preferences_independently() {
    assert_eq!(Preferences::default().menu_key, "F7");
    for key in ["Escape", "", "Key:V"] {
        let prefs = Preferences::from_json(&format!(r#"{{"menu_key":"{key}","cps":17}}"#));
        assert_eq!(prefs.menu_key, PANEL_KEY);
        assert_eq!(prefs.cps, 17);
    }
    let prefs = Preferences::from_json(r#"{"menu_key":"F6","clicker_key":"F6"}"#);
    assert_ne!(prefs.menu_key, prefs.clicker_key);
    assert_eq!(
        Preferences::from_json(r#"{"menu_key":"ShiftRight"}"#).menu_key,
        "ShiftRight"
    );
}

#[test]
fn adding_the_menu_binding_preserves_legacy_module_shortcuts() {
    let prefs = Preferences::from_json(r#"{"clicker_key":"F7","aim_key":"F6"}"#);
    assert_eq!(prefs.clicker_key, "F7");
    assert_eq!(prefs.aim_key, "F6");
    assert_ne!(prefs.menu_key, prefs.clicker_key);
    assert_ne!(prefs.menu_key, prefs.aim_key);
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&prefs).unwrap()),
        prefs
    );
}

#[test]
fn disclosure_does_not_enable_modules_or_persist_navigation() {
    let mut state = state();
    let preferences = state.preferences.clone();
    assert_eq!(state.expanded, [false; 9]);
    state.panel_dirty = false;
    state.reservations_dirty = false;
    controls(&mut state, &[], &[("expand_reach", 1.0)]);
    assert_eq!(
        state.expanded,
        [false, false, true, false, false, false, false, false, false]
    );
    assert!(!state.modules.reach);
    assert_eq!(state.preferences, preferences);
    assert!(state.panel_dirty);
    assert!(!state.preferences_dirty && !state.reservations_dirty);
    controls(&mut state, &[], &[("expand_aim", 1.0)]);
    assert_eq!(
        state.expanded,
        [false, true, true, false, false, false, false, false, false],
        "independent rows"
    );
    controls(&mut state, &[], &[("expand_reach", 1.0)]);
    assert_eq!(
        state.expanded,
        [false, true, false, false, false, false, false, false, false]
    );
    assert!(!state.preferences_dirty);
}

/// One frame of shared pointer-pad input, with MouseLeft edges.
fn pad(state: &mut State, _header: &str, pressed: bool, held: bool, point: Option<[f32; 2]>) {
    let keys: Vec<String> = if pressed {
        vec!["MouseLeft".into()]
    } else {
        Vec::new()
    };
    let held: Vec<String> = if held {
        vec!["MouseLeft".into()]
    } else {
        Vec::new()
    };
    let events: Vec<Event> = point
        .map(|[x, y]| {
            vec![
                Event {
                    id: "drag_x".into(),
                    value: x,
                },
                Event {
                    id: "drag_y".into(),
                    value: y,
                },
            ]
        })
        .unwrap_or_default();
    state.controls(Controls {
        focused: true,
        panel_open: true,
        keys: &keys,
        held: &held,
        seconds: 1.0 / 120.0,
        events: &events,
    });
}

fn pad_values(state: &State) -> Vec<f32> {
    let panel: Value = serde_json::from_str(&state.panel_shell_json().unwrap()).unwrap();
    panel["controls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|control| control["id"].as_str().unwrap().starts_with("drag_"))
        .map(|control| control["value"].as_f64().unwrap() as f32)
        .collect()
}

#[test]
fn dragging_a_header_moves_its_column_by_the_grab_point_and_persists_on_release() {
    let mut state = state();
    let [x, y] = state.preferences.columns[1];
    assert_eq!(
        pad_values(&state),
        [-2048.0, -2048.0],
        "idle pads always report a press"
    );
    pad(&mut state, "network", true, true, Some([x + 40.0, y + 8.0]));
    assert!(state.drag.is_some());
    assert_eq!(pad_values(&state), [x + 40.0, y + 8.0]);
    assert_eq!(
        state.preferences.columns[1],
        [x, y],
        "a press alone does not move"
    );
    pad(
        &mut state,
        "network",
        false,
        true,
        Some([x + 41.0, y + 9.0]),
    );
    assert_eq!(
        state.preferences.columns[1],
        [x, y],
        "below the drag threshold"
    );
    pad(
        &mut state,
        "network",
        false,
        true,
        Some([x + 70.0, y + 58.0]),
    );
    assert_eq!(state.preferences.columns[1], [x + 30.0, y + 50.0]);
    assert!(!state.preferences_dirty, "saved once, on release");
    pad(&mut state, "network", false, true, None);
    assert_eq!(
        state.preferences.columns[1],
        [x + 30.0, y + 50.0],
        "holding still keeps it"
    );
    pad(&mut state, "network", false, false, None);
    assert!(state.drag.is_none() && state.preferences_dirty);
    assert!(!state.preferences.collapsed[1], "a drag never collapses");
    assert_eq!(pad_values(&state), [-2048.0, -2048.0]);
    let saved = serde_json::to_string(&state.preferences).unwrap();
    assert_eq!(
        Preferences::from_json(&saved).columns[1],
        [x + 30.0, y + 50.0]
    );
}

#[test]
fn module_columns_drag_anywhere_on_screen_but_settings_stays_reachable() {
    let mut state = state();
    pad(&mut state, "combat", true, true, Some([-370.0, -160.0]));
    pad(&mut state, "combat", false, true, Some([-900.0, 520.0]));
    assert_eq!(
        state.preferences.columns[0],
        [-910.0, 514.0],
        "beyond the reference box"
    );
    pad(&mut state, "combat", false, true, Some([2040.0, -2040.0]));
    assert_eq!(
        state.preferences.columns[0],
        [1898.0, -2046.0],
        "kept on the pad"
    );
    pad(&mut state, "combat", false, false, None);
    pad(&mut state, "menu", true, true, Some([240.0, -160.0]));
    pad(&mut state, "menu", false, true, Some([1500.0, 900.0]));
    let [width, height] = crate::panel::REFERENCE;
    assert_eq!(
        state.preferences.columns[3],
        [
            width / 2.0 - crate::panel::COLUMN_WIDTH,
            height / 2.0 - 22.0
        ]
    );
}

#[test]
fn clicking_a_header_collapses_its_column_and_reset_restores_the_layout() {
    let mut state = state();
    pad(&mut state, "render", true, true, Some([40.0, -160.0]));
    pad(&mut state, "render", false, false, Some([41.0, -160.0]));
    assert!(state.drag.is_none());
    assert_eq!(state.preferences.collapsed, [false, false, true, false]);
    assert!(state.preferences_dirty);
    pad(&mut state, "render", true, true, Some([40.0, -160.0]));
    pad(&mut state, "render", false, true, Some([140.0, -100.0]));
    pad(&mut state, "render", false, false, None);
    assert!(
        state.preferences.collapsed[2],
        "dragging keeps the collapsed state"
    );
    assert_ne!(
        state.preferences.columns[2],
        crate::panel::DEFAULT_COLUMNS[2]
    );
    controls(&mut state, &[], &[("reset_layout", 1.0)]);
    assert_eq!(state.preferences.columns, crate::panel::DEFAULT_COLUMNS);
    assert_eq!(state.preferences.collapsed, [false; 4]);
}

#[test]
fn closing_the_panel_cancels_a_drag_without_collapsing() {
    let mut state = state();
    pad(&mut state, "combat", true, true, Some([-300.0, -160.0]));
    state.controls(Controls {
        focused: true,
        panel_open: false,
        keys: &[],
        held: &["MouseLeft".into()],
        seconds: 0.0,
        events: &[],
    });
    assert!(state.drag.is_none());
    assert_eq!(state.preferences.collapsed, [false; 4]);
    assert_eq!(pad_values(&state)[0], -2048.0);
}

#[test]
fn the_press_point_selects_the_topmost_visible_header() {
    let mut state = state();
    state.preferences.columns[2] = [-370.0, -170.0];
    pad(&mut state, "", true, true, Some([-300.0, -160.0]));
    assert_eq!(
        state.drag.map(|drag| drag.column),
        Some(2),
        "Render draws above Combat"
    );
    pad(&mut state, "", false, false, None);
    state.preferences.columns[3] = [-370.0, -170.0];
    pad(&mut state, "", true, true, Some([-300.0, -160.0]));
    assert_eq!(
        state.drag.map(|drag| drag.column),
        Some(3),
        "Settings draws above the module columns"
    );
    pad(&mut state, "", false, false, None);
    pad(&mut state, "", true, true, Some([300.0, 100.0]));
    assert!(state.drag.is_none(), "no header under the pointer");
}

#[test]
fn pad_samples_without_a_fresh_press_never_start_a_drag() {
    let mut state = state();
    pad(&mut state, "combat", false, true, Some([-300.0, -170.0]));
    assert!(state.drag.is_none());
    assert_eq!(state.preferences.columns, crate::panel::DEFAULT_COLUMNS);
}

#[test]
fn changing_disclosures_cancels_capture_without_clearing_a_binding() {
    let mut state = state_with_bindings();
    controls(&mut state, &[], &[("clicker_key", 1.0)]);
    assert!(state.capturing_key.is_some());
    controls(&mut state, &[], &[("expand_aim", 1.0)]);
    assert!(state.capturing_key.is_none());
    assert_eq!(state.preferences.clicker_key, BOUND_CLICKER_KEY);
    assert!(!state.preferences_dirty);
}

#[test]
fn default_module_shortcuts_are_unbound_and_do_not_reserve_or_toggle_old_keys() {
    for preferences in [
        Preferences::default(),
        Preferences::from_json("{}"),
        Preferences::from_json("broken"),
    ] {
        let mut state = State::new(preferences);
        assert!(state.preferences.clicker_key.is_empty());
        assert!(state.preferences.aim_key.is_empty());
        assert!(state.preferences.reach_key.is_empty());
        assert!(state.preferences.jump_reset_key.is_empty());
        assert_eq!(state.reserved_keys(), [PANEL_KEY]);
        controls(&mut state, &["F8", "KeyR", "KeyV", "KeyB"], &[]);
        assert!(
            !state.modules.clicker
                && !state.modules.aim
                && !state.modules.reach
                && !state.modules.jump_reset
        );
        let panel: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
        for control in panel["controls"].as_array().unwrap() {
            if ["clicker_key", "aim_key", "reach_key", "jump_reset_key"]
                .iter()
                .any(|id| control["id"] == *id)
            {
                assert_eq!(control["key"], "None");
            }
        }
    }
}

#[test]
fn duplicate_saved_module_shortcuts_clear_without_assigning_new_keys() {
    let preferences = Preferences::from_json(
        r#"{"clicker_key":"KeyG","aim_key":"KeyG","reach_key":"KeyG","jump_reset_key":"KeyG"}"#,
    );
    assert_eq!(preferences.clicker_key, "KeyG");
    assert!(
        preferences.aim_key.is_empty()
            && preferences.reach_key.is_empty()
            && preferences.jump_reset_key.is_empty()
    );
}

#[test]
fn esp_preferences_round_trip_clamp_and_do_not_restore_enablement() {
    let p = Preferences::from_json(
        r#"{"player_esp":true,"esp_range":999,"esp_color":999,"esp_names":false,"esp_distance":false,"esp_health":false}"#,
    );
    assert_eq!(p.esp_range, 128);
    assert_eq!(p.esp_color, 3);
    assert!(!p.esp_names && !p.esp_distance && !p.esp_health);
    assert!(!State::new(p.clone()).modules.player_esp);
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&p).unwrap()),
        p
    );
    let mut s = state();
    controls(&mut s, &[], &[("player_esp", 1.), ("expand_esp", 1.)]);
    assert!(s.modules.player_esp && s.expanded[7]);
    assert!(!s.preferences_dirty);
    controls(
        &mut s,
        &[],
        &[("esp_range", 0.), ("esp_color", -1.), ("esp_health", 0.)],
    );
    assert_eq!(s.preferences.esp_range, 1);
    assert_eq!(s.preferences.esp_color, 0);
    assert!(!s.preferences.esp_health);
    assert!(s.preferences_dirty);
}

/// A refusal latches repeats off across time, settings and authority gaps until admission ends.
#[test]
fn refused_attack_waits_for_an_admitted_release_and_a_fresh_cadence() {
    let mut state = state();
    state.modules.clicker = true;
    assert!(!state.attack_pulse(Some((1, 0, true, 0.05))));
    assert!(state.attack_pulse(Some((1, 0, true, 0.05))));
    state.attack_refused();
    for _ in 0..5 {
        assert!(!state.attack_pulse(Some((1, 0, true, 1.0))));
    }
    assert!(!state.attack_pulse(None));
    state.modules.clicker = false;
    assert!(!state.attack_pulse(Some((2, 1, true, 1.0))));
    state.modules.clicker = true;
    assert!(!state.attack_pulse(Some((2, 1, true, 1.0))));
    assert!(!state.attack_pulse(Some((2, 1, false, 0.05))));
    assert!(!state.attack_pulse(Some((2, 1, true, 0.05))));
    assert!(state.attack_pulse(Some((2, 1, true, 0.05))));
}

#[test]
fn auto_totem_is_a_runtime_toggle_and_does_not_persist_enabled_state() {
    let mut state = state();
    assert!(!state.modules.auto_totem);
    state.preferences_dirty = false;
    controls(&mut state, &[], &[("auto_totem", 1.)]);
    assert!(state.modules.auto_totem);
    assert!(!state.preferences_dirty);
    assert!(!State::new(state.preferences.clone()).modules.auto_totem);
    controls(&mut state, &[], &[("auto_totem", 0.)]);
    assert!(!state.modules.auto_totem);
}
