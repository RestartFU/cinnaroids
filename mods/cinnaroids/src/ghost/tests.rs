use super::*;
use mod_api::bindings::cinnabar::world_draw::drawing::Depth;

fn sample(session: u64, x: f32) -> Option<(u64, [f32; 3])> {
    Some((session, [x, 0., 0.]))
}

#[test]
fn smoothing_is_frame_rate_independent_and_never_overshoots() {
    let mut slow = Smoothed::default();
    let mut fast = Smoothed::default();
    for state in [&mut slow, &mut fast] {
        state.update(sample(1, 0.), 0.);
    }
    for _ in 0..3 {
        slow.update(sample(1, 1.), 1. / 30.);
    }
    for _ in 0..12 {
        fast.update(sample(1, 1.), 1. / 120.);
    }
    let (slow_x, fast_x) = (slow.position.unwrap()[0], fast.position.unwrap()[0]);
    assert!((slow_x - fast_x).abs() < 1e-5);
    assert!((0.0..1.0).contains(&slow_x));
    assert_eq!(slow.update(sample(1, 1.), 1.), Some([1., 0., 0.]));
}

#[test]
fn new_sessions_teleports_and_reenabled_tracking_snap() {
    let mut smooth = Smoothed::default();
    smooth.update(sample(1, 0.), 0.);
    assert_eq!(smooth.update(sample(2, 1.), 0.), Some([1., 0., 0.]));
    assert_eq!(smooth.update(sample(2, 20.), 0.), Some([20., 0., 0.]));
    assert_eq!(smooth.update(None, 0.), None);
    assert_eq!(smooth.update(sample(2, 1.), 0.), Some([1., 0., 0.]));
    assert_eq!(smooth.update(sample(2, f32::NAN), 0.), None);
    assert_eq!(smooth.update(sample(0, 1.), 0.), None);
}

#[test]
fn marker_uses_standing_bounds_and_hides_around_the_camera() {
    let feet = [10., 64., -3.];
    let [min, max] = bounds(feet, [0., 66., 0.]).unwrap();
    let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|axis| (a[axis] - b[axis]).abs() < 1e-5);
    assert!(near(min, [9.7, 64., -3.3]), "{min:?}");
    assert!(near(max, [10.3, 65.8, -2.7]), "{max:?}");
    // A first-person camera inside the box would otherwise tint the whole view; a
    // third-person camera behind the same eye sees it.
    assert!(bounds(feet, [10., 65.62, -3.]).is_none());
    assert!(bounds(feet, [10., 66.5, 1.]).is_some());
    let mut marker = Marker::default();
    assert_eq!(marker.update(Some((1, feet)), [10., 65.62, -3.], 0.), None);
    assert_eq!(
        marker.update(Some((1, feet)), [14., 65.62, -3.], 0.),
        Some(feet)
    );
    assert_eq!(marker.update(None, [14., 65.62, -3.], 0.), None);
}

#[test]
fn translucent_faces_and_every_edge_once() {
    let [faces, edges @ ..] = geometries();
    let GeometrySpec::Triangles(mesh) = faces else {
        panic!("faces are a mesh");
    };
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.indices.len(), 36);
    assert!(
        mesh.vertices
            .iter()
            .all(|v| v.color == (0.888, 0.195, 0.125, 0.12))
    );
    let mut segments = std::collections::BTreeSet::new();
    for edge in edges {
        let GeometrySpec::Line(line) = edge else {
            panic!("edges are polylines");
        };
        assert!(!line.closed);
        assert!(
            line.vertices
                .iter()
                .all(|v| v.color == (0.888, 0.195, 0.125, 0.9))
        );
        for pair in line.vertices.windows(2) {
            let key = |v: &Vertex| {
                let (x, y, z) = v.position;
                ((x * 10.) as i32, (y * 10.) as i32, (z * 10.) as i32)
            };
            let (a, b) = (key(&pair[0]), key(&pair[1]));
            let differing = [a.0 != b.0, a.1 != b.1, a.2 != b.2];
            assert_eq!(differing.iter().filter(|d| **d).count(), 1, "box edge");
            assert!(segments.insert((a.min(b), a.max(b))), "edge drawn twice");
        }
    }
    assert_eq!(segments.len(), 12);
}

#[test]
fn instances_follow_the_feet_with_world_width_edges_and_depth() {
    let handles = std::array::from_fn(|slot| Handle {
        owner: 1,
        generation: 1,
        slot: slot as u64,
    });
    let drawn = instances(&handles, 1, [0.5, 70.25, -8.]);
    assert_eq!(drawn.len(), INSTANCES);
    for (index, instance) in drawn.iter().enumerate() {
        assert_eq!(instance.geometry.slot, index as u64);
        assert_eq!(instance.depth, Depth::Test);
        assert_eq!(instance.tint, (1., 1., 1., 1.));
        let Anchor::World(anchor) = &instance.anchor else {
            panic!("world anchored");
        };
        assert_eq!(anchor.dimension, 1);
        assert_eq!(anchor.origin, (0.5, 70.25, -8.));
        if index > 0 {
            assert_eq!(instance.width, EDGE_WIDTH);
            assert_eq!(instance.width_mode, WidthMode::World);
        }
    }
}
