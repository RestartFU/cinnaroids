//! One ordinary ground jump per recent server motion event, timed in simulation ticks.

pub const MIN_WINDOW_TICKS: u8 = 1;
pub const MAX_WINDOW_TICKS: u8 = 10;
pub const DEFAULT_WINDOW_TICKS: u8 = 4;

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub session: u64,
    pub dimension: i32,
    pub tick: u64,
    pub knockback_sequence: u64,
    pub on_ground: bool,
    pub jump_held: bool,
    pub eligible: bool,
    pub velocity: [f32; 3],
}

#[derive(Debug, Default)]
pub struct JumpReset {
    context: Option<(u64, i32)>,
    last_tick: u64,
    last_sequence: u64,
    armed_at: Option<u64>,
}

impl JumpReset {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, enabled: bool, window_ticks: u8, frame: Option<Frame>) -> bool {
        let Some(frame) = frame else {
            self.reset();
            return false;
        };
        if !enabled || !frame.eligible || !frame.velocity.iter().all(|v| v.is_finite()) {
            self.reset();
            return false;
        }
        let context = (frame.session, frame.dimension);
        if self.context != Some(context) || frame.tick < self.last_tick {
            self.context = Some(context);
            self.last_tick = frame.tick;
            self.last_sequence = frame.knockback_sequence;
            self.armed_at = None;
            return false;
        }
        self.last_tick = frame.tick;
        if frame.knockback_sequence != self.last_sequence {
            self.last_sequence = frame.knockback_sequence;
            self.armed_at = (frame.knockback_sequence != 0).then_some(frame.tick);
        }
        let Some(hit_tick) = self.armed_at else {
            return false;
        };
        let window = window_ticks.clamp(MIN_WINDOW_TICKS, MAX_WINDOW_TICKS);
        if frame.tick - hit_tick >= u64::from(window) || frame.jump_held {
            self.armed_at = None;
            return false;
        }
        // Airborne jump presses cannot initiate a normal ground jump.
        if !frame.on_ground {
            return false;
        }
        self.armed_at = None;
        true
    }
}

#[cfg(test)]
mod tests;
