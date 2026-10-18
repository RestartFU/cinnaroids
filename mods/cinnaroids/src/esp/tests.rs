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
fn selects_native_player_identity_and_preserves_reported_health() {
    let result = output(&[actor()], &Preferences::default());
    assert_eq!(result.players, [8]);
    let labels: Value = serde_json::from_str(&result.labels).unwrap();
    let text = labels["world_labels"][0]["text"].as_str().unwrap();
    assert!(text.contains("Player \"One\""));
    assert!(text.contains("10/40 HP"));
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
        assert!(result.players.is_empty());
        assert_eq!(
            serde_json::from_str::<Value>(&result.labels).unwrap()["world_labels"],
            serde_json::json!([])
        );
    }
}

#[test]
fn missing_health_stays_unknown_and_label_options_are_independent() {
    let mut a = actor();
    a.max_health = None;
    let labels: Value =
        serde_json::from_str(&output(&[a.clone()], &Preferences::default()).labels).unwrap();
    assert!(
        labels["world_labels"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("10 HP")
    );
    a.health = None;
    let p = Preferences {
        esp_names: false,
        esp_distance: false,
        ..Default::default()
    };
    let result = output(&[a.clone()], &p);
    assert_eq!(result.players.len(), 1);
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
    assert_eq!(result.players.len(), 1);
    assert_eq!(result.labels, "{\"cards\":[],\"world_labels\":[]}");
}

#[test]
fn full_actor_snapshot_stays_bounded_and_excludes_camera() {
    let actors: Vec<_> = (0..mod_api::MAX_OBSERVED_ACTORS)
        .map(|index| {
            let mut a = actor();
            a.runtime_id += index as u64;
            a
        })
        .collect();
    let result = output(&actors, &Preferences::default());
    assert_eq!(result.players.len(), MAX_PLAYERS);
    assert_eq!(
        serde_json::from_str::<Value>(&result.labels).unwrap()["world_labels"]
            .as_array()
            .unwrap()
            .len(),
        MAX_PLAYERS
    );
    assert!(
        build(&Preferences::default(), 3, 0, [1., 0.6, -4.], &[actor()])
            .players
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

#[test]
fn host_style_has_a_crisp_edge_and_bounded_matching_halo() {
    for color in 0..COLOR_NAMES.len() as u8 {
        let style = appearance(color);
        assert_eq!(
            (style.outline.r, style.outline.g, style.outline.b),
            (style.glow.r, style.glow.g, style.glow.b)
        );
        assert!(
            style.outline_radius > 0.
                && style.outline_radius <= mod_api::drawing::MAX_MASK_OUTLINE_RADIUS
        );
        assert!(
            style.glow_radius > style.outline_radius
                && style.glow_radius <= mod_api::drawing::MAX_MASK_GLOW_RADIUS
        );
        assert!(style.glow.a > 0. && style.glow.a < style.outline.a);
    }
}
