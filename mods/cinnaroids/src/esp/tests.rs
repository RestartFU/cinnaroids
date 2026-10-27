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

/// Builds a first frame, against a freshly set surface, with an eye outside every actor.
fn output(actors: &[ObservedActor], p: &Preferences) -> Output {
    Labels::default().update(p, 3, 0, [0., 1.6, 0.], actors)
}

/// The changed bindings of one HUD surface update, which never replaces the whole set.
fn changes(output: &Output) -> serde_json::Map<String, Value> {
    output
        .labels
        .as_ref()
        .map_or_else(Default::default, |labels| {
            let update: Value = serde_json::from_str(labels).unwrap();
            assert!(update.get("bindings").is_none());
            update["binding_changes"].as_object().unwrap().clone()
        })
}

/// The label slots one first update shows, each with every binding it needs.
fn labels(output: &Output) -> Vec<Value> {
    let bindings = changes(output);
    (0..MAX_PLAYERS)
        .filter(|index| bindings.get(&format!("#esp_{index}_shown")) == Some(&Value::Bool(true)))
        .map(|index| {
            // A fresh surface already holds empty lines.
            let line = |line: u8| {
                bindings
                    .get(&format!("#esp_{index}_line{line}"))
                    .map_or("", |text| text.as_str().unwrap())
            };
            for line in [line(1), line(2)] {
                assert!(!line.chars().any(char::is_control), "{line:?}");
            }
            let text = if line(2).is_empty() {
                line(1).to_owned()
            } else {
                format!("{}\n{}", line(1), line(2))
            };
            serde_json::json!({
                "text": text,
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
    // Cinnabar follows the actor every frame; the plate centres above its bounds.
    assert_eq!(
        labels[0]["at"],
        serde_json::json!({"project": {"actor": 8, "offset": [0.0, 1.85, 0.0]}})
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
        for name in [
            "_at",
            "_at_visible",
            "_line1",
            "_line2",
            "_w",
            "_h",
            "_shown",
        ] {
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
    let mut slots = Labels::default();
    let p = Preferences::default();
    let result = slots.update(&p, 3, 0, [0., 1.6, 0.], &actors);
    assert_eq!(result.players.len(), MAX_PLAYERS);
    assert!(result.named);
    assert_eq!(
        labels(&result).len(),
        NAMES_PER_FRAME,
        "a few new names per frame"
    );
    let rest = slots.update(&p, 3, 0, [0., 1.6, 0.], &actors);
    assert_eq!(labels(&rest).len(), MAX_PLAYERS - NAMES_PER_FRAME);
    let settled = slots.update(&p, 3, 0, [0., 1.6, 0.], &actors);
    assert!(settled.labels.is_none() && !settled.named);
    assert!(
        Labels::default()
            .update(&Preferences::default(), 3, 0, [1., 0.6, -4.], &[actor()])
            .players
            .is_empty()
    );
}

#[test]
fn players_keep_their_slots_when_the_nearest_order_changes() {
    let p = Preferences::default();
    let mut slots = Labels::default();
    let eye = [0., 1.6, 0.];
    let mut second = actor();
    second.actor.runtime_id = 9;
    second.actor.name = "Second".into();
    let first = slots.update(&p, 3, 0, eye, &[actor(), second.clone()]);
    assert_eq!(labels(&first).len(), 2);
    let swapped = slots.update(&p, 3, 0, eye, &[second, actor()]);
    assert_eq!(swapped.players, [9, 8]);
    assert!(swapped.labels.is_none(), "no slot changed");
}

#[test]
fn later_updates_send_only_the_bindings_that_changed() {
    let p = Preferences::default();
    let mut labels = Labels::default();
    let eye = [0., 1.6, 0.];
    let first = labels.update(&p, 3, 0, eye, &[actor()]);
    assert_eq!(changes(&first).len(), 6, "a new slot sends all it needs");
    let again = labels.update(&p, 3, 0, eye, &[actor()]);
    assert_eq!(again.players, [8]);
    assert!(again.labels.is_none(), "an unchanged frame sends nothing");
    // Walking away changes only the distance line; the host keeps following the actor.
    let mut moved = actor();
    moved.actor.position.2 -= 10.;
    for bounds in &mut moved.actor.boxes {
        bounds.min.2 -= 10.;
        bounds.max.2 -= 10.;
    }
    let walked = changes(&labels.update(&p, 3, 0, eye, &[moved.clone()]));
    assert_eq!(
        walked.keys().collect::<Vec<_>>(),
        ["#esp_0_line2", "#esp_0_w"],
        "a longer line widens the plate: {walked:?}"
    );
    assert!(
        walked["#esp_0_line2"]
            .as_str()
            .unwrap()
            .starts_with("14 blocks")
    );
    // A player leaving hides its slot once.
    let gone = changes(&labels.update(&p, 3, 0, eye, &[]));
    assert_eq!(gone.len(), 1);
    assert_eq!(gone["#esp_0_shown"], false);
    assert!(labels.update(&p, 3, 0, eye, &[]).labels.is_none());
    // Showing it again resends only what the slot no longer holds.
    let back = changes(&labels.update(&p, 3, 0, eye, &[moved]));
    assert_eq!(back.keys().collect::<Vec<_>>(), ["#esp_0_shown"]);
    // A surface set again starts from empty slots.
    labels.reset();
    assert_eq!(changes(&labels.update(&p, 3, 0, eye, &[actor()])).len(), 6);
}

#[test]
fn names_lose_control_characters_and_lines_split_into_their_own_bindings() {
    let mut a = actor();
    a.actor.name = "Evil\nName\u{7}\u{85}".into();
    let named = labels(&output(&[a], &Preferences::default()));
    let text = named[0]["text"].as_str().unwrap();
    assert_eq!(text.lines().next(), Some("EvilName"));
    assert!(text.lines().nth(1).unwrap().ends_with("10/40 HP"));
    let distance_only = Preferences {
        esp_names: false,
        esp_health: false,
        ..Default::default()
    };
    let single = labels(&output(&[actor()], &distance_only));
    assert!(!single[0]["text"].as_str().unwrap().contains('\n'));
    assert!((single[0]["size"][1].as_f64().unwrap() - (12. + 4.8)).abs() < 1e-3);
}

#[test]
fn word_checks_agree_with_byte_checks() {
    let samples = [
        "",
        "Steve",
        "exactly8",
        "PlayerNameThatIsLonger_123",
        "Aé",
        "quote\"inside",
        "back\\slash",
        "tab\there",
        "del\u{7f}x",
        "space at end ",
        "~tilde",
        "Ξ-name with ünïcode",
        "§cRed §lName longer than eight",
        "c1\u{85}control after ascii",
        "nbsp\u{a0}is not a control",
    ];
    for a in samples {
        let escapes = a
            .bytes()
            .any(|byte| byte < 0x20 || byte == b'"' || byte == b'\\');
        assert_eq!(super::escapes(a), escapes, "{a:?}");
        assert_eq!(super::controls(a), a.chars().any(char::is_control), "{a:?}");
        assert_eq!(super::columns(a), a.chars().count(), "{a:?}");
        for b in samples {
            assert_eq!(super::same(a, b), a == b, "{a:?} {b:?}");
        }
    }
    let long = "x".repeat(mod_api::MAX_OBSERVED_ACTOR_NAME_BYTES);
    let mut changed = long.clone();
    changed.replace_range(77..78, "y");
    assert!(!super::same(&long, &changed));
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

/// Deferred label writes rotate fairly and never publish an old actor's text for a new actor.
#[test]
fn budgeted_labels_make_progress_and_hide_reassigned_slots() {
    let actors: Vec<_> = (0..MAX_PLAYERS)
        .map(|i| {
            let mut actor = actor();
            actor.actor.runtime_id += i as u64;
            actor.actor.name = format!("Player {i}");
            actor
        })
        .collect();
    let mut labels = Labels::default();
    for _ in 0..MAX_PLAYERS {
        let mut remaining = 1usize;
        labels.update_budgeted(
            &Preferences::default(),
            3,
            0,
            [0., 1.6, 0.],
            &actors,
            |actor| {
                if matches!(actor, Work::Actor) {
                    return true;
                }
                let allowed = remaining != 0;
                remaining = remaining.saturating_sub(1);
                allowed
            },
        );
    }
    assert!(labels.slots.iter().all(|slot| slot.shown));
    for (i, slot) in labels.slots.iter().enumerate() {
        assert_eq!(slot.name, format!("Player {i}"));
    }
    let mut replacement = actors;
    for actor in &mut replacement {
        actor.actor.runtime_id += 100;
    }
    labels.update_budgeted(
        &Preferences::default(),
        3,
        0,
        [0., 1.6, 0.],
        &replacement,
        |actor| matches!(actor, Work::Actor),
    );
    assert!(labels.slots.iter().all(|slot| !slot.shown));
}

/// Actor turns must wait for publication when alternate callbacks cannot write labels.
#[test]
fn periodic_label_pressure_cannot_skip_the_same_actors_forever() {
    let actors: Vec<_> = (0..MAX_PLAYERS)
        .map(|i| {
            let mut actor = actor();
            actor.actor.runtime_id += i as u64;
            actor.actor.name = format!("Player {i}");
            actor
        })
        .collect();
    let mut labels = Labels::default();
    labels.stage_writes(true);
    for frame in 0..24 {
        let mut checked = false;
        let mut written = false;
        labels.update_budgeted(
            &Preferences::default(),
            3,
            0,
            [0., 1.6, 0.],
            &actors,
            |work| match work {
                Work::Actor => {
                    let allowed = !checked;
                    checked = true;
                    allowed
                }
                Work::Label(_) => {
                    let allowed = frame % 2 == 1 && !written;
                    written = true;
                    allowed
                }
            },
        );
    }
    assert!(labels.slots.iter().all(|slot| slot.shown));
    let mut names: Vec<_> = labels.slots.iter().map(|slot| slot.name.clone()).collect();
    names.sort();
    assert_eq!(
        names,
        (0..MAX_PLAYERS)
            .map(|i| format!("Player {i}"))
            .collect::<Vec<_>>()
    );
}

/// Admission counts serialized bytes exactly across word boundaries and UTF-8 names.
#[test]
fn escaped_size_matches_json_for_every_host_text_byte() {
    for byte in 0x20u8..0x7f {
        for length in 0..=32 {
            let value = format!(
                "§{}\\\"{}",
                char::from(byte).to_string().repeat(length),
                char::from(byte)
            );
            assert_eq!(
                escaped_len(&value),
                serde_json::to_string(&value).unwrap().len() - 2
            );
        }
    }
}

#[test]
fn oversized_actor_offsets_stay_inside_the_hud_contract() {
    let mut observed = actor();
    observed.actor.boxes[0].max.1 = 256.;
    let result = output(&[observed], &Preferences::default());
    assert_eq!(result.players, [8]);
    assert_eq!(
        labels(&result)[0]["at"]["project"]["offset"],
        serde_json::json!([0., 256., 0.])
    );
}

#[test]
fn label_offsets_clamp_both_signs_on_every_axis() {
    for (min, max, expected) in [(1000., 2000., 256.), (-2000., -1000., -256.)] {
        let mut observed = actor();
        observed.actor.boxes[0] = Bounds {
            min: (min, min, min),
            max: (max, max, max),
        };
        let result = output(&[observed], &Preferences::default());
        assert_eq!(result.players, [8]);
        assert_eq!(
            labels(&result)[0]["at"]["project"]["offset"],
            serde_json::json!([expected, expected, expected])
        );
    }
}
