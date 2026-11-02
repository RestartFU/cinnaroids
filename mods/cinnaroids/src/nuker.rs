//! Breaks a spherical area after the manually selected block is broken.

use mod_api::bindings::cinnabar::extension::terrain::{
    Geometry, Position, ReadResult, RegionQuery, Stamp,
};

/// Radius measured from the block the player starts breaking.
pub const MAX_RANGE: u8 = 5;
pub const DEFAULT_RANGE: u8 = 3;

/// Current gameplay and the block under the crosshair before mod requests.
#[derive(Clone, Copy)]
pub struct Frame {
    pub session: u64,
    pub terrain_session: u64,
    pub dimension: i32,
    pub eye: [f32; 3],
    pub block: Option<[i32; 3]>,
    pub seconds: f32,
}

/// Explicit surrounding positions submitted together after the original becomes air.
#[derive(Default, Debug, PartialEq)]
pub struct Action {
    pub blocks: Vec<[i32; 3]>,
}

#[derive(Clone, Copy, PartialEq)]
struct Context {
    session: u64,
    terrain_session: u64,
    dimension: i32,
    center: [i32; 3],
    range: u8,
}

/// Watches one block per physical hold and emits at most one radius batch.
#[derive(Default)]
pub struct Nuker {
    context: Option<Context>,
    stamp: Option<Stamp>,
    completed: bool,
}

impl Nuker {
    /// Releases the anchor on disable, lost input, world changes or a failed request.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Subscribes only to the original block; crosshair changes never move an active anchor.
    pub fn query(&mut self, frame: Frame, range: u8) -> Option<(RegionQuery, Option<Stamp>)> {
        if !frame.eye.iter().all(|v| v.is_finite())
            || !frame.seconds.is_finite()
            || frame.seconds < 0.
        {
            self.reset();
            return None;
        }
        if self.context.is_some_and(|c| {
            c.session != frame.session
                || c.terrain_session != frame.terrain_session
                || c.dimension != frame.dimension
        }) {
            self.reset();
        }
        if self.completed {
            return None;
        }
        if self.context.is_none() {
            let center = frame.block?;
            if center
                .iter()
                .any(|v| v.unsigned_abs() >= (i32::MAX - 16) as u32)
            {
                return None;
            }
            self.context = Some(Context {
                session: frame.session,
                terrain_session: frame.terrain_session,
                dimension: frame.dimension,
                center,
                range: range.clamp(1, MAX_RANGE),
            });
        }
        let context = self.context?;
        Some((
            RegionQuery {
                min: position(context.center),
                max: position(context.center.map(|v| v + 1)),
                geometry: Geometry::empty(),
            },
            self.stamp,
        ))
    }

    /// A known air cell completes the original break; pending and unknown cells wait.
    pub fn update(&mut self, _frame: Frame, _query: &RegionQuery, answer: ReadResult) -> Action {
        let Some(context) = self.context.filter(|_| !self.completed) else {
            return Action::default();
        };
        let region = match answer {
            ReadResult::Pending | ReadResult::Unchanged(_) => return Action::default(),
            ReadResult::Changed(region)
                if region.stamp.session == context.terrain_session
                    && region.stamp.dimension == context.dimension =>
            {
                region
            }
            _ => {
                self.reset();
                return Action::default();
            }
        };
        self.stamp = Some(region.stamp);
        let known = region.loaded.first().is_some_and(|mask| mask & 1 != 0);
        let state = region
            .states
            .first()
            .and_then(|&index| region.palette.get(usize::from(index)));
        if !known
            || region.layer_count == 0
            || state.and_then(|state| state.identifier.as_deref()) != Some("minecraft:air")
        {
            return Action::default();
        }
        self.completed = true;
        let radius = i32::from(context.range);
        let mut blocks = Vec::new();
        for x in -radius..=radius {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    let distance = x * x + y * y + z * z;
                    if distance > 0 && distance <= radius * radius {
                        blocks.push([
                            context.center[0] + x,
                            context.center[1] + y,
                            context.center[2] + z,
                        ]);
                    }
                }
            }
        }
        Action { blocks }
    }
}

/// Converts exact integer world coordinates to a terrain position.
fn position([x, y, z]: [i32; 3]) -> Position {
    Position { x, y, z }
}

#[cfg(test)]
mod tests;
