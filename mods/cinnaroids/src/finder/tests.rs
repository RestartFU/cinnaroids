use super::*;
use mod_api::bindings::cinnabar::world_draw::drawing::{Anchor, Depth, DrawError};

fn position(x: i32, y: i32, z: i32) -> Position {
    Position { x, y, z }
}

fn handle() -> Handle {
    Handle {
        owner: 1,
        generation: 2,
        slot: 3,
    }
}

#[test]
fn cube_covers_one_whole_block_from_its_minimum_corner() {
    let GeometrySpec::Triangles(mesh) = cube() else {
        panic!("cube is a triangle mesh");
    };
    assert_eq!(mesh.vertices.len(), 8);
    assert!(mesh.vertices.iter().all(|v| v.color == (1., 1., 1., 1.)));
    let corners: std::collections::BTreeSet<_> = mesh
        .vertices
        .iter()
        .map(|v| {
            (
                v.position.0 as i32,
                v.position.1 as i32,
                v.position.2 as i32,
            )
        })
        .collect();
    assert_eq!(corners.len(), 8);
    assert!(
        corners
            .iter()
            .all(|(x, y, z)| [x, y, z].iter().all(|v| [0, 1].contains(*v)))
    );
    // Six faces of two triangles, each face lying on one side of the block.
    assert_eq!(mesh.indices.len(), 36);
    let mut sides = std::collections::BTreeSet::new();
    for face in mesh.indices.chunks(6) {
        let points: Vec<_> = face
            .iter()
            .map(|index| mesh.vertices[*index as usize].position)
            .map(|(x, y, z)| [x, y, z])
            .collect();
        let side = (0..3)
            .find_map(|axis| {
                let value = points[0][axis];
                points
                    .iter()
                    .all(|point| point[axis] == value)
                    .then_some((axis, value as i32))
            })
            .expect("each face is axis-aligned");
        assert!(sides.insert(side));
    }
    assert_eq!(sides.len(), 6);
}

#[test]
fn every_found_block_gets_a_pink_cube_through_walls() {
    for (position, through_walls) in [(position(-9, 4, -4), true), (position(120, -60, 7), false)] {
        let instance = cube_instance(handle(), -1, position, through_walls);
        assert_eq!(instance.geometry.slot, handle().slot);
        assert_eq!(instance.tint, (1.0, 0.12, 0.55, 1.0));
        // Without through-wall authority the cubes respect world depth.
        let depth = if through_walls {
            Depth::Always
        } else {
            Depth::Test
        };
        assert_eq!(instance.depth, depth);
        assert_eq!(instance.transform.scale, (1., 1., 1.));
        let Anchor::World(anchor) = &instance.anchor else {
            panic!("cubes are world-anchored");
        };
        assert_eq!(anchor.dimension, -1);
        assert_eq!(
            anchor.origin,
            (
                f64::from(position.x),
                f64::from(position.y),
                f64::from(position.z)
            )
        );
        assert_eq!(
            instance.params,
            mod_api::materials::Preset::default().0.to_vec()
        );
    }
}

#[test]
fn search_and_draw_bounds_fit_the_host_limits() {
    assert_eq!(MAX_RANGE, mod_api::MAX_TERRAIN_DISTANCE as f32);
    let GeometrySpec::Triangles(mesh) = cube() else {
        unreachable!()
    };
    assert!(LIMIT * mesh.indices.len() <= mod_api::MAX_DRAW_VERTEX_WORK);
    let many: Vec<_> = (0..LIMIT as i32 + 10).map(|x| position(x, 0, 0)).collect();
    let mut finder = Finder::default();
    assert!(finder.apply(0, true, &many));
    let mut drawn = Drawn::default();
    settle(&mut finder, &mut drawn);
    assert_eq!(finder.cubes(), LIMIT, "only the nearest are drawn");
    assert!(drawn.blocks().iter().all(|block| block.0 < LIMIT as i32));
}

#[test]
fn block_keys_sort_by_position_and_round_trip() {
    for block in [
        position(0, 0, 0),
        position(-30_000_000, -64, 29_999_999),
        position(29_999_999, 319, -30_000_000),
        position(-1, -2048, 1),
    ] {
        let back = super::position(key(&block));
        assert_eq!((back.x, back.y, back.z), (block.x, block.y, block.z));
    }
    // Keys order by x, then z, then y.
    assert!(key(&position(-1, 300, 9)) < key(&position(0, -60, -9)));
    assert!(key(&position(0, 300, -9)) < key(&position(0, -60, 9)));
    assert!(key(&position(0, -60, 0)) < key(&position(0, 300, 0)));
}

/// A stand-in for Cinnabar's retained instances.
#[derive(Default)]
struct Drawn {
    next: u64,
    cubes: std::collections::BTreeMap<u64, (i32, i32, i32)>,
    calls: usize,
}

impl Drawn {
    /// Performs one change as `world-draw` would.
    fn draw(&mut self, change: Change) -> Result<Option<Handle>, DrawError> {
        self.calls += 1;
        let key = |position: Position| (position.x, position.y, position.z);
        Ok(match change {
            Change::Create(position) => {
                self.next += 1;
                self.cubes.insert(self.next, key(position));
                Some(Handle {
                    slot: self.next,
                    ..handle()
                })
            }
            Change::Move(handle, position) => {
                *self.cubes.get_mut(&handle.slot).unwrap() = key(position);
                None
            }
            Change::Remove(handle) => {
                self.cubes.remove(&handle.slot).unwrap();
                None
            }
        })
    }

    /// The drawn blocks, sorted.
    fn blocks(&self) -> Vec<(i32, i32, i32)> {
        let mut blocks: Vec<_> = self.cubes.values().copied().collect();
        blocks.sort_unstable();
        blocks
    }
}

/// Compares the applied answer, as the frames after it do, and returns the frames it took.
fn compare(finder: &mut Finder) -> usize {
    let mut frames = 0;
    while finder.comparing() {
        let calls = finder.sync(SYNC_CALLS, |_| panic!("no drawing while comparing"));
        assert!(calls.is_ok());
        finder.compare(COMPARE_BLOCKS);
        frames += 1;
    }
    frames
}

/// Compares, then syncs until the cubes show the selection; returns the syncing frames.
fn settle(finder: &mut Finder, drawn: &mut Drawn) -> usize {
    compare(finder);
    let mut frames = 0;
    while !finder.synced() {
        let before = drawn.calls;
        finder
            .sync(SYNC_CALLS, |change| drawn.draw(change))
            .unwrap();
        assert!(drawn.calls - before <= SYNC_CALLS);
        frames += 1;
    }
    frames
}

#[test]
fn a_changed_selection_moves_spare_cubes_before_adding_or_removing() {
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    assert!(finder.synced());
    let first: Vec<_> = (0..40).map(|x| position(x, 5, -x)).collect();
    assert!(finder.apply(0, true, &first));
    assert_eq!(compare(&mut finder), 1, "64 blocks compared per frame");
    assert_eq!(settle(&mut finder, &mut drawn), 3, "16 cubes per frame");
    assert_eq!(drawn.calls, 40, "a synced selection makes no further calls");
    assert_eq!(drawn.cubes.len(), 40);
    // Ten blocks are mined and ten others found: ten moves and nothing else.
    let second: Vec<_> = (10..50).map(|x| position(x, 5, -x)).collect();
    assert!(finder.apply(0, true, &second));
    let calls = drawn.calls;
    assert_eq!(settle(&mut finder, &mut drawn), 1);
    assert_eq!(drawn.calls - calls, 10);
    assert_eq!(drawn.cubes.len(), 40);
    let mut expected: Vec<_> = second.iter().map(|p| (p.x, p.y, p.z)).collect();
    expected.sort_unstable();
    assert_eq!(drawn.blocks(), expected);
    // Fewer blocks remove the spare cubes; none removes them all.
    assert!(finder.apply(0, true, &second[..5]));
    settle(&mut finder, &mut drawn);
    assert_eq!(drawn.cubes.len(), 5);
    finder.clear();
    settle(&mut finder, &mut drawn);
    assert!(drawn.cubes.is_empty());
    assert_eq!(finder.cubes(), 0);
}

#[test]
fn a_failed_call_keeps_the_changes_made_before_it() {
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    let blocks: Vec<_> = (0..8).map(|x| position(x, 0, 0)).collect();
    assert!(finder.apply(0, true, &blocks));
    compare(&mut finder);
    let error = DrawError {
        code: mod_api::bindings::cinnabar::world_draw::drawing::ErrorCode::BudgetExceeded,
        detail: "instances".into(),
    };
    let mut allowed = 3;
    let result = finder.sync(SYNC_CALLS, |change| {
        if allowed == 0 {
            return Err(error.clone());
        }
        allowed -= 1;
        drawn.draw(change)
    });
    assert_eq!(result.unwrap_err().detail, error.detail);
    assert_eq!(finder.cubes(), 3);
    assert_eq!(settle(&mut finder, &mut drawn), 1);
    assert_eq!(drawn.cubes.len(), 8);
}

#[test]
fn nearest_first_reordering_keeps_the_selection() {
    let a = [position(1, 2, 3), position(-4, 5, -6), position(7, -8, 9)];
    let b = [a[2], a[0], a[1]];
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    assert!(finder.apply(0, true, &a));
    settle(&mut finder, &mut drawn);
    let calls = drawn.calls;
    assert!(
        !finder.apply(0, true, &a),
        "an identical answer is not compared"
    );
    finder.apply(0, true, &b);
    assert_eq!(settle(&mut finder, &mut drawn), 0, "only the order moved");
    assert_eq!(drawn.calls, calls);
    assert!(finder.apply(0, true, &a[..2]));
    settle(&mut finder, &mut drawn);
    assert!(finder.apply(0, true, &[]));
    settle(&mut finder, &mut drawn);
    assert!(
        drawn.cubes.is_empty(),
        "an emptied selection removes the cubes"
    );
    assert!(!finder.apply(0, true, &[]));
    assert!(finder.apply(-1, false, &a));
    assert_eq!(finder.placement(), (-1, false));
}

#[test]
fn the_cube_table_tracks_every_answer_through_collisions_and_removals() {
    // Blocks packed close together share probe runs; random answers churn the table.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    for _ in 0..60 {
        let count = (next() % (LIMIT as u64 + 40)) as usize;
        let mut blocks: Vec<_> = (0..count)
            .map(|_| {
                let value = next();
                position(
                    (value % 24) as i32 - 12,
                    ((value >> 8) % 6) as i32 - 64,
                    ((value >> 16) % 24) as i32 - 12,
                )
            })
            .collect();
        blocks.sort_unstable_by_key(|block| (block.x, block.y, block.z));
        blocks.dedup_by_key(|block| (block.x, block.y, block.z));
        finder.apply(0, true, &blocks);
        assert!(compare(&mut finder) <= (2 * LIMIT + 1).div_ceil(COMPARE_BLOCKS));
        settle(&mut finder, &mut drawn);
        let mut expected: Vec<_> = blocks[..blocks.len().min(LIMIT)]
            .iter()
            .map(|block| (block.x, block.y, block.z))
            .collect();
        expected.sort_unstable();
        assert_eq!(drawn.blocks(), expected);
        assert_eq!(finder.cubes(), expected.len());
        assert!(
            !finder.apply(0, true, &blocks),
            "a settled answer needs nothing"
        );
    }
}

#[test]
fn applies_the_first_scanned_answer_after_changes_then_on_a_short_cadence() {
    let mut finder = Finder::default();
    // A new search answers nothing on the frame that starts it; the next frame applies.
    assert!(!finder.due(1, 0, 64, true, 0.016));
    assert!(finder.due(1, 0, 64, true, 0.016));
    assert!(!finder.due(1, 0, 64, true, 0.1));
    assert!(!finder.due(1, 0, 64, true, f32::NAN));
    assert!(!finder.due(1, 0, 64, true, -1.0));
    assert!(finder.due(1, 0, 64, true, REFRESH_SECONDS));
    assert!(!finder.due(1, 0, 64, true, 0.0));
    // A new range, session, dimension or depth authority applies on the following frame.
    for (session, dimension, range, through_walls) in [
        (1, 0, 65, true),
        (2, 0, 65, true),
        (2, -1, 65, true),
        (2, -1, 65, false),
    ] {
        assert!(!finder.due(session, dimension, range, through_walls, 0.0));
        assert!(finder.due(session, dimension, range, through_walls, 0.0));
    }
    // Leaving gameplay keeps the selection; returning starts a search that keeps it until
    // the scanned answer arrives.
    assert!(finder.apply(-1, false, &[position(1, 1, 1)]));
    compare(&mut finder);
    finder.pause();
    assert!(finder.places());
    assert!(!finder.due(2, -1, 65, false, 0.0), "a search restarts");
    assert!(finder.due(2, -1, 65, false, 0.0));
    finder.clear();
    assert!(!finder.places());
    assert!(
        !finder.due(2, -1, 65, false, 0.0),
        "re-enabling starts a search"
    );
}

#[test]
fn a_denied_search_is_reported_once_until_disabled() {
    let mut finder = Finder::default();
    assert!(finder.notify_denied());
    assert!(!finder.notify_denied());
    finder.reset();
    assert!(
        !finder.notify_denied(),
        "a drawing restart is not a new enablement"
    );
    finder.clear_notice();
    assert!(finder.notify_denied());
}

/// The complete coordinate domain round-trips, and repeated answers take bounded sync units.
#[test]
fn full_coordinate_keys_and_duplicate_work_are_bounded() {
    for p in [
        position(i32::MIN, i32::MAX, 0),
        position(0, i32::MIN, i32::MAX),
    ] {
        let round = super::position(key(&p));
        assert_eq!((round.x, round.y, round.z), (p.x, p.y, p.z));
    }
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    finder.apply(0, true, &vec![position(1, 2, 3); LIMIT]);
    compare(&mut finder);
    finder.sync(1, |c| drawn.draw(c)).unwrap();
    let left = finder.needed.len();
    finder
        .sync(1, |_| panic!("duplicate needs no drawing call"))
        .unwrap();
    assert_eq!(finder.needed.len(), left - 1);
    settle(&mut finder, &mut drawn);
    assert_eq!(finder.cubes(), 1);
    finder.clear();
    finder.sync(1, |c| drawn.draw(c)).unwrap();
    assert!(finder.synced());
}

/// Non-drawable s32 coordinates cannot block valid nearby selection or teardown.
#[test]
fn positions_outside_drawing_bounds_do_not_stall_the_queue() {
    let mut finder = Finder::default();
    let mut drawn = Drawn::default();
    finder.apply(
        0,
        true,
        &[
            position(i32::MAX, 0, 0),
            position(1, 2, 3),
            position(0, i32::MIN, 0),
        ],
    );
    settle(&mut finder, &mut drawn);
    assert_eq!(drawn.blocks(), vec![(1, 2, 3)]);
    finder.apply(0, true, &[position(i32::MIN, 0, 0)]);
    settle(&mut finder, &mut drawn);
    assert!(finder.synced());
    assert!(drawn.blocks().is_empty());
}
