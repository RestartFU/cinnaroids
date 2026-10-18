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
    build(p, 3, 0, [0., 1.6, 0.], actors)
}

#[test]
fn boxes_cover_every_native_edge_and_health_uses_reported_maximum() {
    let result = output(&[actor()], &Preferences::default());
    assert_eq!(result.boxes.len(), 1);
    assert_eq!(result.lines.len(), 1);
    assert_eq!(result.boxes[0].min.x, 0.5);
    assert_eq!(result.boxes[0].max.y, 1.2);
    assert_eq!(result.lines[0].points[1].y, 0.3);
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
        assert!(result.boxes.is_empty() && result.lines.is_empty());
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
    assert_eq!(output(&[a.clone()], &Preferences::default()).lines.len(), 0);
    a.health = None;
    let p = Preferences {
        esp_names: false,
        esp_distance: false,
        ..Default::default()
    };
    let result = output(&[a.clone()], &p);
    assert_eq!(result.lines.len(), 0);
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
    assert_eq!(result.lines.len(), 0);
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
    assert!(result.boxes.len() + result.lines.len() <= mod_api::MAX_RENDER_RIBBONS);
    assert!(
        result
            .lines
            .iter()
            .all(|line| line.points.len() <= mod_api::MAX_RIBBON_POINTS)
    );
    assert_eq!(
        serde_json::from_str::<Value>(&result.labels).unwrap()["world_labels"]
            .as_array()
            .unwrap()
            .len(),
        MAX_PLAYERS
    );
    assert!(
        build(&Preferences::default(), 3, 0, [1., 0.6, -4.], &[actor()])
            .boxes
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
