//! Ore ESP: Netherite Finder's retained cubes for a chosen set of ores.
//!
//! One `terrain.find-blocks` search matches at most eight identifiers, and searches share the
//! component's terrain queries with Netherite Finder and Nuker. Ore ESP therefore searches only
//! the selected ores that generate in the player's dimension, Nether ores in the Nether and the
//! others elsewhere, in at most [`MAX_SEARCHES`] searches. Their answers merge nearest first.

use mod_api::bindings::cinnabar::extension::terrain::Position;

/// One selectable ore and the block identifiers of all its variants.
pub struct Ore {
    /// Panel control ID.
    pub id: &'static str,
    pub label: &'static str,
    /// Whether the ore generates in the Nether rather than elsewhere.
    pub nether: bool,
    pub blocks: &'static [&'static str],
}

/// Selectable ores in panel order. Redstone ore briefly becomes a separate lit block after it
/// is touched; that variant is not searched, so the eight non-Nether ores fit two searches.
pub const ORES: [Ore; 10] = [
    Ore {
        id: "ore_diamond",
        label: "Diamond",
        nether: false,
        blocks: &["minecraft:diamond_ore", "minecraft:deepslate_diamond_ore"],
    },
    Ore {
        id: "ore_emerald",
        label: "Emerald",
        nether: false,
        blocks: &["minecraft:emerald_ore", "minecraft:deepslate_emerald_ore"],
    },
    Ore {
        id: "ore_gold",
        label: "Gold",
        nether: false,
        blocks: &["minecraft:gold_ore", "minecraft:deepslate_gold_ore"],
    },
    Ore {
        id: "ore_iron",
        label: "Iron",
        nether: false,
        blocks: &["minecraft:iron_ore", "minecraft:deepslate_iron_ore"],
    },
    Ore {
        id: "ore_redstone",
        label: "Redstone",
        nether: false,
        blocks: &["minecraft:redstone_ore", "minecraft:deepslate_redstone_ore"],
    },
    Ore {
        id: "ore_lapis",
        label: "Lapis lazuli",
        nether: false,
        blocks: &["minecraft:lapis_ore", "minecraft:deepslate_lapis_ore"],
    },
    Ore {
        id: "ore_copper",
        label: "Copper",
        nether: false,
        blocks: &["minecraft:copper_ore", "minecraft:deepslate_copper_ore"],
    },
    Ore {
        id: "ore_coal",
        label: "Coal",
        nether: false,
        blocks: &["minecraft:coal_ore", "minecraft:deepslate_coal_ore"],
    },
    Ore {
        id: "ore_nether_gold",
        label: "Nether gold",
        nether: true,
        blocks: &["minecraft:nether_gold_ore"],
    },
    Ore {
        id: "ore_quartz",
        label: "Nether quartz",
        nether: true,
        blocks: &["minecraft:quartz_ore"],
    },
];
/// Ores selected until the player changes them: diamond and emerald.
pub const DEFAULT: [bool; ORES.len()] = {
    let mut selected = [false; ORES.len()];
    selected[0] = true;
    selected[1] = true;
    selected
};
/// Linear RGBA of every ore cube: cyan, distinct from Netherite Finder's pink.
pub const COLOR: [f32; 4] = [0.1, 0.8, 1.0, 1.0];
/// Most searches Ore ESP keeps, leaving terrain queries for Netherite Finder and Nuker.
pub const MAX_SEARCHES: usize = 2;
/// Bedrock's Nether dimension.
const NETHER: i32 = 1;

const _: () = {
    // Greedy packing below never splits an ore, so each dimension's ores must fit exactly.
    let mut nether = 0;
    let mut other = 0;
    let mut index = 0;
    while index < ORES.len() {
        assert!(ORES[index].blocks.len() == 2 || ORES[index].nether);
        if ORES[index].nether {
            nether += ORES[index].blocks.len();
        } else {
            other += ORES[index].blocks.len();
        }
        index += 1;
    }
    assert!(other <= MAX_SEARCHES * mod_api::MAX_FOUND_BLOCK_IDENTIFIERS);
    assert!(nether <= mod_api::MAX_FOUND_BLOCK_IDENTIFIERS);
    assert!(mod_api::MAX_FOUND_BLOCK_IDENTIFIERS.is_multiple_of(2));
    assert!(ORES.len() <= u16::BITS as usize);
    assert!(MAX_SEARCHES + 2 <= mod_api::MAX_TERRAIN_QUERIES);
};

/// The selection as bits in [`ORES`] order, identifying a search for change detection.
pub fn mask(selected: &[bool; ORES.len()]) -> u16 {
    selected
        .iter()
        .enumerate()
        .fold(0, |mask, (index, on)| mask | (u16::from(*on) << index))
}

/// Identifier lists for the selected ores that generate in `dimension`, one per search, each
/// within the host's identifier limit. Empty when none of the selected ores generate there.
pub fn searches(mask: u16, dimension: i32) -> Vec<Vec<String>> {
    let nether = dimension == NETHER;
    let mut searches: Vec<Vec<String>> = Vec::new();
    for (_, ore) in ORES
        .iter()
        .enumerate()
        .filter(|(index, ore)| mask & (1 << index) != 0 && ore.nether == nether)
    {
        match searches.last_mut() {
            Some(search)
                if search.len() + ore.blocks.len() <= mod_api::MAX_FOUND_BLOCK_IDENTIFIERS =>
            {
                search.extend(ore.blocks.iter().map(|block| String::from(*block)));
            }
            _ => searches.push(
                ore.blocks
                    .iter()
                    .map(|block| String::from(*block))
                    .collect(),
            ),
        }
    }
    searches
}

/// Merges Ore ESP's answers, each nearest first from `origin`, into the nearest `limit` blocks
/// to it, comparing block centres; ties keep the first answer's block.
#[cfg(test)]
pub fn merge(answers: Vec<Vec<Position>>, origin: [f32; 3], limit: usize) -> Vec<Position> {
    let mut merge = Merge::new(answers, origin, limit);
    while !merge.advance(usize::MAX) {}
    merge.finish()
}

/// Ore ESP's answers being merged, nearest first from an origin, into the nearest `limit`
/// blocks to it, a bounded number of blocks per [`Merge::advance`], so a full merge can span
/// callbacks. Ties keep the first answer's block. A single answer needs no merging.
pub struct Merge {
    first: Vec<Position>,
    second: Vec<Position>,
    origin: [f64; 3],
    merged: Vec<Position>,
    next: (usize, usize),
    heads: (Option<f64>, Option<f64>),
    limit: usize,
}

impl Merge {
    /// Starts merging up to [`MAX_SEARCHES`] answers; an empty answer is left out.
    pub fn new(answers: Vec<Vec<Position>>, origin: [f32; 3], limit: usize) -> Self {
        const { assert!(MAX_SEARCHES == 2) };
        debug_assert!(answers.len() <= MAX_SEARCHES);
        let mut answers = answers.into_iter().filter(|answer| !answer.is_empty());
        let first = answers.next().unwrap_or_default();
        let second = answers.next().unwrap_or_default();
        let limit = if second.is_empty() {
            limit
        } else {
            limit.min(first.len() + second.len())
        };
        let mut merge = Self {
            first,
            second,
            origin: origin.map(f64::from),
            merged: Vec::new(),
            next: (0, 0),
            heads: (None, None),
            limit,
        };
        if !merge.second.is_empty() {
            merge.merged.reserve_exact(limit);
            let origin = merge.origin;
            merge.heads = (
                Some(distance(origin, &merge.first[0])),
                Some(distance(origin, &merge.second[0])),
            );
        }
        merge
    }

    /// A complete merge of one nearest-first answer, which needs no merging.
    pub fn single(answer: Vec<Position>, limit: usize) -> Self {
        Self {
            first: answer,
            second: Vec::new(),
            origin: [0.; 3],
            merged: Vec::new(),
            next: (0, 0),
            heads: (None, None),
            limit,
        }
    }

    /// Takes up to `blocks` more of the nearest blocks, computing each block's distance once.
    /// Returns whether the merge is complete.
    pub fn advance(&mut self, blocks: usize) -> bool {
        let (first, second, origin) = (&self.first, &self.second, self.origin);
        let (i, j) = &mut self.next;
        let mut taken = 0;
        while taken < blocks && self.merged.len() < self.limit && !second.is_empty() {
            let rest = self.limit - self.merged.len();
            match self.heads {
                (Some(a), Some(b)) if a <= b => {
                    self.merged.push(first[*i]);
                    *i += 1;
                    self.heads.0 = first.get(*i).map(|block| distance(origin, block));
                }
                (Some(_), Some(_)) => {
                    self.merged.push(second[*j]);
                    *j += 1;
                    self.heads.1 = second.get(*j).map(|block| distance(origin, block));
                }
                // Once one answer runs out, the rest of the other follows in order.
                (Some(_), None) => self.merged.extend_from_slice(&first[*i..*i + rest]),
                (None, _) => self.merged.extend_from_slice(&second[*j..*j + rest]),
            }
            taken += 1;
        }
        self.done()
    }

    /// Whether every block of the merge is taken.
    pub fn done(&self) -> bool {
        self.second.is_empty() || self.merged.len() == self.limit
    }

    /// The nearest blocks of a complete merge, nearest first.
    pub fn finish(self) -> Vec<Position> {
        debug_assert!(self.done());
        if self.second.is_empty() {
            let mut positions = self.first;
            positions.truncate(self.limit);
            positions
        } else {
            self.merged
        }
    }
}

/// Squared distance from `origin` to a block's centre.
fn distance([x, y, z]: [f64; 3], position: &Position) -> f64 {
    let (dx, dy, dz) = (
        f64::from(position.x) + 0.5 - x,
        f64::from(position.y) + 0.5 - y,
        f64::from(position.z) + 0.5 - z,
    );
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests;
