//! Capability-only adapter for the generated Cinnabar component contract.

use std::cell::RefCell;

use mod_api::bindings::{
    Guest,
    cinnabar::extension::{
        environment, gameplay, hud, input, panel, player_state, render, settings,
    },
};

use mod_api::bindings::cinnabar::world_draw::drawing::{self, Permissions};

use crate::{
    aim::{AimAssist, Frame, Player, Vec3},
    jump_reset::{Frame as JumpFrame, JumpReset},
    state::{Controls, Event, Preferences, State},
};

thread_local! {
    static MODULES: RefCell<State> = RefCell::new(State::new(Preferences::default()));
    static JUMP_RESET: RefCell<JumpReset> = RefCell::new(JumpReset::default());
    static ESP_GEOMETRY: RefCell<Option<(u64, i32, crate::esp::Geometry)>> = const { RefCell::new(None) };
    static ESP_LABELS: RefCell<bool> = const { RefCell::new(false) };
    static AIM: RefCell<AimAssist> = RefCell::new(AimAssist::default());
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
        ESP_GEOMETRY.with(|geometry| *geometry.borrow_mut() = None);
        ESP_LABELS.with(|labels| *labels.borrow_mut() = false);
        AIM.with(|aim| aim.borrow_mut().reset());
        JUMP_RESET.with(|reset| reset.borrow_mut().reset());
    }

    fn frame() {
        let Ok(controls) = input::read_controls() else {
            AIM.with(|aim| aim.borrow_mut().reset());
            JUMP_RESET.with(|reset| reset.borrow_mut().reset());
            let _ = gameplay::cancel_jump();
            clear_esp_labels();
            return;
        };
        MODULES.with(|modules| {
            let mut modules = modules.borrow_mut();
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
            let _ = gameplay::set_show_real_position(modules.show_real_position());

            let snapshot = if controls.focused && controls.gameplay {
                gameplay::read_frame().ok().flatten()
            } else {
                None
            };
            let _ = gameplay::set_attack_reach(modules.attack_reach(snapshot.is_some()));
            let cadence = snapshot.as_ref().map(|frame| {
                (
                    frame.session,
                    frame.dimension,
                    frame.attack_held,
                    frame.frame_seconds,
                )
            });
            if modules.attack_pulse(cadence) {
                let _ = gameplay::pulse_attack();
            }

            let movement = if controls.focused && controls.gameplay && !controls.panel_open {
                gameplay::read_movement().ok().flatten()
            } else {
                None
            };
            if !modules.modules.jump_reset
                || movement
                    .as_ref()
                    .is_none_or(|frame| !frame.eligible || frame.jump_held)
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
                    jump_held: frame.jump_held,
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

            if modules.modules.player_esp
                && !controls.panel_open
                && drawing::capabilities()
                    .contains(Permissions::WORLD_DRAW | Permissions::RENDER_THROUGH_WALLS)
                && let Some(frame) = snapshot.as_ref()
                && let Some(geometry) = esp_geometry(frame.session, frame.dimension)
                && let Ok(actors) =
                    player_state::read_actors_limited(crate::esp::MAX_PLAYERS as u32)
            {
                let output = crate::esp::build(
                    &modules.preferences,
                    frame.session,
                    frame.dimension,
                    [frame.eye.x, frame.eye.y, frame.eye.z],
                    &actors,
                    geometry,
                );
                if drawing::replace_frame_group("player_esp", &output.draws).is_ok() {
                    if hud::set_content(&output.labels).is_ok() {
                        ESP_LABELS.with(|labels| *labels.borrow_mut() = true);
                    } else {
                        clear_esp_labels();
                    }
                } else {
                    clear_esp_labels();
                }
            } else {
                clear_esp_labels();
            }

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
                attack_held: snapshot.attack_held,
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
    if modules.finder_dirty {
        let spec = modules
            .block_highlights()
            .map(|spec| render::BlockHighlightSpec {
                identifiers: spec.identifiers,
                range: spec.range,
                color: render::Rgba {
                    r: spec.color[0],
                    g: spec.color[1],
                    b: spec.color[2],
                    a: spec.color[3],
                },
            });
        match render::set_block_highlights(spec.as_ref()) {
            Ok(()) => modules.finder_dirty = false,
            Err(error) => {
                let _ = hud::set_label(&format!("Netherite Finder: {error}"));
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

/// Clears retained labels whenever ESP, focus, session data or drawing authority is absent.
fn clear_esp_labels() {
    ESP_LABELS.with(|labels| {
        let mut labels = labels.borrow_mut();
        if *labels && hud::set_content("").is_ok() {
            *labels = false;
        }
    });
}

/// Reuses resources within one session and releases superseded handles when still valid.
fn esp_geometry(session: u64, dimension: i32) -> Option<crate::esp::Geometry> {
    ESP_GEOMETRY.with(|cached| {
        let mut cached = cached.borrow_mut();
        if let Some((old_session, old_dimension, geometry)) = *cached {
            if old_session == session && old_dimension == dimension {
                return Some(geometry);
            }
            let _ = drawing::remove(geometry.boxes);
            let _ = drawing::remove(geometry.health);
            *cached = None;
        }
        let geometry = crate::esp::Geometry::create()?;
        *cached = Some((session, dimension, geometry));
        Some(geometry)
    })
}
