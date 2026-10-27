use super::*;

/// A complete small settings page.
fn full(revision: u64, entries: Vec<ActionBinding>) -> Bindings {
    Bindings {
        revision,
        total: entries.len() as u32,
        entries,
    }
}

use mod_api::bindings::cinnabar::extension::input::ActionBinding;

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

/// Focused raw observations include reservations, which the game cannot use.
fn controls(keys: &[&str]) -> Controls {
    Controls {
        seconds: 0.05,
        focused: true,
        gameplay: true,
        panel_open: false,
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
    assert!(cache.held("attack", &controls(&["MouseLeft"]), &[]));
    cache.refresh(|after, _, _| {
        assert_eq!(after, Some(1));
        Ok(Some(Bindings {
            revision: 2,
            total: 1,
            entries: vec![chord("attack", "KeyK")],
        }))
    });
    assert!(!cache.held("attack", &controls(&["MouseLeft"]), &[]));
    assert!(cache.held("attack", &controls(&["KeyK"]), &[]));
    cache.refresh(|after, _, _| {
        assert_eq!(after, Some(2));
        Ok(None)
    });
    assert!(cache.held("attack", &controls(&["KeyK"]), &[]));
    cache.refresh(|_, _, _| Err("denied".into()));
    assert!(!cache.held("attack", &controls(&["KeyK"]), &[]));
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
    assert!(cache.held("attack", &controls(&["KeyK"]), &[]));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftRight"]), &[]));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftLeft"]), &[]));
    assert!(!cache.held("attack", &controls(&["KeyK"]), &["KeyK".into()]));
    assert!(cache.held("jump", &controls(&["Space"]), &[]));
    for loss in 0..3 {
        let mut controls = controls(&["KeyK", "Space"]);
        match loss {
            0 => controls.focused = false,
            1 => controls.gameplay = false,
            _ => controls.panel_open = true,
        }
        assert!(!cache.held("attack", &controls, &[]));
        assert!(!cache.held("jump", &controls, &[]));
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
    assert!(cache.held("attack", &controls(&["KeyK"]), &[]));
    assert!(!cache.held("attack", &controls(&["KeyK", "ShiftRight"]), &[]));
    assert!(cache.held(
        "attack",
        &controls(&["KeyK", "ShiftRight"]),
        &["ShiftRight".into()]
    ));
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
    assert!(!cache.held("attack", &controls(&["MouseLeft"]), &[]));
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
    assert!(!cache.held("attack", &controls(&["MouseLeft"]), &[]));
    assert!(cache.held("attack", &controls(&["KeyK"]), &[]));
}
