use super::*;
use mod_api::bindings::cinnabar::extension::player_state::Bounds;
use serde_json::Value;

/// A remote actor with deliberately nonstandard bounds and health capacity.
fn actor() -> Actor {
    Actor {
        session: 3,
        dimension: 0,
        runtime_id: 8,
        player: true,
        position: (1., 0., -4.),
        boxes: vec![Bounds {
            min: (0.5, 0., -4.5),
            max: (1.5, 1.2, -3.5),
        }],
        name: "Player \"One\"".into(),
        identifier: "minecraft:player".into(),
        yaw: 0.,
        pitch: 0.,
        pickable: true,
        hittable: None,
        look_origin: None,
        look_direction: None,
        invisible: false,
        sneaking: true,
        hurt: false,
        health: Some(10.),
        max_health: Some(40.),
    }
}

/// Builds one frame with an eye outside every fixture actor.
fn output(actors: &[Actor], p: &Preferences) -> Output {
    build(p, 3, 0, [0., 1.6, 0.], actors, geometry())
}

#[test]
fn boxes_cover_every_native_edge_and_health_uses_reported_maximum() {
    let result = output(&[actor()], &Preferences::default());
    assert_eq!(result.draws.len(), 2);
    assert!(matches!(
        result.draws[0].anchor,
        Anchor::World(WorldAnchor {
            origin: (0.5, 0., -4.5),
            ..
        })
    ));
    assert_eq!(result.draws[0].transform.scale.1, 1.2);
    assert_eq!(result.draws[1].transform.scale.1, 0.3);
    assert!(
        result
            .draws
            .iter()
            .all(|draw| draw.depth == Depth::Always && draw.width_mode == WidthMode::Pixels)
    );
    let labels: Value = serde_json::from_str(&result.labels).unwrap();
    let text = labels["world_labels"][0]["text"].as_str().unwrap();
    assert!(text.contains("Player \"One\""));
    assert!(text.contains("10 HP"));
    assert!(text.contains("blocks"));
}

#[test]
fn stale_foreign_non_player_out_of_range_and_missing_bounds_draw_nothing() {
    for change in 0..6 {
        let mut a = actor();
        match change {
            0 => a.session = 4,
            1 => a.dimension = 1,
            2 => a.player = false,
            3 => a.position.2 = -200.,
            4 => a.boxes.clear(),
            _ => a.boxes[0].min.0 = f32::NAN,
        }
        let result = output(&[a], &Preferences::default());
        assert!(result.draws.is_empty());
        assert_eq!(
            serde_json::from_str::<Value>(&result.labels).unwrap()["world_labels"],
            serde_json::json!([])
        );
    }
}

#[test]
fn missing_maximum_never_fabricates_a_health_bar_and_options_are_independent() {
    let mut a = actor();
    a.max_health = None;
    assert_eq!(output(&[a.clone()], &Preferences::default()).draws.len(), 1);
    a.health = None;
    let p = Preferences {
        esp_names: false,
        esp_distance: false,
        ..Default::default()
    };
    let result = output(&[a.clone()], &p);
    assert_eq!(result.draws.len(), 1);
    assert_eq!(result.labels, "{\"cards\":[],\"world_labels\":[]}");
    a.max_health = Some(40.);
    a.health = Some(10.);
    let result = output(
        &[a],
        &Preferences {
            esp_health: false,
            ..p
        },
    );
    assert_eq!(result.draws.len(), 1);
    assert_eq!(result.labels, "{\"cards\":[],\"world_labels\":[]}");
}

#[test]
fn full_actor_snapshot_fits_shared_line_budget_and_never_wraps_camera() {
    let actors: Vec<_> = (0..mod_api::MAX_OBSERVED_ACTORS)
        .map(|index| {
            let mut a = actor();
            a.runtime_id += index as u64;
            a
        })
        .collect();
    let result = output(&actors, &Preferences::default());
    assert_eq!(result.draws.len(), MAX_PLAYERS * 2);
    assert!(
        result
            .draws
            .iter()
            .all(|draw| draw.geometry.slot == geometry().boxes.slot
                || draw.geometry.slot == geometry().health.slot)
    );
    assert_eq!(
        serde_json::from_str::<Value>(&result.labels).unwrap()["world_labels"]
            .as_array()
            .unwrap()
            .len(),
        MAX_PLAYERS
    );
    assert!(
        build(
            &Preferences::default(),
            3,
            0,
            [1., 0.6, -4.],
            &[actor()],
            geometry()
        )
        .draws
        .is_empty()
    );
}

#[test]
fn label_escaping_preserves_unicode_quotes_backslashes_and_control_bytes() {
    let text = "Aé\n\"\\\u{0001}";
    let mut json = String::new();
    quoted(&mut json, text);
    assert_eq!(serde_json::from_str::<String>(&json).unwrap(), text);
}

/// Distinct synthetic tokens keep pure placement tests independent of a component host.
fn geometry() -> Geometry {
    Geometry {
        boxes: Handle {
            owner: 1,
            generation: 1,
            slot: 1,
        },
        health: Handle {
            owner: 1,
            generation: 1,
            slot: 2,
        },
    }
}

#[test]
fn unit_box_covers_each_edge_without_diagonals() {
    let GeometrySpec::Line(line) = box_geometry() else {
        panic!("polyline")
    };
    let mut edges = std::collections::BTreeSet::new();
    for pair in line.vertices.windows(2) {
        let a: [f32; 3] = pair[0].position.into();
        let b: [f32; 3] = pair[1].position.into();
        assert_eq!((0..3).filter(|i| a[*i] != b[*i]).count(), 1);
        let encode = |p: [f32; 3]| p[0] as u8 | (p[1] as u8) << 1 | (p[2] as u8) << 2;
        let (a, b) = (encode(a), encode(b));
        edges.insert((a.min(b), a.max(b)));
    }
    assert_eq!(edges.len(), 12);
}
