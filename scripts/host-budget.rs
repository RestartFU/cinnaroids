use mod_host::{empty_controls, ControlEvent, ModGrants, ModHost};

#[test]
fn saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for settings in [
        "{}",
        r#"{"clicker_key":"F7","aim_key":"F6","menu_key":"F7"}"#,
        r#"{"cps":20,"toggle_key":"VK:119","aim_strength":100,"aim_mode":"continuous","dark_mode":true}"#,
        r#"{"cps":30,"clicker_key":"NumpadSubtract","aim_key":"ArrowRight","reach_key":"NumpadDecimal","aim_strength":100,"aim_mode":"continuous","reach_blocks":6,"dark_mode":false,"fake_lag_ms":1000,"show_real_position":true,"netherite_range":128}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.wasm");
        std::fs::write(path.with_extension("settings.json"), settings).unwrap();
        let mut host = ModHost::load_snapshot_with_grants(
            &path,
            &component,
            ModGrants {
                players: true,
                camera: true,
                controls: true,
                interaction: true,
                settings: true,
                packet_delay: true,
                block_highlights: true,
                fullbright: true,
                movement: true,
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("init settings {settings}: {error:#}"));
        for (id, value) in [
            ("expand_clicker", 1.0),
            ("expand_aim", 1.0),
            ("expand_reach", 1.0),
            ("expand_jump_reset", 1.0),
            ("expand_fake_lag", 1.0),
            ("expand_netherite", 1.0),
            ("expand_netherite", 1.0),
            ("menu_settings", 1.0),
            ("menu_settings", 1.0),
            ("fullbright", 1.0),
            ("fullbright", 0.0),
            ("fullbright", 1.0),
            ("netherite", 1.0),
            ("netherite_range", 128.0),
            ("dark_mode", 1.0),
            ("aim", 1.0),
            ("clicker", 1.0),
            ("jump_reset", 1.0),
            ("jump_reset_window_ticks", 10.0),
            ("stop_all", 1.0),
        ] {
            let mut controls = empty_controls();
            controls.focused = true;
            controls.panel_open = true;
            controls.events.push(ControlEvent {
                id: id.into(),
                value,
            });
            host.frame_with_controls(false, None, controls)
                .unwrap_or_else(|error| panic!("event {id}, settings {settings}: {error:#}"));
            assert!(host.is_active());
            assert!(host
                .panel()
                .unwrap()
                .controls
                .iter()
                .any(|control| control.id() == "jump_reset"));
            if id == "netherite" || id == "netherite_range" {
                let spec = host.block_highlights().unwrap();
                assert_eq!(
                    spec.identifiers,
                    ["minecraft:ancient_debris", "minecraft:netherite_block"]
                );
                assert_eq!(spec.color, [1.0, 0.12, 0.55, 1.0]);
                if id == "netherite_range" {
                    assert_eq!(spec.range, 128.0);
                }
                assert!(host.fullbright(), "Finder does not disable Fullbright");
            }
            if id == "fullbright" {
                assert_eq!(host.fullbright(), value >= 0.5);
            }
            if id == "stop_all" {
                assert!(!host.fullbright());
            }
        }
        assert!(host.block_highlights().is_none());
        assert!(!host.fullbright());
    }
}

#[test]
fn jump_reset_uses_the_real_component_movement_imports_and_stops_at_f10() {
    use mod_host::{GameplayMovementSnapshot, GameplaySnapshot, GameplayVector3};
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jump-reset.wasm");
    let mut host = ModHost::load_snapshot_with_grants(
        &path,
        &component,
        ModGrants {
            players: true,
            controls: true,
            movement: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "jump_reset".into(),
        value: 1.0,
    });
    host.frame_with_controls(false, None, controls).unwrap();
    let frame = || GameplaySnapshot {
        session: 1,
        dimension: 0,
        eye: GameplayVector3 {
            x: 0.0,
            y: 2.62,
            z: 0.0,
        },
        yaw: 0.0,
        pitch: 0.0,
        attack_held: false,
        frame_seconds: 1.0 / 120.0,
        players: Vec::new(),
    };
    let movement = |tick, sequence, grounded| GameplayMovementSnapshot {
        session: 1,
        dimension: 0,
        tick,
        knockback_sequence: sequence,
        on_ground: grounded,
        jump_held: false,
        eligible: true,
        velocity: GameplayVector3 {
            x: 0.2,
            y: -0.08,
            z: 0.0,
        },
    };
    for (tick, sequence, grounded, expected) in [
        (10, 0, true, false),
        (11, 1, false, false),
        (12, 1, true, true),
        (12, 1, true, false),
    ] {
        let mut controls = empty_controls();
        controls.focused = true;
        controls.gameplay = true;
        host.frame_with_movement(
            false,
            Some(frame()),
            Vec::new(),
            None,
            Some(movement(tick, sequence, grounded)),
            controls,
        )
        .unwrap();
        assert_eq!(host.take_jump_pulse(), expected);
        assert!(!host.take_jump_pulse(), "pulse is consumed once");
    }
    let mut controls = empty_controls();
    controls.focused = true;
    controls.gameplay = true;
    controls.keys_pressed.push("F10".into());
    host.frame_with_movement(
        false,
        Some(frame()),
        Vec::new(),
        None,
        Some(movement(13, 2, true)),
        controls,
    )
    .unwrap();
    assert!(!host.take_jump_pulse());
    assert!(host.take_jump_cancel());
}

#[test]
fn menu_rebinding_publishes_and_saves_within_the_real_host_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("menu.wasm");
    let mut host = ModHost::load_snapshot_with_grants(
        &path,
        &component,
        ModGrants {
            controls: true,
            settings: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut controls = empty_controls();
    controls.focused = true;
    host.frame_with_controls(false, None, controls).unwrap();
    assert_eq!(host.panel().unwrap().toggle_key, "F7");
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "menu_key".into(),
        value: 1.0,
    });
    host.frame_with_controls(false, None, controls).unwrap();
    assert!(host.panel().unwrap().capture_key);
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.keys_pressed.push("F6".into());
    host.frame_with_controls(false, None, controls).unwrap();
    assert!(host.is_active());
    assert_eq!(host.panel().unwrap().toggle_key, "F6");
    assert!(!host.panel().unwrap().capture_key);
    assert!(host.reserved_keys().contains(&"F6".into()));
    assert!(!host.reserved_keys().contains(&"F7".into()));
    let preferences: serde_json::Value =
        serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
    assert_eq!(preferences["menu_key"], "F6");
}

#[test]
fn single_file_import_declares_metadata_and_loads_with_its_requested_grants() {
    let source = std::path::PathBuf::from(
        std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap(),
    );
    let declarations = mod_host::declarations(&source).unwrap();
    assert_eq!(declarations.metadata.name.as_deref(), Some("Cinnaroids"));
    assert!(declarations.metadata.version.is_some());
    assert!(declarations.metadata.icon.is_some());
    let grants = declarations.requested.unwrap().expect("explicit import permissions");
    assert!(grants.controls && grants.settings && grants.hud);
    assert!(grants.players && grants.camera && grants.interaction);
    assert!(grants.packet_delay && grants.movement);
    assert!(grants.fullbright && grants.block_highlights);
    assert!(!grants.environment && !grants.render && !grants.player_state);
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("cinnaroids.wasm");
    let component = std::fs::read(&source).unwrap();
    let mut host = ModHost::load_snapshot_with_grants(&installed, &component, grants).unwrap();
    let mut controls = empty_controls();
    controls.focused = true;
    host.frame_with_controls(false, None, controls).unwrap();
    assert!(host.is_active());
    assert_eq!(host.panel().unwrap().toggle_key, "F7");
}
