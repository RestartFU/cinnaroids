use super::*;
use mod_api::bindings::cinnabar::extension::terrain::{BlockState, ShapeOverride};

/// A stationary view facing the next full block north of the eye.
fn frame() -> Frame {
    Frame {
        session: 1,
        terrain_session: 1,
        dimension: 0,
        eye: [0.5, 2.5, 0.5],
        yaw: 0.,
        pitch: 0.,
        seconds: 0.016,
    }
}

/// A loaded small tile with one full selection cube and otherwise known air.
fn region(query: &RegionQuery, block: [i32; 3]) -> Region {
    let mut states = vec![0; CELLS];
    let cell =
        ((block[0] - query.min.x) * TILE + block[2] - query.min.z) * TILE + block[1] - query.min.y;
    states[cell as usize] = 1;
    let state = |runtime_id, identifier: &str, shape| BlockState {
        runtime_id,
        identifier: Some(identifier.into()),
        canonical_state: None,
        tags: Vec::new(),
        physics: None,
        collision: ShapeBinding::NotRequested,
        selection: ShapeBinding::Fixed(ResolvedShape::Known(shape)),
    };
    Region {
        stamp: Stamp {
            session: 1,
            dimension: 0,
            registry: 1,
            revision: 1,
        },
        chunks: Vec::new(),
        layer_count: 1,
        loaded: vec![(1 << CELLS) - 1],
        states,
        palette: vec![state(0, "minecraft:air", 0), state(1, "minecraft:stone", 1)],
        shape_offsets: vec![0, 0, 1],
        shape_boxes: vec![0., 0., 0., 1., 1., 1.],
        overrides: Vec::new(),
    }
}

#[test]
fn pending_and_unaligned_frames_never_mine_and_removal_releases_immediately() {
    let mut policy = Nuker::default();
    let f = frame();
    let (query, stamp) = policy.query(f, 3).unwrap();
    assert!(stamp.is_none());
    assert_eq!(
        policy.update(f, &query, ReadResult::Pending),
        Action::default()
    );
    let mut answer = region(&query, [0, 2, -1]);
    let stamp = answer.stamp;
    assert!(
        !policy
            .update(f, &query, ReadResult::Changed(answer.clone()))
            .mine
    );
    assert_eq!(
        policy.query(f, 3).unwrap().1.map(stamp_key),
        Some(stamp_key(stamp))
    );
    assert!(policy.update(f, &query, ReadResult::Unchanged(stamp)).mine);
    answer.states.fill(0);
    answer.stamp.revision += 1;
    assert_eq!(
        policy.update(f, &query, ReadResult::Changed(answer)),
        Action::default()
    );
    assert!(policy.query(f, 3).unwrap().1.is_none());
}

#[test]
fn camera_turns_are_bounded_and_hold_waits_for_the_presented_view() {
    let mut policy = Nuker::default();
    let mut f = frame();
    let (query, _) = policy.query(f, 3).unwrap();
    let answer = region(&query, [1, 2, 0]);
    let stamp = answer.stamp;
    let first = policy.update(f, &query, ReadResult::Changed(answer));
    assert!(!first.mine);
    assert!(first.rotation.unwrap()[0] < 0.);
    let mut action = first;
    let mut mined = false;
    for _ in 0..100 {
        if let Some([yaw, pitch]) = action.rotation {
            assert!(yaw.hypot(pitch) <= mod_api::MAX_CAMERA_DELTA_RADIANS + 1e-6);
            f.yaw += yaw;
            f.pitch += pitch;
        }
        policy.query(f, 3).unwrap();
        action = policy.update(f, &query, ReadResult::Unchanged(stamp));
        if action.mine {
            mined = true;
            break;
        }
    }
    assert!(mined);
    assert!((f.yaw + PI / 2.).abs() < 0.01);
}

#[test]
fn unknown_unloaded_empty_shapes_and_invalid_palette_entries_are_skipped() {
    for kind in 0..5 {
        let mut policy = Nuker::default();
        let f = frame();
        let (query, _) = policy.query(f, 3).unwrap();
        let mut answer = region(&query, [0, 2, -1]);
        match kind {
            0 => answer.loaded.fill(0),
            1 => answer.palette[1].selection = ShapeBinding::Fixed(ResolvedShape::Unavailable),
            2 => answer.palette[1].selection = ShapeBinding::Fixed(ResolvedShape::Known(0)),
            3 => answer.states.fill(u16::MAX),
            _ => answer.palette[1].identifier = None,
        }
        assert_eq!(
            policy.update(f, &query, ReadResult::Changed(answer)),
            Action::default()
        );
        assert!(policy.target.is_none());
    }
}

#[test]
fn position_dependent_shapes_supply_the_actual_aim_point() {
    let mut policy = Nuker::default();
    let f = frame();
    let (query, _) = policy.query(f, 3).unwrap();
    let mut answer = region(&query, [0, 2, -1]);
    answer.palette[1].selection = ShapeBinding::PerCell;
    answer.overrides.push(ShapeOverride {
        slot: 10,
        collision: None,
        selection: Some(ResolvedShape::Known(1)),
    });
    answer.shape_boxes = vec![0.1, 0., 0., 0.2, 1., 1.];
    let action = policy.update(f, &query, ReadResult::Changed(answer));
    assert!((policy.target.unwrap().point[0] - 0.15).abs() < 1e-6);
    assert!(action.rotation.unwrap()[0] > 0.);
    assert!(!action.mine);
}

#[test]
fn changed_world_movement_range_and_invalid_input_discard_old_work() {
    for kind in 0..5 {
        let mut policy = Nuker::default();
        let mut f = frame();
        let (query, _) = policy.query(f, 3).unwrap();
        let answer = region(&query, [0, 2, -1]);
        policy.update(f, &query, ReadResult::Changed(answer));
        let mut range = 3;
        match kind {
            0 => f.session += 1,
            1 => f.dimension += 1,
            2 => f.eye[0] += 1.,
            3 => range = 1,
            _ => f.yaw = f32::NAN,
        }
        let next = policy.query(f, range);
        assert!(policy.target.is_none());
        assert!(next.is_none_or(|(_, stamp)| stamp.is_none()));
    }
}

#[test]
fn unchanged_blocks_time_out_and_scan_moves_to_another_small_tile() {
    let mut policy = Nuker::default();
    let mut f = frame();
    f.seconds = 1.;
    let (query, _) = policy.query(f, MAX_RANGE).unwrap();
    let answer = region(&query, [0, 2, -1]);
    let stamp = answer.stamp;
    policy.update(f, &query, ReadResult::Changed(answer));
    for _ in 0..MAX_HOLD_SECONDS as usize {
        policy.update(f, &query, ReadResult::Unchanged(stamp));
    }
    assert!(policy.target.is_none());
    let (next, stamp) = policy.query(f, MAX_RANGE).unwrap();
    assert!(stamp.is_none());
    assert_ne!(
        (next.min.x, next.min.y, next.min.z),
        (query.min.x, query.min.y, query.min.z)
    );
    assert_eq!(
        (
            next.max.x - next.min.x,
            next.max.y - next.min.y,
            next.max.z - next.min.z
        ),
        (TILE, TILE, TILE)
    );
}

#[test]
fn terrain_and_camera_epochs_are_distinct_and_unknown_answers_cannot_resume_a_hold() {
    let mut policy = Nuker::default();
    let mut f = frame();
    f.terrain_session = 2;
    let (query, _) = policy.query(f, 3).unwrap();
    let mut answer = region(&query, [0, 2, -1]);
    answer.stamp.session = 2;
    let stamp = answer.stamp;
    policy.update(f, &query, ReadResult::Changed(answer));
    assert!(policy.update(f, &query, ReadResult::Unchanged(stamp)).mine);
    assert_eq!(
        policy.update(f, &query, ReadResult::Pending),
        Action::default()
    );
    assert!(!policy.update(f, &query, ReadResult::Unchanged(stamp)).mine);
    f.terrain_session += 1;
    assert!(policy.query(f, 3).unwrap().1.is_none());
    assert_eq!(
        policy.update(f, &query, ReadResult::Unchanged(stamp)),
        Action::default()
    );
}
