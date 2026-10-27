use mod_host::{
    BlockSearch, BlockSearchSource, CallbackBudget, ControlEvent, FoundBlocks, GameplayCameraPose,
    GameplayFacts, GameplaySnapshot, GameplayVector3, ModGrants, ModHost, TerrainPosition,
    TerrainReadError, empty_controls,
};

/// Regression target for a mod frame, leaving room below the host's callback limit.
const FRAME_FUEL: u64 = 100_000;

/// Attaches shared accounting so every tested callback can be measured independently.
fn frame_budget(host: &mut ModHost) -> CallbackBudget {
    let budget = CallbackBudget::new(mod_api::TOTAL_MOD_FRAME_FUEL);
    host.set_frame_authority(budget.clone(), false, false, 0);
    budget
}

/// Runs one callback, rejecting both target overruns and recovered execution traps.
fn metered_callback(
    host: &mut ModHost,
    budget: &CallbackBudget,
    callback: impl FnOnce(&mut ModHost),
) -> u64 {
    budget.reset(mod_api::TOTAL_MOD_FRAME_FUEL);
    callback(host);
    assert!(host.is_active());
    assert!(!host.is_recovering(), "callback trapped");
    let used = mod_api::TOTAL_MOD_FRAME_FUEL - budget.remaining();
    assert!(used <= FRAME_FUEL, "{used} fuel (target {FRAME_FUEL})");
    used
}

/// Shown Player ESP label slots in the HUD layer Cinnabar retained for this component.
/// This is the one place that reads the host's HUD surface accessor.
fn shown_labels(host: &ModHost) -> usize {
    host.hud_surface().map_or(0, |(surface, _)| {
        surface
            .bindings
            .iter()
            .filter(|(name, value)| {
                name.starts_with("#esp_")
                    && name.ends_with("_shown")
                    && **value == ui::mod_panel::SurfaceValue::Bool(true)
            })
            .count()
    })
}

/// Retained world-drawing instances across this component's owners.
fn drawn_instances(host: &ModHost) -> usize {
    host.render()
        .0
        .retained
        .iter()
        .map(|retained| retained.instances.len())
        .sum()
}

/// World origins of the drawn instances, sorted.
fn drawn_origins(host: &ModHost) -> Vec<[i64; 3]> {
    let mut origins: Vec<_> = host
        .render()
        .0
        .retained
        .iter()
        .flat_map(|retained| retained.instances.values())
        .map(|instance| match instance.anchor {
            mod_render::retained::Anchor::World { origin, .. } => origin.map(|axis| axis as i64),
            _ => panic!("world-anchored"),
        })
        .collect();
    origins.sort_unstable();
    origins
}

/// Loads the component at a fresh path with `grants` and saved `settings`, binds a local world
/// with every drawing permission, and meters its frames through the returned budget.
fn drawing_host(grants: ModGrants, settings: &str) -> (tempfile::TempDir, ModHost, CallbackBudget) {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drawing.wasm");
    std::fs::write(path.with_extension("settings.json"), settings).unwrap();
    let mut host = ModHost::load_snapshot_with_grants(&path, &component, grants).unwrap();
    std::fs::write(&path, &component).unwrap();
    host.bind_drawing_session(mod_api::drawing::DrawingSession {
        connection: 1,
        dimension: 0,
        local: true,
        permissions: mod_api::drawing::DrawingPermissions::ALL,
    });
    assert!(host.reload_if_changed().unwrap());
    observe_bindings(&mut host, "MouseLeft", 1);
    let budget = frame_budget(&mut host);
    (directory, host, budget)
}

/// One metered frame: the local player stands at the origin of world session 2, and `gameplay`
/// gives a focused gameplay frame, otherwise the panel is open with `events`. Returns the fuel
/// the callback used.
fn metered_frame(
    host: &mut ModHost,
    budget: &CallbackBudget,
    gameplay: bool,
    events: &[(&str, f32)],
    facts: GameplayFacts,
) -> u64 {
    host.observe_presentation(
        Some(mod_host::WorldObservation {
            session: 2,
            dimension: 0,
            world: "local-fixture".into(),
            position: (0., 0., 0.),
            alive: true,
            runtime_id: 99,
        }),
        None,
        None,
        Vec::new(),
    )
    .unwrap();
    host.observe_gameplay(facts).unwrap();
    let mut controls = empty_controls();
    controls.focused = true;
    controls.gameplay = gameplay;
    controls.panel_open = !gameplay;
    for (id, value) in events {
        controls.events.push(ControlEvent {
            id: (*id).into(),
            value: *value,
        });
    }
    let snapshot = gameplay.then(|| GameplaySnapshot {
        session: 1,
        dimension: 0,
        eye: GameplayVector3 {
            x: 0.,
            y: 1.62,
            z: 0.,
        },
        yaw: 0.,
        pitch: 0.,
        attack_held: false,
        frame_seconds: 1. / 60.,
        players: vec![],
    });
    metered_callback(host, budget, |host| {
        host.frame_with_controls(false, snapshot, controls)
            .unwrap_or_else(|error| panic!("metered frame: {error:#}"));
    })
}

/// Loaded terrain holding `count` ancient debris blocks in a slab north of the player, nearest
/// first, moved `shift` blocks east.
struct Debris {
    count: usize,
    shift: i32,
}

impl Debris {
    fn blocks(&self, limit: usize) -> Vec<TerrainPosition> {
        (0..self.count.min(limit) as i32)
            .map(|index| TerrainPosition {
                x: index % 32 - 16 + self.shift,
                y: index / 32 % 16,
                z: -1 - index / 512,
            })
            .collect()
    }
}

impl BlockSearchSource for Debris {
    fn find(&mut self, search: &BlockSearch) -> Result<FoundBlocks, TerrainReadError> {
        assert_eq!(
            search.identifiers,
            ["minecraft:ancient_debris", "minecraft:netherite_block"]
        );
        Ok(FoundBlocks {
            session: 2,
            dimension: 0,
            positions: self.blocks(search.limit as usize),
        })
    }
}

#[test]
fn saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget() {
    let mut most = 0;
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for settings in [
        "{}",
        r#"{"aim_horizontal_only":true}"#,
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
                terrain: true,
                world_draw: true,
                render_through_walls: true,
                fullbright: true,
                movement: true,
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("init settings {settings}: {error:#}"));
        let budget = frame_budget(&mut host);
        for (id, value) in [
            ("expand_clicker", 1.0),
            ("expand_aim", 1.0),
            ("aim_horizontal_only", 1.0),
            ("aim_horizontal_only", 0.0),
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
            most = most.max(metered_callback(&mut host, &budget, |host| {
                host.frame_with_controls(false, None, controls)
                    .unwrap_or_else(|error| panic!("event {id}, settings {settings}: {error:#}"));
            }));
            assert!(host.is_active());
            assert!(
                host.panel()
                    .unwrap()
                    .controls
                    .iter()
                    .any(|control| control.id() == "jump_reset")
            );
            if id == "aim_horizontal_only" {
                let preferences: serde_json::Value =
                    serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
                assert_eq!(preferences["aim_horizontal_only"], value >= 0.5);
                assert!(
                    host.panel()
                        .unwrap()
                        .controls
                        .iter()
                        .any(|control| control.id() == "aim_horizontal_only")
                );
            }
            if (id == "netherite" && value >= 0.5) || id == "netherite_range" {
                // The finder searches and draws only over focused gameplay, not in the panel.
                assert_eq!(drawn_instances(&host), 0);
                assert!(host.fullbright(), "Finder does not disable Fullbright");
            }
            if id == "netherite_range" {
                let preferences: serde_json::Value =
                    serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
                assert_eq!(preferences["netherite_range"], 128);
            }
            if id == "fullbright" {
                assert_eq!(host.fullbright(), value >= 0.5);
            }
        }
        assert_eq!(drawn_instances(&host), 0);
        assert!(!host.fullbright());
    }
    println!(
        "saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn dragging_a_column_header_moves_and_saves_within_the_real_host_budget() {
    let mut most = 0;
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
    let budget = frame_budget(&mut host);
    let mut controls = empty_controls();
    controls.focused = true;
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
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
            for (axis, value) in [
                ("x", -370.0 + frame as f32),
                ("y", -160.0 + frame as f32 / 2.0),
            ] {
                controls.events.push(ControlEvent {
                    id: format!("drag_{axis}"),
                    value,
                });
            }
        }
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, None, controls)
                .unwrap_or_else(|error| panic!("drag frame {frame}: {error:#}"));
        }));
        assert!(host.is_active());
    }
    assert!(pad(&host, "drag_x").contains("-2048"));
    let preferences: serde_json::Value =
        serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
    assert_eq!(
        preferences["columns"][0],
        serde_json::json!([-261.0, -106.5])
    );
    assert_eq!(preferences["collapsed"][0], false);
    println!(
        "dragging_a_column_header_moves_and_saves_within_the_real_host_budget: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn jump_reset_uses_the_real_component_movement_imports_and_disables_in_the_panel() {
    let mut most = 0;
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
    observe_bindings(&mut host, "MouseLeft", 1);
    let budget = frame_budget(&mut host);
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "jump_reset".into(),
        value: 1.0,
    });
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
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
        sprint_held: false,
        sneak_held: false,
        eligible: true,
        velocity: GameplayVector3 {
            x: 0.2,
            y: -0.08,
            z: 0.0,
        },
    };
    assert!(host.take_jump_cancel());
    for (tick, sequence, grounded, held, expected) in [
        (10, 0, true, false, false),
        (11, 1, false, false, false),
        (12, 1, true, false, true),
        (12, 1, true, false, false),
        (13, 2, false, false, false),
        (14, 2, true, true, false),
        (15, 2, true, false, false),
        (16, 3, true, false, true),
    ] {
        let mut controls = empty_controls();
        controls.focused = true;
        controls.gameplay = true;
        if held {
            controls.keys_held.push("Space".into());
            controls.keys_admitted.push("Space".into());
        }
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_movement(
                false,
                Some(frame()),
                Vec::new(),
                None,
                Some(movement(tick, sequence, grounded)),
                controls,
            )
            .unwrap();
        }));
        assert_eq!(host.take_jump_pulse(), expected);
        assert!(!host.take_jump_pulse(), "pulse is consumed once");
        assert_eq!(host.take_jump_cancel(), held);
    }
    let mut controls = empty_controls();
    controls.focused = true;
    controls.gameplay = true;
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "jump_reset".into(),
        value: 0.0,
    });
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_movement(
            false,
            Some(frame()),
            Vec::new(),
            None,
            Some(movement(17, 4, true)),
            controls,
        )
        .unwrap();
    }));
    assert!(!host.take_jump_pulse());
    assert!(host.take_jump_cancel());
    println!(
        "jump_reset_uses_the_real_component_movement_imports_and_disables_in_the_panel: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn menu_rebinding_publishes_and_saves_within_the_real_host_budget() {
    let mut most = 0;
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
    let budget = frame_budget(&mut host);
    let mut controls = empty_controls();
    controls.focused = true;
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
    assert_eq!(host.panel().unwrap().toggle_key, "F7");
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.events.push(ControlEvent {
        id: "menu_key".into(),
        value: 1.0,
    });
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
    assert!(host.panel().unwrap().capture_key);
    let mut controls = empty_controls();
    controls.focused = true;
    controls.panel_open = true;
    controls.keys_pressed.push("F6".into());
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
    assert!(host.is_active());
    assert_eq!(host.panel().unwrap().toggle_key, "F6");
    assert!(!host.panel().unwrap().capture_key);
    assert!(host.reserved_keys().contains(&"F6".into()));
    assert!(!host.reserved_keys().contains(&"F7".into()));
    let preferences: serde_json::Value =
        serde_json::from_str(host.settings_snapshot().unwrap()).unwrap();
    assert_eq!(preferences["menu_key"], "F6");
    println!(
        "menu_rebinding_publishes_and_saves_within_the_real_host_budget: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn single_file_import_declares_metadata_and_loads_with_its_requested_grants() {
    let mut most = 0;
    let source = std::path::PathBuf::from(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap());
    let declarations = mod_host::declarations(&source).unwrap();
    assert_eq!(declarations.metadata.name.as_deref(), Some("Cinnaroids"));
    assert!(declarations.metadata.version.is_some());
    assert!(declarations.metadata.icon.is_some());
    let grants = declarations
        .requested
        .unwrap()
        .expect("explicit import permissions");
    assert!(grants.controls && grants.settings && grants.hud);
    assert!(grants.players && grants.camera && grants.interaction);
    assert!(grants.packet_delay && grants.movement);
    assert!(grants.fullbright && grants.terrain);
    assert!(!grants.environment && grants.player_state);
    assert!(
        grants.render
            && grants.world_draw
            && grants.entity_highlight
            && grants.entity_observe
            && grants.render_through_walls
    );
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("cinnaroids.wasm");
    let component = std::fs::read(&source).unwrap();
    let mut host = ModHost::load_snapshot_with_grants(&installed, &component, grants).unwrap();
    let budget = frame_budget(&mut host);
    let mut controls = empty_controls();
    controls.focused = true;
    most = most.max(metered_callback(&mut host, &budget, |host| {
        host.frame_with_controls(false, None, controls).unwrap();
    }));
    assert!(host.is_active());
    assert_eq!(host.panel().unwrap().toggle_key, "F7");
    assert_eq!(host.reserved_keys(), ["F7"]);
    println!(
        "single_file_import_declares_metadata_and_loads_with_its_requested_grants: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn player_esp_uses_real_actor_masks_and_clears_retained_output() {
    let mut most = 0;
    use mod_host::{
        ActorObservation, ActorObservationBounds, GameplaySnapshot, GameplayVector3, ObservedActor,
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
        observe_bindings(&mut host, "MouseLeft", 1);
        let budget = frame_budget(&mut host);
        let mut controls = empty_controls();
        controls.focused = true;
        controls.panel_open = true;
        controls.events.push(ControlEvent {
            id: "player_esp".into(),
            value: 1.,
        });
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, None, controls).unwrap();
        }));
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
        let observe = |host: &mut ModHost, non_players: usize| {
            // Connection generations and actor-store generations are different identity domains.
            host.observe_presentation(
                Some(mod_host::WorldObservation {
                    session: 2,
                    dimension: 0,
                    world: "local-fixture".into(),
                    position: (0., 0., 0.),
                    alive: true,
                    runtime_id: 99,
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
                    .map(|index| ObservedActor {
                        actor: ActorObservation {
                            session: 2,
                            dimension: 0,
                            runtime_id: index as u64 + 1,
                            player: index >= non_players,
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
                        },
                        max_health: Some(40.),
                    })
                    .collect(),
                ..Default::default()
            })
            .unwrap();
        };
        for frame in 0..6 {
            // Player-only consent filters mobs before the bounded six-row result.
            observe(&mut host, frame.min(2) * 5);
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            most = most.max(metered_callback(&mut host, &budget, |host| {
                host.frame_with_controls(false, Some(snapshot()), controls)
                    .unwrap_or_else(|error| {
                        panic!("ESP frame {frame}, granted {granted}: {error:#}")
                    });
            }));
            assert!(host.is_active());
            if frame == 5 || !granted {
                assert_eq!(host.render().0.masks.len(), usize::from(granted));
            }
            // Three labels take a new name per frame; the fixture's players share one name.
            if frame == 5 || !granted {
                assert_eq!(shown_labels(&host), if granted { 6 } else { 0 });
            }
            if granted && !host.render().0.masks.is_empty() {
                let output = host.render().0;
                assert!(
                    output
                        .retained
                        .iter()
                        .all(|r| r.instances.is_empty() && r.geometry.is_empty())
                );
                if frame == 5 {
                    assert_eq!(output.masks[0].entities.len(), 6);
                }
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
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            most = most.max(metered_callback(&mut host, &budget, |host| {
                host.frame_with_controls(false, Some(snapshot()), controls)
                    .unwrap();
            }));
            assert!(host.render().0.masks.is_empty());
            assert_eq!(shown_labels(&host), 0);
        }
        // Focus loss, returning to a menu, disabling and missing observations all clear retained HUD.
        for reason in 0..5 {
            observe(&mut host, 10);
            let mut active = empty_controls();
            active.focused = true;
            active.gameplay = true;
            most = most.max(metered_callback(&mut host, &budget, |host| {
                host.frame_with_controls(false, Some(snapshot()), active)
                    .unwrap();
            }));
            observe(&mut host, 10);
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
            most = most.max(metered_callback(&mut host, &budget, |host| {
                host.frame_with_controls(
                    false,
                    if reason == 2 { None } else { Some(snapshot()) },
                    controls,
                )
                .unwrap();
            }));
            assert!(host.render().0.masks.is_empty());
            assert_eq!(shown_labels(&host), 0);
        }
    }
    println!(
        "player_esp_uses_real_actor_masks_and_clears_retained_output: peak {most} fuel (target {FRAME_FUEL})"
    );
}

#[test]
fn netherite_finder_keeps_256_retained_cubes_within_the_frame_budget() {
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            controls: true,
            settings: true,
            terrain: true,
            world_draw: true,
            render_through_walls: true,
            ..Default::default()
        },
        r#"{"netherite_range":128}"#,
    );
    let setup = metered_frame(
        &mut host,
        &budget,
        false,
        &[("netherite", 1.)],
        GameplayFacts::default(),
    );
    let mut debris = Debris {
        count: mod_api::MAX_FOUND_BLOCKS,
        shift: 0,
    };
    // Draws the nearest 256 of 1024 found blocks, sixteen placements per frame.
    let run = |host: &mut ModHost, debris: &mut Debris, frames: usize| {
        let mut most = 0;
        for _ in 0..frames {
            mod_host::refresh_block_searches(&mut [&mut *host], &mut *debris);
            most = most.max(metered_frame(
                host,
                &budget,
                true,
                &[],
                GameplayFacts::default(),
            ));
        }
        most
    };
    let expected = |debris: &Debris| {
        let mut blocks: Vec<_> = debris
            .blocks(256)
            .iter()
            .map(|block| [block.x as i64, block.y as i64, block.z as i64])
            .collect();
        blocks.sort_unstable();
        blocks
    };
    let drawing = run(&mut host, &mut debris, 40);
    assert_eq!(drawn_origins(&host), expected(&debris));
    let steady = run(&mut host, &mut debris, 30);
    // Every block moves: the cubes move to the new blocks without being recreated.
    debris.shift = 1;
    let moving = run(&mut host, &mut debris, 50);
    assert_eq!(drawn_origins(&host), expected(&debris));
    // Opening the panel keeps the cubes; disabling the finder removes them.
    let panel = metered_frame(&mut host, &budget, false, &[], GameplayFacts::default());
    assert_eq!(drawn_instances(&host), 256);
    let mut clearing = 0;
    for frame in 0..20 {
        let events = if frame == 0 {
            &[("netherite", 0.)][..]
        } else {
            &[]
        };
        clearing = clearing.max(metered_frame(
            &mut host,
            &budget,
            false,
            events,
            GameplayFacts::default(),
        ));
    }
    assert_eq!(drawn_instances(&host), 0);
    let most = [setup, panel, drawing, steady, moving, clearing]
        .into_iter()
        .max()
        .unwrap();
    println!(
        "Netherite Finder, 256 cubes: peak {most}, {setup} setup, {panel} panel, \
         {drawing} drawing, {steady} steady, \
         {moving} moving, {clearing} clearing (frame budget {FRAME_FUEL})"
    );
}

#[test]
fn fake_lag_marker_follows_the_relayed_feet_and_hides_around_the_camera() {
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            controls: true,
            settings: true,
            players: true,
            packet_delay: true,
            world_draw: true,
            ..Default::default()
        },
        r#"{"fake_lag_ms":200,"show_real_position":true}"#,
    );
    let mut most = metered_frame(
        &mut host,
        &budget,
        false,
        &[("fake_lag", 1.)],
        GameplayFacts::default(),
    );
    assert_eq!(host.packet_delay_ms(), 200);
    let facts = |camera: [f32; 3], relayed: Option<[f32; 3]>| GameplayFacts {
        camera: Some(GameplayCameraPose {
            position: GameplayVector3 {
                x: camera[0],
                y: camera[1],
                z: camera[2],
            },
            yaw: 0.,
            pitch: 0.,
        }),
        relayed_position: relayed.map(|[x, y, z]| GameplayVector3 { x, y, z }),
        ..Default::default()
    };
    // The eye in the gameplay frame stays at the origin; only the camera pose decides.
    for (camera, relayed, shown) in [
        ([0., 1.62, 4.], Some([2., 0., 0.]), true),
        ([2., 1.62, 0.], Some([2., 0., 0.]), false),
        ([2., 3.2, 4.], Some([2., 0., 0.]), true),
        ([0., 1.62, 4.], None, false),
        ([0., 1.62, 4.], Some([12., 0., 0.]), true),
    ] {
        most = most.max(metered_frame(
            &mut host,
            &budget,
            true,
            &[],
            facts(camera, relayed),
        ));
        assert_eq!(
            drawn_instances(&host),
            if shown { 5 } else { 0 },
            "{camera:?}"
        );
        if let Some(relayed) = relayed.filter(|_| shown) {
            // New samples snap to a fresh marker; the faces stand at the feet.
            assert!(
                drawn_origins(&host)
                    .iter()
                    .all(|origin| *origin == relayed.map(|axis| axis as i64))
            );
        }
    }
    println!("FakeLag marker: most fuel per frame {most} (frame budget {FRAME_FUEL})");
}

#[test]
fn every_drawing_module_together_stays_within_the_frame_budget() {
    use mod_host::{
        ActorObservation, ActorObservationBounds, ObservedActor,
        module_observations::ModuleObservations,
    };
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            hud: true,
            players: true,
            player_state: true,
            controls: true,
            settings: true,
            render: true,
            world_draw: true,
            entity_observe: true,
            entity_highlight: true,
            render_through_walls: true,
            terrain: true,
            packet_delay: true,
            fullbright: true,
            ..Default::default()
        },
        r#"{"netherite_range":128,"fake_lag_ms":200,"show_real_position":true}"#,
    );
    let mut most = metered_frame(
        &mut host,
        &budget,
        false,
        &[
            ("player_esp", 1.),
            ("netherite", 1.),
            ("fake_lag", 1.),
            ("fullbright", 1.),
        ],
        GameplayFacts::default(),
    );
    for frame in 0..90 {
        // Every 20 frames all blocks move and every player gets a new, longest name.
        let generation = frame / 20;
        mod_host::refresh_block_searches(
            &mut [&mut host],
            &mut Debris {
                count: mod_api::MAX_FOUND_BLOCKS,
                shift: generation as i32,
            },
        );
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
                .map(|index| ObservedActor {
                    actor: ActorObservation {
                        session: 2,
                        dimension: 0,
                        runtime_id: index as u64 + 1,
                        player: true,
                        position: (index as f32 + frame as f32 * 0.05, 0., -4.),
                        boxes: vec![
                            ActorObservationBounds {
                                min: (index as f32 - 0.3, 0., -4.3),
                                max: (index as f32 + 0.3, 1.8, -3.7)
                            };
                            mod_api::MAX_OBSERVED_ACTOR_BOXES
                        ],
                        name: format!("§{generation}")
                            .repeat(mod_api::MAX_OBSERVED_ACTOR_NAME_BYTES / 3),
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
                        health: Some(10. + generation as f32),
                    },
                    max_health: Some(40.),
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
        let facts = GameplayFacts {
            camera: Some(GameplayCameraPose {
                position: GameplayVector3 {
                    x: 0.,
                    y: 1.62,
                    z: 0.,
                },
                yaw: 0.,
                pitch: 0.,
            }),
            relayed_position: Some(GameplayVector3 {
                x: 2. + frame as f32 * 0.1,
                y: 0.,
                z: 0.,
            }),
            ..Default::default()
        };
        most = most.max(metered_frame(&mut host, &budget, true, &[], facts));
    }
    assert_eq!(host.render().0.masks.len(), 1);
    assert_eq!(shown_labels(&host), 6);
    assert_eq!(drawn_instances(&host), 256 + 5);
    assert!(host.fullbright());
    println!("Player ESP, Netherite Finder and FakeLag marker: most fuel per frame {most}");
}

/// Current player assignments, shared with raw controls rather than guest gameplay records.
fn observe_bindings(host: &mut ModHost, attack: &str, revision: u64) {
    use mod_host::{BindingContext, BindingModifiers, InputActionBinding, InputBindings};
    host.observe_input_bindings(std::sync::Arc::new(InputBindings {
        revision,
        total: 2,
        entries: [("attack", attack), ("jump", "Space")]
            .into_iter()
            .map(|(action, key)| InputActionBinding {
                action: action.into(),
                key: key.into(),
                context: BindingContext::Gameplay,
                modifiers: BindingModifiers {
                    shift: false,
                    control: false,
                    alt: false,
                    super_key: false,
                },
            })
            .collect(),
    }))
    .unwrap();
}

/// The real component follows a rebound key, releases on UI ownership and never pulses merely
/// because an unrelated old default is down. Host-only attack evidence still enforces requests.
#[test]
fn rebound_attack_resolves_in_the_guest_within_the_callback_budget() {
    let mut most = 0;
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            controls: true,
            players: true,
            interaction: true,
            camera: true,
            ..Default::default()
        },
        r#"{"clicker":true,"cps":20}"#,
    );
    observe_bindings(&mut host, "KeyK", 2);
    most = most.max(metered_frame(
        &mut host,
        &budget,
        false,
        &[("clicker", 1.), ("aim", 1.)],
        GameplayFacts::default(),
    ));
    let snapshot = |gate| GameplaySnapshot {
        session: 1,
        dimension: 0,
        eye: GameplayVector3 {
            x: 0.,
            y: 1.6,
            z: 0.,
        },
        yaw: 0.,
        pitch: 0.,
        frame_seconds: 0.05,
        players: vec![mod_host::GameplayPlayer {
            runtime_id: 2,
            position: GameplayVector3 {
                x: -0.5,
                y: 0.,
                z: -3.,
            },
        }],
        attack_held: gate,
    };
    for (key, panel, gate, expected) in [
        ("MouseLeft", false, false, false),
        ("KeyK", false, true, false),
        ("KeyK", false, true, true),
        ("KeyK", true, false, false),
        ("KeyK", false, true, false),
        ("KeyK", false, true, true),
    ] {
        let mut controls = empty_controls();
        controls.seconds = 0.05;
        controls.focused = true;
        controls.gameplay = true;
        controls.panel_open = panel;
        controls.keys_held = vec![key.into()];
        controls.keys_admitted = if panel { vec![] } else { vec![key.into()] };
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, Some(snapshot(gate)), controls)
                .unwrap();
        }));
        assert!(host.is_active());
        assert_eq!(host.take_interaction().attack_pulse, expected);
        assert_eq!(host.take_camera_delta().is_some(), key == "KeyK" && !panel);
    }
    // Both keys are already held when Attack changes. The router removes admission
    // until release, even though focused gameplay and raw key capture continue.
    observe_bindings(&mut host, "MouseLeft", 3);
    for (revision, admitted, gate, pulse, aim) in [
        (3, vec![], false, false, false),
        (3, vec!["MouseLeft", "KeyK"], true, false, true),
        (3, vec!["MouseLeft", "KeyK"], true, true, true),
        (4, vec![], false, false, false),
        (4, vec![], false, false, false),
        (4, vec!["MouseLeft"], false, false, false),
        (4, vec!["MouseLeft", "KeyK"], true, false, true),
        (4, vec!["MouseLeft", "KeyK"], true, true, true),
        // A later request can be refused despite an admitted chord. Do not retry
        // that hold even if the host gate recovers; wait for admission to end.
        (4, vec!["KeyK"], false, false, true),
        (4, vec!["KeyK"], true, false, true),
        (4, vec!["KeyK"], true, false, true),
        (4, vec![], false, false, false),
        (4, vec!["KeyK"], true, false, true),
        (4, vec!["KeyK"], true, true, true),
    ] {
        if revision == 4 {
            observe_bindings(&mut host, "KeyK", revision);
        }
        let mut controls = empty_controls();
        controls.seconds = 0.05;
        controls.focused = true;
        controls.gameplay = true;
        controls.keys_held = vec!["MouseLeft".into(), "KeyK".into()];
        controls.keys_admitted = admitted.into_iter().map(String::from).collect();
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, Some(snapshot(gate)), controls)
                .unwrap();
        }));
        assert_eq!(host.take_interaction().attack_pulse, pulse);
        assert_eq!(host.take_camera_delta().is_some(), aim);
    }
    // These are finalized host observations. Raw keys alone must never drive
    // either Clicker or While clicking aim under any admission interruption.
    for reason in [
        "chat macro",
        "UI barrier",
        "controller selected",
        "reserved key",
        "focus loss",
    ] {
        let mut controls = empty_controls();
        controls.focused = reason != "focus loss";
        controls.gameplay = true;
        controls.keys_held = vec!["KeyK".into()];
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, Some(snapshot(false)), controls)
                .unwrap();
        }));
        assert!(!host.take_interaction().attack_pulse, "{reason}");
        assert!(host.take_camera_delta().is_none(), "{reason}");
    }
    println!(
        "rebound_attack_resolves_in_the_guest_within_the_callback_budget: peak {most} fuel (target {FRAME_FUEL})"
    );
}

/// Largest admitted binding reads and unchanged refreshes stay within real guest frame fuel.
#[test]
fn maximum_binding_snapshot_stays_within_the_frame_budget() {
    let mut most = 0;
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            controls: true,
            players: true,
            ..Default::default()
        },
        "{}",
    );
    use mod_host::{BindingContext, BindingModifiers, InputActionBinding, InputBindings};
    let entries: Vec<_> = (0..mod_api::MAX_INPUT_BINDINGS)
        .map(|_| InputActionBinding {
            action: "interact-with-toast".into(),
            key: "ControlRight".into(),
            context: BindingContext::Ui,
            modifiers: BindingModifiers {
                shift: true,
                control: true,
                alt: true,
                super_key: true,
            },
        })
        .collect();
    host.observe_input_bindings(std::sync::Arc::new(InputBindings {
        revision: 10,
        total: entries.len() as u32,
        entries,
    }))
    .unwrap();
    for _ in 0..=mod_api::MAX_INPUT_BINDINGS {
        most = most.max(metered_frame(
            &mut host,
            &budget,
            true,
            &[],
            GameplayFacts::default(),
        ));
    }
    println!(
        "maximum_binding_snapshot_stays_within_the_frame_budget: peak {most} fuel (target {FRAME_FUEL})"
    );
}

/// Full raw key and chord lists, including stronger bindings to other actions, stay bounded.
#[test]
fn maximum_observed_chords_resolve_without_a_quadratic_frame_scan() {
    let mut most = 0;
    use mod_host::{BindingContext, BindingModifiers, InputActionBinding, InputBindings};
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            controls: true,
            players: true,
            interaction: true,
            ..Default::default()
        },
        "{}",
    );
    let keys: Vec<String> = (b'A'..=b'Z')
        .map(|key| format!("Key{}", char::from(key)))
        .chain((0..10).map(|key| format!("Digit{key}")))
        .chain(
            (1..=24)
                .filter(|key| *key != 7)
                .map(|key| format!("F{key}")),
        )
        .chain(["Space", "Tab", "Enter", "Backspace", "Delete"].map(String::from))
        .collect();
    assert_eq!(keys.len(), mod_api::MAX_CONTROL_KEYS);
    let entries: Vec<_> = keys
        .iter()
        .flat_map(|key| {
            [false, true].map(|shift| InputActionBinding {
                action: if shift { "use" } else { "attack" }.into(),
                key: key.clone(),
                context: BindingContext::Gameplay,
                modifiers: BindingModifiers {
                    shift,
                    control: false,
                    alt: false,
                    super_key: false,
                },
            })
        })
        .collect();
    host.observe_input_bindings(std::sync::Arc::new(InputBindings {
        revision: 10,
        total: entries.len() as u32,
        entries,
    }))
    .unwrap();
    most = most.max(metered_frame(
        &mut host,
        &budget,
        false,
        &[("clicker", 1.)],
        GameplayFacts::default(),
    ));
    for _ in 0..=mod_api::MAX_INPUT_BINDINGS {
        let mut controls = empty_controls();
        controls.focused = true;
        controls.gameplay = true;
        controls.keys_held = keys
            .iter()
            .take(keys.len() - 1)
            .cloned()
            .chain(["ShiftLeft".into()])
            .collect();
        controls.keys_admitted = controls.keys_held.clone();
        let snapshot = GameplaySnapshot {
            session: 1,
            dimension: 0,
            eye: GameplayVector3 {
                x: 0.,
                y: 1.6,
                z: 0.,
            },
            yaw: 0.,
            pitch: 0.,
            frame_seconds: 0.05,
            players: Vec::new(),
            attack_held: false,
        };
        most = most.max(metered_callback(&mut host, &budget, |host| {
            host.frame_with_controls(false, Some(snapshot), controls)
                .unwrap();
        }));
        assert!(host.is_active());
        assert!(!host.take_interaction().attack_pulse);
    }
    println!(
        "maximum_observed_chords_resolve_without_a_quadratic_frame_scan: peak {most} fuel (target {FRAME_FUEL})"
    );
}

/// Completing a full catalog cannot multiply held-key scans while drawing modules update.
#[test]
fn binding_completion_with_unheld_jump_and_moving_drawings_fits_the_frame_budget() {
    combined_binding_completion(false, false, false, false, false);
}

/// Full aim observation and drawing work must not resolve an unused Jump policy.
#[test]
fn binding_completion_with_128_aim_targets_and_unused_jump_fits_the_frame_budget() {
    combined_binding_completion(true, false, false, false, false);
}

/// Both hold policies, full aim work and moving drawings must fit together.
#[test]
fn binding_completion_with_all_input_modules_and_drawings_fits_the_frame_budget() {
    combined_binding_completion(true, true, false, false, false);
}

/// Modifier-ineligible alternatives cannot make the last admitted match expensive.
#[test]
fn late_attack_with_maximum_aim_targets_fits_the_frame_budget() {
    combined_binding_completion(true, true, true, false, false);
}

/// ESP observations, labels and masks share the callback with combat and retained drawings.
#[test]
fn fast_attack_with_player_esp_fits_the_frame_budget() {
    combined_binding_completion(true, true, false, true, false);
}

/// The last-match input case includes every drawing module, not just world instances.
#[test]
fn late_attack_with_all_drawings_fits_the_frame_budget() {
    combined_binding_completion(true, true, true, true, false);
}

/// Every module is enabled with maximum observations, changing names and moving terrain.
#[test]
fn all_modules_with_maximum_observations_fit_the_frame_budget() {
    combined_binding_completion(true, true, true, true, true);
}

/// Exercises collection, completion and steady state with a full catalog and admission sample.
fn combined_binding_completion(aim: bool, jump_reset: bool, late: bool, esp: bool, maximum: bool) {
    combined_workload(aim, jump_reset, late, esp, maximum, Stress::Standard);
}

/// Input dimensions share one workload so regressions cannot accidentally omit other modules.
fn combined_workload(
    aim: bool,
    jump_reset: bool,
    late: bool,
    esp: bool,
    maximum: bool,
    stress: Stress,
) {
    use mod_host::{BindingContext, BindingModifiers, InputActionBinding, InputBindings};
    let (_directory, mut host, budget) = drawing_host(
        ModGrants {
            hud: true,
            player_state: true,
            render: true,
            entity_observe: true,
            entity_highlight: true,
            fullbright: true,
            controls: true,
            camera: true,
            movement: true,
            settings: true,
            players: true,
            interaction: true,
            terrain: true,
            packet_delay: true,
            world_draw: true,
            render_through_walls: true,
            ..Default::default()
        },
        &maximum_settings(stress == Stress::Maximum),
    );
    let mut events = vec![("netherite", 1.), ("fake_lag", 1.), ("clicker", 1.)];
    if esp {
        events.extend([("player_esp", 1.), ("fullbright", 1.)]);
    }
    if maximum {
        events.push(("reach", 1.));
    }
    if aim {
        events.push(("aim", 1.));
    }
    if jump_reset {
        events.push(("jump_reset", 1.));
    }
    let mut most = metered_frame(&mut host, &budget, false, &events, GameplayFacts::default());
    // Supported non-modifier desktop keys, excluding the reserved menu key F7.
    let mut keys: Vec<String> =
        "ArrowDown ArrowLeft ArrowRight ArrowUp Backquote Backslash Backspace BracketLeft
        BracketRight CapsLock Comma ContextMenu Delete Digit0 Digit1 Digit2
        Digit3 Digit4 Digit5 Digit6 Digit7 Digit8 Digit9 End
        Enter Equal Escape F1 F10 F11 F12 F13
        F14 F15 F16 F17 F18 F19 F2 F20
        F21 F22 F23 F24 F3 F4 F5 F6
        F8 F9 Home Insert IntlBackslash KeyA KeyB KeyC
        KeyD KeyE KeyF KeyG KeyH KeyI KeyJ KeyK
        KeyL KeyM KeyN KeyO KeyP KeyQ KeyR KeyS
        KeyT KeyU KeyV KeyW KeyX KeyY KeyZ Minus
        NumLock Numpad0 Numpad1 Numpad2 Numpad3 Numpad4 Numpad5 Numpad6
        Numpad7 Numpad8 Numpad9 NumpadAdd NumpadComma NumpadDecimal NumpadDivide NumpadEnter
        NumpadEqual NumpadMultiply NumpadSubtract PageDown PageUp Pause Period PrintScreen
        Quote ScrollLock Semicolon Slash Space Tab"
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect();
    assert_eq!(keys.len(), 110);
    if matches!(stress, Stress::Longest) {
        keys.sort_by_key(|name| std::cmp::Reverse(name.len()));
        let k = keys.iter().position(|key| key == "KeyK").unwrap();
        keys.swap(k, 63);
        let j = keys.iter().position(|key| key == "NumpadMultiply").unwrap();
        keys.swap(j, 109);
    }
    let held: Vec<_> = keys
        .iter()
        .take(mod_api::MAX_CONTROL_KEYS)
        .cloned()
        .collect();
    assert_eq!(held.len(), 64);
    assert!(!held.iter().any(|key| key == "NumpadMultiply"));
    let mut entries: Vec<_> = keys
        .into_iter()
        .map(|key| InputActionBinding {
            action: if key == "NumpadMultiply" {
                "jump"
            } else {
                "attack"
            }
            .into(),
            key: key.clone(),
            context: BindingContext::Gameplay,
            modifiers: BindingModifiers {
                shift: late && key != "KeyK",
                control: false,
                alt: false,
                super_key: false,
            },
        })
        .collect();
    while entries.len() < mod_api::MAX_INPUT_BINDINGS {
        entries.push(entries[0].clone());
    }
    assert_eq!(entries.len(), 128);
    if stress == Stress::Maximum {
        for (index, entry) in entries.iter_mut().enumerate().take(126) {
            entry.key = format!("K{index:031}");
            entry.action = "x".repeat(mod_api::MAX_INPUT_ACTION_BYTES);
        }
        entries[126].key = "KeyK".into();
        entries[126].action = "attack".into();
        entries[126].modifiers = BindingModifiers {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        };
        entries[127].key = "NumpadMultiply".into();
        entries[127].action = "jump".into();
        entries[127].modifiers = BindingModifiers {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        };
    }
    host.observe_input_bindings(std::sync::Arc::new(InputBindings {
        revision: 9,
        total: entries.len() as u32,
        entries,
    }))
    .unwrap();
    let mut debris = StressTerrain {
        base: Debris {
            count: mod_api::MAX_FOUND_BLOCKS,
            shift: 0,
        },
        clustered: stress.collisions().then(clustered_blocks),
    };
    let mut first_marker = Vec::new();
    let mut middle_cubes = Vec::new();
    let mut completion = 0;
    let mut collecting = 0;
    let mut steady = 0;
    let completed = (mod_api::MAX_INPUT_BINDINGS as u64).div_ceil(4) - 1;
    let frames = if stress == Stress::Maximum { 1000 } else { 160 };
    let mut updates = [0; 6];
    let mut previous: [String; 6] = Default::default();
    for frame in 0..frames {
        if esp {
            use mod_host::{
                ActorObservation, ActorObservationBounds, ObservedActor,
                module_observations::ModuleObservations,
            };
            let generation = frame.min(99) / 20;
            host.publish_mask_targets(
                &(0..mod_api::MAX_OBSERVED_ACTORS)
                    .map(|index| {
                        (
                            mod_api::drawing::MaskEntity {
                                connection: 1,
                                dimension: 0,
                                runtime_id: if stress == Stress::Maximum {
                                    u64::MAX - index as u64
                                } else {
                                    index as u64 + 1
                                },
                                spawn_revision: 1,
                            },
                            72,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            host.observe_modules(ModuleObservations {
                actors: (0..mod_api::MAX_OBSERVED_ACTORS)
                    .map(|index| ObservedActor {
                        actor: ActorObservation {
                            session: 2,
                            dimension: 0,
                            runtime_id: if stress == Stress::Maximum {
                                u64::MAX - index as u64
                            } else {
                                index as u64 + 1
                            },
                            player: true,
                            position: (index as f32 + frame as f32 * 0.05, 0., -4.),
                            boxes: vec![
                                ActorObservationBounds {
                                    min: (index as f32 - 0.3, 0., -4.3),
                                    max: (index as f32 + 0.3, 1.8, -3.7)
                                };
                                mod_api::MAX_OBSERVED_ACTOR_BOXES
                            ],
                            name: stress.name(frame),
                            identifier: if stress == Stress::Maximum {
                                "x".repeat(128)
                            } else {
                                "minecraft:player".into()
                            },
                            yaw: 0.,
                            pitch: 0.,
                            pickable: true,
                            hittable: None,
                            look_origin: None,
                            look_direction: None,
                            invisible: false,
                            sneaking: false,
                            hurt: false,
                            health: Some(if stress == Stress::Maximum {
                                f32::MAX
                            } else {
                                10. + generation as f32
                            }),
                        },
                        max_health: Some(if stress == Stress::Maximum {
                            f32::MAX
                        } else {
                            40.
                        }),
                    })
                    .collect(),
                ..Default::default()
            })
            .unwrap();
        }
        if maximum {
            debris.base.shift = (if stress == Stress::Maximum {
                frame.min(100)
            } else {
                frame
            } / 20) as i32;
        }
        host.observe_presentation(
            Some(mod_host::WorldObservation {
                session: 2,
                dimension: 0,
                world: "x".repeat(mod_api::MAX_WORLD_IDENTITY_BYTES),
                position: (0., 0., 0.),
                alive: true,
                runtime_id: 99,
            }),
            None,
            None,
            Vec::new(),
        )
        .unwrap();
        host.observe_gameplay(GameplayFacts {
            camera: Some(GameplayCameraPose {
                position: GameplayVector3 {
                    x: 0.,
                    y: 1.6,
                    z: 4.,
                },
                yaw: 0.,
                pitch: 0.,
            }),
            relayed_position: Some(GameplayVector3 {
                x: 2. + frame as f32,
                y: 0.,
                z: 0.,
            }),
            ..Default::default()
        })
        .unwrap();
        mod_host::refresh_block_searches(&mut [&mut host], &mut debris);
        let mut controls = empty_controls();
        controls.seconds = 0.05;
        controls.focused = true;
        controls.gameplay = true;
        controls.keys_held = held.clone();
        controls.keys_admitted = held.clone();
        controls.keys_pressed = held.clone();
        let toggles = stress.toggles(frame);
        if !toggles.is_empty() {
            controls.panel_open = true;
            controls.keys_admitted.clear();
            for (id, value) in &toggles {
                controls.events.push(ControlEvent {
                    id: (*id).into(),
                    value: *value,
                });
            }
            while controls.events.len() < ui::mod_panel::MAX_PANEL_EVENTS {
                controls.events.push(ControlEvent {
                    id: "x".repeat(ui::mod_panel::MAX_PANEL_ID_BYTES),
                    value: 1.,
                });
            }
        }
        let used = metered_callback(&mut host, &budget, |host| {
            host.frame_with_movement(
                false,
                Some(GameplaySnapshot {
                    session: 2,
                    dimension: 0,
                    eye: GameplayVector3 {
                        x: 0.,
                        y: 1.6,
                        z: 0.,
                    },
                    yaw: 0.,
                    pitch: 0.,
                    frame_seconds: 0.05,
                    players: if aim {
                        (0..mod_api::MAX_GAMEPLAY_PLAYERS)
                            .map(|index| mod_host::GameplayPlayer {
                                runtime_id: index as u64 + 200,
                                position: GameplayVector3 {
                                    x: -0.5 + index as f32 * 0.001,
                                    y: 0.,
                                    z: -3.,
                                },
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    attack_held: true,
                }),
                Vec::new(),
                None,
                Some(mod_host::GameplayMovementSnapshot {
                    session: 2,
                    dimension: 0,
                    tick: frame,
                    knockback_sequence: frame / 2,
                    on_ground: frame % 2 == 1,
                    jump_held: false,
                    sprint_held: false,
                    sneak_held: false,
                    eligible: true,
                    velocity: GameplayVector3 {
                        x: 0.2,
                        y: -0.08,
                        z: 0.,
                    },
                }),
                controls,
            )
            .unwrap_or_else(|error| panic!("binding/drawing frame {frame}: {error:#}"));
        });
        most = most.max(used);
        if esp {
            if let Some((surface, _)) = host.hud_surface() {
                for slot in 0..6 {
                    if let Some(ui::mod_panel::SurfaceValue::Text(name)) =
                        surface.bindings.get(&format!("#esp_{slot}_line1"))
                    {
                        if *name != previous[slot] {
                            updates[slot] += 1;
                            previous[slot] = name.clone();
                        }
                    }
                }
            }
        }
        if esp && frame == frames - 1 && stress != Stress::Churn {
            assert_eq!(shown_labels(&host), 6, "deferred ESP must catch up");
            let expected = stress.name(frame);
            let surface = host.hud_surface().unwrap().0;
            for slot in 0..6 {
                assert_eq!(
                    surface.bindings[&format!("#esp_{slot}_line1")],
                    ui::mod_panel::SurfaceValue::Text(expected.clone()),
                    "name update must progress"
                );
            }
        }
        if stress == Stress::Maximum {
            if frame >= 12 {
                for (id, value) in stress.toggles(frame - 12) {
                    let control = host
                        .panel()
                        .unwrap()
                        .controls
                        .iter()
                        .find(|control| control.id() == id)
                        .unwrap();
                    let ui::mod_panel::Control::Toggle {
                        value: published, ..
                    } = control
                    else {
                        panic!("toggle control")
                    };
                    assert_eq!(*published, value >= 0.5, "panel queue deadline for {id}");
                    if id == "show_real_position" {
                        let saved: serde_json::Value = serde_json::from_slice(
                            &std::fs::read(_directory.path().join("drawing.settings.json"))
                                .unwrap(),
                        )
                        .unwrap();
                        assert_eq!(saved[id], value >= 0.5, "settings queue deadline");
                    }
                }
            }
            // Disable deadlines: 32 callbacks for 256 cubes; immediate HUD/mask removal;
            // five marker instances retire in one bounded batch.
            if [331, 831].contains(&frame) {
                assert!(
                    drawn_instances(&host) <= if frame == 831 { 0 } else { 5 },
                    "Finder teardown deadline"
                );
            }
            if [501, 815].contains(&frame) {
                assert_eq!(shown_labels(&host), 0);
                assert!(host.render().0.masks.is_empty());
            }
            if frame == 601 {
                assert_eq!(drawn_instances(&host), 256);
            }
            if frame == 701 {
                assert!(!host.fullbright());
            }
            if [148, 380, 564, 664, 764, 880].contains(&frame) {
                assert_eq!(
                    shown_labels(&host),
                    6,
                    "48-callback ESP deadline at {frame}"
                );
                let surface = host.hud_surface().unwrap().0;
                for slot in 0..6 {
                    assert_eq!(
                        surface.bindings[&format!("#esp_{slot}_line1")],
                        ui::mod_panel::SurfaceValue::Text(stress.name(frame)),
                        "48-callback name deadline"
                    );
                }
            }
            // Every producer must converge after initial load and every re-enable, with
            // names and terrain settled. This checks exact output, not only queue activity.
            if [290, 490, 590, 690, 790, 990].contains(&frame) {
                assert_eq!(
                    drawn_instances(&host),
                    261,
                    "drawing convergence at {frame}"
                );
                assert_eq!(shown_labels(&host), 6, "label convergence at {frame}");
                assert_eq!(host.render().0.masks.len(), 1);
                assert!(host.fullbright());
                let origins = drawn_origins(&host);
                for point in debris.blocks(256) {
                    assert!(
                        origins.contains(&[point.x as i64, point.y as i64, point.z as i64]),
                        "terrain convergence at {frame}"
                    );
                }
            }
        }
        if frame == 80 {
            middle_cubes = drawn_origins(&host)
                .into_iter()
                .filter(|origin| origin[2] < 0)
                .collect();
        }
        // Four binding rows per callback; completion must stay silent until the full revision.
        if frame < completed {
            collecting = collecting.max(used);
        } else if frame == completed {
            completion = used;
        } else {
            steady = steady.max(used);
        }
        let attack = host.take_interaction().attack_pulse;
        let camera = host.take_camera_delta().is_some();
        let jump = host.take_jump_pulse();
        let cancel = host.take_jump_cancel();
        if stress != Stress::Maximum {
            assert_eq!(attack, frame > completed);
            assert_eq!(camera, aim && frame >= completed);
            assert_eq!(jump, jump_reset && frame >= completed + 2 && frame % 2 == 1);
            assert_eq!(cancel, !jump_reset || frame < completed);
        } else if (100..190).contains(&frame) {
            assert!(attack && camera, "mandatory work continues");
        }
        let origins = drawn_origins(&host);
        if (!esp || frame >= completed) && !stress.collisions() {
            assert_eq!(
                origins.iter().filter(|origin| origin[2] == 0).count(),
                5,
                "moving FakeLag marker on frame {frame}"
            );
        }
        let marker: Vec<_> = origins
            .iter()
            .copied()
            .filter(|origin| origin[2] == 0)
            .collect();
        if frame == 0 {
            first_marker = marker.clone();
        }
        if frame == completed {
            assert_ne!(
                marker, first_marker,
                "marker moved before collection completed"
            );
            if !esp {
                assert!(
                    origins.iter().any(|origin| origin[2] < 0),
                    "Finder active at completion"
                );
            }
        }
    }
    assert_eq!(drawn_instances(&host), 256 + 5);
    if stress == Stress::Churn {
        assert!(
            updates.iter().all(|n| *n >= 10),
            "name queue starvation: {updates:?}"
        );
    }
    println!("stress {stress:?}: name updates {updates:?}");
    if maximum {
        let last: Vec<_> = drawn_origins(&host)
            .into_iter()
            .filter(|origin| origin[2] < 0)
            .collect();
        assert_ne!(middle_cubes, last, "deferred terrain updates must progress");
    }
    if esp {
        assert_eq!(shown_labels(&host), 6);
        assert_eq!(host.render().0.masks.len(), 1);
        assert!(host.fullbright());
    }
    assert!(
        most <= 95_000,
        "combined workload needs 5% headroom: {most}"
    );
    println!(
        "128 binding rows, 110 keys, 64 held, unheld jump + Finder + moving FakeLag \
        (aim={aim}, jump_reset={jump_reset}, late={late}, esp={esp}, maximum={maximum}): \
        peak {most}, collecting {collecting}, completion {completion}, steady {steady} fuel \
        (target {FRAME_FUEL})"
    );
}

/// Adversarial and lifecycle variants of the same full gameplay load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stress {
    Standard,
    Quotes,
    Escaped,
    Control,
    Longest,
    Churn,
    Collision,
    Maximum,
}
impl Stress {
    /// Whether to use 256 nearby positions that cluster a multiplicative lookup index.
    fn collisions(self) -> bool {
        matches!(self, Self::Collision | Self::Maximum)
    }
    /// Maximum-byte legal names, including their serialized quote/backslash expansion.
    fn name(self, frame: u64) -> String {
        let generation = if self == Self::Churn {
            frame
        } else {
            frame.min(99) / 20
        };
        match self {
            Self::Quotes => format!("{generation:03}{}", "\"".repeat(125)),
            Self::Escaped | Self::Maximum => {
                (if generation % 2 == 0 { "\"" } else { "\\" }).repeat(128)
            }
            Self::Control => format!("{generation:03}{}\u{00a0}", "x".repeat(123)),
            _ => {
                let unit = format!("§{generation}");
                unit.repeat(128 / unit.len())
            }
        }
    }
    /// Actual panel control events toggle each drawing producer under the full input load.
    fn toggles(self, frame: u64) -> Vec<(&'static str, f32)> {
        if self != Self::Maximum {
            return Vec::new();
        }
        let (ids, enabled): (&[&str], bool) = match frame {
            300 | 332 => (&["netherite"], frame == 332),
            500 | 516 => (&["player_esp"], frame == 516),
            600 | 616 => (&["show_real_position"], frame == 616),
            700 | 716 => (&["fullbright"], frame == 716),
            800 | 832 => (
                &[
                    "netherite",
                    "player_esp",
                    "show_real_position",
                    "fullbright",
                ],
                frame == 832,
            ),
            _ => return Vec::new(),
        };
        ids.iter()
            .map(|id| (*id, if enabled { 1. } else { 0. }))
            .collect()
    }
}

/// Fills the host's settings byte allowance while preserving valid maximum-range preferences.
fn maximum_settings(maximum: bool) -> String {
    let mut settings = r#"{"netherite_range":128,"esp_range":128,"show_real_position":true,"cps":20,"padding":""}"#.to_owned();
    if maximum {
        settings = r#"{"netherite_range":128,"esp_range":128,"show_real_position":true,"cps":30,"aim_strength":100,"aim_mode":"continuous","reach_blocks":6,"fake_lag_ms":1000,"jump_reset_window_ticks":10,"padding":""}"#.into();
    }
    settings.insert_str(
        settings.len() - 2,
        &"x".repeat(mod_api::MAX_SETTINGS_BYTES - settings.len()),
    );
    assert_eq!(settings.len(), mod_api::MAX_SETTINGS_BYTES);
    settings
}

/// Generates nearby distinct positions sorted by distance; no fixture file is needed.
fn clustered_blocks() -> Vec<[i32; 3]> {
    let mut buckets = vec![Vec::new(); 512];
    for x in -64i32..=64 {
        for z in -64i32..=64 {
            for y in 0i32..=32 {
                let key = ((x as i64 + (1 << 25)) as u64) << 38
                    | ((z as i64 + (1 << 25)) as u64) << 12
                    | (y as u64 + (1 << 11));
                let bucket = (key.wrapping_mul(0x9e3779b97f4a7c15) >> 55) as usize;
                buckets[bucket].push([x, y, z]);
            }
        }
    }
    let mut points = buckets.into_iter().max_by_key(Vec::len).unwrap();
    points.sort_by_key(|p| p.iter().map(|v| v * v).sum::<i32>());
    points.truncate(256);
    assert!(
        points
            .iter()
            .all(|p| p.iter().map(|v| v * v).sum::<i32>() < 54 * 54)
    );
    points
}

/// Search source with either an ordinary slab or a clustered nearby selection.
struct StressTerrain {
    base: Debris,
    clustered: Option<Vec<[i32; 3]>>,
}
impl StressTerrain {
    /// The selected points for exact convergence checks.
    fn blocks(&self, limit: usize) -> Vec<TerrainPosition> {
        match &self.clustered {
            None => self.base.blocks(limit),
            Some(points) => points
                .iter()
                .take(limit)
                .map(|[x, y, z]| TerrainPosition {
                    x: x + self.base.shift,
                    y: *y,
                    z: *z,
                })
                .collect(),
        }
    }
}
impl BlockSearchSource for StressTerrain {
    fn find(&mut self, search: &BlockSearch) -> Result<FoundBlocks, TerrainReadError> {
        Ok(FoundBlocks {
            session: 2,
            dimension: 0,
            positions: self.blocks(search.limit as usize),
        })
    }
}

/// Maximum observations, escaping and retained geometry coexist with panel lifecycle changes.
#[test]
fn maximum_everything_and_drawing_lifecycles_fit_with_headroom() {
    combined_workload(true, true, true, true, true, Stress::Maximum);
}
/// Each independently reported boundary remains covered alongside all enabled modules.
#[test]
fn escaped_names_fit_with_every_module() {
    combined_workload(true, true, true, true, true, Stress::Escaped);
}
#[test]
fn quote_names_fit_with_every_module() {
    combined_workload(true, true, true, true, true, Stress::Quotes);
}
#[test]
fn unicode_names_fit_with_every_module() {
    combined_workload(true, true, true, true, true, Stress::Control);
}
#[test]
fn longest_desktop_keys_fit_with_every_module() {
    combined_workload(true, true, true, true, true, Stress::Longest);
}
#[test]
fn colliding_terrain_positions_fit_with_every_module() {
    combined_workload(true, true, true, true, true, Stress::Collision);
}
#[test]
fn continuously_changed_names_keep_progressing() {
    combined_workload(true, true, true, true, true, Stress::Churn);
}
