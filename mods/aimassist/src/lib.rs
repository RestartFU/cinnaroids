//! Deterministic local camera assistance through Cinnabar's consented WASM API.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

const RANGE: f32 = 6.0;
#[cfg(test)]
const CONE: f32 = PI / 6.0;
const CONE_COSINE: f32 = 0.866_025_4;
const CHEST_HEIGHT: f32 = 1.35;
const MAX_FRAME_SECONDS: f32 = 0.1;
const MAX_FRAME_ROTATION: f32 = 0.25;
const TARGET_SWITCH_ADVANTAGE: f32 = PI / 90.0;

// The desktop patches this one data segment before the host loads the component.
// Volatile reads ensure LLVM cannot replace the configured bytes with defaults.
#[used]
#[unsafe(no_mangle)]
static mut CINNABAR_AIM_CONFIG: [u8; 24] = *b"CNBR_AIM_CFG_v1!\x00\x23\x01\x00\x00\x00\x00\x00";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub strength: u8,
    pub only_when_clicking: bool,
}

impl Config {
    /// Reads a fixed-size, explicitly initialized block with no host or OS access.
    #[cfg(any(target_arch = "wasm32", test))]
    fn current() -> Self {
        let base = std::ptr::addr_of!(CINNABAR_AIM_CONFIG).cast::<u8>();
        // SAFETY: offsets 16..=18 are within the static 24-byte configuration.
        let payload = unsafe {
            [
                base.add(16).read_volatile(),
                base.add(17).read_volatile(),
                base.add(18).read_volatile(),
            ]
        };
        Self {
            enabled: payload[0] == 1,
            strength: payload[1].min(100),
            only_when_clicking: payload[2] != 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub id: u64,
    pub feet: Vec3,
}

pub struct Frame<'a> {
    pub session: u64,
    pub dimension: i32,
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub attack_held: bool,
    pub seconds: f32,
    pub players: &'a [Player],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy)]
struct Candidate {
    id: u64,
    alignment: f32,
    offset: Vec3,
}

#[derive(Default)]
pub struct AimAssist {
    context: Option<(u64, i32)>,
    target: Option<u64>,
}

impl AimAssist {
    pub fn reset(&mut self) {
        self.context = None;
        self.target = None;
    }

    /// Selects a visible-direction player and returns one bounded camera change.
    /// The API exposes positions, so this makes no claim about block occlusion.
    pub fn update(&mut self, config: Config, frame: Option<&Frame<'_>>) -> Option<Rotation> {
        let Some(frame) = frame else {
            self.reset();
            return None;
        };
        if !config.enabled
            || config.strength == 0
            || (config.only_when_clicking && !frame.attack_held)
            || !frame.eye.finite()
            || !frame.yaw.is_finite()
            || !frame.pitch.is_finite()
            || !frame.seconds.is_finite()
        {
            self.reset();
            return None;
        }
        let context = (frame.session, frame.dimension);
        if self.context != Some(context) {
            self.target = None;
            self.context = Some(context);
        }

        let mut best: Option<Candidate> = None;
        let mut previous: Option<Candidate> = None;
        let direction = view_direction(frame);
        for player in frame.players.iter().take(128) {
            let Some(candidate) = candidate_with_direction(frame, *player, direction) else {
                continue;
            };
            if self.target == Some(candidate.id) {
                previous = Some(candidate);
            }
            if best.is_none_or(|current| {
                candidate.alignment > current.alignment
                    || (candidate.alignment == current.alignment && candidate.id < current.id)
            }) {
                best = Some(candidate);
            }
        }
        let Some(best) = best else {
            self.target = None;
            return None;
        };
        // Keep a valid current target unless another is at least two degrees closer.
        let selected = previous
            .filter(|current| {
                current.alignment.acos() <= best.alignment.acos() + TARGET_SWITCH_ADVANTAGE
            })
            .unwrap_or(best);
        self.target = Some(selected.id);

        let seconds = frame.seconds.clamp(0.0, MAX_FRAME_SECONDS);
        if seconds == 0.0 {
            return None;
        }
        let strength = f32::from(config.strength.min(100)) / 100.0;
        let error = rotation_error(frame, selected);
        // Exponential convergence is independent of frame rate. A separate angular
        // speed cap prevents snapping when acquiring a target near the cone edge.
        let alpha = 1.0 - (-strength * 16.0 * seconds).exp();
        let mut yaw = error.yaw * alpha;
        let mut pitch = error.pitch * alpha;
        let movement = yaw.hypot(pitch);
        let limit = (strength * 3.0 * seconds).min(MAX_FRAME_ROTATION);
        if movement > limit {
            let scale = limit / movement;
            yaw *= scale;
            pitch *= scale;
        }
        // Also respect the host's per-axis accumulated-delta budget.
        yaw = yaw.clamp(-MAX_FRAME_ROTATION, MAX_FRAME_ROTATION);
        pitch = pitch.clamp(-MAX_FRAME_ROTATION, MAX_FRAME_ROTATION);
        if yaw.abs() < 1e-7 && pitch.abs() < 1e-7 {
            None
        } else {
            Some(Rotation { yaw, pitch })
        }
    }
}

fn view_direction(frame: &Frame<'_>) -> Vec3 {
    let (yaw_sin, yaw_cos) = frame.yaw.sin_cos();
    let (pitch_sin, pitch_cos) = frame.pitch.sin_cos();
    Vec3 {
        x: -yaw_sin * pitch_cos,
        y: pitch_sin,
        z: -yaw_cos * pitch_cos,
    }
}

#[cfg(test)]
fn candidate(frame: &Frame<'_>, player: Player) -> Option<Candidate> {
    candidate_with_direction(frame, player, view_direction(frame))
}

fn candidate_with_direction(
    frame: &Frame<'_>,
    player: Player,
    direction: Vec3,
) -> Option<Candidate> {
    if player.id == 0 || !player.feet.finite() {
        return None;
    }
    let dx = player.feet.x - frame.eye.x;
    let dy = player.feet.y + CHEST_HEIGHT - frame.eye.y;
    let dz = player.feet.z - frame.eye.z;
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    if !distance.is_finite() || distance <= 0.01 || distance > RANGE {
        return None;
    }
    if dx.abs() + dz.abs() <= 1e-6 {
        return None;
    }
    // True angle between the camera direction and the target direction, rather
    // than a yaw-only cone. Rank by cosine; expensive trig runs only for the
    // selected target, keeping large player lists within the host fuel budget.
    let alignment =
        ((dx * direction.x + dy * direction.y + dz * direction.z) / distance).clamp(-1.0, 1.0);
    (alignment >= CONE_COSINE).then_some(Candidate {
        id: player.id,
        alignment,
        offset: Vec3 {
            x: dx,
            y: dy,
            z: dz,
        },
    })
}

fn rotation_error(frame: &Frame<'_>, selected: Candidate) -> Rotation {
    let Vec3 { x, y, z } = selected.offset;
    let yaw = (-x).atan2(-z);
    let pitch = y.atan2(x.hypot(z)).clamp(-FRAC_PI_2, FRAC_PI_2);
    Rotation {
        yaw: wrap(yaw - frame.yaw),
        pitch: pitch - frame.pitch,
    }
}

fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    use mod_api::bindings::{Guest, cinnabar::extension::gameplay};
    use std::cell::RefCell;

    thread_local! {
        static AIM: RefCell<AimAssist> = RefCell::new(AimAssist::default());
    }

    struct CinnabarAimAssist;

    impl Guest for CinnabarAimAssist {
        fn init() {
            AIM.with(|aim| aim.borrow_mut().reset());
        }

        fn frame() {
            let config = Config::current();
            if !config.enabled || config.strength == 0 {
                AIM.with(|aim| aim.borrow_mut().reset());
                return;
            }
            let Ok(Some(snapshot)) = gameplay::read_frame() else {
                AIM.with(|aim| aim.borrow_mut().reset());
                return;
            };
            let players: Vec<_> = snapshot
                .players
                .iter()
                .take(mod_api::MAX_GAMEPLAY_PLAYERS)
                .map(|player| Player {
                    id: player.runtime_id,
                    feet: Vec3 {
                        x: player.position.x,
                        y: player.position.y,
                        z: player.position.z,
                    },
                })
                .collect();
            let frame = Frame {
                session: snapshot.session,
                dimension: snapshot.dimension,
                eye: Vec3 {
                    x: snapshot.eye.x,
                    y: snapshot.eye.y,
                    z: snapshot.eye.z,
                },
                yaw: snapshot.yaw,
                pitch: snapshot.pitch,
                attack_held: snapshot.attack_held,
                seconds: snapshot.frame_seconds,
                players: &players,
            };
            AIM.with(|aim| {
                let mut aim = aim.borrow_mut();
                if let Some(rotation) = aim.update(config, Some(&frame))
                    && gameplay::rotate(rotation.yaw, rotation.pitch).is_err()
                {
                    aim.reset();
                }
            });
        }
    }

    mod_api::bindings::export!(CinnabarAimAssist with_types_in mod_api::bindings);
}

#[cfg(test)]
mod tests;
