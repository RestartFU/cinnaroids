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

/// Merges answers, each nearest first from `origin`, into the nearest `limit` blocks to it,
/// comparing block centres. Takes at most `limit` steps, whatever the answers' lengths.
pub fn merge(mut answers: Vec<Vec<Position>>, origin: [f32; 3], limit: usize) -> Vec<Position> {
    if answers.len() <= 1 {
        let mut positions = answers.pop().unwrap_or_default();
        positions.truncate(limit);
        return positions;
    }
    let distance = |position: &Position| {
        [position.x, position.y, position.z]
            .iter()
            .zip(origin)
            .map(|(block, origin)| {
                let offset = f64::from(*block) + 0.5 - f64::from(origin);
                offset * offset
            })
            .sum::<f64>()
    };
    // Each answer's cursor and the distance of the block under it, computed once per block.
    let mut heads: Vec<_> = answers
        .iter()
        .map(|answer| (0, answer.first().map(distance)))
        .collect();
    let mut merged = Vec::with_capacity(limit);
    while merged.len() < limit {
        let nearest = heads
            .iter()
            .enumerate()
            .filter_map(|(index, (_, head))| Some((index, (*head)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((index, _)) = nearest else {
            break;
        };
        let (cursor, head) = &mut heads[index];
        merged.push(answers[index][*cursor]);
        *cursor += 1;
        *head = answers[index].get(*cursor).map(distance);
    }
    merged
}

#[cfg(test)]
mod tests;
