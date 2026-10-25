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
/// Answer blocks compared with the cubes per callback; a full answer takes four frames.
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

/// Packs a block position into one sortable word: x and z in 26 bits each, y in 12, each
/// offset to be non-negative. Single-word compares keep sorting within callback fuel.
pub fn key(position: &Position) -> u64 {
    (((position.x as i64 + HORIZONTAL_OFFSET) as u64) << 38)
        | (((position.z as i64 + HORIZONTAL_OFFSET) as u64 & HORIZONTAL_MASK) << 12)
        | ((position.y as i64 + VERTICAL_OFFSET) as u64 & VERTICAL_MASK)
}

/// The block a [`key`] packs.
pub fn position(key: u64) -> Position {
    Position {
        x: ((key >> 38) as i64 - HORIZONTAL_OFFSET) as i32,
        y: ((key & VERTICAL_MASK) as i64 - VERTICAL_OFFSET) as i32,
        z: (((key >> 12) & HORIZONTAL_MASK) as i64 - HORIZONTAL_OFFSET) as i32,
    }
}

const HORIZONTAL_OFFSET: i64 = 1 << 25;
const HORIZONTAL_MASK: u64 = (1 << 26) - 1;
const VERTICAL_OFFSET: i64 = 1 << 11;
const VERTICAL_MASK: u64 = (1 << 12) - 1;

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

/// Retained cubes by block [`key`]: open addressing with linear probing, at most half full, so
/// finding, adding and removing a cube take a few probes. The table is allocated with the first
/// cube.
#[derive(Default)]
struct Cubes {
    keys: Vec<u64>,
    handles: Vec<Handle>,
    len: usize,
}

/// Table slots; twice the most cubes.
const SLOTS: usize = 2 * LIMIT;
const _: () = assert!(SLOTS.is_power_of_two());
/// A free slot; no reachable block packs to it.
const EMPTY: u64 = u64::MAX;

impl Cubes {
    /// The slot a key's probe starts at.
    fn home(key: u64) -> usize {
        (key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (64 - SLOTS.trailing_zeros())) as usize
    }

    /// The slot holding `key`.
    fn find(&self, key: u64) -> Option<usize> {
        if self.keys.is_empty() {
            return None;
        }
        let mut slot = Self::home(key);
        loop {
            match self.keys[slot] {
                found if found == key => return Some(slot),
                EMPTY => return None,
                _ => slot = (slot + 1) % SLOTS,
            }
        }
    }

    fn insert(&mut self, key: u64, handle: Handle) {
        if self.keys.is_empty() {
            self.keys = vec![EMPTY; SLOTS];
            self.handles = vec![handle; SLOTS];
        }
        let mut slot = Self::home(key);
        while self.keys[slot] != EMPTY {
            slot = (slot + 1) % SLOTS;
        }
        self.keys[slot] = key;
        self.handles[slot] = handle;
        self.len += 1;
    }

    /// Frees `slot`, shifting later entries of its probe run back so lookups still find them.
    fn remove(&mut self, mut slot: usize) {
        let mut next = slot;
        loop {
            next = (next + 1) % SLOTS;
            let key = self.keys[next];
            if key == EMPTY {
                break;
            }
            // Move the entry back unless its home lies cyclically within (slot, next].
            let home = Self::home(key);
            let stays = if slot <= next {
                slot < home && home <= next
            } else {
                slot < home || home <= next
            };
            if !stays {
                self.keys[slot] = key;
                self.handles[slot] = self.handles[next];
                slot = next;
            }
        }
        self.keys[slot] = EMPTY;
        self.len -= 1;
    }
}

/// Search cadence, the selection to show and the retained cubes showing it.
#[derive(Default)]
pub struct Finder {
    /// The uploaded cube; dropped after a drawing error so it is recreated.
    pub geometry: Option<Handle>,
    elapsed: f32,
    /// Session, dimension, range and depth authority of the current search.
    context: Option<(u64, i32, u8, bool)>,
    /// The dimension and depth authority the selection was found with.
    placement: (i32, bool),
    cubes: Cubes,
    /// Selected blocks without a cube, nearest last, and cubes on blocks no longer selected,
    /// by key.
    needed: Vec<u64>,
    spare: Vec<u64>,
    /// The answer being compared with the cubes and how far, with the blocks it needs so far
    /// and the table slots it keeps.
    answer: Vec<Position>,
    compared: Option<usize>,
    found: Vec<u64>,
    kept: Vec<bool>,
    /// The last answer compared in full; an identical answer needs nothing.
    last: Vec<Position>,
    /// Whether a denied search was reported since the module was last disabled.
    notified: bool,
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
        self.placement = (dimension, through_walls);
        self.answer.clear();
        self.answer.extend_from_slice(positions);
        self.compared = Some(0);
        self.found.clear();
        self.kept.clear();
        self.kept.resize(SLOTS, false);
        true
    }

    /// Whether an answer is being compared with the cubes.
    pub fn comparing(&self) -> bool {
        self.compared.is_some()
    }

    /// Compares up to `blocks` more answer blocks with the cubes. After the last, cubes on
    /// blocks the answer left out become spare, and syncing can start. The table does not
    /// change meanwhile, because syncing waits for the comparison.
    pub fn compare(&mut self, blocks: usize) {
        let Some(start) = self.compared else {
            return;
        };
        let end = self.answer.len().min(start + blocks);
        for position in &self.answer[start..end] {
            let key = key(position);
            match self.cubes.find(key) {
                Some(slot) => self.kept[slot] = true,
                None => self.found.push(key),
            }
        }
        if end < self.answer.len() {
            self.compared = Some(end);
            return;
        }
        self.compared = None;
        // Pop the nearest needed block first.
        self.needed.clear();
        self.needed.extend(self.found.drain(..).rev());
        self.spare.clear();
        for (key, kept) in self.cubes.keys.iter().zip(&self.kept) {
            if *key != EMPTY && !kept {
                self.spare.push(*key);
            }
        }
        std::mem::swap(&mut self.last, &mut self.answer);
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
        // Once every cube is spare, removing one also takes it off the list.
        if self.spare.len() != self.cubes.len {
            self.spare.clear();
            self.spare
                .extend(self.cubes.keys.iter().copied().filter(|key| *key != EMPTY));
        }
    }

    /// Whether the retained cubes show the selection.
    pub fn synced(&self) -> bool {
        self.compared.is_none() && self.needed.is_empty() && self.spare.is_empty()
    }

    /// The number of retained cubes.
    pub fn cubes(&self) -> usize {
        self.cubes.len
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
            match (self.needed.last().copied(), self.spare.last().copied()) {
                (Some(wanted), _) if self.cubes.find(wanted).is_some() => {
                    // Answers list each block once; never draw one twice.
                    self.needed.pop();
                }
                (Some(wanted), Some(unused)) => {
                    let slot = self.cubes.find(unused).expect("spare cubes are retained");
                    let handle = self.cubes.handles[slot];
                    draw(Change::Move(handle, position(wanted)))?;
                    made += 1;
                    self.cubes.remove(slot);
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
                    let slot = self.cubes.find(unused).expect("spare cubes are retained");
                    draw(Change::Remove(self.cubes.handles[slot]))?;
                    made += 1;
                    self.cubes.remove(slot);
                    self.spare.pop();
                }
                (None, None) => break,
            }
        }
        Ok(())
    }

    /// Drops all retained state after a lost drawing token so the next search starts over.
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
