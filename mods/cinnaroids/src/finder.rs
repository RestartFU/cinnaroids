//! Netherite Finder: the nearest loaded matching blocks, drawn as solid cubes through walls.
//!
//! Cinnabar reports positions through `terrain.find-blocks`; Cinnaroids owns the selection,
//! colour and geometry. Each cube is a retained `world-draw` instance of one shared unit cube,
//! because frame groups last a single callback. A changed selection moves, adds or removes
//! cubes a bounded number per frame, within the callback's drawing calls and fuel.

use mod_api::bindings::cinnabar::{
    extension::terrain::Position,
    world_draw::drawing::{
        Anchor, Depth, DrawError, GeometrySpec, Handle, Instance, Mesh, Vertex, WorldAnchor,
    },
};

/// Blocks the finder searches for.
pub const IDENTIFIERS: [&str; 2] = ["minecraft:ancient_debris", "minecraft:netherite_block"];
/// Linear RGBA of every highlighted cube, matching the earlier host highlight.
pub const COLOR: [f32; 4] = [1.0, 0.12, 0.55, 1.0];
/// Largest range setting: terrain searches reach this far from the player's feet.
pub const MAX_RANGE: f32 = mod_api::MAX_TERRAIN_DISTANCE as f32;
/// Most cubes drawn at once. Cinnabar can return 1024 blocks; comparing an answer with the
/// drawn cubes costs fuel for every block, so the finder keeps the nearest 256.
pub const LIMIT: usize = 256;
const _: () = assert!(LIMIT <= mod_api::MAX_FOUND_BLOCKS);
const _: () = assert!(LIMIT + crate::ghost::INSTANCES <= mod_api::MAX_DRAW_INSTANCES);
/// Cube placements added, moved or removed per callback; each is one drawing call.
pub const SYNC_CALLS: usize = 16;
/// Maximum comparison units per callback; fuel is checked before every position.
pub const COMPARE_BLOCKS: usize = 64;
/// Seconds between applying search answers; a changed search applies its first answer at once.
pub const REFRESH_SECONDS: f32 = 0.25;

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

/// Lossless ordered identity for all host position coordinates, sorted by x, z, then y.
pub fn key(position: &Position) -> [i32; 3] {
    [position.x, position.z, position.y]
}

/// The block an ordered key identifies.
pub fn position(key: [i32; 3]) -> Position {
    Position {
        x: key[0],
        y: key[2],
        z: key[1],
    }
}

/// A pink cube at a block, through walls when that authority is granted.
pub fn cube_instance(
    geometry: Handle,
    dimension: i32,
    position: Position,
    through_walls: bool,
) -> Instance {
    let mut instance = mod_api::drawing::instance(geometry, anchor(dimension, position));
    instance.tint = COLOR.into();
    instance.depth = if through_walls {
        Depth::Always
    } else {
        Depth::Test
    };
    instance
}

/// A block's minimum corner in the world.
pub fn anchor(dimension: i32, position: Position) -> Anchor {
    Anchor::World(WorldAnchor {
        dimension,
        origin: (
            f64::from(position.x),
            f64::from(position.y),
            f64::from(position.z),
        ),
    })
}

/// One drawing call that brings the retained cubes closer to the selection.
#[derive(Clone, Copy, Debug)]
pub enum Change {
    /// Places a new cube; the call returns its handle.
    Create(Position),
    /// Moves an unneeded cube to a needed block.
    Move(Handle, Position),
    Remove(Handle),
}

/// Sorted retained cubes: at most nine key comparisons per lookup, with no hash buckets.
/// Insert/remove shifts at most LIMIT fixed-size entries, inside one fuel-gated sync unit.
struct Cubes(Vec<([i32; 3], Handle, bool)>);
impl Default for Cubes {
    /// Allocates the bounded index during initialization, avoiding growth during sync.
    fn default() -> Self {
        Self(Vec::with_capacity(LIMIT))
    }
}
impl Cubes {
    /// Searches a fixed-size ordered index independently of coordinate distribution.
    fn find(&self, key: [i32; 3]) -> Result<usize, usize> {
        self.0.binary_search_by_key(&key, |entry| entry.0)
    }
    /// Inserts at the binary-search position; capacity never exceeds LIMIT.
    fn insert(&mut self, key: [i32; 3], handle: Handle) {
        let index = self.find(key).unwrap_err();
        self.0.insert(index, (key, handle, false));
    }
    /// Removes one known key, shifting at most LIMIT entries.
    fn remove(&mut self, key: [i32; 3]) {
        self.0.remove(self.find(key).expect("retained cube"));
    }
    /// The retained handle for one known key.
    fn handle(&self, key: [i32; 3]) -> Handle {
        self.0[self.find(key).expect("retained cube")].1
    }
}

/// Search cadence, the selection to show and the retained cubes showing it.
pub struct Finder {
    /// The uploaded cube; dropped after a drawing error so it is recreated.
    pub geometry: Option<Handle>,
    elapsed: f32,
    /// Session, dimension, range and depth authority of the current search.
    context: Option<(u64, i32, u8, bool)>,
    /// The dimension and depth authority the selection was found with.
    placement: (i32, bool),
    cubes: Cubes,
    clearing: bool,
    /// Selected blocks without a cube, nearest last, and cubes on blocks no longer selected,
    /// by key.
    needed: Vec<[i32; 3]>,
    spare: Vec<[i32; 3]>,
    /// The answer being compared with the cubes and how far, with the blocks it needs so far
    /// and the table slots it keeps.
    answer: Vec<Position>,
    compared: Option<usize>,
    partition: usize,
    found: Vec<[i32; 3]>,
    /// The last answer compared in full; an identical answer needs nothing.
    last: Vec<Position>,
    /// Whether a denied search was reported since the module was last disabled.
    notified: bool,
}

impl Default for Finder {
    /// Allocates every bounded queue before frame callbacks begin.
    fn default() -> Self {
        Self {
            geometry: None,
            elapsed: 0.,
            context: None,
            placement: (0, false),
            cubes: Cubes::default(),
            clearing: false,
            needed: Vec::with_capacity(LIMIT),
            spare: Vec::with_capacity(LIMIT),
            answer: Vec::with_capacity(LIMIT),
            compared: None,
            partition: 0,
            found: Vec::with_capacity(LIMIT),
            last: Vec::with_capacity(LIMIT),
            notified: false,
        }
    }
}

impl Finder {
    /// Whether to apply this frame's search answer: on the frame after a context or setting
    /// change, then on a short cadence. The search is renewed every frame regardless, since one
    /// left out of a callback is unsubscribed. A changed search answers nothing until the client
    /// has scanned for it, so the frame that starts it keeps the cubes already drawn.
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
            self.elapsed = REFRESH_SECONDS;
            return false;
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

    /// Keeps a due answer for the next frame, for a frame with too little fuel left.
    #[cfg(test)]
    pub fn postpone(&mut self) {
        self.elapsed = self.elapsed.max(REFRESH_SECONDS);
    }

    /// Takes a search answer, nearest first, as the selection: the nearest [`LIMIT`] blocks.
    /// Returns whether it differs from the last; [`Finder::compare`] then finds what changed
    /// over the next frames. The cubes stay as they are until it has.
    pub fn apply(&mut self, dimension: i32, through_walls: bool, positions: &[Position]) -> bool {
        let positions = &positions[..positions.len().min(LIMIT)];
        if self.compared.is_none() && (dimension, through_walls) == self.placement && {
            let same = |a: &Position, b: &Position| (a.x, a.y, a.z) == (b.x, b.y, b.z);
            positions.len() == self.last.len()
                && positions.iter().zip(&self.last).all(|(a, b)| same(a, b))
        } {
            return false;
        }
        self.clearing = false;
        self.placement = (dimension, through_walls);
        self.answer.clear();
        self.answer.extend_from_slice(positions);
        self.compared = Some(0);
        self.partition = 0;
        self.spare.clear();
        self.found.clear();
        for (_, _, kept) in &mut self.cubes.0 {
            *kept = false;
        }
        true
    }

    /// Whether an answer is being compared with the cubes.
    pub fn comparing(&self) -> bool {
        self.compared.is_some()
    }

    /// Advances up to `blocks` bounded units: an answer lookup or one retained-index slot.
    /// Partitioning also resumes across calls; syncing waits until both phases finish.
    pub fn compare(&mut self, blocks: usize) {
        let Some(start) = self.compared else {
            return;
        };
        let mut cursor = start;
        for _ in 0..blocks {
            if cursor < self.answer.len() {
                let position = &self.answer[cursor];
                let limit = mod_api::MAX_PRIMITIVE_COORDINATE as i64;
                if [position.x, position.y, position.z]
                    .iter()
                    .any(|coordinate| i64::from(*coordinate).abs() > limit)
                {
                    // Terrain coordinates span s32; drawing anchors have a narrower contract.
                    cursor += 1;
                    continue;
                }
                let key = key(position);
                match self.cubes.find(key) {
                    Ok(slot) => self.cubes.0[slot].2 = true,
                    Err(_) => self.found.push(key),
                }
                cursor += 1;
                continue;
            }
            // The index stays fixed while comparing; partition one retained slot per unit.
            if let Some((key, _, kept)) = self.cubes.0.get(self.partition) {
                self.partition += 1;
                if !kept {
                    self.spare.push(*key);
                }
            } else {
                self.compared = None;
                // Pop nearest first; this copy is bounded by LIMIT, independent of lookups.
                self.needed.clear();
                std::mem::swap(&mut self.needed, &mut self.found);
                self.needed.reverse();
                std::mem::swap(&mut self.last, &mut self.answer);
                return;
            }
        }
        self.compared = Some(cursor);
    }

    /// Stops searching and keeps the cubes, as while a menu owns input; the next search starts
    /// over without dropping them.
    pub fn pause(&mut self) {
        self.context = None;
        self.elapsed = 0.0;
    }

    /// Stops searching and empties the selection, so syncing removes every cube.
    pub fn clear(&mut self) {
        self.pause();
        self.compared = None;
        self.last.clear();
        self.needed.clear();
        self.spare.clear();
        self.clearing = !self.cubes.0.is_empty();
    }

    /// Whether the retained cubes show the selection.
    pub fn synced(&self) -> bool {
        self.compared.is_none() && self.needed.is_empty() && self.spare.is_empty() && !self.clearing
    }

    /// The number of retained cubes.
    pub fn cubes(&self) -> usize {
        self.cubes.0.len()
    }

    /// Whether syncing will draw a cube, which needs the uploaded geometry.
    pub fn places(&self) -> bool {
        !self.needed.is_empty()
    }

    /// The dimension and depth authority the selection was found with.
    pub fn placement(&self) -> (i32, bool) {
        self.placement
    }

    /// Makes up to `calls` changes toward the selection through `draw`, which performs one
    /// drawing call and returns the created handle for [`Change::Create`]. Cubes on blocks no
    /// longer selected move to newly selected ones before cubes are added or removed. Stops at
    /// the first error, keeping every change made so far.
    pub fn sync(
        &mut self,
        calls: usize,
        mut draw: impl FnMut(Change) -> Result<Option<Handle>, DrawError>,
    ) -> Result<(), DrawError> {
        if self.comparing() {
            return Ok(());
        }
        let mut made = 0;
        while made < calls {
            if self.clearing {
                if let Some((_, handle, _)) = self.cubes.0.last() {
                    draw(Change::Remove(*handle))?;
                    self.cubes.0.pop();
                    made += 1;
                }
                self.clearing = !self.cubes.0.is_empty();
                if self.clearing {
                    continue;
                }
            }
            match (self.needed.last().copied(), self.spare.last().copied()) {
                (Some(wanted), _) if self.cubes.find(wanted).is_ok() => {
                    // A duplicate answer consumes a bounded unit, too.
                    self.needed.pop();
                    made += 1;
                }
                (Some(wanted), Some(unused)) => {
                    let handle = self.cubes.handle(unused);
                    draw(Change::Move(handle, position(wanted)))?;
                    made += 1;
                    self.cubes.remove(unused);
                    self.cubes.insert(wanted, handle);
                    self.needed.pop();
                    self.spare.pop();
                }
                (Some(wanted), None) => {
                    let handle = draw(Change::Create(position(wanted)))?;
                    made += 1;
                    if let Some(handle) = handle {
                        self.cubes.insert(wanted, handle);
                    }
                    self.needed.pop();
                }
                (None, Some(unused)) => {
                    draw(Change::Remove(self.cubes.handle(unused)))?;
                    made += 1;
                    self.cubes.remove(unused);
                    self.spare.pop();
                }
                (None, None) => break,
            }
        }
        Ok(())
    }

    /// Drops all retained state after a lost drawing token so the next search starts over.
    pub fn reset(&mut self) {
        self.geometry = None;
        self.elapsed = 0.;
        self.context = None;
        self.placement = (0, false);
        self.cubes.0.clear();
        self.clearing = false;
        self.needed.clear();
        self.spare.clear();
        self.answer.clear();
        self.compared = None;
        self.partition = 0;
        self.found.clear();
        self.last.clear();
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
