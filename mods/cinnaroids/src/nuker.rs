//! Nearby mining policy; the host owns ordinary block picks and destroy admission.

use mod_api::bindings::cinnabar::extension::terrain::{
    Geometry, Position, ReadResult, Region, RegionQuery, ResolvedShape, ShapeBinding, Stamp,
};

/// Radius measured from the manually selected block, bounded independently of the host's ordinary block reach.
pub const MAX_RANGE: u8 = 5;
pub const DEFAULT_RANGE: u8 = 3;
/// Small subscriptions bound ordinary terrain import and selection work per callback.
const TILE: i32 = 3;
const CELLS: usize = (TILE * TILE * TILE) as usize;
/// Move on when a target stays unchanged, including server-refused and very slow breaks.
pub const MAX_HOLD_SECONDS: f32 = 30.;

/// Input-owned mining observation; the block under the crosshair seeds each hold.
#[derive(Clone, Copy)]
pub struct Frame {
    pub session: u64,
    pub terrain_session: u64,
    pub dimension: i32,
    pub eye: [f32; 3],
    pub block: Option<[i32; 3]>,
    pub seconds: f32,
}

/// A mining destination; the host still requires physical attack input.
#[derive(Default, Debug, PartialEq)]
pub struct Action {
    pub point: Option<[f32; 3]>,
}

#[derive(Clone, Copy, PartialEq)]
struct Target {
    position: [i32; 3],
    runtime_id: u32,
    point: [f32; 3],
}

#[derive(Clone, Copy, PartialEq)]
struct Context {
    session: u64,
    terrain_session: u64,
    dimension: i32,
    center: [i32; 3],
    range: u8,
}

/// Retains only one small tile's candidates and its currently selected block.
#[derive(Default)]
pub struct Nuker {
    context: Option<Context>,
    offsets: Vec<[i32; 3]>,
    cursor: usize,
    stamp: Option<Stamp>,
    candidates: Vec<Target>,
    target: Option<Target>,
    held_seconds: f32,
}

impl Nuker {
    /// Withdraws targeting on disable, lost input, unavailable terrain or failed requests.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Renews a small subscription around the block selected at the start of this hold.
    pub fn query(&mut self, frame: Frame, range: u8) -> Option<(RegionQuery, Option<Stamp>)> {
        if !frame
            .eye
            .iter()
            .all(|v| v.is_finite() && v.abs() < (i32::MAX - 16) as f32)
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
        let center = self.context.map(|context| context.center).or(frame.block)?;
        if center
            .iter()
            .any(|v| v.unsigned_abs() >= (i32::MAX - 16) as u32)
        {
            self.reset();
            return None;
        }
        let context = Context {
            session: frame.session,
            terrain_session: frame.terrain_session,
            dimension: frame.dimension,
            center,
            range: range.clamp(1, MAX_RANGE),
        };
        if self.context != Some(context) {
            self.reset();
            self.context = Some(context);
            let extent = (i32::from(context.range) + TILE - 1) / TILE;
            for x in -extent..=extent {
                for y in -extent..=extent {
                    for z in -extent..=extent {
                        self.offsets.push([x, y, z]);
                    }
                }
            }
            self.offsets
                .sort_unstable_by_key(|p| (p.iter().map(|v| v * v).sum::<i32>(), *p));
        }
        let offset = self.offsets[self.cursor];
        let min = std::array::from_fn(|i| context.center[i] + offset[i] * TILE - 1);
        Some((
            RegionQuery {
                min: position(min),
                max: position(min.map(|v| v + TILE)),
                geometry: Geometry::SELECTION,
            },
            self.stamp,
        ))
    }

    /// Accepts a host answer only for the renewed query; pending answers pause all output.
    pub fn update(&mut self, frame: Frame, query: &RegionQuery, answer: ReadResult) -> Action {
        let Some(context) = self.context else {
            return Action::default();
        };
        match answer {
            ReadResult::Pending => {
                return Action::default();
            }
            ReadResult::Unchanged(stamp) if self.stamp.map(stamp_key) == Some(stamp_key(stamp)) => {
            }
            ReadResult::Changed(region)
                if (region.stamp.session, region.stamp.dimension)
                    == (context.terrain_session, context.dimension) =>
            {
                self.candidates = candidates(query, &region);
                self.stamp = Some(region.stamp);
            }
            _ => {
                self.reset();
                return Action::default();
            }
        }
        let distance = |target: &Target| {
            target
                .position
                .iter()
                .zip(context.center)
                .map(|(a, b)| (*a as f32 - b as f32).powi(2))
                .sum::<f32>()
        };
        let eligible = |target: &&Target| distance(target) <= f32::from(context.range).powi(2);
        let selected = self
            .target
            .filter(|previous| {
                self.candidates
                    .iter()
                    .filter(eligible)
                    .any(|t| t == previous)
            })
            .or_else(|| {
                self.candidates
                    .iter()
                    .filter(eligible)
                    .min_by(|a, b| {
                        distance(a)
                            .total_cmp(&distance(b))
                            .then_with(|| a.position.cmp(&b.position))
                    })
                    .copied()
            });
        if selected != self.target {
            self.target = selected;
            self.held_seconds = 0.;
        }
        let Some(target) = selected else {
            self.advance();
            return Action::default();
        };
        self.held_seconds += frame.seconds.min(1.);
        if self.held_seconds >= MAX_HOLD_SECONDS {
            self.advance();
            return Action::default();
        }
        Action {
            point: Some(target.point),
        }
    }

    /// Visits the next tile without carrying its predecessor's terrain or mining state.
    fn advance(&mut self) {
        self.cursor = (self.cursor + 1) % self.offsets.len();
        self.stamp = None;
        self.candidates.clear();
        self.target = None;
        self.held_seconds = 0.;
    }
}

/// Converts the policy's exact integer coordinates to the SDK's record.
fn position([x, y, z]: [i32; 3]) -> Position {
    Position { x, y, z }
}

/// Reads only known base-layer cells with a resolved, nonempty selection shape.
fn candidates(query: &RegionQuery, region: &Region) -> Vec<Target> {
    let mut targets = Vec::new();
    if region.layer_count == 0 || region.states.len() < CELLS {
        return targets;
    }
    for cell in 0..CELLS {
        if region
            .loaded
            .get(cell / 64)
            .is_none_or(|mask| mask & (1 << (cell % 64)) == 0)
        {
            continue;
        }
        let Some(state) = region.palette.get(usize::from(region.states[cell])) else {
            continue;
        };
        if state
            .identifier
            .as_deref()
            .is_none_or(|id| id == "minecraft:air")
        {
            continue;
        }
        let shape = match &state.selection {
            ShapeBinding::Fixed(shape) => Some(*shape),
            ShapeBinding::PerCell => region
                .overrides
                .binary_search_by_key(&(cell as u32), |o| o.slot)
                .ok()
                .and_then(|index| region.overrides[index].selection),
            ShapeBinding::NotRequested => None,
        };
        let Some(ResolvedShape::Known(shape)) = shape else {
            continue;
        };
        let shape = shape as usize;
        let Some(next) = shape.checked_add(1) else {
            continue;
        };
        let Some((&start, &end)) = region
            .shape_offsets
            .get(shape)
            .zip(region.shape_offsets.get(next))
        else {
            continue;
        };
        if start >= end {
            continue;
        }
        let Some(first) = (start as usize).checked_mul(6) else {
            continue;
        };
        let Some(last) = first.checked_add(6) else {
            continue;
        };
        let Some(bounds) = region.shape_boxes.get(first..last) else {
            continue;
        };
        if !(0..3).all(|i| {
            bounds[i].is_finite() && bounds[i + 3].is_finite() && bounds[i] < bounds[i + 3]
        }) {
            continue;
        }
        let position = [
            query.min.x + (cell / (TILE * TILE) as usize) as i32,
            query.min.y + (cell % TILE as usize) as i32,
            query.min.z + ((cell / TILE as usize) % TILE as usize) as i32,
        ];
        let point = std::array::from_fn(|i| {
            position[i] as f32 + ((bounds[i] + bounds[i + 3]) * 0.5) as f32
        });
        if point.iter().all(|v| v.is_finite()) {
            targets.push(Target {
                position,
                runtime_id: state.runtime_id,
                point,
            });
        }
    }
    targets
}

/// Compares every member of a terrain stamp without depending on generated equality derives.
fn stamp_key(stamp: Stamp) -> (u64, i32, u64, u64) {
    (
        stamp.session,
        stamp.dimension,
        stamp.registry,
        stamp.revision,
    )
}

#[cfg(test)]
mod tests;
