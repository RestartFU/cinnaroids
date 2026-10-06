use mod_host::{ControlEvent, ModGrants, ModHost, empty_controls};

#[test]
fn saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for settings in [
        "{}",
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
            assert!(
                host.panel()
                    .unwrap()
                    .controls
                    .iter()
                    .any(|control| control.id() == "jump_reset")
            );
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
        Some(movement(13, 2, true)),
        controls,
    )
    .unwrap();
    assert!(!host.take_jump_pulse());
    assert!(host.take_jump_cancel());
}
