use mod_host::{empty_controls, ControlEvent, ModGrants, ModHost};

#[test]
fn saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for settings in [
        "{}",
        r#"{"clicker_key":"F7","aim_key":"F6","menu_key":"F7"}"#,
        r#"{"cps":20,"toggle_key":"VK:119","aim_strength":100,"aim_mode":"continuous","dark_mode":true,"columns":[[0,0],[-9000,9000]],"collapsed":[true,true]}"#,
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
            ("expand_esp", 1.0),
            ("esp_range", 128.0),
            ("esp_color", 3.0),
            ("esp_names", 0.0),
            ("esp_distance", 0.0),
            ("esp_health", 0.0),
            ("expand_netherite", 1.0),
            ("fullbright", 1.0),
            ("fullbright", 0.0),
            ("fullbright", 1.0),
            ("netherite", 1.0),
            ("netherite_range", 128.0),
            ("reset_layout", 1.0),
            ("aim", 1.0),
            ("clicker", 1.0),
            ("jump_reset", 1.0),
            ("jump_reset_window_ticks", 10.0),
            ("fullbright", 0.0),
            ("netherite", 0.0),
            ("jump_reset", 0.0),
            ("aim", 0.0),
            ("clicker", 0.0),
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
            if (id == "netherite" && value >= 0.5) || id == "netherite_range" {
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
        }
        assert!(host.block_highlights().is_none());
        assert!(!host.fullbright());
    }
}

#[test]
fn dragging_a_column_header_moves_and_saves_within_the_real_host_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drag.wasm");
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
    let pad = |host: &ModHost, id: &str| {
        host.panel()
            .unwrap()
            .controls
            .iter()
            .find(|control| control.id() == id)
            .map(|control| format!("{control:?}"))
            .unwrap()
    };
    let mut controls = empty_controls();
    controls.focused = true;
    host.frame_with_controls(false, None, controls).unwrap();
    assert!(pad(&host, "drag_x").contains("-2048"));
    // Press, travel and release over 120 frames, as a real pointer would.
    for frame in 0..=120 {
        let mut controls = empty_controls();
        controls.focused = true;
        controls.panel_open = true;
        if frame == 0 {
            controls.keys_pressed.push("MouseLeft".into());
        }
        if frame < 120 {
            controls.keys_held.push("MouseLeft".into());
            for (axis, value) in [("x", -370.0 + frame as f32), ("y", -160.0 + frame as f32 / 2.0)] {
                controls.events.push(ControlEvent {
                    id: format!("drag_{axis}"),
                    value,
                });
            }
        }
        host.frame_with_controls(false, None, controls)
            .unwrap_or_else(|error| panic!("drag frame {frame}: {error:#}"));
        assert!(host.is_active());
    }
    assert!(pad(&host, "drag_x").contains("-2048"));
    let preferences: serde_json::Value =
        serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
    assert_eq!(preferences["columns"][0], serde_json::json!([-261.0, -106.5]));
    assert_eq!(preferences["collapsed"][0], false);
}

#[test]
fn jump_reset_uses_the_real_component_movement_imports_and_disables_in_the_panel() {
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
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "jump_reset".into(),
        value: 0.0,
    });
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
    assert!(!grants.environment && grants.player_state);
    assert!(grants.render && grants.world_draw && grants.entity_highlight && grants.entity_observe && grants.render_through_walls);
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("cinnaroids.wasm");
    let component = std::fs::read(&source).unwrap();
    let mut host = ModHost::load_snapshot_with_grants(&installed, &component, grants).unwrap();
    let mut controls = empty_controls();
    controls.focused = true;
    host.frame_with_controls(false, None, controls).unwrap();
    assert!(host.is_active());
    assert_eq!(host.panel().unwrap().toggle_key, "F7");
    assert_eq!(host.reserved_keys(), ["F7"]);
}

#[test]
fn player_esp_uses_real_actor_masks_and_clears_retained_output() {
    use mod_host::{
        ActorObservation, ActorObservationBounds, GameplaySnapshot, GameplayVector3,
        module_observations::ModuleObservations,
    };
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for (highlight, through_walls) in [(true, true), (true, false), (false, true)] {
        let granted = highlight && through_walls;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("esp.wasm");
        let mut host = ModHost::load_snapshot_with_grants(
            &path,
            &component,
            ModGrants {
                hud: true,
                players: true,
                player_state: true,
                controls: true,
                render: true,
                world_draw: true,
                entity_observe: true,
                entity_highlight: highlight,
                render_through_walls: through_walls,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(&path, &component).unwrap();
        host.bind_drawing_session(mod_api::drawing::DrawingSession {
            connection: 1,
            dimension: 0,
            local: true,
            permissions: mod_api::drawing::DrawingPermissions::ALL,
        });
        assert!(host.reload_if_changed().unwrap());
        let mut controls = empty_controls();
        controls.focused = true;
        controls.panel_open = true;
        controls.events.push(ControlEvent {
            id: "player_esp".into(),
            value: 1.,
        });
        host.frame_with_controls(false, None, controls).unwrap();
        let snapshot = || GameplaySnapshot {
            session: 1,
            dimension: 0,
            eye: GameplayVector3 {
                x: 0.,
                y: 1.6,
                z: 0.,
            },
            yaw: 0.,
            pitch: 0.,
            attack_held: false,
            frame_seconds: 1. / 120.,
            players: vec![],
        };
        let observe = |host: &mut ModHost| {
            // Connection generations and actor-store generations are different identity domains.
            host.observe_presentation(
                Some(mod_host::WorldObservation {
                    session: 2,
                    dimension: 0,
                    world: "local-fixture".into(),
                    position: (0., 0., 0.),
                    alive: true,
                }),
                None,
                None,
                Vec::new(),
            )
            .unwrap();
            host.publish_mask_targets(
                &(0..mod_api::MAX_OBSERVED_ACTORS)
                    .map(|index| {
                        (
                            mod_api::drawing::MaskEntity {
                                connection: 1,
                                dimension: 0,
                                runtime_id: index as u64 + 1,
                                spawn_revision: 1,
                            },
                            72,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            host.observe_modules(ModuleObservations {
                actors: (0..mod_api::MAX_OBSERVED_ACTORS)
                    .map(|index| ActorObservation {
                        session: 2,
                        dimension: 0,
                        runtime_id: index as u64 + 1,
                        player: true,
                        position: (index as f32, 0., -4.),
                        boxes: vec![
                            ActorObservationBounds {
                                min: (index as f32 - 0.3, 0., -4.3),
                                max: (index as f32 + 0.3, 1.8, -3.7)
                            };
                            mod_api::MAX_OBSERVED_ACTOR_BOXES
                        ],
                        name: "P".repeat(mod_api::MAX_OBSERVED_ACTOR_NAME_BYTES),
                        identifier: "minecraft:player".into(),
                        yaw: 0.,
                        pitch: 0.,
                        pickable: true,
                        hittable: None,
                        look_origin: None,
                        look_direction: None,
                        invisible: false,
                        sneaking: false,
                        hurt: false,
                        health: Some(10.),
                        max_health: Some(40.),
                    })
                    .collect(),
                ..Default::default()
            })
            .unwrap();
        };
        for frame in 0..3 {
            observe(&mut host);
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), controls)
                .unwrap_or_else(|error| panic!("ESP frame {frame}, granted {granted}: {error:#}"));
            assert!(host.is_active());
            assert_eq!(host.render().0.masks.len(), usize::from(granted));
            assert_eq!(
                host.hud().map_or(0, |hud| hud.world_labels.len()),
                if granted { 6 } else { 0 }
            );
            if granted {
                let output = host.render().0;
                assert!(
                    output
                        .retained
                        .iter()
                        .all(|r| r.instances.is_empty() && r.geometry.is_empty())
                );
                assert_eq!(output.masks[0].entities.len(), 6);
                assert_eq!(output.masks[0].depth, mod_api::drawing::MaskDepth::Always);
                let style = output.masks[0].style.unwrap();
                assert!(
                    style.outline_radius > 0.
                        && style.glow_radius > style.outline_radius
                        && style.glow[3] > 0.
                );
            }
        }
        if granted {
            // Missing admitted meshes cannot leave old silhouettes or labels behind.
            host.publish_mask_targets(&[]);
            let mut controls = empty_controls(); controls.focused = true; controls.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), controls).unwrap();
            assert!(host.render().0.masks.is_empty());
            assert!(host.hud().is_none_or(|hud| hud.world_labels.is_empty()));
        }
        // Focus loss, returning to a menu, disabling and missing observations all clear retained HUD.
        for reason in 0..5 {
            observe(&mut host);
            let mut active = empty_controls();
            active.focused = true;
            active.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), active)
                .unwrap();
            observe(&mut host);
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            match reason {
                0 => controls.focused = false,
                1 => controls.panel_open = true,
                3 => host
                    .observe_presentation(None, None, None, Vec::new())
                    .unwrap(),
                4 => {
                    controls.panel_open = true;
                    controls.events.push(ControlEvent {
                        id: "player_esp".into(),
                        value: 0.,
                    });
                }
                _ => (),
            }
            host.frame_with_controls(
                false,
                if reason == 2 { None } else { Some(snapshot()) },
                controls,
            )
            .unwrap();
            assert!(host.render().0.masks.is_empty());
            assert!(host.hud().is_none_or(|hud| hud.world_labels.is_empty()));
        }
    }
}
