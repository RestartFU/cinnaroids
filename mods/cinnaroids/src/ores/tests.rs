use super::*;

fn position(x: i32, y: i32, z: i32) -> Position {
    Position { x, y, z }
}

#[test]
fn every_ore_fits_its_dimensions_searches() {
    let all = mask(&[true; ORES.len()]);
    let overworld = searches(all, 0);
    assert_eq!(overworld.len(), MAX_SEARCHES);
    assert!(
        overworld
            .iter()
            .all(|search| search.len() <= mod_api::MAX_FOUND_BLOCK_IDENTIFIERS)
    );
    assert_eq!(overworld.concat().len(), 16);
    assert!(
        overworld
            .concat()
            .contains(&"minecraft:deepslate_diamond_ore".into())
    );
    assert!(!overworld.concat().contains(&"minecraft:quartz_ore".into()));
    // The End and custom dimensions search the same ores as the Overworld.
    assert_eq!(searches(all, 2), overworld);

    let nether = searches(all, 1);
    assert_eq!(
        nether,
        [vec![
            String::from("minecraft:nether_gold_ore"),
            "minecraft:quartz_ore".into()
        ]]
    );
}

#[test]
fn only_selected_ores_are_searched() {
    assert_eq!(mask(&DEFAULT), 0b11);
    assert_eq!(
        searches(mask(&DEFAULT), 0),
        [[
            "minecraft:diamond_ore",
            "minecraft:deepslate_diamond_ore",
            "minecraft:emerald_ore",
            "minecraft:deepslate_emerald_ore"
        ]
        .map(String::from)
        .to_vec()]
    );
    // No selected ore generates in the Nether, and nothing selected searches nothing.
    assert!(searches(mask(&DEFAULT), 1).is_empty());
    assert!(searches(0, 0).is_empty());
    let mut quartz = [false; ORES.len()];
    quartz[9] = true;
    assert_eq!(
        searches(mask(&quartz), 1).concat(),
        ["minecraft:quartz_ore"]
    );
    assert!(searches(mask(&quartz), 0).is_empty());
}

#[test]
fn answers_merge_nearest_first_up_to_the_limit() {
    let origin = [0.5, 64.5, 0.5];
    let near = vec![position(1, 64, 0), position(4, 64, 0), position(9, 64, 0)];
    let far = vec![position(0, 66, 0), position(0, 70, 0)];
    let merged = merge(vec![near.clone(), far.clone()], origin, 8);
    let keys: Vec<_> = merged.iter().map(|p| (p.x, p.y, p.z)).collect();
    assert_eq!(
        keys,
        [(1, 64, 0), (0, 66, 0), (4, 64, 0), (0, 70, 0), (9, 64, 0)]
    );
    // Either answer may run out first; the other's rest follows in order.
    let swapped: Vec<_> = merge(vec![far.clone(), near.clone()], origin, 8)
        .iter()
        .map(|p| (p.x, p.y, p.z))
        .collect();
    assert_eq!(swapped, keys);
    // At equal distances the first answer's block comes first.
    let tie = merge(
        vec![vec![position(0, 64, 1)], vec![position(1, 64, 0)]],
        origin,
        1,
    );
    assert_eq!((tie[0].x, tie[0].z), (0, 1));
    assert_eq!(merge(vec![near.clone(), far], origin, 2).len(), 2);
    // One answer is kept in its order, truncated; none is empty.
    assert_eq!(merge(vec![near.clone()], origin, 2).len(), 2);
    assert_eq!(merge(vec![near], origin, 2)[1].x, 4);
    assert!(merge(Vec::new(), origin, 8).is_empty());
    assert!(merge(vec![Vec::new(), Vec::new()], origin, 8).is_empty());
}

#[test]
fn a_merge_resumes_in_bounded_steps() {
    let origin = [0.5, 64.5, 0.5];
    let near: Vec<_> = (0..40).map(|x| position(x, 64, 0)).collect();
    let far: Vec<_> = (0..40).map(|x| position(x, 65, 0)).collect();
    let mut stepped = Merge::new(vec![near.clone(), far.clone()], origin, 50);
    let mut steps = 0;
    while !stepped.advance(8) {
        steps += 1;
        assert!(!stepped.done());
    }
    assert!(steps >= 50 / 8 - 1, "a step takes at most 8 blocks");
    let stepped = stepped.finish();
    assert_eq!(stepped.len(), 50);
    let whole = merge(vec![near.clone(), far], origin, 50);
    let keys = |blocks: &[Position]| blocks.iter().map(|p| (p.x, p.y, p.z)).collect::<Vec<_>>();
    assert_eq!(keys(&stepped), keys(&whole));
    // A single answer is complete at once, without merging.
    let single = Merge::new(vec![Vec::new(), near.clone()], origin, 10);
    assert!(single.done());
    assert_eq!(keys(&single.finish()), keys(&near[..10]));
}

#[test]
fn every_ore_block_identifier_is_pinned() {
    let blocks: Vec<&str> = ORES
        .iter()
        .flat_map(|ore| ore.blocks.iter().copied())
        .collect();
    assert_eq!(
        blocks,
        [
            "minecraft:diamond_ore",
            "minecraft:deepslate_diamond_ore",
            "minecraft:emerald_ore",
            "minecraft:deepslate_emerald_ore",
            "minecraft:gold_ore",
            "minecraft:deepslate_gold_ore",
            "minecraft:iron_ore",
            "minecraft:deepslate_iron_ore",
            "minecraft:redstone_ore",
            "minecraft:deepslate_redstone_ore",
            "minecraft:lapis_ore",
            "minecraft:deepslate_lapis_ore",
            "minecraft:copper_ore",
            "minecraft:deepslate_copper_ore",
            "minecraft:coal_ore",
            "minecraft:deepslate_coal_ore",
            "minecraft:nether_gold_ore",
            "minecraft:quartz_ore",
        ]
    );
}
