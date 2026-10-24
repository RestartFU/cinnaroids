use super::*;
use mod_api::bindings::cinnabar::world_draw::drawing::{Anchor, Depth};

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

/// Places positions into a fresh list.
fn instances(
    geometry: Handle,
    dimension: i32,
    positions: &[Position],
    through_walls: bool,
) -> Vec<Instance> {
    let mut out = Vec::new();
    place(&mut out, geometry, dimension, positions, through_walls);
    out
}

#[test]
fn every_found_block_gets_a_pink_cube_through_walls() {
    let positions = [position(-9, 4, -4), position(120, -60, 7)];
    let drawn = instances(handle(), -1, &positions, true);
    assert_eq!(drawn.len(), 2);
    for (instance, position) in drawn.iter().zip(&positions) {
        assert_eq!(instance.geometry.slot, handle().slot);
        assert_eq!(instance.tint, (1.0, 0.12, 0.55, 1.0));
        assert_eq!(instance.depth, Depth::Always);
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
    // Without through-wall authority the cubes respect world depth.
    assert!(
        instances(handle(), 0, &positions, false)
            .iter()
            .all(|instance| instance.depth == Depth::Test)
    );
}

#[test]
fn search_and_draw_bounds_fit_the_host_limits() {
    assert_eq!(MAX_RANGE, mod_api::MAX_TERRAIN_DISTANCE as f32);
    assert!(GROUP.len() <= mod_api::MAX_DRAW_GROUP_NAME_BYTES);
    let many: Vec<_> = (0..LIMIT as i32 + 10).map(|x| position(x, 0, 0)).collect();
    assert_eq!(instances(handle(), 0, &many, true).len(), LIMIT);
    let GeometrySpec::Triangles(mesh) = cube() else {
        unreachable!()
    };
    assert!(LIMIT * mesh.indices.len() <= mod_api::MAX_DRAW_VERTEX_WORK);
}

#[test]
fn nearest_first_reordering_keeps_the_drawn_selection() {
    let a = [position(1, 2, 3), position(-4, 5, -6), position(7, -8, 9)];
    let b = [a[2], a[0], a[1]];
    assert_eq!(signature(&a), signature(&b));
    assert_ne!(signature(&a), signature(&a[..2]));
    assert_ne!(
        signature(&[position(0, 0, 0), position(1, 0, 0)]),
        signature(&[position(0, 0, 0), position(0, 1, 0)])
    );
    let mut finder = Finder::default();
    assert!(finder.changed(&a));
    assert!(!finder.changed(&b), "only the order moved");
    assert!(finder.changed(&a[..2]));
    assert!(finder.changed(&[]), "an emptied selection clears the cubes");
    assert!(!finder.changed(&[]));
    assert!(finder.changed(&a));
    assert!(finder.hide(), "a shown group must be emptied");
    assert!(!finder.hide());
}

#[test]
fn searches_at_once_after_changes_then_on_a_short_cadence() {
    let mut finder = Finder::default();
    assert!(finder.due(1, 0, 64, true, 0.016));
    assert!(!finder.due(1, 0, 64, true, 0.1));
    assert!(!finder.due(1, 0, 64, true, f32::NAN));
    assert!(!finder.due(1, 0, 64, true, -1.0));
    assert!(finder.due(1, 0, 64, true, REFRESH_SECONDS));
    assert!(!finder.due(1, 0, 64, true, 0.0));
    // A new range, session, dimension or depth authority searches immediately.
    assert!(finder.due(1, 0, 65, true, 0.0));
    assert!(finder.due(2, 0, 65, true, 0.0));
    assert!(finder.due(2, -1, 65, true, 0.0));
    assert!(finder.due(2, -1, 65, false, 0.0));
    finder.hide();
    assert!(
        finder.due(2, -1, 65, false, 0.0),
        "re-enabling searches at once"
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

#[test]
fn placements_are_rewritten_in_place_for_a_new_selection() {
    let mut out = Vec::new();
    place(
        &mut out,
        handle(),
        0,
        &[position(1, 2, 3), position(4, 5, 6)],
        true,
    );
    let params = out[0].params.as_ptr();
    place(&mut out, handle(), -1, &[position(7, 8, 9)], false);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].params.as_ptr(), params, "no new parameter block");
    let Anchor::World(anchor) = &out[0].anchor else {
        unreachable!()
    };
    assert_eq!((anchor.dimension, anchor.origin), (-1, (7., 8., 9.)));
    assert_eq!(out[0].depth, Depth::Test);
    assert_eq!(out[0].tint, (1.0, 0.12, 0.55, 1.0));
    place(&mut out, handle(), 0, &[], true);
    assert!(out.is_empty());
}
