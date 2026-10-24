//! Netherite Finder: the nearest loaded matching blocks, drawn as solid cubes through walls.
//!
//! Cinnabar reports positions through `terrain.find-blocks`; Cinnaroids owns the selection,
//! colour and geometry. One retained unit cube is placed once per found block, so a changed
//! selection resends only instance anchors.

use mod_api::bindings::cinnabar::{
    extension::terrain::Position,
    world_draw::drawing::{
        Anchor, Depth, GeometrySpec, Handle, Instance, Mesh, Vertex, WorldAnchor,
    },
};

/// Blocks the finder searches for.
pub const IDENTIFIERS: [&str; 2] = ["minecraft:ancient_debris", "minecraft:netherite_block"];
/// Linear RGBA of every highlighted cube, matching the earlier host highlight.
pub const COLOR: [f32; 4] = [1.0, 0.12, 0.55, 1.0];
/// Largest range setting: terrain searches reach this far from the player's feet.
pub const MAX_RANGE: f32 = mod_api::MAX_TERRAIN_DISTANCE as f32;
/// Most cubes drawn at once, leaving retained instances for the real-position marker.
pub const LIMIT: usize =
    if mod_api::MAX_FOUND_BLOCKS < mod_api::MAX_DRAW_INSTANCES - crate::ghost::INSTANCES {
        mod_api::MAX_FOUND_BLOCKS
    } else {
        mod_api::MAX_DRAW_INSTANCES - crate::ghost::INSTANCES
    };
const _: () = assert!(LIMIT <= mod_api::MAX_FOUND_BLOCKS);
const _: () = assert!(LIMIT + crate::ghost::INSTANCES <= mod_api::MAX_DRAW_INSTANCES);
/// Seconds between searches while the selection is unchanged; settings changes search at once.
pub const REFRESH_SECONDS: f32 = 0.25;
/// Retained drawing group holding the cubes.
pub const GROUP: &str = "netherite";

/// A unit cube from the block's minimum corner, white so the instance tint sets its colour.
pub fn cube() -> GeometrySpec {
    let vertices = (0..8)
        .map(|corner| Vertex {
            position: (
                (corner & 1) as f32,
                ((corner >> 1) & 1) as f32,
                ((corner >> 2) & 1) as f32,
            ),
            color: (1., 1., 1., 1.),
            uv: (0., 0.),
            data: (0., 0., 0., 0.),
        })
        .collect();
    // Corner bits are x, y and z; each face is two triangles.
    let indices = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ]
    .into_iter()
    .flat_map(|[a, b, c, d]| [a, b, c, a, c, d])
    .collect();
    GeometrySpec::Triangles(Mesh { vertices, indices })
}

/// Order-independent identity of a found selection; nearest-first order changes as the
/// player moves without changing what is drawn.
pub fn signature(positions: &[Position]) -> (usize, u64) {
    let sum = positions.iter().fold(0u64, |sum, position| {
        let mut value = (position.x as u32 as u64)
            ^ ((position.y as u32 as u64) << 21)
            ^ ((position.z as u32 as u64) << 42);
        // SplitMix64 finaliser spreads neighbouring coordinates across the word.
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        sum.wrapping_add(value ^ (value >> 31))
    });
    (positions.len(), sum)
}

/// Places one pink cube per position, through walls when that authority is granted.
/// `out` keeps its entries between searches, so a changed selection rewrites anchors in place
/// instead of allocating every instance again within the callback's fuel.
pub fn place(
    out: &mut Vec<Instance>,
    geometry: Handle,
    dimension: i32,
    positions: &[Position],
    through_walls: bool,
) {
    let positions = &positions[..positions.len().min(LIMIT)];
    out.truncate(positions.len());
    let depth = if through_walls {
        Depth::Always
    } else {
        Depth::Test
    };
    for (index, position) in positions.iter().enumerate() {
        let anchor = Anchor::World(WorldAnchor {
            dimension,
            origin: (
                f64::from(position.x),
                f64::from(position.y),
                f64::from(position.z),
            ),
        });
        match out.get_mut(index) {
            Some(instance) => {
                instance.geometry = geometry;
                instance.anchor = anchor;
                instance.depth = depth;
            }
            None => {
                let mut instance = mod_api::drawing::instance(geometry, anchor);
                instance.tint = COLOR.into();
                instance.depth = depth;
                out.push(instance);
            }
        }
    }
}

/// Search cadence and the retained cubes currently drawn.
#[derive(Default)]
pub struct Finder {
    /// The uploaded cube; dropped after a drawing error so it is recreated.
    pub geometry: Option<Handle>,
    elapsed: f32,
    /// Session, dimension, range and depth authority of the last search.
    context: Option<(u64, i32, u8, bool)>,
    /// What the drawn group holds; none when it is empty.
    drawn: Option<(usize, u64)>,
    /// Whether a denied search was reported since the module was last disabled.
    notified: bool,
    /// Reused cube placements; see [`place`].
    pub instances: Vec<Instance>,
}

impl Finder {
    /// Whether to search this frame: at once after a context or setting change, then on a
    /// short cadence.
    pub fn due(
        &mut self,
        session: u64,
        dimension: i32,
        range: u8,
        through_walls: bool,
        seconds: f32,
    ) -> bool {
        let context = Some((session, dimension, range, through_walls));
        if self.context != context {
            self.context = context;
            self.elapsed = 0.0;
            return true;
        }
        if seconds.is_finite() && seconds > 0.0 {
            self.elapsed += seconds;
        }
        if self.elapsed >= REFRESH_SECONDS {
            self.elapsed = 0.0;
            return true;
        }
        false
    }

    /// Accepts a search result and returns whether the drawn group must be replaced.
    pub fn changed(&mut self, positions: &[Position]) -> bool {
        let positions = &positions[..positions.len().min(LIMIT)];
        let next = (!positions.is_empty()).then(|| signature(positions));
        std::mem::replace(&mut self.drawn, next) != next
    }

    /// Forgets the drawn group and search context; returns whether a non-empty group was shown.
    pub fn hide(&mut self) -> bool {
        self.context = None;
        self.elapsed = 0.0;
        self.drawn.take().is_some()
    }

    /// Drops all retained state after a lost geometry token so the next search starts over.
    pub fn reset(&mut self) {
        let notified = self.notified;
        *self = Self {
            notified,
            ..Self::default()
        };
    }

    /// Whether to report a denied search: once until the module is disabled again.
    pub fn notify_denied(&mut self) -> bool {
        !std::mem::replace(&mut self.notified, true)
    }

    /// Re-arms the denied-search report once the module is disabled.
    pub fn clear_notice(&mut self) {
        self.notified = false;
    }
}

#[cfg(test)]
mod tests;
