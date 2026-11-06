//! Block table: textures, tinting and physical flags for every block the world can contain.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint { None, Grass, Foliage, Water, Birch, Spruce }

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
    /// Faces between two blocks of this kind are hidden (glass, leaves, water, ice).
    pub cull_same: bool,
    pub hardness: f32,
    /// 0 none, 1 pickaxe, 2 axe, 3 shovel, 4 hoe.
    pub tool: u8,
    /// Minimum tool tier to harvest (-1 = anything, including bare hands).
    pub level: i8,
    pub flammable: bool,
    /// Creative tab: 0 building, 1 natural, 2 colored, 3 functional.
    pub category: u8,
}

const fn cube(name: &'static str, tex: &'static str) -> BlockDef {
    BlockDef { name, top: tex, side: tex, bottom: tex, overlay: None, tint: Tint::None, opaque: true, translucent: false, solid: true, emit: 0, cross: false, cull_same: false, hardness: 1.0, tool: 0, level: -1, flammable: false, category: 0 }
}
const fn cols(name: &'static str, top: &'static str, side: &'static str, bottom: &'static str) -> BlockDef {
    BlockDef { name, top, side, bottom, overlay: None, tint: Tint::None, opaque: true, translucent: false, solid: true, emit: 0, cross: false, cull_same: false, hardness: 1.0, tool: 0, level: -1, flammable: false, category: 0 }
}

pub const AIR: u16 = 0;

static BASE: &[BlockDef] = &[
    BlockDef { name: "air", top: "", side: "", bottom: "", overlay: None, tint: Tint::None, opaque: false, translucent: false, solid: false, emit: 0, cross: false, cull_same: false, hardness: 1.0, tool: 0, level: -1, flammable: false, category: 0 },
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

pub fn by_name(name: &str) -> Option<&'static BlockDef> { BLOCKS.iter().find(|b| b.name == name) }


pub fn def(id: u16) -> &'static BlockDef { &BLOCKS[id as usize] }

pub const HOTBAR: [&str; 9] = ["grass_block", "dirt", "stone", "cobblestone", "oak_planks", "oak_log", "glass", "bricks", "glowstone"];

/// Mining hardness (vanilla values); negative means unbreakable.
pub fn hardness(name: &str) -> f32 { by_name(name).map(|d| d.hardness).unwrap_or(1.0) }

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

use std::collections::HashSet;
use std::sync::LazyLock;

/// All block textures available in the resources jar.
fn texture_set() -> HashSet<String> {
    let mut out = HashSet::new();
    if let Ok(f) = std::fs::File::open(crate::jar_path()) {
        if let Ok(z) = zip::ZipArchive::new(f) {
            for n in z.file_names() {
                if let Some(rest) = n.strip_prefix("assets/minecraft/textures/block/") {
                    if let Some(name) = rest.strip_suffix(".png") { out.insert(name.to_string()); }
                }
            }
        }
    }
    out
}

fn leak(s: String) -> &'static str { Box::leak(s.into_boxed_str()) }

fn classify(n: &str) -> (f32, u8, i8) {
    let ore_level = |n: &str| -> i8 {
        if n.contains("coal") || n.contains("nether_") { 0 } else if n.contains("copper") || n.contains("iron") || n.contains("lapis") { 1 } else { 2 }
    };
    if n == "bedrock" { return (-1.0, 0, -1); }
    if n.contains("obsidian") { return (50.0, 1, 3); }
    if n == "ancient_debris" { return (30.0, 1, 3); }
    if n.ends_with("_ore") { return (if n.starts_with("deepslate") { 4.5 } else { 3.0 }, 1, ore_level(n)); }
    match n {
        "iron_block" | "raw_iron_block" => return (5.0, 1, 1),
        "gold_block" | "raw_gold_block" => return (3.0, 1, 2),
        "diamond_block" | "emerald_block" => return (5.0, 1, 2),
        "lapis_block" => return (3.0, 1, 1),
        "redstone_block" | "coal_block" => return (5.0, 1, 0),
        "netherite_block" => return (50.0, 1, 3),
        "copper_block" | "exposed_copper" | "weathered_copper" | "oxidized_copper" | "cut_copper" | "raw_copper_block" => return (3.0, 1, 1),
        "tnt" | "fire" | "slime_block" | "honey_block" => return (0.0, 0, -1),
        "glowstone" | "sea_lantern" | "ice" => return (if n == "ice" { 0.5 } else { 0.3 }, if n == "ice" { 1 } else { 0 }, -1),
        "packed_ice" => return (0.5, 1, -1),
        "blue_ice" => return (2.8, 1, -1),
        "netherrack" => return (0.4, 1, 0),
        "magma_block" => return (0.5, 1, 0),
        "bone_block" => return (2.0, 1, 0),
        "end_stone" | "end_stone_bricks" => return (3.0, 1, 0),
        "calcite" => return (0.75, 1, 0),
        "mud" | "soul_sand" | "soul_soil" | "dirt" | "coarse_dirt" | "podzol" | "rooted_dirt" | "mycelium" | "mud_bricks_x" => return (0.5, 3, -1),
        "grass_block" | "gravel" | "clay" => return (0.6, 3, -1),
        "sand" | "red_sand" => return (0.5, 3, -1),
        "snow_block" => return (0.2, 3, -1),
        "moss_block" => return (0.1, 4, -1),
        "sponge" | "wet_sponge" => return (0.6, 4, -1),
        "hay_block" => return (0.5, 4, -1),
        "shroomlight" | "nether_wart_block" | "warped_wart_block" => return (1.0, 4, -1),
        "bookshelf" => return (1.5, 2, -1),
        "crafting_table" => return (2.5, 2, -1),
        "note_block" | "jukebox" => return (0.8, 2, -1),
        "pumpkin" | "melon" | "jack_o_lantern" => return (1.0, 2, -1),
        "furnace" => return (3.5, 1, 0),
        _ => {}
    }
    if n.ends_with("_leaves") { return (0.2, 4, -1); }
    if n.ends_with("concrete_powder") { return (0.5, 3, -1); }
    if n.ends_with("_wool") { return (0.8, 0, -1); }
    if n.contains("glass") { return (0.3, 0, -1); }
    if n.ends_with("_planks") || n.ends_with("_log") || n.ends_with("_wood") || n.ends_with("_stem") { return (2.0, 2, -1); }
    if n.ends_with("_sapling") || n.ends_with("_tulip") { return (0.0, 0, -1); }
    if n.ends_with("terracotta") { return (1.25, 1, 0); }
    if n.ends_with("concrete") { return (1.8, 1, 0); }
    if n.contains("deepslate") { return (if n.starts_with("cobbled") { 3.5 } else { 3.0 }, 1, 0); }
    if n.contains("sandstone") || n.contains("quartz") || n.contains("smooth_") { return (0.8, 1, 0); }
    if n == "cobblestone" || n.contains("brick") || n.contains("mossy_") || n.contains("purpur") { return (2.0, 1, 0); }
    if n.contains("basalt") { return (1.25, 1, 0); }
    if n.contains("stone") || n.contains("andesite") || n.contains("diorite") || n.contains("granite") || n.contains("tuff")
        || n.contains("prismarine") || n.contains("dripstone") || n.contains("amethyst") || n.contains("blackstone") { return (1.5, 1, 0); }
    (1.0, 0, -1)
}

fn category(n: &str, cross: bool) -> u8 {
    if matches!(n, "tnt" | "crafting_table" | "furnace" | "bookshelf" | "glowstone" | "sea_lantern" | "sponge" | "wet_sponge" | "note_block" | "jukebox" | "target" | "shroomlight" | "hay_block" | "slime_block" | "honey_block" | "pumpkin" | "melon" | "bone_block") { return 3; }
    if n.ends_with("_wool") || n.ends_with("concrete") || n.ends_with("concrete_powder") || n.ends_with("terracotta") || n.contains("stained_glass") { return 2; }
    if cross || n.ends_with("_leaves") || n.ends_with("_log") || n.ends_with("_wood") || n.ends_with("_ore") || n.ends_with("_stem")
        || matches!(n, "grass_block" | "dirt" | "coarse_dirt" | "podzol" | "mycelium" | "rooted_dirt" | "mud" | "sand" | "red_sand" | "gravel" | "clay" | "snow_block" | "ice" | "packed_ice" | "blue_ice" | "moss_block" | "stone" | "deepslate" | "tuff" | "calcite" | "netherrack" | "end_stone" | "basalt" | "magma_block" | "soul_sand" | "soul_soil" | "granite" | "andesite" | "diorite" | "obsidian" | "ancient_debris" | "dripstone_block" | "amethyst_block" | "warped_wart_block" | "nether_wart_block" | "blackstone") { return 1; }
    0
}

fn build_blocks() -> Vec<BlockDef> {
    let tex = texture_set();
    let mut v: Vec<BlockDef> = BASE.to_vec();
    let mut seen: HashSet<&'static str> = v.iter().map(|b| b.name).collect();
    let has = |t: &str| tex.contains(t);
    macro_rules! push {
        ($d:expr) => {{ let d: BlockDef = $d; if seen.insert(d.name) { v.push(d); } }};
    }
    let cube_t = |name: &str, top: &str, side: &str, bottom: &str| -> Option<BlockDef> {
        if has(top) && has(side) && has(bottom) { Some(cols(leak(name.to_string()), leak(top.to_string()), leak(side.to_string()), leak(bottom.to_string()))) } else { None }
    };
    let single = |name: &str, t: &str| cube_t(name, t, t, t);
    // Woods.
    for w in ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry"] {
        if let Some(d) = single(&format!("{w}_planks"), &format!("{w}_planks")) { push!(d); }
        if let Some(d) = cube_t(&format!("{w}_log"), &format!("{w}_log_top"), &format!("{w}_log"), &format!("{w}_log_top")) { push!(d); }
        if let Some(d) = cube_t(&format!("stripped_{w}_log"), &format!("stripped_{w}_log_top"), &format!("stripped_{w}_log"), &format!("stripped_{w}_log_top")) { push!(d); }
        if let Some(d) = cube_t(&format!("{w}_wood"), &format!("{w}_log"), &format!("{w}_log"), &format!("{w}_log")) { push!(d); }
        if let Some(mut d) = single(&format!("{w}_leaves"), &format!("{w}_leaves")) {
            d.opaque = false;
            d.tint = match w { "birch" => Tint::Birch, "spruce" => Tint::Spruce, "cherry" => Tint::None, _ => Tint::Foliage };
            push!(d);
        }
    }
    for w in ["crimson", "warped"] {
        if let Some(d) = cube_t(&format!("{w}_stem"), &format!("{w}_stem_top"), &format!("{w}_stem"), &format!("{w}_stem_top")) { push!(d); }
        if let Some(d) = single(&format!("{w}_planks"), &format!("{w}_planks")) { push!(d); }
    }
    // Stone and building blocks (single texture).
    for n in ["stone_bricks", "mossy_stone_bricks", "cracked_stone_bricks", "chiseled_stone_bricks", "mossy_cobblestone", "andesite", "diorite",
        "polished_andesite", "polished_diorite", "polished_granite", "smooth_stone", "obsidian", "crying_obsidian", "netherrack", "nether_bricks",
        "red_nether_bricks", "soul_sand", "soul_soil", "nether_wart_block", "shroomlight", "end_stone_bricks", "purpur_block", "prismarine",
        "prismarine_bricks", "dark_prismarine", "sea_lantern", "deepslate_bricks", "cracked_deepslate_bricks", "polished_deepslate",
        "cobbled_deepslate", "chiseled_deepslate", "deepslate_tiles", "polished_blackstone", "polished_blackstone_bricks", "gilded_blackstone",
        "chiseled_polished_blackstone", "dripstone_block", "amethyst_block", "copper_block", "exposed_copper", "weathered_copper", "oxidized_copper",
        "cut_copper", "iron_block", "gold_block", "diamond_block", "emerald_block", "lapis_block", "redstone_block", "coal_block", "netherite_block",
        "raw_gold_block", "sponge", "wet_sponge", "tuff_bricks", "polished_tuff", "chiseled_tuff", "mud_bricks", "blue_ice", "mossy_stone_bricks",
        "ancient_debris_x", "target_x", "note_block", "jukebox_side_x", "lodestone_x", "smooth_basalt", "polished_basalt_x", "crimson_nylium_x",
        "tinted_glass", "reinforced_deepslate_x", "bamboo_planks", "bamboo_mosaic", "resin_block", "pale_oak_planks", "chiseled_copper", "copper_grate", "calcite"] {
        if let Some(mut d) = single(n, n) { if n == "tinted_glass" { d.opaque = false; } push!(d); }
    }
    // Ores.
    for n in ["coal_ore", "lapis_ore", "emerald_ore", "diamond_ore", "redstone_ore", "gold_ore", "iron_ore", "copper_ore", "nether_gold_ore", "nether_quartz_ore",
        "deepslate_coal_ore", "deepslate_lapis_ore", "deepslate_emerald_ore", "deepslate_diamond_ore", "deepslate_redstone_ore", "deepslate_gold_ore", "deepslate_iron_ore", "deepslate_copper_ore"] {
        if let Some(d) = single(n, n) { push!(d); }
    }
    if let Some(d) = cube_t("ancient_debris", "ancient_debris_top", "ancient_debris_side", "ancient_debris_top") { push!(d); }
    // Multi-texture blocks.
    for (n, t, s, bt) in [
        ("sandstone", "sandstone_top", "sandstone", "sandstone_bottom"), ("cut_sandstone", "sandstone_top", "cut_sandstone", "sandstone_top"),
        ("chiseled_sandstone", "sandstone_top", "chiseled_sandstone", "sandstone_top"), ("smooth_sandstone", "sandstone_top", "sandstone_top", "sandstone_top"),
        ("cut_red_sandstone", "red_sandstone_top", "cut_red_sandstone", "red_sandstone_top"), ("chiseled_red_sandstone", "red_sandstone_top", "chiseled_red_sandstone", "red_sandstone_top"),
        ("smooth_red_sandstone", "red_sandstone_top", "red_sandstone_top", "red_sandstone_top"),
        ("quartz_block", "quartz_block_top", "quartz_block_side", "quartz_block_bottom"), ("chiseled_quartz_block", "chiseled_quartz_block_top", "chiseled_quartz_block", "chiseled_quartz_block_top"),
        ("quartz_pillar", "quartz_pillar_top", "quartz_pillar", "quartz_pillar_top"), ("smooth_quartz", "quartz_block_bottom", "quartz_block_bottom", "quartz_block_bottom"),
        ("quartz_bricks", "quartz_bricks", "quartz_bricks", "quartz_bricks"),
        ("bookshelf", "oak_planks", "bookshelf", "oak_planks"), ("crafting_table", "crafting_table_top", "crafting_table_side", "oak_planks"),
        ("furnace", "furnace_top", "furnace_side", "furnace_top"), ("hay_block", "hay_block_top", "hay_block_side", "hay_block_top"),
        ("melon", "melon_top", "melon_side", "melon_top"), ("pumpkin", "pumpkin_top", "pumpkin_side", "pumpkin_top"),
        ("bone_block", "bone_block_top", "bone_block_side", "bone_block_top"), ("honey_block", "honey_block_top", "honey_block_side", "honey_block_bottom"),
        ("target", "target_top", "target_side", "target_top"), ("slime_block", "slime_block", "slime_block", "slime_block"),
        ("jack_o_lantern", "pumpkin_top", "jack_o_lantern", "pumpkin_top"), ("smoker_x", "x", "x", "x"),
    ] {
        if let Some(mut d) = cube_t(n, t, s, bt) {
            if matches!(n, "slime_block" | "honey_block") { d.opaque = false; d.translucent = true; }
            if matches!(n, "jack_o_lantern") { d.emit = 15; }
            if matches!(n, "sea_lantern" | "glowstone") { d.emit = 15; }
            push!(d);
        }
    }
    // Colours.
    const COLORS: [&str; 16] = ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black"];
    for c in COLORS {
        for suffix in ["wool", "concrete", "concrete_powder", "terracotta"] {
            let n = format!("{c}_{suffix}");
            if let Some(d) = single(&n, &n) { push!(d); }
        }
        let g = format!("{c}_stained_glass");
        if let Some(mut d) = single(&g, &g) { d.opaque = false; push!(d); }
    }
    if let Some(d) = single("terracotta", "terracotta") { push!(d); }
    // Plants (crossed quads).
    for n in ["short_grass", "fern", "dead_bush", "dandelion", "poppy", "blue_orchid", "allium", "azure_bluet", "red_tulip", "orange_tulip", "white_tulip",
        "pink_tulip", "oxeye_daisy", "cornflower", "lily_of_the_valley", "brown_mushroom", "red_mushroom", "sugar_cane", "oak_sapling", "spruce_sapling",
        "birch_sapling", "jungle_sapling", "acacia_sapling", "dark_oak_sapling", "cherry_sapling", "wither_rose", "torchflower", "pink_petals_x"] {
        if has(n) {
            let mut d = cube(leak(n.to_string()), leak(n.to_string()));
            d.cross = true; d.solid = false; d.opaque = false;
            if matches!(n, "short_grass" | "fern") { d.tint = Tint::Grass; }
            push!(d);
        }
    }
    // Post-process: physical properties, culling and creative tab.
    for d in v.iter_mut() {
        let (h, tool, lvl) = classify(d.name);
        if d.name != "air" && d.name != "water" && d.name != "lava" { d.hardness = h; d.tool = tool; d.level = lvl; }
        if d.cross { d.hardness = 0.0; d.tool = 0; d.level = -1; }
        d.cull_same = d.translucent || d.name.ends_with("_leaves") || d.name.contains("glass") || d.name == "ice" || d.name == "water" || d.name == "slime_block";
        d.flammable = d.name.ends_with("_planks") || d.name.ends_with("_log") || d.name.ends_with("_wood") || d.name.ends_with("_leaves")
            || d.name.ends_with("_wool") || matches!(d.name, "hay_block" | "bookshelf");
        d.category = category(d.name, d.cross);
    }
    v
}

pub static BLOCKS: LazyLock<Vec<BlockDef>> = LazyLock::new(build_blocks);
