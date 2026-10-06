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
    assert_eq!(json["netherite_range"], 64);
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
            DEFAULT_REACH_KEY,
            DEFAULT_JUMP_RESET_KEY
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
    assert!(
        json["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| control["id"] == "jump_reset")
    );
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
fn netherite_finder_is_full_pink_loaded_block_selection_and_stop_clears_it() {
    let mut state = state();
    assert!(state.block_highlights().is_none());
    state.finder_dirty = false;
    state.preferences_dirty = false;
    controls(&mut state, &[], &[("netherite", 1.0)]);
    let spec = state.block_highlights().unwrap();
    assert_eq!(
        spec.identifiers,
        ["minecraft:ancient_debris", "minecraft:netherite_block"]
    );
    assert_eq!(spec.range, 64.0);
    assert_eq!(spec.color[3], 1.0);
    assert!(spec.color[0] > spec.color[1] && spec.color[2] > spec.color[1]);
    assert!(state.finder_dirty && !state.preferences_dirty);
    controls(&mut state, &[], &[("netherite_range", 500.0)]);
    assert_eq!(
        state.block_highlights().unwrap().range,
        mod_api::MAX_BLOCK_HIGHLIGHT_RANGE
    );
    let saved = serde_json::to_string(&state.preferences).unwrap();
    let restored = State::new(Preferences::from_json(&saved));
    assert_eq!(
        restored.preferences.netherite_range,
        mod_api::MAX_BLOCK_HIGHLIGHT_RANGE as u8
    );
    assert!(restored.block_highlights().is_none());
    state.finder_dirty = false;
    controls(&mut state, &[STOP_KEY], &[]);
    assert!(state.block_highlights().is_none() && state.finder_dirty);
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
    assert_eq!(
        categories,
        HashSet::from(["Combat", "Settings", "Visual", "Movement"])
    );
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
    for field in ["aim_key", "reach_key", "jump_reset_key"] {
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
        [
            PANEL_KEY,
            STOP_KEY,
            DEFAULT_CLICKER_KEY,
            DEFAULT_JUMP_RESET_KEY
        ]
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

#[test]
fn fake_lag_delay_is_bounded_persisted_and_disabled_by_stop() {
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
        events: &[],
    });
    assert_eq!(state.packet_delay_ms(), 250);
    controls(&mut state, &[], &[("fake_lag_ms", f32::NAN)]);
    assert_eq!(state.packet_delay_ms(), 250);
    controls(&mut state, &[], &[("fake_lag_ms", -5.0)]);
    assert_eq!(state.packet_delay_ms(), 0);
    controls(&mut state, &[], &[("fake_lag_ms", 800.0)]);
    controls(&mut state, &[STOP_KEY], &[]);
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
    assert_eq!(delay["label"], "Delay (ms)");
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
    state.stop_all();
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
fn fullbright_is_runtime_only_independent_and_stopped_by_f10() {
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
    controls(&mut state, &[STOP_KEY], &[]);
    assert!(!state.modules.fullbright && state.fullbright_dirty);
    assert!(!state.modules.netherite);
    let saved = serde_json::to_value(&state.preferences).unwrap();
    assert!(saved.get("fullbright").is_none());
}

#[test]
fn fullbright_has_a_visual_toggle_and_preserves_category_order() {
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
    let sections = panel["sections"].as_array().unwrap();
    let fullbright = sections
        .iter()
        .find(|section| section["toggle"] == "fullbright")
        .unwrap();
    assert_eq!(fullbright["category"], "Visual");
    let mut categories: Vec<&str> = sections
        .iter()
        .map(|section| section["category"].as_str().unwrap())
        .collect();
    categories.sort_unstable();
    categories.dedup();
    assert_eq!(categories, ["Combat", "Movement", "Settings", "Visual"]);
}

#[test]
fn jump_reset_is_off_by_default_and_emergency_stop_disables_it() {
    let mut state = state();
    assert!(!state.modules.jump_reset);
    controls(&mut state, &[DEFAULT_JUMP_RESET_KEY], &[]);
    assert!(state.modules.jump_reset && !state.preferences_dirty);
    controls(
        &mut state,
        &[STOP_KEY, DEFAULT_JUMP_RESET_KEY],
        &[("jump_reset", 1.0)],
    );
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
    let mut state = state();
    controls(
        &mut state,
        &[],
        &[("jump_reset_window_ticks", 0.0), ("jump_reset_key", 1.0)],
    );
    assert_eq!(state.preferences.jump_reset_window_ticks, MIN_WINDOW_TICKS);
    controls(&mut state, &[DEFAULT_AIM_KEY], &[]);
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
