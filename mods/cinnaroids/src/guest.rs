//! Capability-only adapter for the generated Cinnabar component contract.

use std::cell::RefCell;

use mod_api::bindings::{
    Guest,
    cinnabar::extension::{
        environment, gameplay, hud, input, panel, player_state, settings, terrain,
    },
};

use mod_api::bindings::cinnabar::world_draw::{
    drawing::{self, DrawError, ErrorCode, Handle, Instance, Permissions},
    masks,
};

use crate::{
    aim::{AimAssist, Frame, Player, Vec3},
    finder::{self, Change},
    ghost::{self, Marker},
    jump_reset::{Frame as JumpFrame, JumpReset},
    state::{Controls, Event, Preferences, State},
};

// Drawing calls one callback may make, which Cinnabar enforces by trapping: capability checks
// for the finder, marker and ESP, the finder's cube geometry and placements, and the marker's
// geometry and placements.
const _: () =
    assert!(3 + 1 + finder::SYNC_CALLS + 2 * crate::ghost::INSTANCES <= mod_api::MAX_RENDER_CALLS);

thread_local! {
    static MODULES: RefCell<State> = RefCell::new(State::new(Preferences::default()));
    static JUMP_RESET: RefCell<JumpReset> = RefCell::new(JumpReset::default());
    static ESP_MASK: RefCell<Option<(u64, i32, Handle)>> = const { RefCell::new(None) };
    /// The ESP label slots while their HUD surface is set.
    static ESP_LABELS: RefCell<Option<crate::esp::Labels>> = const { RefCell::new(None) };
    static BINDINGS: RefCell<crate::bindings::Cache> = RefCell::new(crate::bindings::Cache::default());
    static AIM: RefCell<AimAssist> = RefCell::new(AimAssist::default());
    static FINDER: RefCell<finder::Finder> = RefCell::new(finder::Finder::default());
    static FINDER_BLOCKS: Vec<String> = finder::IDENTIFIERS.map(String::from).to_vec();
    static MARKER: RefCell<Marker> = RefCell::new(Marker::default());
}

struct Cinnaroids;

impl Guest for Cinnaroids {
    fn init() {
        let preferences = settings::load()
            .ok()
            .map_or_else(Preferences::default, |json| Preferences::from_json(&json));
        MODULES.with(|modules| {
            let mut modules = modules.borrow_mut();
            *modules = State::new(preferences);
            // Publish on the first frame, with a fresh budget after settings parsing.
        });
        ESP_MASK.with(|mask| *mask.borrow_mut() = None);
        ESP_LABELS.with(|labels| *labels.borrow_mut() = None);
        BINDINGS.with(|bindings| *bindings.borrow_mut() = crate::bindings::Cache::default());
        AIM.with(|aim| aim.borrow_mut().reset());
        JUMP_RESET.with(|reset| reset.borrow_mut().reset());
        FINDER.with(|finder| finder.borrow_mut().reset());
        MARKER.with(|marker| marker.borrow_mut().reset());
    }

    fn frame() {
        let Ok(controls) = input::read_controls() else {
            AIM.with(|aim| aim.borrow_mut().reset());
            JUMP_RESET.with(|reset| reset.borrow_mut().reset());
            let _ = gameplay::cancel_jump();
            clear_esp();
            netherite(None, None, false);
            real_position(false, None, false);
            return;
        };
        MODULES.with(|modules| {
            let mut modules = modules.borrow_mut();
            let (attack_held, jump_held, bindings_ready) = BINDINGS.with(|bindings| {
                let mut bindings = bindings.borrow_mut();
                bindings.read();
                let reserved = modules.reserved_keys();
                let holds = bindings.resolve(&controls, &reserved);
                (holds.attack, holds.jump, bindings.ready())
            });
            let events: Vec<_> = controls
                .events
                .into_iter()
                .map(|event| Event {
                    id: event.id,
                    value: event.value,
                })
                .collect();
            modules.controls(Controls {
                focused: controls.focused,
                panel_open: controls.panel_open,
                keys: &controls.keys_pressed,
                held: &controls.keys_held,
                seconds: controls.seconds,
                events: &events,
            });
            publish(&mut modules);
            let _ = gameplay::set_packet_delay(modules.packet_delay_ms());

            let snapshot = if controls.focused && controls.gameplay {
                gameplay::read_frame().ok().flatten()
            } else {
                None
            };
            // These draw only over focused gameplay, as the host-drawn versions did.
            let drawn = snapshot.as_ref().filter(|_| !controls.panel_open);
            let labelled = player_esp(&modules, drawn);
            // A frame that names new labels leaves no fuel for comparing the finder's answer.
            netherite(modules.finder_range(), drawn, labelled);
            real_position(modules.show_real_position(), drawn, labelled);
            let _ = gameplay::set_attack_reach(modules.attack_reach(snapshot.is_some()));
            let cadence = snapshot.as_ref().map(|frame| {
                (
                    frame.session,
                    frame.dimension,
                    attack_held,
                    frame.frame_seconds,
                )
            });
            if modules.attack_pulse(cadence) {
                let _ = gameplay::pulse_attack();
            }

            let movement = if bindings_ready
                && controls.focused
                && controls.gameplay
                && !controls.panel_open
            {
                gameplay::read_movement().ok().flatten()
            } else {
                None
            };
            if !modules.modules.jump_reset
                || movement
                    .as_ref()
                    .is_none_or(|frame| !frame.eligible || jump_held)
            {
                let _ = gameplay::cancel_jump();
            }
            JUMP_RESET.with(|reset| {
                let mut reset = reset.borrow_mut();
                let frame = movement.map(|frame| JumpFrame {
                    session: frame.session,
                    dimension: frame.dimension,
                    tick: frame.tick,
                    knockback_sequence: frame.knockback_sequence,
                    on_ground: frame.on_ground,
                    jump_held,
                    eligible: frame.eligible,
                    velocity: [frame.velocity.x, frame.velocity.y, frame.velocity.z],
                });
                if reset.update(
                    modules.modules.jump_reset,
                    modules.preferences.jump_reset_window_ticks,
                    frame,
                ) && gameplay::pulse_jump().is_err()
                {
                    reset.reset();
                }
            });

            let Some(snapshot) = snapshot else {
                AIM.with(|aim| aim.borrow_mut().reset());
                return;
            };
            let config = modules.aim_config();
            if !config.enabled || config.strength == 0 {
                AIM.with(|aim| aim.borrow_mut().reset());
                return;
            }
            let players: Vec<_> = snapshot
                .players
                .iter()
                .take(mod_api::MAX_GAMEPLAY_PLAYERS)
                .map(|player| Player {
                    id: player.runtime_id,
                    feet: Vec3 {
                        x: player.position.x,
                        y: player.position.y,
                        z: player.position.z,
                    },
                })
                .collect();
            let frame = Frame {
                session: snapshot.session,
                dimension: snapshot.dimension,
                eye: Vec3 {
                    x: snapshot.eye.x,
                    y: snapshot.eye.y,
                    z: snapshot.eye.z,
                },
                yaw: snapshot.yaw,
                pitch: snapshot.pitch,
                attack_held,
                seconds: snapshot.frame_seconds,
                players: &players,
            };
            AIM.with(|aim| {
                let mut aim = aim.borrow_mut();
                if let Some(rotation) = aim.update(config, Some(&frame))
                    && gameplay::rotate(rotation.yaw, rotation.pitch).is_err()
                {
                    aim.reset();
                }
            });
        });
    }
}

fn publish(modules: &mut State) {
    if modules.fullbright_dirty {
        match environment::set_fullbright(modules.modules.fullbright) {
            Ok(()) => modules.fullbright_dirty = false,
            Err(error) => {
                let _ = hud::set_label(&format!("Fullbright: {error}"));
            }
        }
    }
    if modules.reservations_dirty && input::reserve_keys(&modules.reserved_keys()).is_ok() {
        modules.reservations_dirty = false;
    }
    if modules.panel_dirty
        && let Ok(json) = modules.panel_shell_json()
        && let Ok(update) = modules.panel_update_json()
        && panel::set_content(&json).is_ok()
        && panel::update_content(&update).is_ok()
    {
        modules.panel_dirty = false;
    }
    if modules.preferences_dirty {
        let saved = serde_json::to_string(&modules.preferences)
            .ok()
            .is_some_and(|json| settings::save(&json).is_ok());
        if saved {
            modules.preferences_dirty = false;
        } else {
            let _ = hud::set_label("Cinnaroids: preferences could not be saved");
        }
    }
}

mod_api::bindings::export!(Cinnaroids with_types_in mod_api::bindings);

/// Outlines and labels nearby players over focused gameplay, `frame`, while Player ESP is on
/// and its drawing authority is granted; otherwise clears both. Returns whether a label took a
/// new name, the costliest change.
fn player_esp(modules: &State, frame: Option<&gameplay::Snapshot>) -> bool {
    if modules.modules.player_esp
        && drawing::capabilities().contains(
            Permissions::ENTITY_OBSERVE
                | Permissions::ENTITY_HIGHLIGHT
                | Permissions::RENDER_THROUGH_WALLS,
        )
        && let Some(frame) = frame
        && let Ok(Some(world)) = player_state::read_world()
        && world.dimension == frame.dimension
        && let Ok(actors) = player_state::read_actors_limited(crate::esp::MAX_PLAYERS as u32)
    {
        let (shown, named) = ESP_LABELS.with(|labels| {
            let mut labels = labels.borrow_mut();
            let fresh = labels.is_none();
            let output = labels.get_or_insert_with(Default::default).update(
                &modules.preferences,
                world.session,
                world.dimension,
                [frame.eye.x, frame.eye.y, frame.eye.z],
                &actors,
            );
            let shown = show_esp(
                world.session,
                world.dimension,
                modules.preferences.esp_color,
                &output.players,
            ) && (!fresh || hud::set_surface(crate::esp::SURFACE).is_ok())
                && output
                    .labels
                    .is_none_or(|update| hud::update_surface(&update).is_ok());
            (shown, output.named)
        });
        if shown {
            return named;
        }
    }
    clear_esp();
    false
}

/// Releases the retained selection and labels when the overlay cannot be presented.
fn clear_esp() {
    ESP_MASK.with(|cached| {
        if let Some((_, _, mask)) = cached.borrow_mut().take() {
            let _ = masks::remove(mask);
        }
    });
    if ESP_LABELS.with(|labels| labels.borrow_mut().take().is_some()) {
        let _ = hud::set_surface("");
    }
}

/// Whether a drawing error means a retained geometry token no longer exists.
fn stale(error: &DrawError) -> bool {
    error.code == ErrorCode::StaleHandle
}

/// Renews the finder's block search every frame and, on its cadence, takes a changed answer as
/// the selection; then moves the retained cubes a bounded step toward it. `range` is set while
/// the module is enabled. Outside focused gameplay the cubes stay as they are. A `busy` frame
/// only renews the search.
fn netherite(range: Option<u8>, frame: Option<&gameplay::Snapshot>, busy: bool) {
    FINDER.with(|finder| {
        let mut finder = finder.borrow_mut();
        let capabilities = drawing::capabilities();
        if !capabilities.contains(Permissions::WORLD_DRAW) {
            // Drawing authority changes reload the component; nothing was drawn.
            finder.reset();
            return;
        }
        match (range, frame) {
            (None, _) => {
                finder.clear_notice();
                finder.clear();
            }
            (Some(_), None) => finder.pause(),
            (Some(range), Some(frame)) => {
                let through_walls = capabilities.contains(Permissions::RENDER_THROUGH_WALLS);
                // A search left out of a callback is unsubscribed and the client stops scanning.
                let found = FINDER_BLOCKS.with(|blocks| {
                    terrain::find_blocks(blocks, f32::from(range), finder::LIMIT as u32)
                });
                let due = finder.due(
                    frame.session,
                    frame.dimension,
                    range,
                    through_walls,
                    frame.frame_seconds,
                );
                match found {
                    Ok(_) if due && busy => finder.postpone(),
                    Ok(positions) if due => {
                        finder.apply(frame.dimension, through_walls, &positions);
                    }
                    Ok(_) => {}
                    Err(terrain::ReadError::Denied) => {
                        finder.clear();
                        if finder.notify_denied() {
                            let _ = hud::set_label("Netherite Finder: terrain permission denied");
                        }
                    }
                    // Other failures keep the cubes until a later answer.
                    Err(_) => {}
                }
            }
        }
        if busy {
            return;
        }
        if finder.comparing() {
            finder.compare(finder::COMPARE_BLOCKS);
            return;
        }
        if finder.synced() {
            return;
        }
        let geometry = match finder.geometry {
            Some(geometry) => geometry,
            None if !finder.places() => return,
            None => match drawing::create_geometry(&finder::cube()) {
                Ok(geometry) => *finder.geometry.insert(geometry),
                Err(_) => return,
            },
        };
        let (dimension, through_walls) = finder.placement();
        // One placement is reused for every call; only its anchor changes.
        let mut placement: Option<Instance> = None;
        let synced = finder.sync(finder::SYNC_CALLS, |change| match change {
            Change::Create(position) | Change::Move(_, position) => {
                let instance = placement.get_or_insert_with(|| {
                    finder::cube_instance(geometry, dimension, position, through_walls)
                });
                instance.anchor = finder::anchor(dimension, position);
                match change {
                    Change::Move(handle, _) => drawing::update(handle, instance).map(|()| None),
                    _ => drawing::create(instance).map(Some),
                }
            }
            Change::Remove(handle) => drawing::remove(handle).map(|()| None),
        });
        if let Err(error) = synced
            && stale(&error)
        {
            finder.reset();
        }
    });
}

/// Draws FakeLag's last relayed position while the option is on and packets are delayed. A
/// `busy` frame does not place a hidden marker.
fn real_position(show: bool, frame: Option<&gameplay::Snapshot>, busy: bool) {
    MARKER.with(|marker| {
        let mut marker = marker.borrow_mut();
        let frame =
            frame.filter(|_| show && drawing::capabilities().contains(Permissions::WORLD_DRAW));
        let sample = frame.and_then(|frame| {
            let position = gameplay::read_relayed_position().ok().flatten()?;
            Some((frame.session, [position.x, position.y, position.z]))
        });
        // The presented camera, including third person, decides whether it is inside the box;
        // the eye stands in when the host has no pose.
        let camera = sample.and(frame).map_or([0.; 3], |frame| {
            let camera = gameplay::read_camera()
                .ok()
                .flatten()
                .map_or(frame.eye, |pose| pose.position);
            [camera.x, camera.y, camera.z]
        });
        let seconds = frame.map_or(0., |frame| frame.frame_seconds);
        let target = marker
            .update(sample, camera, seconds)
            .zip(frame)
            .map(|(feet, frame)| (frame.dimension, feet));
        if let Err(error) = place_marker(&mut marker, target, busy)
            && stale(&error)
        {
            marker.reset();
        }
    });
}

/// Places, moves or removes the marker's retained instances to stand at `target`, a dimension
/// and feet position; at most one drawing call per instance, plus the geometry once. Placing a
/// hidden marker waits for a frame that is not `busy`.
fn place_marker(
    marker: &mut Marker,
    target: Option<(i32, [f32; 3])>,
    busy: bool,
) -> Result<(), DrawError> {
    let Some((dimension, feet)) = target else {
        if let Some(placed) = marker.placed.take() {
            for handle in placed.handles {
                drawing::remove(handle)?;
            }
        }
        return Ok(());
    };
    if marker
        .placed
        .is_some_and(|placed| (placed.dimension, placed.feet) == (dimension, feet))
        || (busy && marker.placed.is_none())
    {
        return Ok(());
    }
    let geometries = match marker.geometries {
        Some(geometries) => geometries,
        None => {
            let mut created = Vec::with_capacity(ghost::INSTANCES);
            for spec in &ghost::geometries() {
                match drawing::create_geometry(spec) {
                    Ok(handle) => created.push(handle),
                    Err(error) => {
                        for handle in created {
                            let _ = drawing::remove(handle);
                        }
                        return Err(error);
                    }
                }
            }
            let geometries =
                <[Handle; ghost::INSTANCES]>::try_from(created).expect("one geometry per instance");
            *marker.geometries.insert(geometries)
        }
    };
    let instances = ghost::instances(&geometries, dimension, feet);
    if let Some(placed) = &mut marker.placed {
        for (handle, instance) in placed.handles.iter().zip(&instances) {
            drawing::update(*handle, instance)?;
        }
        placed.dimension = dimension;
        placed.feet = feet;
        return Ok(());
    }
    let mut created = Vec::with_capacity(ghost::INSTANCES);
    for instance in &instances {
        match drawing::create(instance) {
            Ok(handle) => created.push(handle),
            Err(error) => {
                for handle in created {
                    let _ = drawing::remove(handle);
                }
                return Err(error);
            }
        }
    }
    marker.placed = Some(ghost::Placed {
        handles: <[Handle; ghost::INSTANCES]>::try_from(created).expect("one handle per instance"),
        dimension,
        feet,
    });
    Ok(())
}

/// Refreshes a bounded union of admitted actor meshes with host-owned outline and glow.
fn show_esp(session: u64, dimension: i32, color: u8, players: &[u64]) -> bool {
    if players.is_empty() {
        return false;
    }
    ESP_MASK.with(|cached| {
        let mut cached = cached.borrow_mut();
        if cached
            .as_ref()
            .is_some_and(|(old_session, old_dimension, _)| {
                (*old_session, *old_dimension) != (session, dimension)
            })
            && let Some((_, _, mask)) = cached.take()
        {
            let _ = masks::remove(mask);
        }
        let mask = match *cached {
            Some((_, _, mask)) => mask,
            None => {
                let Ok(mask) = masks::create(masks::DepthPolicy::Always) else {
                    return false;
                };
                *cached = Some((session, dimension, mask));
                mask
            }
        };
        let entities: Vec<_> = players
            .iter()
            .filter_map(|id| masks::entity(*id).ok())
            .collect();
        !entities.is_empty()
            && masks::select(mask, &entities).is_ok()
            && masks::set_style(mask, Some(crate::esp::appearance(color))).is_ok()
    })
}
