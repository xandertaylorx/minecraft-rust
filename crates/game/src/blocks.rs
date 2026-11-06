//! Block table: textures, tinting and physical flags for every block the world can contain.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint { None, Grass, Foliage, Water }

#[derive(Clone, Copy, Debug)]
pub struct BlockDef {
    pub name: &'static str,
    pub top: &'static str,
    pub side: &'static str,
    pub bottom: &'static str,
    pub overlay: Option<&'static str>,
    pub tint: Tint,
    /// Blocks light and hides neighbouring faces.
    pub opaque: bool,
    /// Rendered in the blended pass.
    pub translucent: bool,
    pub solid: bool,
    pub emit: u8,
    /// Crossed-quad plant/fire shape instead of a cube.
    pub cross: bool,
}

const fn cube(name: &'static str, tex: &'static str) -> BlockDef {
    BlockDef { name, top: tex, side: tex, bottom: tex, overlay: None, tint: Tint::None, opaque: true, translucent: false, solid: true, emit: 0, cross: false }
}
const fn cols(name: &'static str, top: &'static str, side: &'static str, bottom: &'static str) -> BlockDef {
    BlockDef { name, top, side, bottom, overlay: None, tint: Tint::None, opaque: true, translucent: false, solid: true, emit: 0, cross: false }
}

pub const AIR: u16 = 0;

pub static BLOCKS: &[BlockDef] = &[
    BlockDef { name: "air", top: "", side: "", bottom: "", overlay: None, tint: Tint::None, opaque: false, translucent: false, solid: false, emit: 0, cross: false },
    cube("stone", "stone"),
    cols("basalt", "basalt_top", "basalt_side", "basalt_top"),
    cube("tuff", "tuff"),
    cube("end_stone", "end_stone"),
    cols("deepslate", "deepslate_top", "deepslate", "deepslate_top"),
    cube("snow_block", "snow"),
    cols("blackstone", "blackstone_top", "blackstone", "blackstone_top"),
    cube("sand", "sand"),
    cube("green_terracotta", "green_terracotta"),
    BlockDef { overlay: Some("grass_block_side_overlay"), tint: Tint::Grass, ..cols("grass_block", "grass_block_top", "grass_block_side", "dirt") },
    cube("dirt", "dirt"),
    cols("sandstone", "sandstone_top", "sandstone", "sandstone_bottom"),
    cols("red_sandstone", "red_sandstone_top", "red_sandstone", "red_sandstone_bottom"),
    cube("iron_ore", "iron_ore"),
    cube("gold_ore", "gold_ore"),
    cube("redstone_ore", "redstone_ore"),
    cube("diamond_ore", "diamond_ore"),
    cube("coal_ore", "coal_ore"),
    BlockDef { solid: false, emit: 15, ..cube("lava", "lava_still") },
    BlockDef { opaque: false, translucent: true, solid: false, tint: Tint::Water, ..cube("water", "water_still") },
    BlockDef { opaque: false, translucent: true, ..cube("ice", "ice") },
    cube("packed_ice", "packed_ice"),
    BlockDef { emit: 3, ..cube("magma_block", "magma") },
    cube("warped_wart_block", "warped_wart_block"),
    cube("rooted_dirt", "rooted_dirt"),
    cube("moss_block", "moss_block"),
    cube("clay", "clay"),
    BlockDef { opaque: false, tint: Tint::Foliage, ..cube("oak_leaves", "oak_leaves") },
    // Placeable extras for the hotbar.
    cube("cobblestone", "cobblestone"),
    cube("oak_planks", "oak_planks"),
    cols("oak_log", "oak_log_top", "oak_log", "oak_log_top"),
    BlockDef { opaque: false, ..cube("glass", "glass") },
    cube("bricks", "bricks"),
    cube("bedrock", "bedrock"),
    BlockDef { emit: 15, ..cube("glowstone", "glowstone") },
    cube("calcite", "calcite"),
    cube("coarse_dirt", "coarse_dirt"),
    cube("gravel", "gravel"),
    cube("mud", "mud"),
    cols("mycelium", "mycelium_top", "mycelium_side", "dirt"),
    cols("podzol", "podzol_top", "podzol_side", "dirt"),
    cube("orange_terracotta", "orange_terracotta"),
    cube("white_terracotta", "white_terracotta"),
    cube("yellow_terracotta", "yellow_terracotta"),
    cube("brown_terracotta", "brown_terracotta"),
    cube("red_terracotta", "red_terracotta"),
    cube("light_gray_terracotta", "light_gray_terracotta"),
    cube("terracotta", "terracotta"),
    cube("powder_snow", "powder_snow"),
    cube("red_sand", "red_sand"),
    cube("granite", "granite"),
    cube("copper_ore", "copper_ore"),
    cube("raw_copper_block", "raw_copper_block"),
    cube("raw_iron_block", "raw_iron_block"),
    cube("deepslate_iron_ore", "deepslate_iron_ore"),
    cube("packed_mud", "packed_mud"),
    BlockDef { opaque: false, solid: false, emit: 15, cross: true, ..cube("fire", "fire_0") },
    cols("tnt", "tnt_top", "tnt_side", "tnt_bottom"),
];


pub fn id_of(name: &str) -> u16 {
    BLOCKS.iter().position(|b| b.name == name).unwrap_or_else(|| panic!("unknown block {name}")) as u16
}


pub fn def(id: u16) -> &'static BlockDef { &BLOCKS[id as usize] }

pub const HOTBAR: [&str; 9] = ["grass_block", "dirt", "stone", "cobblestone", "oak_planks", "oak_log", "glass", "bricks", "glowstone"];

/// Mining hardness (vanilla values); negative means unbreakable.
pub fn hardness(name: &str) -> f32 {
    match name {
        "bedrock" => -1.0,
        "stone" | "tuff" | "blackstone" | "granite" | "end_stone" => 1.5,
        "cobblestone" | "oak_planks" | "oak_log" | "bricks" => 2.0,
        "deepslate" => 3.0,
        "iron_ore" | "gold_ore" | "diamond_ore" | "coal_ore" | "redstone_ore" | "copper_ore" => 3.0,
        "deepslate_iron_ore" => 4.5,
        "dirt" | "sand" | "red_sand" | "coarse_dirt" | "mud" | "podzol" | "rooted_dirt" => 0.5,
        "grass_block" | "gravel" | "clay" | "mycelium" | "green_terracotta" => 0.6,
        "snow_block" | "oak_leaves" => 0.2,
        "ice" | "packed_ice" | "magma_block" | "sandstone" | "red_sandstone" | "basalt" => 0.8,
        "glass" | "glowstone" => 0.3,
        "tnt" | "fire" => 0.0,
        "moss_block" => 0.1,
        "calcite" => 0.75,
        n if n.ends_with("terracotta") => 1.25,
        _ => 1.0,
    }
}

/// What a broken block gives (vanilla-style conversions for the common cases).
pub fn drop_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "grass_block" | "podzol" | "mycelium" | "coarse_dirt" | "rooted_dirt" => "dirt",
        "stone" => "cobblestone",
        "oak_leaves" | "glass" | "ice" | "water" | "lava" | "bedrock" | "powder_snow" => return None,
        n => BLOCKS.iter().find(|b| b.name == n).map(|b| b.name)?,
    })
}

pub fn display_name(name: &str) -> String {
    let mut out = String::new();
    for (i, w) in name.split('_').enumerate() {
        if i > 0 { out.push(' '); }
        let mut c = w.chars();
        if let Some(f) = c.next() { out.extend(f.to_uppercase()); out.push_str(c.as_str()); }
    }
    out
}

/// Blocks that can appear in the creative inventory.
pub fn creative_items() -> Vec<u16> {
    (1..BLOCKS.len() as u16).filter(|&i| !matches!(BLOCKS[i as usize].name, "water" | "lava" | "bedrock" | "powder_snow" | "fire")).collect()
}

/// Blocks that need a pickaxe to harvest (mining without one is 3.3x slower and drops nothing in vanilla).
pub fn requires_tool(name: &str) -> bool {
    matches!(name, "stone" | "cobblestone" | "deepslate" | "tuff" | "granite" | "basalt" | "blackstone" | "end_stone" | "bricks"
        | "iron_ore" | "gold_ore" | "diamond_ore" | "coal_ore" | "redstone_ore" | "copper_ore" | "deepslate_iron_ore"
        | "sandstone" | "red_sandstone" | "calcite" | "packed_ice" | "magma_block" | "raw_copper_block" | "raw_iron_block")
        || name.ends_with("terracotta")
}
