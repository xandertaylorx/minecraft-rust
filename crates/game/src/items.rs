//! Items: tools, weapons, food and materials, with vanilla tier/attribute values.
use crate::blocks::{self, BLOCKS};

/// Item ids below this are blocks (indices into `BLOCKS`); the rest index `ITEMS`.
pub const ITEM_BASE: u16 = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Sword, Pickaxe, Axe, Shovel, Hoe, Bow, Arrow, Food(u8), FlintSteel, Misc }

#[derive(Clone, Copy, Debug)]
pub struct ItemDef {
    pub name: &'static str,
    pub kind: Kind,
    pub tier: u8,
    pub max_stack: u8,
    pub durability: u16,
}

const fn it(name: &'static str, kind: Kind, tier: u8, max_stack: u8, durability: u16) -> ItemDef { ItemDef { name, kind, tier, max_stack, durability } }

/// (mining speed, harvest level, durability)
const TIERS: [(f32, u8, u16); 4] = [(2.0, 0, 59), (4.0, 1, 131), (6.0, 2, 250), (8.0, 3, 1561)];

pub static ITEMS: &[ItemDef] = &[
    it("wooden_sword", Kind::Sword, 0, 1, 59), it("stone_sword", Kind::Sword, 1, 1, 131), it("iron_sword", Kind::Sword, 2, 1, 250), it("diamond_sword", Kind::Sword, 3, 1, 1561),
    it("wooden_pickaxe", Kind::Pickaxe, 0, 1, 59), it("stone_pickaxe", Kind::Pickaxe, 1, 1, 131), it("iron_pickaxe", Kind::Pickaxe, 2, 1, 250), it("diamond_pickaxe", Kind::Pickaxe, 3, 1, 1561),
    it("wooden_axe", Kind::Axe, 0, 1, 59), it("stone_axe", Kind::Axe, 1, 1, 131), it("iron_axe", Kind::Axe, 2, 1, 250), it("diamond_axe", Kind::Axe, 3, 1, 1561),
    it("wooden_shovel", Kind::Shovel, 0, 1, 59), it("stone_shovel", Kind::Shovel, 1, 1, 131), it("iron_shovel", Kind::Shovel, 2, 1, 250), it("diamond_shovel", Kind::Shovel, 3, 1, 1561),
    it("wooden_hoe", Kind::Hoe, 0, 1, 59), it("stone_hoe", Kind::Hoe, 1, 1, 131), it("iron_hoe", Kind::Hoe, 2, 1, 250), it("diamond_hoe", Kind::Hoe, 3, 1, 1561),
    it("bow", Kind::Bow, 0, 1, 384), it("arrow", Kind::Arrow, 0, 64, 0), it("flint_and_steel", Kind::FlintSteel, 0, 1, 64),
    it("beef", Kind::Food(3), 0, 64, 0), it("cooked_beef", Kind::Food(8), 0, 64, 0), it("porkchop", Kind::Food(3), 0, 64, 0),
    it("cooked_porkchop", Kind::Food(8), 0, 64, 0), it("apple", Kind::Food(4), 0, 64, 0), it("bread", Kind::Food(5), 0, 64, 0),
    it("rotten_flesh", Kind::Food(4), 0, 64, 0), it("chicken", Kind::Food(2), 0, 64, 0), it("mutton", Kind::Food(2), 0, 64, 0),
    it("bone", Kind::Misc, 0, 64, 0), it("string", Kind::Misc, 0, 64, 0), it("feather", Kind::Misc, 0, 64, 0),
    it("gunpowder", Kind::Misc, 0, 64, 0), it("leather", Kind::Misc, 0, 64, 0), it("stick", Kind::Misc, 0, 64, 0),
    it("coal", Kind::Misc, 0, 64, 0), it("iron_ingot", Kind::Misc, 0, 64, 0), it("diamond", Kind::Misc, 0, 64, 0),
];

pub fn is_block(id: u16) -> bool { id < ITEM_BASE }
pub fn item_def(id: u16) -> Option<&'static ItemDef> { id.checked_sub(ITEM_BASE).and_then(|i| ITEMS.get(i as usize)) }
pub fn item_id(name: &str) -> Option<u16> { ITEMS.iter().position(|i| i.name == name).map(|i| ITEM_BASE + i as u16) }
pub fn id_by_name(name: &str) -> Option<u16> {
    let n = name.trim_start_matches("minecraft:");
    item_id(n).or_else(|| BLOCKS.iter().position(|b| b.name == n).filter(|&i| i != 0).map(|i| i as u16))
}
pub fn name_of(id: u16) -> &'static str { if is_block(id) { blocks::def(id).name } else { item_def(id).map(|d| d.name).unwrap_or("air") } }
pub fn max_stack(id: u16) -> u8 { item_def(id).map(|d| d.max_stack).unwrap_or(64) }
pub fn max_damage(id: u16) -> u16 { item_def(id).map(|d| d.durability).unwrap_or(0) }

/// Icon slot (0..) in the GUI atlas for an item or block.
pub fn icon_slot(id: u16) -> usize { if is_block(id) { id as usize } else { BLOCKS.len() + (id - ITEM_BASE) as usize } }

pub fn all_item_ids() -> impl Iterator<Item = u16> { (0..ITEMS.len() as u16).map(|i| ITEM_BASE + i) }

/// Attack damage added to the player's base 1.0, and attack speed (4.0 base + modifier), from the vanilla item attributes.
pub fn weapon_stats(id: u16) -> (f32, f32) {
    let Some(d) = item_def(id) else { return (0.0, 4.0) };
    let t = d.tier as usize;
    match d.kind {
        Kind::Sword => (3.0 + t as f32, 1.6),
        Kind::Axe => ([6.0, 8.0, 8.0, 8.0][t], [0.8, 0.8, 0.9, 1.0][t]),
        Kind::Pickaxe => (1.0 + t as f32, 1.2),
        Kind::Shovel => (1.5 + t as f32, 1.0),
        Kind::Hoe => ([0.0, 0.0, 0.0, 0.0][t], [1.0, 2.0, 3.0, 4.0][t]),
        _ => (0.0, 4.0),
    }
}

/// Which tool kind digs this block quickly, and the minimum pickaxe level to harvest it (None = no requirement).
pub fn block_tool(name: &str) -> (Option<Kind>, Option<u8>) {
    match name {
        "stone" | "cobblestone" | "deepslate" | "tuff" | "granite" | "basalt" | "blackstone" | "end_stone" | "bricks" | "coal_ore"
        | "sandstone" | "red_sandstone" | "calcite" | "packed_ice" | "ice" | "magma_block" | "copper_ore" | "raw_copper_block" => (Some(Kind::Pickaxe), if name == "ice" { None } else { Some(0) }),
        "iron_ore" | "deepslate_iron_ore" | "raw_iron_block" => (Some(Kind::Pickaxe), Some(1)),
        "gold_ore" | "diamond_ore" | "redstone_ore" => (Some(Kind::Pickaxe), Some(2)),
        n if n.ends_with("terracotta") => (Some(Kind::Pickaxe), Some(0)),
        "oak_log" | "oak_planks" => (Some(Kind::Axe), None),
        "dirt" | "grass_block" | "sand" | "red_sand" | "gravel" | "clay" | "snow_block" | "mycelium" | "podzol" | "coarse_dirt" | "mud" | "rooted_dirt" | "powder_snow" => (Some(Kind::Shovel), None),
        "oak_leaves" | "moss_block" | "warped_wart_block" => (Some(Kind::Hoe), None),
        _ => (None, None),
    }
}

/// Destroy progress added per tick (`BlockState.getDestroyProgress`), and whether the drop is allowed.
pub fn destroy_progress(held: Option<u16>, name: &str) -> (f32, bool) {
    let hard = blocks::hardness(name);
    if hard < 0.0 { return (0.0, false); }
    let (tool, level) = block_tool(name);
    let held_def = held.and_then(item_def);
    let mut speed = 1.0;
    let mut level_ok = true;
    if let (Some(kind), Some(d)) = (tool, held_def) {
        if d.kind == kind { speed = TIERS[d.tier as usize].0; }
    }
    if let Some(need) = level {
        let have = held_def.filter(|d| d.kind == Kind::Pickaxe).map(|d| TIERS[d.tier as usize].1 as i32).unwrap_or(-1);
        level_ok = have >= need as i32;
    }
    // Pickaxe-only blocks (stone family) require a pickaxe to drop anything.
    let needs_tool = matches!(tool, Some(Kind::Pickaxe)) && level.is_some();
    let can_harvest = !needs_tool || level_ok;
    let divisor = if can_harvest { 30.0 } else { 100.0 };
    (speed / hard.max(0.001) / divisor, can_harvest)
}

pub fn drop_for(name: &str) -> Option<u16> {
    match name {
        "grass_block" | "podzol" | "mycelium" | "coarse_dirt" | "rooted_dirt" => blocks::BLOCKS.iter().position(|b| b.name == "dirt").map(|i| i as u16),
        "stone" => blocks::BLOCKS.iter().position(|b| b.name == "cobblestone").map(|i| i as u16),
        "coal_ore" => item_id("coal"),
        "diamond_ore" => item_id("diamond"),
        "oak_leaves" | "glass" | "ice" | "water" | "lava" | "bedrock" | "powder_snow" | "air" => None,
        n => BLOCKS.iter().position(|b| b.name == n).map(|i| i as u16),
    }
}
