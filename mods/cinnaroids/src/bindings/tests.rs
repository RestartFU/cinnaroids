use super::*;

const BOTH: Holds = Holds {
    attack: true,
    jump: true,
};

/// A complete small settings page.
fn full(revision: u64, entries: Vec<ActionBinding>) -> Bindings {
    Bindings {
        revision,
        total: entries.len() as u32,
        entries,
    }
}

use mod_api::bindings::cinnabar::extension::input::ActionBinding;

/// Packed names preserve suffixes and lengths through the full input byte limit.
#[test]
fn key_names_are_lossless_at_the_input_boundaries() {
    let longest = "K".repeat(mod_api::MAX_CONTROL_KEY_BYTES);
    let mut different = longest.clone();
    different.pop();
    different.push('L');
    assert_ne!(KeyName::new(&longest), KeyName::new(&different));
    assert_ne!(KeyName::new("KeyK"), KeyName::new("KeyK\0"));
    assert_ne!(KeyName::new("KeyK"), KeyName::new("KeyL"));
    assert!(KeyName::new(&longest).is_some());
    assert!(KeyName::new("").is_none());
    assert!(KeyName::new(&(longest + "K")).is_none());
}

/// Builds an actual player-assigned chord, without assuming a default attack key.
fn chord(action: &str, key: &str) -> ActionBinding {
    ActionBinding {
        action: action.into(),
        context: BindingContext::Gameplay,
        key: key.into(),
        modifiers: BindingModifiers {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        },
    }
}

/// Focused observations admit every supplied physical key by default.
fn controls(keys: &[&str]) -> Controls {
    Controls {
        seconds: 0.05,
        focused: true,
        gameplay: true,
        panel_open: false,
        keys_admitted: keys.iter().map(|key| (*key).into()).collect(),
        keys_held: keys.iter().map(|key| (*key).into()).collect(),
        keys_pressed: Vec::new(),
        events: Vec::new(),
        pointer: None,
        wheel: 0.,
    }
}

/// Changed player assignments replace cached keys; idle refreshes copy no binding lists.
#[test]
fn rebound_attack_replaces_default_and_expired_snapshots_fail_closed() {
    let mut cache = Cache::default();
    cache.refresh(|after, _, _| {
        assert_eq!(after, None);
        Ok(Some(Bindings {
            revision: 1,
            total: 1,
            entries: vec![chord("attack", "MouseLeft")],
        }))
    });
    assert!(cache.held("attack", &controls(&["MouseLeft"])));
    cache.refresh(|after, _, _| {
        assert_eq!(after, Some(1));
        Ok(Some(Bindings {
            revision: 2,
            total: 1,
            entries: vec![chord("attack", "KeyK")],
        }))
    });
    assert!(!cache.held("attack", &controls(&["MouseLeft"])));
    assert!(cache.held("attack", &controls(&["KeyK"])));
    cache.refresh(|after, _, _| {
        assert_eq!(after, Some(2));
        Ok(None)
    });
    assert!(cache.held("attack", &controls(&["KeyK"])));
    cache.refresh(|_, _, _| Err("denied".into()));
    assert!(!cache.held("attack", &controls(&["KeyK"])));
}

/// Chords respect modifiers, router precedence, reservations, context and input interruptions.
#[test]
fn physical_chords_respect_gameplay_ownership() {
    let mut cache = Cache::default();
    let mut shifted = chord("use", "KeyK");
    shifted.modifiers.shift = true;
    cache.refresh(|_, _, _| {
        Ok(Some(full(
            1,
            vec![chord("attack", "KeyK"), shifted, chord("jump", "Space")],
        )))
    });
    assert!(cache.held("attack", &controls(&["KeyK"])));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftRight"])));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftLeft"])));
    let mut reserved = controls(&["KeyK"]);
    reserved.keys_admitted.clear();
    assert!(!cache.held("attack", &reserved));
    assert!(cache.held("jump", &controls(&["Space"])));
    for loss in 0..3 {
        let mut controls = controls(&["KeyK", "Space"]);
        match loss {
            0 => controls.focused = false,
            1 => controls.gameplay = false,
            _ => controls.panel_open = true,
        }
        assert!(!cache.held("attack", &controls));
        assert!(!cache.held("jump", &controls));
    }
}

/// Duplicate bindings collapse to one key while stronger chords from other actions suppress it.
#[test]
fn maximum_duplicate_chords_preserve_precedence_and_reserved_modifiers() {
    let mut entries: Vec<_> = (0..mod_api::MAX_INPUT_BINDINGS - 1)
        .map(|_| chord("attack", "KeyK"))
        .collect();
    let mut shifted = chord("use", "KeyK");
    shifted.modifiers.shift = true;
    entries.push(shifted);
    let mut cache = Cache::default();
    for _ in 0..(mod_api::MAX_INPUT_BINDINGS as u32).div_ceil(BINDINGS_PER_FRAME) {
        cache.refresh(|after, offset, limit| {
            assert_eq!(after, None);
            Ok(Some(Bindings {
                revision: 1,
                total: entries.len() as u32,
                entries: entries[offset as usize..(offset + limit) as usize].to_vec(),
            }))
        });
    }
    assert_eq!(cache.keys.len(), 1);
    assert!(cache.held("attack", &controls(&["KeyK"])));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftRight"])));
    let mut reserved_modifier = controls(&["KeyK", "ShiftRight"]);
    reserved_modifier
        .keys_admitted
        .retain(|key| key != "ShiftRight");
    assert!(cache.held("attack", &reserved_modifier));
}

/// A revision change during collection discards both the prior policy and the partial page set.
#[test]
fn pages_are_atomic_and_a_mid_refresh_rebind_restarts_at_zero() {
    let mut cache = Cache::default();
    cache.refresh(|_, offset, limit| {
        assert_eq!(offset, 0);
        Ok(Some(Bindings {
            revision: 1,
            total: limit + 1,
            entries: (0..limit).map(|_| chord("attack", "MouseLeft")).collect(),
        }))
    });
    assert!(!cache.ready());
    assert!(!cache.held("attack", &controls(&["MouseLeft"])));
    cache.refresh(|after, offset, _| {
        assert_eq!(after, None);
        assert_eq!(offset, BINDINGS_PER_FRAME);
        Ok(Some(Bindings {
            revision: 2,
            total: BINDINGS_PER_FRAME + 1,
            entries: vec![chord("attack", "KeyK")],
        }))
    });
    assert!(!cache.ready());
    cache.refresh(|after, offset, _| {
        assert_eq!(after, None);
        assert_eq!(offset, 0);
        Ok(Some(full(2, vec![chord("attack", "KeyK")])))
    });
    // A revision has one stable total, so inconsistent host metadata is refused.
    assert!(!cache.ready());
    cache.refresh(|_, offset, _| {
        assert_eq!(offset, 0);
        Ok(Some(full(3, vec![chord("attack", "KeyK")])))
    });
    assert!(cache.ready());
    assert!(!cache.held("attack", &controls(&["MouseLeft"])));
    assert!(cache.held("attack", &controls(&["KeyK"])));
}

/// Rebinding a held key cannot restart attack until native admission returns after release.
#[test]
fn held_key_rebind_waits_for_admission() {
    let mut cache = Cache::default();
    cache.refresh(|_, _, _| Ok(Some(full(1, vec![chord("attack", "MouseLeft")]))));
    let mut input = controls(&["MouseLeft", "KeyK"]);
    assert!(cache.resolve(&input, BOTH).attack);
    cache.refresh(|_, _, _| Ok(Some(full(2, vec![chord("attack", "KeyK")]))));
    input.keys_admitted.clear();
    for _ in 0..4 {
        assert!(!cache.resolve(&input, BOTH).attack);
    }
    input = controls(&["MouseLeft"]);
    assert!(!cache.resolve(&input, BOTH).attack);
    input = controls(&["MouseLeft", "KeyK"]);
    assert!(cache.resolve(&input, BOTH).attack);
}

/// Raw holds never bypass consumed keys, UI barriers, device selection or reservations.
#[test]
fn unadmitted_raw_holds_gate_attack_jump_and_click_only_aim() {
    use crate::aim::{AimAssist, Config, Frame, Player, Vec3};
    let mut cache = Cache::default();
    cache.refresh(|_, _, _| {
        Ok(Some(full(
            1,
            vec![chord("attack", "KeyK"), chord("jump", "Space")],
        )))
    });
    let players = [Player {
        id: 1,
        feet: Vec3 {
            x: -0.5,
            y: 0.,
            z: -3.,
        },
    }];
    let config = Config {
        enabled: true,
        strength: 100,
        only_when_clicking: true,
        horizontal_only: false,
    };
    let mut aim = AimAssist::default();
    for reason in [
        "chat macro",
        "UI barrier",
        "controller selected",
        "reserved key",
        "focus loss",
    ] {
        let mut input = controls(&["KeyK", "Space"]);
        let mut frame = Frame {
            session: 1,
            dimension: 0,
            eye: Vec3 {
                x: 0.,
                y: 1.6,
                z: 0.,
            },
            yaw: 0.,
            pitch: 0.,
            seconds: 0.05,
            players: &players,
            attack_held: true,
        };
        assert!(aim.update(config, Some(&frame)).is_some());
        input.keys_admitted.clear();
        if reason == "focus loss" {
            input.focused = false;
        }
        let holds = cache.resolve(&input, BOTH);
        assert_eq!(holds, Holds::default(), "{reason}");
        frame.attack_held = holds.attack;
        assert!(aim.update(config, Some(&frame)).is_none(), "{reason}");
    }
}

/// Every modifier group requires an admitted side; raw modifiers cannot strengthen a chord.
#[test]
fn chords_use_only_admitted_modifiers_and_keep_equal_specificity_alternatives() {
    for (group, sides) in [
        (0, ["ShiftLeft", "ShiftRight"]),
        (1, ["ControlLeft", "ControlRight"]),
        (2, ["AltLeft", "AltRight"]),
        (3, ["SuperLeft", "SuperRight"]),
    ] {
        let mut attack = chord("attack", "KeyK");
        attack.modifiers = BindingModifiers {
            shift: group == 0,
            control: group == 1,
            alt: group == 2,
            super_key: group == 3,
        };
        let mut jump = attack.clone();
        jump.action = "jump".into();
        let mut cache = Cache::default();
        cache.refresh(|_, _, _| Ok(Some(full(1, vec![chord("use", "KeyK"), attack, jump]))));
        for side in sides {
            let mut input = controls(&["KeyK", side]);
            assert_eq!(
                cache.resolve(&input, BOTH),
                Holds {
                    attack: true,
                    jump: true
                }
            );
            input.keys_admitted.retain(|key| key != side);
            assert_eq!(cache.resolve(&input, BOTH), Holds::default());
        }
    }
}

/// Startup collection suspends all dependent policies and discards an old knockback window.
#[test]
fn startup_paging_waits_for_complete_bindings_and_starts_a_fresh_hold() {
    use crate::{
        aim::{AimAssist, Config, Frame, Player, Vec3},
        jump_reset::{Frame as JumpFrame, JumpReset},
        state::{Preferences, State},
    };
    let mut cache = Cache::default();
    cache.refresh(|_, offset, limit| {
        assert_eq!(offset, 0);
        Ok(Some(Bindings {
            revision: 1,
            total: limit + 1,
            entries: (0..limit).map(|_| chord("attack", "KeyK")).collect(),
        }))
    });
    let input = controls(&["KeyK"]);
    assert!(!cache.ready());
    let mut state = State::new(Preferences {
        cps: 20,
        ..Default::default()
    });
    state.modules.clicker = true;
    let players = [Player {
        id: 1,
        feet: Vec3 {
            x: -0.5,
            y: 0.,
            z: -3.,
        },
    }];
    let mut frame = Frame {
        session: 1,
        dimension: 0,
        eye: Vec3 {
            x: 0.,
            y: 1.6,
            z: 0.,
        },
        yaw: 0.,
        pitch: 0.,
        attack_held: cache.resolve(&input, BOTH).attack,
        seconds: 0.05,
        players: &players,
    };
    let config = Config {
        enabled: true,
        strength: 100,
        only_when_clicking: true,
        horizontal_only: false,
    };
    let mut aim = AimAssist::default();
    assert!(!state.attack_pulse(Some((1, 0, frame.attack_held, 0.05))));
    assert!(aim.update(config, Some(&frame)).is_none());
    let jump_frame = |tick, knockback_sequence| JumpFrame {
        session: 1,
        dimension: 0,
        tick,
        knockback_sequence,
        on_ground: false,
        jump_held: false,
        eligible: true,
        velocity: [0.; 3],
    };
    let mut jump = JumpReset::default();
    assert!(!jump.update(true, 4, Some(jump_frame(1, 1))));
    assert!(!jump.update(true, 4, Some(jump_frame(2, 2))));
    assert!(!jump.update(true, 4, cache.ready().then(|| jump_frame(3, 2))));
    cache.refresh(|_, offset, _| {
        assert_eq!(offset, BINDINGS_PER_FRAME);
        Ok(Some(Bindings {
            revision: 1,
            total: BINDINGS_PER_FRAME + 1,
            entries: vec![chord("jump", "Space")],
        }))
    });
    assert!(cache.ready());
    frame.attack_held = cache.resolve(&input, BOTH).attack;
    assert!(frame.attack_held);
    assert!(!state.attack_pulse(Some((1, 0, frame.attack_held, 0.05))));
    assert!(state.attack_pulse(Some((1, 0, frame.attack_held, 0.05))));
    assert!(aim.update(config, Some(&frame)).is_some());
    assert!(!jump.update(
        true,
        4,
        Some(JumpFrame {
            on_ground: true,
            ..jump_frame(4, 2)
        })
    ));
}

/// Unrequested policies stay false, including a stronger chord belonging to an unused action.
#[test]
fn requested_policies_share_precedence_without_resolving_unused_holds() {
    let mut cache = Cache::default();
    let mut shifted = chord("jump", "KeyK");
    shifted.modifiers.shift = true;
    cache.refresh(|_, _, _| {
        Ok(Some(full(
            1,
            vec![chord("attack", "KeyK"), chord("jump", "Space"), shifted],
        )))
    });
    for (input, expected) in [
        (controls(&["KeyK", "Space"]), BOTH),
        (
            controls(&["KeyK", "ShiftLeft"]),
            Holds {
                attack: false,
                jump: true,
            },
        ),
    ] {
        for needed in [
            Holds::default(),
            Holds {
                attack: true,
                jump: false,
            },
            Holds {
                attack: false,
                jump: true,
            },
            BOTH,
        ] {
            assert_eq!(
                cache.resolve(&input, needed),
                Holds {
                    attack: needed.attack && expected.attack,
                    jump: needed.jump && expected.jump,
                }
            );
        }
    }
}

/// Filter collisions only add exact lookups; they cannot invent or suppress action holds.
#[test]
fn action_key_filters_preserve_exact_names_across_collisions_and_revisions() {
    let first = "Key000K";
    let other = (0..1000)
        .map(|i| format!("Key{i}K"))
        .find(|name| KeyFilter::bit(name) == KeyFilter::bit(first))
        .unwrap();
    let mut cache = Cache::default();
    cache.refresh(|_, _, _| Ok(Some(full(1, vec![chord("jump", first)]))));
    assert!(!cache.held("jump", &controls(&[&other])));
    assert!(cache.held("jump", &controls(&[first])));
    cache.refresh(|_, _, _| Ok(Some(full(2, vec![chord("attack", &other)]))));
    assert!(!cache.held("attack", &controls(&[first])));
    assert!(cache.held("attack", &controls(&[&other])));
    assert!(!cache.held("jump", &controls(&[first])));
}
