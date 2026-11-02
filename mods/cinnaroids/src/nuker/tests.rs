use super::*;
use mod_api::bindings::cinnabar::extension::terrain::{BlockState, Region, ShapeBinding};

/// A manual break in a connected world.
fn frame() -> Frame {
    Frame {
        session: 1,
        terrain_session: 2,
        dimension: 0,
        eye: [0.5, 2.5, 0.5],
        block: Some([0, 2, -1]),
        seconds: 0.016,
    }
}

/// A one-cell terrain answer with no shape data.
fn answer(identifier: Option<&str>, loaded: bool) -> ReadResult {
    ReadResult::Changed(Region {
        stamp: Stamp {
            session: 2,
            dimension: 0,
            registry: 1,
            revision: 1,
        },
        chunks: Vec::new(),
        layer_count: 1,
        loaded: vec![u64::from(loaded)],
        states: vec![0],
        palette: vec![BlockState {
            runtime_id: 0,
            identifier: identifier.map(str::to_owned),
            canonical_state: None,
            tags: Vec::new(),
            physics: None,
            collision: ShapeBinding::NotRequested,
            selection: ShapeBinding::NotRequested,
        }],
        shape_offsets: Vec::new(),
        shape_boxes: Vec::new(),
        overrides: Vec::new(),
    })
}

#[test]
fn original_break_emits_every_neighbor_in_one_batch_without_mining_them_first() {
    for radius in 1..=MAX_RANGE {
        let mut policy = Nuker::default();
        let f = frame();
        let (query, _) = policy.query(f, radius).unwrap();
        assert_eq!(
            policy.update(f, &query, answer(Some("minecraft:stone"), true)),
            Action::default()
        );
        let action = policy.update(f, &query, answer(Some("minecraft:air"), true));
        let center = f.block.unwrap();
        let mut expected = Vec::new();
        for x in -5..=5_i32 {
            for y in -5..=5_i32 {
                for z in -5..=5_i32 {
                    let d = x * x + y * y + z * z;
                    if d > 0 && d <= i32::from(radius).pow(2) {
                        expected.push([center[0] + x, center[1] + y, center[2] + z]);
                    }
                }
            }
        }
        assert_eq!(action.blocks, expected);
        assert!(action.blocks.len() <= mod_api::MAX_BLOCK_BREAKS);
        assert!(policy.query(f, radius).is_none());
        assert_eq!(
            policy.update(f, &query, answer(Some("minecraft:air"), true)),
            Action::default()
        );
    }
}

#[test]
fn anchor_stays_on_original_block_when_crosshair_moves_or_original_is_already_air() {
    let mut policy = Nuker::default();
    let mut f = frame();
    let (query, _) = policy.query(f, 1).unwrap();
    f.block = Some([8, 9, 10]);
    let (next, _) = policy.query(f, 5).unwrap();
    assert_eq!([next.min.x, next.min.y, next.min.z], [0, 2, -1]);
    let action = policy.update(f, &query, answer(Some("minecraft:air"), true));
    assert_eq!(action.blocks.len(), 6);
    policy.reset();
    assert_eq!(policy.query(f, 1).unwrap().0.min.x, 8);
}

#[test]
fn no_manual_block_pending_unknown_unloaded_and_stale_worlds_never_trigger() {
    let mut policy = Nuker::default();
    let mut f = frame();
    f.block = None;
    assert!(policy.query(f, 3).is_none());
    f = frame();
    for response in [
        ReadResult::Pending,
        answer(None, true),
        answer(Some("minecraft:air"), false),
        answer(Some("minecraft:stone"), true),
    ] {
        let (query, _) = policy.query(f, 3).unwrap();
        assert_eq!(policy.update(f, &query, response), Action::default());
    }
    f.terrain_session += 1;
    let (query, _) = policy.query(f, 3).unwrap();
    assert_eq!(
        policy.update(f, &query, answer(Some("minecraft:air"), true)),
        Action::default()
    );
    assert!(policy.context.is_none());
}

#[test]
fn world_changes_release_completed_hold_and_invalid_coordinates_do_not_overflow() {
    let mut policy = Nuker::default();
    let mut f = frame();
    let (query, _) = policy.query(f, 1).unwrap();
    policy.update(f, &query, answer(Some("minecraft:air"), true));
    f.session += 1;
    assert!(policy.query(f, 1).is_some());
    policy.reset();
    f.block = Some([i32::MAX, 0, 0]);
    assert!(policy.query(f, 5).is_none());
}
