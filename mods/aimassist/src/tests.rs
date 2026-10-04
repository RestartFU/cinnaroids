use super::*;

fn config() -> Config {
    Config {
        enabled: true,
        strength: 35,
        only_when_clicking: false,
    }
}

fn player(id: u64, yaw: f32, distance: f32) -> Player {
    Player {
        id,
        feet: Vec3 {
            x: -yaw.sin() * distance,
            y: 0.0,
            z: -yaw.cos() * distance,
        },
    }
}

fn frame(players: &[Player]) -> Frame<'_> {
    Frame {
        session: 7,
        dimension: 0,
        eye: Vec3 {
            x: 0.0,
            y: CHEST_HEIGHT,
            z: 0.0,
        },
        yaw: 0.0,
        pitch: 0.0,
        attack_held: true,
        seconds: 1.0 / 60.0,
        players,
    }
}

#[test]
fn configuration_defaults_are_disabled_click_only_and_thirty_five_percent() {
    assert_eq!(
        Config::current(),
        Config {
            enabled: false,
            strength: 35,
            only_when_clicking: true,
        }
    );
}

#[test]
fn front_is_negative_z_and_behind_is_rejected() {
    let f = frame(&[]);
    assert!(candidate(&f, player(1, 0.0, 3.0)).is_some());
    assert!(candidate(&f, player(1, PI, 3.0)).is_none());
    assert!(candidate(&f, player(1, CONE + 0.01, 3.0)).is_none());
    assert!(candidate(&f, player(1, CONE - 0.01, 3.0)).is_some());
    assert!(candidate(&f, player(1, 0.0, RANGE + 0.01)).is_none());
}

#[test]
fn yaw_left_and_pitch_up_follow_actor_conventions() {
    let f = frame(&[]);
    let left = candidate(&f, player(1, 0.2, 3.0)).unwrap();
    let right = candidate(&f, player(1, -0.2, 3.0)).unwrap();
    assert!((rotation_error(&f, left).yaw - 0.2).abs() < 1e-5);
    assert!((rotation_error(&f, right).yaw + 0.2).abs() < 1e-5);
    let mut above = player(1, 0.0, 3.0);
    above.feet.y = 0.5;
    assert!(rotation_error(&f, candidate(&f, above).unwrap()).pitch > 0.0);
}

#[test]
fn yaw_wrap_crosses_pi_using_the_shortest_direction() {
    let players = [player(1, -PI + 0.03, 3.0)];
    let mut f = frame(&players);
    f.yaw = PI - 0.01;
    let c = candidate(&f, players[0]).unwrap();
    assert!((rotation_error(&f, c).yaw - 0.04).abs() < 1e-5);
    let rotation = AimAssist::default().update(config(), Some(&f)).unwrap();
    assert!(rotation.yaw > 0.0 && rotation.yaw < 0.04);
}

#[test]
fn zero_strength_disabled_and_click_mode_do_not_move_or_retain_a_target() {
    let players = [player(1, 0.2, 3.0)];
    let mut f = frame(&players);
    let mut aim = AimAssist::default();
    assert!(aim.update(config(), Some(&f)).is_some());
    let mut c = config();
    c.strength = 0;
    assert_eq!(aim.update(c, Some(&f)), None);
    assert_eq!(aim.target, None);
    c = config();
    c.enabled = false;
    assert_eq!(aim.update(c, Some(&f)), None);
    c = config();
    c.only_when_clicking = true;
    f.attack_held = false;
    assert_eq!(aim.update(c, Some(&f)), None);
    assert_eq!(aim.context, None);
    c.only_when_clicking = false;
    assert!(aim.update(c, Some(&f)).is_some());
    c.only_when_clicking = true;
    f.attack_held = true;
    assert!(aim.update(c, Some(&f)).is_some());
}

#[test]
fn strength_increases_motion_and_limits_each_axis_and_total_speed() {
    let players = [player(1, 0.5, 3.0)];
    let f = frame(&players);
    let mut weak = config();
    weak.strength = 10;
    let mut strong = config();
    strong.strength = 100;
    let a = AimAssist::default().update(weak, Some(&f)).unwrap();
    let b = AimAssist::default().update(strong, Some(&f)).unwrap();
    assert!(b.yaw > a.yaw);
    assert!(b.yaw.hypot(b.pitch) <= 3.0 * f.seconds + 1e-6);
    let mut stalled = frame(&players);
    stalled.seconds = 10.0;
    let c = AimAssist::default().update(strong, Some(&stalled)).unwrap();
    assert!(c.yaw.abs() <= MAX_FRAME_ROTATION);
    assert!(c.pitch.abs() <= MAX_FRAME_ROTATION);
    assert!(c.yaw.hypot(c.pitch) <= MAX_FRAME_ROTATION + 1e-6);
}

#[test]
fn frame_duration_is_clamped_and_zero_or_invalid_duration_is_safe() {
    let players = [player(1, 0.2, 3.0)];
    let mut f = frame(&players);
    f.seconds = MAX_FRAME_SECONDS;
    let expected = AimAssist::default().update(config(), Some(&f));
    f.seconds = 5.0;
    assert_eq!(AimAssist::default().update(config(), Some(&f)), expected);
    f.seconds = 0.0;
    assert_eq!(AimAssist::default().update(config(), Some(&f)), None);
    f.seconds = -1.0;
    assert_eq!(AimAssist::default().update(config(), Some(&f)), None);
    f.seconds = f32::NAN;
    assert_eq!(AimAssist::default().update(config(), Some(&f)), None);
}

#[test]
fn exponential_steering_converges_equally_at_thirty_and_one_twenty_fps() {
    fn simulate(fps: usize) -> f32 {
        let players = [player(1, 0.05, 3.0)];
        let mut f = frame(&players);
        f.seconds = 1.0 / fps as f32;
        let mut aim = AimAssist::default();
        for _ in 0..fps {
            if let Some(delta) = aim.update(config(), Some(&f)) {
                f.yaw += delta.yaw;
            }
        }
        f.yaw
    }
    let a = simulate(30);
    let b = simulate(120);
    assert!(a > 0.049 && a < 0.05);
    assert!((a - b).abs() < 2e-6, "30 FPS {a}, 120 FPS {b}");
}

#[test]
fn target_selection_is_angle_first_then_id_and_has_two_degree_hysteresis() {
    let players = [player(9, 0.2, 3.0), player(2, 0.2, 3.0)];
    let mut aim = AimAssist::default();
    aim.update(config(), Some(&frame(&players)));
    assert_eq!(aim.target, Some(2));
    let nearly_better = [player(2, 0.2, 3.0), player(9, 0.19, 3.0)];
    aim.update(config(), Some(&frame(&nearly_better)));
    assert_eq!(aim.target, Some(2));
    let clearly_better = [player(2, 0.2, 3.0), player(9, 0.1, 3.0)];
    aim.update(config(), Some(&frame(&clearly_better)));
    assert_eq!(aim.target, Some(9));
}

#[test]
fn session_dimension_missing_frame_and_empty_players_reset_target() {
    let players = [player(2, 0.2, 3.0), player(9, 0.19, 3.0)];
    for dimension_change in [false, true] {
        let mut aim = AimAssist {
            context: Some((7, 0)),
            target: Some(2),
        };
        let mut f = frame(&players);
        if dimension_change {
            f.dimension = 1;
        } else {
            f.session = 8;
        }
        aim.update(config(), Some(&f));
        assert_eq!(aim.target, Some(9));
        assert_eq!(aim.update(config(), None), None);
        assert_eq!(aim.target, None);
        assert_eq!(aim.context, None);
        aim.update(config(), Some(&frame(&players)));
        assert_eq!(aim.update(config(), Some(&frame(&[]))), None);
        assert_eq!(aim.target, None);
    }
}

#[test]
fn invalid_players_and_vertical_outside_cone_are_rejected() {
    let f = frame(&[]);
    assert!(candidate(&f, player(0, 0.1, 3.0)).is_none());
    let mut invalid = player(1, 0.1, 3.0);
    invalid.feet.x = f32::INFINITY;
    assert!(candidate(&f, invalid).is_none());
    let mut high = player(1, 0.0, 1.0);
    high.feet.y = 2.0;
    assert!(candidate(&f, high).is_none());
    let mut bad_frame = frame(&[]);
    bad_frame.eye.y = f32::NAN;
    assert_eq!(
        AimAssist::default().update(config(), Some(&bad_frame)),
        None
    );
}

#[test]
fn moving_away_removes_retained_target_and_already_aligned_is_no_op() {
    let players = [player(1, 0.2, 3.0)];
    let mut aim = AimAssist::default();
    aim.update(config(), Some(&frame(&players)));
    let behind = [player(1, PI, 3.0)];
    assert_eq!(aim.update(config(), Some(&frame(&behind))), None);
    assert_eq!(aim.target, None);
    let centered = [player(2, 0.0, 3.0)];
    assert_eq!(aim.update(config(), Some(&frame(&centered))), None);
    assert_eq!(aim.target, Some(2));
}
