use mod_host::{
    BlockSearch, BlockSearchSource, CallbackBudget, ControlEvent, FoundBlocks, GameplayCameraPose,
    GameplayFacts, GameplaySnapshot, GameplayVector3, ModGrants, ModHost, TerrainPosition,
    TerrainReadError, empty_controls,
};

/// Fuel of one mod frame callback in `mod-host`; a callback that needs more traps.
const FRAME_FUEL: u64 = 100_000;

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
    let budget = CallbackBudget::new(mod_api::TOTAL_MOD_FRAME_FUEL);
    host.set_frame_authority(budget.clone(), false, false, 0);
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
    budget.reset(mod_api::TOTAL_MOD_FRAME_FUEL);
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
    host.frame_with_controls(false, snapshot, controls)
        .unwrap_or_else(|error| panic!("metered frame: {error:#}"));
    assert!(host.is_active());
    let used = mod_api::TOTAL_MOD_FRAME_FUEL - budget.remaining();
    assert!(used <= FRAME_FUEL, "{used} fuel");
    used
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
        host.frame_with_controls(false, None, controls)
            .unwrap_or_else(|error| panic!("drag frame {frame}: {error:#}"));
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
        sprint_held: false,
        sneak_held: false,
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
        for frame in 0..3 {
            // Player-only consent filters mobs before the bounded six-row result.
            observe(&mut host, frame * 5);
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), controls)
                .unwrap_or_else(|error| panic!("ESP frame {frame}, granted {granted}: {error:#}"));
            assert!(host.is_active());
            assert_eq!(host.render().0.masks.len(), usize::from(granted));
            // Three labels take a new name per frame; the fixture's players share one name.
            let labels = if frame == 0 { 3 } else { 6 };
            assert_eq!(shown_labels(&host), if granted { labels } else { 0 });
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
            let mut controls = empty_controls();
            controls.focused = true;
            controls.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), controls)
                .unwrap();
            assert!(host.render().0.masks.is_empty());
            assert_eq!(shown_labels(&host), 0);
        }
        // Focus loss, returning to a menu, disabling and missing observations all clear retained HUD.
        for reason in 0..5 {
            observe(&mut host, 10);
            let mut active = empty_controls();
            active.focused = true;
            active.gameplay = true;
            host.frame_with_controls(false, Some(snapshot()), active)
                .unwrap();
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
            host.frame_with_controls(
                false,
                if reason == 2 { None } else { Some(snapshot()) },
                controls,
            )
            .unwrap();
            assert!(host.render().0.masks.is_empty());
            assert_eq!(shown_labels(&host), 0);
        }
    }
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
    metered_frame(
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
    let mut run = |host: &mut ModHost, debris: &mut Debris, frames: usize| {
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
    metered_frame(&mut host, &budget, false, &[], GameplayFacts::default());
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
    println!(
        "Netherite Finder, 256 cubes: most fuel per frame {drawing} drawing, {steady} steady, \
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
    metered_frame(
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
        use_held: false,
    };
    let mut most = 0;
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
    metered_frame(
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
    let mut most = 0;
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
            use_held: false,
        };
        most = most.max(metered_frame(&mut host, &budget, true, &[], facts));
    }
    assert_eq!(host.render().0.masks.len(), 1);
    assert_eq!(shown_labels(&host), 6);
    assert_eq!(drawn_instances(&host), 256 + 5);
    assert!(host.fullbright());
    println!("Player ESP, Netherite Finder and FakeLag marker: most fuel per frame {most}");
}
