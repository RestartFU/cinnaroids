use super::*;

fn frame(tick: u64, sequence: u64, grounded: bool) -> Frame {
    Frame {
        session: 1,
        dimension: 0,
        tick,
        knockback_sequence: sequence,
        on_ground: grounded,
        jump_held: false,
        eligible: true,
        velocity: [0.2, -0.08, 0.0],
    }
}

#[test]
fn new_hit_arms_one_jump_at_landing_without_repeating_between_ticks() {
    let mut reset = JumpReset::default();
    assert!(!reset.update(true, 4, Some(frame(10, 1, true))));
    assert!(!reset.update(true, 4, Some(frame(11, 2, false))));
    assert!(!reset.update(true, 4, Some(frame(12, 2, false))));
    assert!(reset.update(true, 4, Some(frame(13, 2, true))));
    assert!(!reset.update(true, 4, Some(frame(13, 2, true))));
    assert!(!reset.update(true, 4, Some(frame(14, 2, true))));
    assert!(reset.update(true, 4, Some(frame(14, 3, true))));
}

#[test]
fn window_uses_physics_ticks_and_expires_without_a_landing() {
    let mut reset = JumpReset::default();
    reset.update(true, 4, Some(frame(10, 0, true)));
    reset.update(true, 4, Some(frame(11, 1, false)));
    for _ in 0..500 {
        assert!(!reset.update(true, 4, Some(frame(11, 1, false))));
    }
    assert!(!reset.update(true, 4, Some(frame(15, 1, true))));
    assert!(!reset.update(true, 4, Some(frame(16, 1, true))));
}

#[test]
fn held_jump_consumes_hit_and_rising_motion_waits_for_ground() {
    let mut reset = JumpReset::default();
    reset.update(true, 4, Some(frame(10, 0, true)));
    let mut rising = frame(11, 1, false);
    rising.velocity[1] = 0.4;
    assert!(!reset.update(true, 4, Some(rising)));
    let mut held = frame(12, 1, true);
    held.jump_held = true;
    assert!(!reset.update(true, 4, Some(held)));
    assert!(!reset.update(true, 4, Some(frame(13, 1, true))));
}

#[test]
fn disable_missing_frames_invalid_motion_and_context_changes_discard_hits() {
    for kind in 0..7 {
        let mut reset = JumpReset::default();
        reset.update(true, 4, Some(frame(10, 0, true)));
        reset.update(true, 4, Some(frame(11, 1, false)));
        let mut changed = frame(12, 1, true);
        match kind {
            0 => {
                assert!(!reset.update(false, 4, Some(changed)));
            }
            1 => {
                assert!(!reset.update(true, 4, None));
            }
            2 => {
                changed.eligible = false;
                assert!(!reset.update(true, 4, Some(changed)));
            }
            3 => {
                changed.velocity[0] = f32::NAN;
                assert!(!reset.update(true, 4, Some(changed)));
            }
            4 => {
                changed.session = 2;
                assert!(!reset.update(true, 4, Some(changed)));
            }
            5 => {
                changed.dimension = 1;
                assert!(!reset.update(true, 4, Some(changed)));
            }
            _ => {
                changed.tick = 1;
                assert!(!reset.update(true, 4, Some(changed)));
            }
        }
        assert!(!reset.update(true, 4, Some(frame(13, 1, true))));
    }
}

#[test]
fn grounded_hit_with_upward_motion_can_request_the_normal_jump() {
    let mut reset = JumpReset::default();
    reset.update(true, 4, Some(frame(10, 0, true)));
    let mut hit = frame(11, 1, true);
    hit.velocity[1] = 0.1;
    assert!(reset.update(true, 4, Some(hit)));
}
