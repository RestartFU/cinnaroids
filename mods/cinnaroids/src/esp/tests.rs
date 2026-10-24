use super::*;
use mod_api::bindings::cinnabar::extension::player_state::Bounds;
use serde_json::Value;

/// A remote actor with deliberately nonstandard bounds and health capacity.
fn actor() -> ObservedActor {
    ObservedActor {
        actor: Actor {
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
        },
        max_health: Some(40.),
    }
}

/// Builds one frame with an eye outside every fixture actor.
fn output(actors: &[ObservedActor], p: &Preferences) -> Output {
    build(p, 3, 0, [0., 1.6, 0.], actors)
}

/// The shown label slots of one HUD surface update, after checking every slot is set.
fn labels(output: &Output) -> Vec<Value> {
    let update: Value = serde_json::from_str(&output.labels).unwrap();
    let bindings = update["bindings"].as_object().unwrap();
    assert_eq!(bindings.len(), MAX_PLAYERS * 5, "every slot is replaced");
    (0..MAX_PLAYERS)
        .filter(|index| bindings[&format!("#esp_{index}_shown")] == true)
        .map(|index| {
            serde_json::json!({
                "text": bindings[&format!("#esp_{index}_text")],
                "at": bindings[&format!("#esp_{index}_at")],
                "size": [bindings[&format!("#esp_{index}_w")], bindings[&format!("#esp_{index}_h")]],
            })
        })
        .collect()
}

#[test]
fn selects_native_player_identity_and_preserves_reported_health() {
    let result = output(&[actor()], &Preferences::default());
    assert_eq!(result.players, [8]);
    let labels = labels(&result);
    let text = labels[0]["text"].as_str().unwrap();
    assert!(text.contains("Player \"One\""));
    assert!(text.contains("10/40 HP"));
    assert!(text.contains("blocks"));
}

#[test]
fn labels_project_above_the_bounds_with_the_earlier_plate_size() {
    let result = output(&[actor()], &Preferences::default());
    let labels = labels(&result);
    assert_eq!(labels.len(), 1);
    // Cinnabar re-projects the world point every frame; the plate centres on it.
    assert_eq!(
        labels[0]["at"],
        serde_json::json!({"project": {"world": [1.0, 1.85, -4.0]}})
    );
    let text = labels[0]["text"].as_str().unwrap();
    let columns = text.lines().map(|line| line.chars().count()).max().unwrap() as f64;
    let size = labels[0]["size"].as_array().unwrap();
    assert!((size[0].as_f64().unwrap() - (columns * 7.2 + 4.8)).abs() < 1e-3);
    assert!((size[1].as_f64().unwrap() - (2. * 12. + 4.8)).abs() < 1e-3);
}

#[test]
fn hud_surface_has_one_hidden_projected_slot_per_player() {
    let surface: Value = serde_json::from_str(SURFACE).unwrap();
    assert_eq!(surface["screen"], "cinnaroids.hud");
    let document = surface["document"].as_str().unwrap();
    for index in 0..MAX_PLAYERS {
        assert_eq!(surface["bindings"][format!("#esp_{index}_shown")], false);
        for name in ["_at", "_at_visible", "_text", "_w", "_h", "_shown"] {
            assert!(
                document.contains(&format!("\"#esp_{index}{name}\"")),
                "{index}{name}"
            );
        }
    }
    assert!(!document.contains(&format!("#esp_{MAX_PLAYERS}_")));
}

#[test]
fn stale_foreign_non_player_out_of_range_and_missing_bounds_draw_nothing() {
    for change in 0..6 {
        let mut a = actor();
        match change {
            0 => a.actor.session = 4,
            1 => a.actor.dimension = 1,
            2 => a.actor.player = false,
            3 => a.actor.position.2 = -200.,
            4 => a.actor.boxes.clear(),
            _ => a.actor.boxes[0].min.0 = f32::NAN,
        }
        let result = output(&[a], &Preferences::default());
        assert!(result.players.is_empty());
        assert!(labels(&result).is_empty());
    }
}

#[test]
fn missing_health_stays_unknown_and_label_options_are_independent() {
    let mut a = actor();
    a.max_health = None;
    assert!(
        labels(&output(&[a.clone()], &Preferences::default()))[0]["text"]
            .as_str()
            .unwrap()
            .ends_with("10 HP")
    );
    a.actor.health = None;
    let p = Preferences {
        esp_names: false,
        esp_distance: false,
        ..Default::default()
    };
    let result = output(&[a.clone()], &p);
    assert_eq!(result.players.len(), 1);
    assert!(labels(&result).is_empty());
    a.max_health = Some(40.);
    a.actor.health = Some(10.);
    let result = output(
        &[a],
        &Preferences {
            esp_health: false,
            ..p
        },
    );
    assert_eq!(result.players.len(), 1);
    assert!(labels(&result).is_empty());
}

#[test]
fn full_actor_snapshot_stays_bounded_and_excludes_camera() {
    let actors: Vec<_> = (0..mod_api::MAX_OBSERVED_ACTORS)
        .map(|index| {
            let mut a = actor();
            a.actor.runtime_id += index as u64;
            a
        })
        .collect();
    let result = output(&actors, &Preferences::default());
    assert_eq!(result.players.len(), MAX_PLAYERS);
    assert_eq!(labels(&result).len(), MAX_PLAYERS);
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
