//! Capability-only adapter for the generated Cinnabar component contract.

use std::cell::RefCell;

use mod_api::bindings::{
    Guest,
    cinnabar::extension::{
        budget, environment, gameplay, hud, input, inventory, panel, permissions, player_state,
        settings, terrain,
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
    assert!(4 + 1 + finder::SYNC_CALLS + 2 * crate::ghost::INSTANCES <= mod_api::MAX_RENDER_CALLS);

/// Serialization and canonical-ABI publication each have a bounded turn.
#[derive(Default)]
enum PanelUpdate {
    #[default]
    Idle,
    Shell {
        json: String,
        key: String,
    },
    Controls(String),
    BuildRoute {
        builder: Box<crate::panel::Bindings>,
        json: String,
    },
    Route(String),
}

type EspSample = (u64, i32, Vec<player_state::ObservedActor>, usize);

type FinderAnswer = (u64, i32, bool, Vec<terrain::Position>);

thread_local! {
    static PANEL_BINDINGS_DIRTY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PANEL_KEY: RefCell<Option<String>> = const { RefCell::new(None) };
    static SETTINGS_PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
    static PANEL_PENDING: RefCell<PanelUpdate> = const { RefCell::new(PanelUpdate::Idle) };
    static WORK_TURN: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static MODULES: RefCell<State> = RefCell::new(State::new(Preferences::default()));
    static AUTO_TOTEM: RefCell<crate::auto_totem::AutoTotem> = RefCell::new(crate::auto_totem::AutoTotem::default());
    static JUMP_RESET: RefCell<JumpReset> = RefCell::new(JumpReset::default());
    static ESP_CLEARING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static ESP_PENDING: RefCell<Option<EspSample>> = const { RefCell::new(None) };
    static ESP_MASK: RefCell<Option<(u64, i32, Handle)>> = const { RefCell::new(None) };
    /// The ESP label slots while their HUD surface is set.
    static ESP_LABELS: RefCell<Option<crate::esp::Labels>> = const { RefCell::new(None) };
    static BINDINGS: RefCell<crate::bindings::Cache> = RefCell::new(crate::bindings::Cache::default());
    static AIM: RefCell<AimAssist> = RefCell::new(AimAssist::default());
    static FINDER_ANSWER: RefCell<Option<FinderAnswer>> = const { RefCell::new(None) };
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
            modules.set_permissions(&permissions::read());
            // Publish on the first frame, with a fresh budget after settings parsing.
        });
        ESP_CLEARING.with(|clearing| clearing.set(false));
        ESP_PENDING.with(|pending| *pending.borrow_mut() = None);
        WORK_TURN.with(|first| first.set(0));
        SETTINGS_PENDING.with(|pending| *pending.borrow_mut() = None);
        PANEL_KEY.with(|key| *key.borrow_mut() = None);
        PANEL_BINDINGS_DIRTY.with(|dirty| dirty.set(false));
        PANEL_PENDING.with(|pending| *pending.borrow_mut() = PanelUpdate::Idle);
        ESP_MASK.with(|mask| *mask.borrow_mut() = None);
        ESP_LABELS.with(|labels| *labels.borrow_mut() = None);
        BINDINGS.with(|bindings| *bindings.borrow_mut() = crate::bindings::Cache::default());
        AIM.with(|aim| aim.borrow_mut().reset());
        AUTO_TOTEM.with(|totem| totem.borrow_mut().reset());
        JUMP_RESET.with(|reset| reset.borrow_mut().reset());
        FINDER_ANSWER.with(|answer| *answer.borrow_mut() = None);
        FINDER.with(|finder| finder.borrow_mut().reset());
        MARKER.with(|marker| marker.borrow_mut().reset());
    }

    fn frame() {
        let fuel = FrameFuel(budget::remaining_fuel());
        let pressed = MODULES.with(|modules| {
            let modules = modules.borrow();
            let p = &modules.preferences;
            [
                "MouseLeft",
                "Escape",
                &p.clicker_key,
                &p.aim_key,
                &p.jump_reset_key,
                &p.omni_sprint_key,
                &p.reach_key,
            ]
            .into_iter()
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .collect()
        });
        let Ok(mut controls) = input::read_selected_controls(&input::Selection {
            keys_pressed: Some(pressed),
            keys_held: Some(vec!["MouseLeft".into()]),
            keys_admitted: None,
            events: true,
        }) else {
            MODULES.with(|modules| {
                modules.borrow_mut().attack_pulse(None);
            });
            AIM.with(|aim| aim.borrow_mut().reset());
            AUTO_TOTEM.with(|totem| totem.borrow_mut().reset());
            JUMP_RESET.with(|reset| reset.borrow_mut().reset());
            let _ = gameplay::cancel_jump();
            clear_esp(&fuel);
            netherite(None, None, false, &fuel);
            real_position(false, None, false, &fuel);
            return;
        };
        let capture = MODULES.with(|modules| modules.borrow().capturing_key.is_some())
            || controls.events.iter().any(|event| {
                matches!(
                    event.id.as_str(),
                    "menu_key"
                        | "clicker_key"
                        | "aim_key"
                        | "reach_key"
                        | "jump_reset_key"
                        | "omni_sprint_key"
                )
            });
        if controls.panel_open && capture {
            // Key capture needs the full edge list only while the panel owns gameplay.
            if let Ok(edges) = input::read_selected_controls(&input::Selection {
                keys_pressed: None,
                keys_held: Some(Vec::new()),
                keys_admitted: Some(Vec::new()),
                events: false,
            }) {
                controls.keys_pressed = edges.keys_pressed;
            }
        }
        MODULES.with(|modules| {
            let mut modules = modules.borrow_mut();
            let events: Vec<_> = std::mem::take(&mut controls.events)
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
            // Charge the entire bounded event batch's destruction before optional work.
            drop(events);
            let config = modules.aim_config();
            let attack_needed =
                modules.modules.clicker || (config.enabled && config.only_when_clicking);
            let (attack_held, jump_held, bindings_ready) = BINDINGS.with(|bindings| {
                let mut bindings = bindings.borrow_mut();
                bindings.read();
                let holds = bindings.resolve(
                    &controls,
                    crate::bindings::Holds {
                        attack: attack_needed,
                        jump: modules.modules.jump_reset,
                    },
                );
                (holds.attack, holds.jump, bindings.ready())
            });
            // Pay for admitted-name cleanup before budgeting optional drawing work.
            controls.keys_admitted.clear();
            controls.keys_pressed.clear();
            controls.keys_held.clear();
            let _ = gameplay::set_packet_delay(modules.packet_delay_ms());
            let sprint = modules.modules.omni_sprint
                && controls.focused
                && controls.gameplay
                && !controls.panel_open;
            if sprint && let Ok(Some(mut input)) = gameplay::read_movement_input() {
                input.sprint = true;
                let _ = gameplay::set_movement_input(Some(input));
            }

            let snapshot = if controls.focused && controls.gameplay {
                gameplay::read_frame().ok().flatten()
            } else {
                None
            };
            let _ = gameplay::set_attack_reach(modules.attack_reach(snapshot.is_some()));
            // An unobserved hold must not clear Clicker's refusal latch while disabled or
            // while changed bindings are still paging in.
            let cadence = snapshot
                .as_ref()
                .filter(|_| attack_needed && bindings_ready)
                .map(|frame| {
                    (
                        frame.session,
                        frame.dimension,
                        attack_held,
                        frame.frame_seconds,
                    )
                });
            if modules.attack_pulse(cadence) && gameplay::pulse_attack().is_err() {
                modules.attack_refused();
            }

            let movement = if modules.modules.jump_reset
                && bindings_ready
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

            auto_totem(
                modules.modules.auto_totem
                    && controls.focused
                    && controls.gameplay
                    && !controls.panel_open,
                controls.seconds,
            );
            assist_aim(config, snapshot.as_ref(), attack_held);
            let drawn = snapshot.as_ref().filter(|_| !controls.panel_open);
            // Disabled drawings receive their teardown turn before any optional producer.
            if modules.finder_range().is_none() {
                netherite(None, drawn, false, &fuel);
            }
            if !modules.show_real_position() {
                real_position(false, None, false, &fuel);
            }
            if !modules.modules.player_esp || drawn.is_none() {
                clear_esp(&fuel);
            }
            // With the panel owning input, drawings are ineligible and settings can save
            // immediately. During gameplay they compete on the same schedule as drawings.
            if controls.panel_open && fuel.publishes(35_000) {
                publish_preferences(&mut modules, &fuel);
            }
            // Every optional producer gets a first turn, independent of other dirty queues.
            // Bindings and gameplay policies above have bounded mandatory work every callback.
            let turn = WORK_TURN.with(|next| {
                let turn = next.get();
                next.set((turn + 1) % 8);
                [1, 0, 1, 2, 1, 0, 3, 4][turn as usize]
            });
            for offset in 0..5 {
                match (turn + offset) % 5 {
                    0 if modules.finder_range().is_some() => {
                        prepare_netherite(modules.finder_range(), drawn);
                        netherite(modules.finder_range(), drawn, false, &fuel);
                    }
                    1 => {
                        player_esp(&modules, drawn, &fuel);
                    }
                    2 if modules.show_real_position() => real_position(true, drawn, false, &fuel),
                    3 => publish_panel(&mut modules, &fuel, controls.panel_open),
                    4 => publish_preferences(&mut modules, &fuel),
                    _ => {}
                }
            }
        });
    }
}

/// Reserves fuel for canonical-ABI return cleanup and dropping maximum observations.
struct FrameFuel(u64);
impl FrameFuel {
    /// Leaves a publication and callback-return reserve after ordinary optional work.
    fn allows(&self, cost: u64) -> bool {
        self.allows_until(cost, 79_000)
    }
    /// Gives bounded publication, saving and observation retirement their reserved turn.
    fn publishes(&self, cost: u64) -> bool {
        self.allows_until(cost, 81_000)
    }
    /// Rechecks live fuel after every host import and bounded unit.
    fn allows_until(&self, cost: u64, limit: u64) -> bool {
        self.0
            .saturating_sub(budget::remaining_fuel())
            .saturating_add(cost)
            <= limit
    }
}

/// Queries compact item matches and requests a swap only after a change or retry delay.
fn auto_totem(enabled: bool, seconds: f32) {
    AUTO_TOTEM.with(|policy| {
        let mut policy = policy.borrow_mut();
        if !enabled {
            policy.reset();
            return;
        }
        let equipment = match player_state::read_equipment_matches(crate::auto_totem::TOTEM) {
            Ok(Some(equipment)) => equipment,
            Ok(None) => {
                policy.reset();
                return;
            }
            Err(error) => {
                policy.reset();
                let _ = hud::set_label(&format!("AutoTotem: {error}"));
                return;
            }
        };
        if policy.observe(equipment.revisions, seconds) {
            if let Some(slot) = policy.select(&equipment)
                && let Err(error) = inventory::swap_offhand(slot)
            {
                let _ = hud::set_label(&format!("AutoTotem: {error}"));
            }
        }
    });
}

/// Combat assistance runs before optional drawing work and uses the current complete sample.
fn assist_aim(
    config: crate::aim::Config,
    snapshot: Option<&gameplay::Snapshot>,
    attack_held: bool,
) {
    let Some(snapshot) = snapshot else {
        AIM.with(|aim| aim.borrow_mut().reset());
        return;
    };
    if !config.enabled || config.strength == 0 || (config.only_when_clicking && !attack_held) {
        AIM.with(|aim| aim.borrow_mut().reset());
        return;
    }
    let players = snapshot
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
        });
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
        players: &[],
    };
    AIM.with(|aim| {
        let mut aim = aim.borrow_mut();
        if let Some(rotation) = aim.update_players(config, Some(&frame), players)
            && gameplay::rotate(rotation.yaw, rotation.pitch).is_err()
        {
            aim.reset();
        }
    });
}

/// Publishes bounded settings and authority changes in their own scheduled turn.
fn publish_preferences(modules: &mut State, fuel: &FrameFuel) {
    if modules.fullbright_dirty && !modules.access.0[6].is_empty() {
        modules.fullbright_dirty = false;
    }
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
    SETTINGS_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        for _ in 0..2 {
            // Small preference units may use the publication reserve. A full event and
            // binding batch can leave too little for the ordinary drawing ceiling.
            if !fuel.allows_until(8_000, 85_000) {
                break;
            }
            if let Some(json) = &*pending {
                if settings::save(json).is_err() {
                    let _ = hud::set_label("Cinnaroids: preferences could not be saved");
                    break;
                }
                *pending = None;
            } else if modules.preferences_dirty {
                let Ok(json) = serde_json::to_string(&modules.preferences) else {
                    break;
                };
                modules.preferences_dirty = false;
                *pending = Some(json);
            } else {
                break;
            }
        }
    });
}

/// Completes a coherent panel snapshot even if controls change again during serialization.
fn publish_panel(modules: &mut State, fuel: &FrameFuel, open: bool) {
    PANEL_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        // A changed menu shortcut replaces the input shell before older visual work.
        if open
            && modules.panel_dirty
            && PANEL_KEY.with(|key| key.borrow().as_ref() != Some(&modules.preferences.menu_key))
        {
            *pending = PanelUpdate::Idle;
            PANEL_BINDINGS_DIRTY.with(|dirty| dirty.set(false));
        }
        // An open panel's input state must not wait behind an older visual snapshot.
        if open
            && modules.panel_dirty
            && matches!(
                *pending,
                PanelUpdate::BuildRoute { .. } | PanelUpdate::Route(_)
            )
            && PANEL_KEY.with(|key| key.borrow().as_ref() == Some(&modules.preferences.menu_key))
            && fuel.publishes(24_000)
        {
            let json = crate::panel::control_patch(modules);
            if panel::update_content(&json).is_ok() {
                modules.panel_dirty = false;
                PANEL_BINDINGS_DIRTY.with(|dirty| dirty.set(true));
            }
        }
        for _ in 0..48 {
            let cost = match &*pending {
                PanelUpdate::BuildRoute { builder, .. } if builder.finishing() => 12_000,
                PanelUpdate::BuildRoute { .. } => 3_000,
                _ => 12_000,
            };
            if !fuel.publishes(cost) {
                break;
            }
            if let PanelUpdate::BuildRoute { builder, .. } = &mut *pending {
                if builder.advance() {
                    let PanelUpdate::BuildRoute { builder, json } = std::mem::take(&mut *pending)
                    else {
                        unreachable!()
                    };
                    *pending =
                        PanelUpdate::Route(crate::panel::with_bindings(json, &builder.finish()));
                }
                continue;
            }
            *pending = match &*pending {
                PanelUpdate::Idle if PANEL_BINDINGS_DIRTY.with(|dirty| dirty.replace(false)) => {
                    PanelUpdate::BuildRoute {
                        builder: Box::new(crate::panel::Bindings::new(modules)),
                        json: "{}".into(),
                    }
                }
                PanelUpdate::Idle if modules.panel_dirty => {
                    let key = modules.preferences.menu_key.clone();
                    let installed =
                        PANEL_KEY.with(|current| current.borrow().as_ref() == Some(&key));
                    let next = if installed {
                        PanelUpdate::Controls(crate::panel::control_patch(modules))
                    } else {
                        // The host requires a full panel to change its menu shortcut.
                        let Ok(json) = modules.panel_shell_json() else {
                            break;
                        };
                        PanelUpdate::Shell { json, key }
                    };
                    modules.panel_dirty = false;
                    next
                }
                PanelUpdate::Shell { json, key, .. } => {
                    if panel::set_content(json).is_err() {
                        break;
                    }
                    PANEL_KEY.with(|current| *current.borrow_mut() = Some(key.clone()));
                    PanelUpdate::BuildRoute {
                        builder: Box::new(crate::panel::Bindings::new(modules)),
                        json: r#"{"surface":"clickgui"}"#.into(),
                    }
                }
                PanelUpdate::Controls(json) => {
                    if panel::update_content(json).is_err() {
                        break;
                    }
                    PanelUpdate::BuildRoute {
                        builder: Box::new(crate::panel::Bindings::new(modules)),
                        json: "{}".into(),
                    }
                }
                PanelUpdate::BuildRoute { .. } => unreachable!("advanced above"),
                PanelUpdate::Route(json) => {
                    if panel::update_content(json).is_err() {
                        break;
                    }
                    PanelUpdate::Idle
                }
                PanelUpdate::Idle => break,
            };
        }
    });
}

mod_api::bindings::export!(Cinnaroids with_types_in mod_api::bindings);

/// Outlines and labels nearby players over focused gameplay, `frame`, while Player ESP is on
/// and its drawing authority is granted; otherwise clears both. Returns whether a label took a
/// new name, the costliest change.
fn player_esp(modules: &State, frame: Option<&gameplay::Snapshot>, fuel: &FrameFuel) -> bool {
    if modules.modules.player_esp
        && drawing::capabilities().contains(
            Permissions::ENTITY_OBSERVE
                | Permissions::ENTITY_HIGHLIGHT
                | Permissions::RENDER_THROUGH_WALLS,
        )
        && let Some(frame) = frame
        && let Ok(Some(world)) = player_state::read_world()
        && world.dimension == frame.dimension
    {
        let context_changed = ESP_MASK.with(|mask| {
            mask.borrow()
                .as_ref()
                .is_some_and(|(session, dimension, _)| {
                    (*session, *dimension) != (world.session, world.dimension)
                })
        });
        if context_changed {
            clear_esp(fuel);
        }
        // Context changes and every failure share the normal incremental retirement path.
        let obsolete = ESP_PENDING.with(|pending| {
            pending
                .borrow()
                .as_ref()
                .is_some_and(|(session, dimension, _, _)| {
                    (*session, *dimension) != (world.session, world.dimension)
                })
        });
        if obsolete {
            clear_esp(fuel);
        }
        retire_esp(fuel);
        if ESP_CLEARING.with(|clearing| clearing.get()) {
            return false;
        }
        if ESP_LABELS.with(|labels| labels.borrow().is_none()) {
            // Initial surface publication is independent of actor import and label writes.
            if fuel.publishes(16_000) && hud::set_surface(crate::esp::SURFACE).is_ok() {
                ESP_LABELS.with(|labels| *labels.borrow_mut() = Some(Default::default()));
            }
            return false;
        }
        let pending = ESP_PENDING.with(|pending| pending.borrow_mut().take());
        let (actors, turns) = match pending {
            Some((session, dimension, actors, turns))
                if (session, dimension) == (world.session, world.dimension) =>
            {
                if turns == 0 {
                    ESP_PENDING.with(|pending| {
                        *pending.borrow_mut() = Some((session, dimension, actors, 0));
                    });
                    return false;
                }
                (actors, turns)
            }
            Some(_) => unreachable!("obsolete samples retire before import"),
            None => {
                if !fuel.allows(8_000) {
                    return true;
                }
                let Ok(actors) = player_state::read_actors_limited(crate::esp::MAX_PLAYERS as u32)
                else {
                    clear_esp(fuel);
                    return false;
                };

                if fuel.allows(45_000) {
                    (actors, 1)
                } else {
                    // Import and destruction of the maximum sample have their own turn.
                    ESP_PENDING.with(|pending| {
                        *pending.borrow_mut() = Some((
                            world.session,
                            world.dimension,
                            actors,
                            crate::esp::MAX_PLAYERS,
                        ));
                    });
                    return true;
                }
            }
        };
        if !fuel.allows(8_000) {
            ESP_PENDING.with(|pending| {
                *pending.borrow_mut() = Some((world.session, world.dimension, actors, turns))
            });
            return true;
        }
        // Under load admit one actor before label publication, preserving its write turn.
        let actor_limit = if fuel.allows(45_000) {
            crate::esp::MAX_PLAYERS
        } else {
            1
        };
        let mut checked = 0;
        let (shown, named) = ESP_LABELS.with(|labels| {
            let mut labels = labels.borrow_mut();
            let labels = labels.get_or_insert_with(Default::default);
            labels.stage_writes(!fuel.allows(45_000));
            let output = labels.update_budgeted(
                &modules.preferences,
                world.session,
                world.dimension,
                [frame.eye.x, frame.eye.y, frame.eye.z],
                &actors,
                |work| {
                    if let crate::esp::Work::Label(cost) = work {
                        fuel.publishes(cost)
                    } else {
                        if checked == actor_limit || !fuel.allows(crate::esp::ACTOR_FUEL) {
                            return false;
                        }
                        checked += 1;
                        true
                    }
                },
            );
            let shown = show_esp(
                world.session,
                world.dimension,
                modules.preferences.esp_color,
                &output.players,
            ) && output
                .labels
                .is_none_or(|update| hud::update_surface(&update).is_ok());
            (shown, output.named)
        });
        let turns = turns.saturating_sub(usize::from(checked != 0));
        // Maximum observation destruction has its own turn, after the last publication.
        ESP_PENDING.with(|pending| {
            *pending.borrow_mut() = Some((world.session, world.dimension, actors, turns));
        });
        if shown {
            return named;
        }
    }
    clear_esp(fuel);
    false
}

/// Removes visible output immediately; backing allocations retire only in admitted units.
fn clear_esp(fuel: &FrameFuel) {
    ESP_CLEARING.with(|clearing| {
        if !clearing.replace(true) {
            ESP_MASK.with(|cached| {
                if let Some((_, _, mask)) = cached.borrow_mut().take() {
                    let _ = masks::remove(mask);
                }
            });
            if ESP_LABELS.with(|labels| labels.borrow().is_some()) {
                let _ = hud::set_surface("");
            }
        }
    });
    ESP_PENDING.with(|pending| {
        if let Some((_, _, _, turns)) = &mut *pending.borrow_mut() {
            *turns = 0;
        }
    });
    retire_esp(fuel);
}

/// Frees one bounded actor at a time, including after read, mask and HUD failures.
fn retire_esp(fuel: &FrameFuel) {
    ESP_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if let Some((_, _, actors, 0)) = &mut *pending {
            while !actors.is_empty() && fuel.publishes(4_000) {
                actors.pop();
            }
            if actors.is_empty() {
                *pending = None;
            }
        }
    });
    if ESP_CLEARING.with(|clearing| clearing.get()) && fuel.publishes(6_000) {
        ESP_LABELS.with(|labels| *labels.borrow_mut() = None);
        if ESP_PENDING.with(|pending| pending.borrow().is_none()) {
            ESP_CLEARING.with(|clearing| clearing.set(false));
        }
    }
}

/// Whether a drawing error means a retained geometry token no longer exists.
fn stale(error: &DrawError) -> bool {
    error.code == ErrorCode::StaleHandle
}

/// Renews the terrain subscription before optional work, so deferred drawing cannot
/// unsubscribe a search. Only due answers cross into the retained drawing work queue.
fn prepare_netherite(range: Option<u8>, frame: Option<&gameplay::Snapshot>) {
    let (Some(range), Some(frame)) = (range, frame) else {
        return;
    };
    let capabilities = drawing::capabilities();
    if !capabilities.contains(Permissions::WORLD_DRAW) {
        return;
    }
    FINDER_ANSWER.with(|answer| {
        let mut answer = answer.borrow_mut();
        if answer.as_ref().is_some_and(|(session, dimension, _, _)| {
            (*session, *dimension) != (frame.session, frame.dimension)
        }) {
            *answer = None;
        }
    });
    FINDER.with(|finder| {
        let mut finder = finder.borrow_mut();
        let through_walls = capabilities.contains(Permissions::RENDER_THROUGH_WALLS);
        let found = FINDER_BLOCKS
            .with(|blocks| terrain::find_blocks(blocks, f32::from(range), finder::LIMIT as u32));
        let due = finder.due(
            frame.session,
            frame.dimension,
            range,
            through_walls,
            frame.frame_seconds,
        );
        match found {
            Ok(positions) if due => FINDER_ANSWER.with(|answer| {
                *answer.borrow_mut() =
                    Some((frame.session, frame.dimension, through_walls, positions));
            }),
            Err(terrain::ReadError::Denied) => {
                FINDER_ANSWER.with(|answer| *answer.borrow_mut() = None);
                finder.clear();
                if finder.notify_denied() {
                    let _ = hud::set_label("Netherite Finder: terrain permission denied");
                }
            }
            _ => {}
        }
    });
}

/// Renews the finder's block search every frame and, on its cadence, takes a changed answer as
/// the selection; then moves the retained cubes a bounded step toward it. `range` is set while
/// the module is enabled. Outside focused gameplay the cubes stay as they are. A `busy` frame
/// only renews the search.
fn netherite(range: Option<u8>, frame: Option<&gameplay::Snapshot>, busy: bool, fuel: &FrameFuel) {
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
                FINDER_ANSWER.with(|answer| *answer.borrow_mut() = None);
                finder.clear_notice();
                finder.clear();
            }
            (Some(_), None) => {
                FINDER_ANSWER.with(|answer| *answer.borrow_mut() = None);
                finder.pause();
            }
            (Some(_), Some(_)) => {
                if busy {
                    return;
                }
                if finder.synced() && FINDER_ANSWER.with(|answer| answer.borrow().is_some()) {
                    if !fuel.allows(5_000) {
                        return;
                    }
                    if let Some((_, dimension, through_walls, positions)) =
                        FINDER_ANSWER.with(|answer| answer.borrow_mut().take())
                    {
                        finder.apply(dimension, through_walls, &positions);
                    }
                }
            }
        }
        if busy && range.is_some() {
            return;
        }
        if finder.comparing() {
            for _ in 0..finder::COMPARE_BLOCKS {
                // Includes the bounded 256-entry final partition on the last position.
                if !finder.comparing() || !fuel.allows(7_000) {
                    break;
                }
                finder.compare(1);
            }
            return;
        }
        if finder.synced() {
            return;
        }
        if !fuel.allows(5_000) {
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
        for _ in 0..finder::SYNC_CALLS {
            if !fuel.allows(3_000) {
                break;
            }
            let synced = finder.sync(1, |change| match change {
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
                break;
            }
        }
    });
}

/// Draws FakeLag's last relayed position while the option is on and packets are delayed. A
/// `busy` frame does not place a hidden marker.
fn real_position(show: bool, frame: Option<&gameplay::Snapshot>, busy: bool, fuel: &FrameFuel) {
    MARKER.with(|marker| {
        let mut marker = marker.borrow_mut();
        retire_marker(&mut marker, fuel);
        if !marker.retiring.is_empty() {
            return;
        }
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
        if target.is_some() && marker.geometries.is_none() {
            for spec in ghost::geometries().iter().skip(marker.uploading.len()) {
                if !fuel.publishes(5_000) {
                    return;
                }
                match drawing::create_geometry(spec) {
                    Ok(handle) => marker.uploading.push(handle),
                    Err(error) => {
                        if stale(&error) {
                            marker.reset();
                        }
                        return;
                    }
                }
            }
            marker.geometries = Some(
                std::mem::take(&mut marker.uploading)
                    .try_into()
                    .expect("five geometries"),
            );
        }
        if !fuel.publishes(7_000) {
            return;
        }
        if let Err(error) = place_marker(&mut marker, target, busy, fuel)
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
    fuel: &FrameFuel,
) -> Result<(), DrawError> {
    let Some((dimension, feet)) = target else {
        if let Some(placed) = marker.placed.take() {
            marker.retiring.extend(placed.handles);
            retire_marker(marker, fuel);
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
        // Geometry uploads already have individually admitted turns in real_position.
        None => return Ok(()),
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
                marker.retiring.extend(created);
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

/// Uses the same bounded removal path for hiding the marker and rolling back a failed batch.
fn retire_marker(marker: &mut Marker, fuel: &FrameFuel) {
    while let Some(&handle) = marker.retiring.last() {
        if !fuel.allows(3_000) {
            break;
        }
        if let Err(error) = drawing::remove(handle)
            && !stale(&error)
        {
            break;
        }
        marker.retiring.pop();
    }
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
        let mut entities = Vec::with_capacity(crate::esp::MAX_PLAYERS);
        entities.extend(players.iter().filter_map(|id| masks::entity(*id).ok()));
        !entities.is_empty()
            && masks::select(mask, &entities).is_ok()
            && masks::set_style(mask, Some(crate::esp::appearance(color))).is_ok()
    })
}
