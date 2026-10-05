use super::*;

fn controls(state: &mut State, keys: &[&str], events: &[(&str, f32)]) {
    state.controls(Controls {
        focused: true,
        panel_open: true,
        keys: &keys.iter().map(|key| (*key).into()).collect::<Vec<_>>(),
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

#[test]
fn preferences_restore_without_enabling_any_module() {
    let preferences = Preferences::from_json(
        r#"{"cps":18,"aim_strength":70,"aim_mode":"continuous","reach_blocks":5.2,"dark_mode":false,"clicker_key":"KeyV","clicker":true,"aim":true,"reach":true,"cinnabar_path":"unused"}"#,
    );
    let state = State::new(preferences);
    assert_eq!(state.preferences.cps, 18);
    assert_eq!(state.preferences.aim_strength, 70);
    assert_eq!(state.preferences.aim_mode, AimMode::Continuous);
    assert_eq!(state.preferences.reach_blocks, 5.2);
    assert!(!state.preferences.dark_mode);
    assert_eq!(state.preferences.clicker_key, "KeyV");
    assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
    let json = serde_json::to_value(&state.preferences).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 8);
    assert!(json.get("aim").is_none());
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
    assert!(!prefs.dark_mode);
    for key in ["F10", "ShiftRight"] {
        let prefs = Preferences::from_json(&format!(r#"{{"clicker_key":"{key}"}}"#));
        assert_eq!(prefs.clicker_key, DEFAULT_CLICKER_KEY);
    }
    assert_eq!(Preferences::from_json("broken"), Preferences::default());
}

#[test]
fn enabling_clicker_keeps_aim_and_reach_independent() {
    let mut state = state();
    controls(&mut state, &[], &[("aim", 1.0), ("reach", 1.0)]);
    controls(&mut state, &[DEFAULT_CLICKER_KEY], &[]);
    assert!(state.modules.clicker && state.modules.aim && state.modules.reach);
    controls(&mut state, &[], &[("clicker", 0.0)]);
    assert!(!state.modules.clicker && state.modules.aim && state.modules.reach);
    assert!(!state.preferences_dirty);
}

#[test]
fn emergency_stop_wins_over_every_other_edge() {
    let mut state = state();
    controls(
        &mut state,
        &[],
        &[("aim", 1.0), ("reach", 1.0), ("clicker", 1.0)],
    );
    controls(
        &mut state,
        &[STOP_KEY, DEFAULT_CLICKER_KEY],
        &[("aim", 1.0), ("clicker_key", 1.0)],
    );
    assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
    assert!(state.capturing_key.is_none());
    controls(&mut state, &[], &[("stop_all", 1.0), ("clicker", 1.0)]);
    assert!(!state.modules.clicker);
}

#[test]
fn key_capture_changes_the_binding_without_toggling_a_module() {
    let mut state = state();
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
            STOP_KEY,
            "ControlRight",
            DEFAULT_AIM_KEY,
            DEFAULT_REACH_KEY
        ]
    );
    controls(&mut state, &["ControlRight"], &[]);
    assert!(state.modules.clicker);
}

#[test]
fn escape_can_be_captured_without_disabling_or_toggling_modules() {
    let mut state = state();
    state.modules.aim = true;
    state.modules.reach = true;
    let before: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(before["capture_key"], false);
    controls(&mut state, &[], &[("clicker_key", 1.0)]);
    let capturing: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(capturing["capture_key"], true);
    controls(&mut state, &["Escape"], &[]);
    assert_eq!(state.preferences.clicker_key, "Escape");
    assert!(state.capturing_key.is_none() && !state.modules.clicker);
    assert!(state.modules.aim && state.modules.reach);
    assert!(state.reservations_dirty && state.preferences_dirty);
    let captured: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(captured["capture_key"], false);
}

#[test]
fn absent_focus_ignores_panel_and_toggle_edges() {
    let mut state = state();
    state.controls(Controls {
        focused: false,
        panel_open: false,
        keys: &[DEFAULT_CLICKER_KEY.into()],
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
        let mut state = state();
        state.reservations_dirty = false;
        controls(&mut state, &[], &[("clicker_key", 1.0)]);
        assert!(state.capturing_key.is_some());
        state.panel_dirty = false;
        state.controls(Controls {
            focused,
            panel_open,
            keys: &["KeyW".into()],
            events: &[],
        });
        assert!(state.capturing_key.is_none());
        assert!(state.panel_dirty);
        assert_eq!(state.preferences.clicker_key, DEFAULT_CLICKER_KEY);
        assert!(!state.reservations_dirty && !state.preferences_dirty);

        state.controls(Controls {
            focused: true,
            panel_open: false,
            keys: &["KeyW".into()],
            events: &[],
        });
        assert_eq!(state.preferences.clicker_key, DEFAULT_CLICKER_KEY);
        assert!(!state.modules.clicker && !state.reservations_dirty);
    }
}

#[test]
fn closed_panel_cannot_start_key_capture_and_preserves_normal_toggle() {
    let mut state = state();
    state.controls(Controls {
        focused: true,
        panel_open: false,
        keys: &[DEFAULT_CLICKER_KEY.into()],
        events: &[Event {
            id: "clicker_key".into(),
            value: 1.0,
        }],
    });
    assert!(state.capturing_key.is_none());
    assert!(state.modules.clicker);
    assert_eq!(state.preferences.clicker_key, DEFAULT_CLICKER_KEY);
}

#[test]
fn panel_has_every_control_in_both_themes_and_only_rebuilds_after_change() {
    let mut state = state();
    let json: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert_eq!(json["title"], PRODUCT_NAME);
    assert_eq!(json["toggle_key"], PANEL_KEY);
    assert_eq!(json["controls"].as_array().unwrap().len(), 12);
    assert!(json["dark"].as_bool().unwrap());
    assert!(state.panel_json().unwrap().len() < 16 * 1024);
    state.panel_dirty = false;
    controls(&mut state, &[], &[]);
    assert!(!state.panel_dirty);
    controls(&mut state, &[], &[("dark_mode", 0.0)]);
    assert!(state.panel_dirty && state.preferences_dirty);
    let json: Value = serde_json::from_str(&state.panel_json().unwrap()).unwrap();
    assert!(!json["dark"].as_bool().unwrap());
}

#[test]
fn grouped_cards_expose_every_control_once_in_combat_or_settings() {
    use std::collections::HashSet;

    let json: Value = serde_json::from_str(&state().panel_json().unwrap()).unwrap();
    let sections = json["sections"].as_array().unwrap();
    let mut assigned = HashSet::new();
    let mut categories = HashSet::new();
    for section in sections {
        categories.insert(section["category"].as_str().unwrap());
        if let Some(toggle) = section["toggle"].as_str() {
            assert!(assigned.insert(toggle));
            let control = json["controls"]
                .as_array()
                .unwrap()
                .iter()
                .find(|control| control["id"] == toggle)
                .unwrap();
            assert_eq!(control["kind"], "toggle");
        }
        for id in section["controls"].as_array().unwrap() {
            assert!(assigned.insert(id.as_str().unwrap()));
        }
    }
    assert_eq!(categories, HashSet::from(["Combat", "Settings"]));
    let controls: HashSet<_> = json["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|control| control["id"].as_str().unwrap())
        .collect();
    assert_eq!(assigned, controls);
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
    assert_eq!(old.aim_key, DEFAULT_AIM_KEY);
    assert_ne!(old.reach_key, old.clicker_key);
    let saved =
        Preferences::from_json(r#"{"clicker_key":"KeyC","aim_key":"KeyG","reach_key":"KeyH"}"#);
    assert_eq!(
        Preferences::from_json(&serde_json::to_string(&saved).unwrap()),
        saved
    );
    for field in ["aim_key", "reach_key"] {
        for invalid in ["F10", "ShiftRight", "", "Key:V"] {
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
            KeybindTarget::Clicker => {
                assert!(state.modules.clicker && state.modules.aim && !state.modules.reach)
            }
            KeybindTarget::Aim => {
                assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach)
            }
            KeybindTarget::Reach => {
                assert!(!state.modules.clicker && state.modules.aim && state.modules.reach)
            }
        }
    }
}

#[test]
fn conflicting_binding_stays_in_capture_and_stop_still_wins() {
    let mut state = state();
    controls(&mut state, &[], &[("aim_key", 1.0)]);
    controls(&mut state, &[DEFAULT_CLICKER_KEY], &[]);
    assert_eq!(state.capturing_key, Some(KeybindTarget::Aim));
    assert!(!state.modules.clicker && !state.modules.aim);
    controls(&mut state, &[STOP_KEY, "KeyG"], &[]);
    assert!(state.capturing_key.is_none());
    assert_eq!(state.preferences.aim_key, DEFAULT_AIM_KEY);
    assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
}

#[test]
fn reservations_are_unique_even_for_programmatically_shared_bindings() {
    let mut state = state();
    state.preferences.aim_key = state.preferences.clicker_key.clone();
    state.preferences.reach_key = state.preferences.clicker_key.clone();
    assert_eq!(
        state.reserved_keys(),
        [PANEL_KEY, STOP_KEY, DEFAULT_CLICKER_KEY]
    );
}

#[test]
fn capture_loss_consumes_module_edges_but_preserves_emergency_stop() {
    for id in ["clicker_key", "aim_key", "reach_key"] {
        for (focused, panel_open) in [(true, false), (false, true)] {
            let mut state = state();
            controls(&mut state, &[], &[(id, 1.0)]);
            state.controls(Controls {
                focused,
                panel_open,
                keys: &[
                    DEFAULT_CLICKER_KEY.into(),
                    DEFAULT_AIM_KEY.into(),
                    DEFAULT_REACH_KEY.into(),
                ],
                events: &[],
            });
            assert!(state.capturing_key.is_none());
            assert!(!state.modules.clicker && !state.modules.aim && !state.modules.reach);
            assert_eq!(state.preferences, Preferences::default());
        }
    }
    let mut state = state();
    state.modules.aim = true;
    controls(&mut state, &[], &[("reach_key", 1.0)]);
    state.controls(Controls {
        focused: true,
        panel_open: false,
        keys: &[STOP_KEY.into()],
        events: &[],
    });
    assert!(!state.modules.aim);
}
