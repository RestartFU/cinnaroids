use super::*;

/// A physical mining frame with a block selected north of the eye.
fn frame() -> Frame {
    Frame {
        session: 1,
        terrain_session: 1,
        dimension: 0,
        eye: [0.5, 2.5, 0.5],
        block: Some([0, 2, -1]),
        seconds: 0.016,
    }
}

/// A loaded small tile containing full selection cubes and known air.
fn region(query: &RegionQuery, blocks: &[[i32; 3]]) -> Region {
    use mod_api::bindings::cinnabar::extension::terrain::BlockState;
    let mut states = vec![0; CELLS];
    for block in blocks {
        let cell = ((block[0] - query.min.x) * TILE + block[2] - query.min.z) * TILE + block[1]
            - query.min.y;
        states[cell as usize] = 1;
    }
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
fn manual_target_seeds_radius_and_removal_moves_to_a_neighbor_without_turning() {
    let mut policy = Nuker::default();
    let f = frame();
    let (query, _) = policy.query(f, 3).unwrap();
    let mut answer = region(&query, &[[0, 2, -1], [1, 2, -1]]);
    assert_eq!(
        policy
            .update(f, &query, ReadResult::Changed(answer.clone()))
            .point,
        Some([0.5, 2.5, -0.5])
    );
    answer = region(&query, &[[1, 2, -1]]);
    answer.stamp.revision += 1;
    assert_eq!(
        policy.update(f, &query, ReadResult::Changed(answer)).point,
        Some([1.5, 2.5, -0.5])
    );
}

#[test]
fn a_hold_requires_a_manual_block_and_reset_releases_its_anchor() {
    let mut policy = Nuker::default();
    let mut f = frame();
    f.block = None;
    assert!(policy.query(f, 3).is_none());
    f.block = Some([0, 2, -1]);
    policy.query(f, 3).unwrap();
    f.block = None;
    assert!(
        policy.query(f, 3).is_some(),
        "keep mining neighbors after the original disappears"
    );
    policy.reset();
    assert!(policy.query(f, 3).is_none());
}

#[test]
fn pending_unknown_unloaded_and_empty_shapes_do_not_request_mining() {
    for kind in 0..6 {
        let mut policy = Nuker::default();
        let f = frame();
        let (query, _) = policy.query(f, 3).unwrap();
        let mut answer = region(&query, &[[0, 2, -1]]);
        match kind {
            0 => answer.loaded.fill(0),
            1 => answer.palette[1].selection = ShapeBinding::Fixed(ResolvedShape::Unavailable),
            2 => answer.palette[1].selection = ShapeBinding::Fixed(ResolvedShape::Known(0)),
            3 => answer.states.fill(u16::MAX),
            4 => answer.palette[1].identifier = None,
            _ => {
                assert_eq!(
                    policy.update(f, &query, ReadResult::Pending),
                    Action::default()
                );
                continue;
            }
        }
        assert_eq!(
            policy.update(f, &query, ReadResult::Changed(answer)),
            Action::default()
        );
    }
}

#[test]
fn radius_is_centered_on_the_manually_selected_block_and_stays_fixed_during_a_hold() {
    let mut policy = Nuker::default();
    let mut f = frame();
    let (query, _) = policy.query(f, 1).unwrap();
    let answer = region(&query, &[[1, 3, 0]]);
    assert_eq!(
        policy.update(f, &query, ReadResult::Changed(answer)),
        Action::default()
    );
    f.eye[0] += 2.;
    f.block = Some([4, 2, -1]);
    policy.query(f, 1).unwrap();
    assert_eq!(policy.context.unwrap().center, [0, 2, -1]);
}

#[test]
fn context_changes_and_invalid_input_discard_old_targets() {
    for kind in 0..5 {
        let mut policy = Nuker::default();
        let mut f = frame();
        let (query, _) = policy.query(f, 3).unwrap();
        policy.update(
            f,
            &query,
            ReadResult::Changed(region(&query, &[[0, 2, -1]])),
        );
        let mut range = 3;
        match kind {
            0 => f.session += 1,
            1 => f.dimension += 1,
            2 => f.terrain_session += 1,
            3 => range = 1,
            _ => f.eye[0] = f32::NAN,
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
    let answer = region(&query, &[[0, 2, -1]]);
    let stamp = answer.stamp;
    policy.update(f, &query, ReadResult::Changed(answer));
    for _ in 0..(MAX_HOLD_SECONDS as usize - 1) {
        policy.update(f, &query, ReadResult::Unchanged(stamp));
    }
    assert!(policy.target.is_none());
    let (next, stamp) = policy.query(f, MAX_RANGE).unwrap();
    assert!(stamp.is_none());
    assert_ne!(
        (next.min.x, next.min.y, next.min.z),
        (query.min.x, query.min.y, query.min.z)
    );
}

#[test]
fn position_dependent_shapes_supply_the_mining_point() {
    use mod_api::bindings::cinnabar::extension::terrain::ShapeOverride;
    let mut policy = Nuker::default();
    let f = frame();
    let (query, _) = policy.query(f, 3).unwrap();
    let mut answer = region(&query, &[[0, 2, -1]]);
    answer.palette[1].selection = ShapeBinding::PerCell;
    let slot = ((0 - query.min.x) * TILE + (-1 - query.min.z)) * TILE + (2 - query.min.y);
    answer.overrides.push(ShapeOverride {
        slot: slot as u32,
        collision: None,
        selection: Some(ResolvedShape::Known(1)),
    });
    answer.shape_boxes = vec![0.1, 0., 0., 0.2, 1., 1.];
    let action = policy.update(f, &query, ReadResult::Changed(answer));
    assert!((action.point.unwrap()[0] - 0.15).abs() < 1e-6);
}
