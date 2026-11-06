//! Vegetation: trees, grass and flowers placed deterministically per chunk (kept inside the chunk so no neighbour writes are needed).
use std::sync::OnceLock;

use worldgen::biome::BiomeInfo;
use worldgen::random::Xoro;

use crate::blocks::id_of;
use crate::world::{idx, HEIGHT, MIN_Y};

struct Ids {
    air: u16, water: u16, grass: u16, dirt: u16, podzol: u16, coarse: u16, sand: u16, red_sand: u16, snow: u16,
    short_grass: u16, fern: u16, dead_bush: u16, flowers: Vec<u16>,
}

fn ids() -> &'static Ids {
    static I: OnceLock<Ids> = OnceLock::new();
    I.get_or_init(|| Ids {
        air: 0, water: id_of("water"), grass: id_of("grass_block"), dirt: id_of("dirt"), podzol: id_of("podzol"), coarse: id_of("coarse_dirt"),
        sand: id_of("sand"), red_sand: id_of("red_sand"), snow: id_of("snow_block"),
        short_grass: id_of("short_grass"), fern: id_of("fern"), dead_bush: id_of("dead_bush"),
        flowers: ["dandelion", "poppy", "azure_bluet", "oxeye_daisy", "cornflower", "red_tulip", "orange_tulip", "white_tulip", "pink_tulip", "allium", "blue_orchid", "lily_of_the_valley"]
            .iter().map(|n| id_of(n)).collect(),
    })
}

#[derive(Clone, Copy)]
enum Tree { Oak, Birch, Spruce, Jungle, Acacia, DarkOak, Cherry }

struct Plan { trees: &'static [(Tree, u32)], count: f32, grass: f32, flowers: f32, fern: bool }

fn plan(biome: &str) -> Plan {
    use Tree::*;
    match biome {
        "plains" | "sunflower_plains" => Plan { trees: &[(Oak, 1)], count: 0.25, grass: 0.30, flowers: 0.02, fern: false },
        "meadow" => Plan { trees: &[(Oak, 1)], count: 0.1, grass: 0.35, flowers: 0.08, fern: false },
        "forest" => Plan { trees: &[(Oak, 3), (Birch, 1)], count: 7.0, grass: 0.12, flowers: 0.01, fern: false },
        "flower_forest" => Plan { trees: &[(Oak, 2), (Birch, 2)], count: 5.0, grass: 0.10, flowers: 0.14, fern: false },
        "birch_forest" | "old_growth_birch_forest" => Plan { trees: &[(Birch, 1)], count: 7.0, grass: 0.12, flowers: 0.005, fern: false },
        "dark_forest" => Plan { trees: &[(DarkOak, 3), (Oak, 1)], count: 9.0, grass: 0.06, flowers: 0.0, fern: false },
        "taiga" | "old_growth_pine_taiga" | "old_growth_spruce_taiga" | "snowy_taiga" => Plan { trees: &[(Spruce, 1)], count: 8.0, grass: 0.06, flowers: 0.0, fern: true },
        "windswept_forest" | "windswept_hills" | "windswept_gravelly_hills" | "grove" => Plan { trees: &[(Spruce, 1)], count: 3.0, grass: 0.1, flowers: 0.0, fern: false },
        "snowy_plains" | "ice_spikes" => Plan { trees: &[(Spruce, 1)], count: 0.1, grass: 0.0, flowers: 0.0, fern: false },
        "jungle" | "bamboo_jungle" => Plan { trees: &[(Jungle, 3), (Oak, 1)], count: 9.0, grass: 0.35, flowers: 0.01, fern: true },
        "sparse_jungle" => Plan { trees: &[(Jungle, 1), (Oak, 1)], count: 3.0, grass: 0.25, flowers: 0.0, fern: true },
        "savanna" | "savanna_plateau" | "windswept_savanna" => Plan { trees: &[(Acacia, 1)], count: 1.2, grass: 0.35, flowers: 0.0, fern: false },
        "swamp" | "mangrove_swamp" => Plan { trees: &[(Oak, 1)], count: 2.5, grass: 0.25, flowers: 0.01, fern: true },
        "cherry_grove" => Plan { trees: &[(Cherry, 1)], count: 4.5, grass: 0.2, flowers: 0.06, fern: false },
        "desert" | "badlands" | "eroded_badlands" | "wooded_badlands" => Plan { trees: &[], count: 0.0, grass: 0.0, flowers: 0.0, fern: false },
        _ => Plan { trees: &[], count: 0.0, grass: 0.0, flowers: 0.0, fern: false },
    }
}

pub fn decorate(blocks: &mut [u16], biome_col: &[u16], biomes: &[BiomeInfo], cx: i32, cz: i32) {
    let id = ids();
    let mut r = Xoro::new(crate::SEED ^ (cx as i64).wrapping_mul(0x2545F4914F6CDD1D) ^ (cz as i64).wrapping_mul(0x9E3779B97F4A7C15u64 as i64) ^ 0xDEC0);
    // Surface heights.
    let mut top = [i32::MIN; 256];
    for z in 0..16 {
        for x in 0..16 {
            let mut y = HEIGHT as i32 - 1;
            while y > 0 && blocks[idx(x, y as usize, z)] == id.air { y -= 1; }
            top[z * 16 + x] = y + MIN_Y;
        }
    }
    let get = |blocks: &[u16], x: i32, y: i32, z: i32| -> u16 { blocks[idx(x as usize, (y - MIN_Y) as usize, z as usize)] };
    let can_support = |b: u16| b == id.grass || b == id.dirt || b == id.podzol || b == id.coarse;
    // Ground cover first (so trees overwrite it).
    for z in 0..16usize {
        for x in 0..16usize {
            let t = top[z * 16 + x];
            if t >= MIN_Y + HEIGHT as i32 - 2 || t < MIN_Y + 1 { continue; }
            let surf = get(blocks, x as i32, t, z as i32);
            let name = &biomes[biome_col[z * 16 + x] as usize].name;
            let p = plan(name);
            let above = get(blocks, x as i32, t + 1, z as i32);
            if above != id.air { continue; }
            if surf == id.grass {
                let roll = r.next_double() as f32;
                if roll < p.flowers { let f = id.flowers[r.next_int_bound(id.flowers.len() as i32) as usize]; blocks[idx(x, (t + 1 - MIN_Y) as usize, z)] = f; }
                else if roll < p.flowers + p.grass { blocks[idx(x, (t + 1 - MIN_Y) as usize, z)] = if p.fern && r.next_float() < 0.35 { id.fern } else { id.short_grass }; }
            } else if (surf == id.sand || surf == id.red_sand) && (name == "desert" || name.contains("badlands")) {
                if r.next_float() < 0.012 { blocks[idx(x, (t + 1 - MIN_Y) as usize, z)] = id.dead_bush; }
            } else if surf == id.podzol && p.fern && r.next_float() < 0.08 {
                blocks[idx(x, (t + 1 - MIN_Y) as usize, z)] = id.fern;
            }
        }
    }
    // Trees: origins kept 4 blocks from chunk edges so canopies stay inside.
    let centre = &biomes[biome_col[8 * 16 + 8] as usize].name;
    let p = plan(centre);
    if p.trees.is_empty() { return; }
    let n = p.count.floor() as i32 + if r.next_float() < p.count.fract() { 1 } else { 0 };
    let total: u32 = p.trees.iter().map(|t| t.1).sum();
    for _ in 0..n {
        let (x, z) = (4 + r.next_int_bound(8), 4 + r.next_int_bound(8));
        let t = top[(z * 16 + x) as usize];
        if t < MIN_Y + 2 || t > MIN_Y + HEIGHT as i32 - 24 { continue; }
        if !can_support(get(blocks, x, t, z)) { continue; }
        if get(blocks, x, t + 1, z) != id.air && !matches!(get(blocks, x, t + 1, z), b if b == id.short_grass || b == id.fern) { continue; }
        let mut pick = r.next_int_bound(total as i32) as u32;
        let mut kind = p.trees[0].0;
        for (k, w) in p.trees { if pick < *w { kind = *k; break; } pick -= *w; }
        grow(blocks, &mut r, kind, x, t + 1, z);
    }
}

fn grow(blocks: &mut [u16], r: &mut Xoro, kind: Tree, x: i32, base: i32, z: i32) {
    let (log, leaves, h) = match kind {
        Tree::Oak => ("oak_log", "oak_leaves", 4 + r.next_int_bound(3)),
        Tree::Birch => ("birch_log", "birch_leaves", 5 + r.next_int_bound(3)),
        Tree::Spruce => ("spruce_log", "spruce_leaves", 6 + r.next_int_bound(4)),
        Tree::Jungle => ("jungle_log", "jungle_leaves", 8 + r.next_int_bound(5)),
        Tree::Acacia => ("acacia_log", "acacia_leaves", 5 + r.next_int_bound(2)),
        Tree::DarkOak => ("dark_oak_log", "dark_oak_leaves", 5 + r.next_int_bound(3)),
        Tree::Cherry => ("cherry_log", "cherry_leaves", 4 + r.next_int_bound(3)),
    };
    let (log, leaf) = (id_of(log), id_of(leaves));
    let air = 0u16;
    let mut set = |blocks: &mut [u16], dx: i32, y: i32, dz: i32, b: u16, only_air: bool| {
        let (px, pz) = (x + dx, z + dz);
        if !(0..16).contains(&px) || !(0..16).contains(&pz) || y < MIN_Y || y >= MIN_Y + HEIGHT as i32 { return; }
        let i = idx(px as usize, (y - MIN_Y) as usize, pz as usize);
        if !only_air || blocks[i] == air || blocks[i] == id_of("short_grass") || blocks[i] == id_of("fern") { blocks[i] = b; }
    };
    let canopy = |blocks: &mut [u16], set: &mut dyn FnMut(&mut [u16], i32, i32, i32, u16, bool), r: &mut Xoro, y: i32, rad: i32, skip_corners: bool| {
        for dx in -rad..=rad { for dz in -rad..=rad {
            if dx.abs() == rad && dz.abs() == rad && (rad == 0 || (skip_corners && r.next_boolean()) || rad >= 3 && r.next_int_bound(3) != 0) { continue; }
            set(blocks, dx, y, dz, leaf, true);
        } }
    };
    match kind {
        Tree::Oak | Tree::Birch | Tree::Cherry => {
            for dy in (h - 3)..=h { let rad = if dy >= h - 1 { 1 } else { 2 }; canopy(blocks, &mut set, r, base + dy, rad, true); }
            for dy in 0..h { set(blocks, 0, base + dy, 0, log, false); }
        }
        Tree::DarkOak => {
            for dy in (h - 3)..=h { let rad = if dy >= h - 1 { 2 } else { 3 }; canopy(blocks, &mut set, r, base + dy, rad, true); }
            for dy in 0..h { set(blocks, 0, base + dy, 0, log, false); }
        }
        Tree::Spruce => {
            for dy in 2..=(h + 1) {
                let t = h + 1 - dy;
                let rad = ((t + 1) / 2).min(3);
                canopy(blocks, &mut set, r, base + dy, rad, false);
            }
            for dy in 0..=h { set(blocks, 0, base + dy, 0, log, false); }
        }
        Tree::Jungle => {
            for (dy, rad) in [(h - 3, 3), (h - 2, 3), (h - 1, 2), (h, 1)] { canopy(blocks, &mut set, r, base + dy, rad, true); }
            for dy in 0..h { set(blocks, 0, base + dy, 0, log, false); }
        }
        Tree::Acacia => {
            for dy in 0..h { set(blocks, 0, base + dy, 0, log, false); }
            let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][r.next_int_bound(4) as usize];
            set(blocks, dx, base + h, dz, log, false);
            set(blocks, dx, base + h + 1, dz, log, false);
            let (cx0, cz0) = (dx, dz);
            for (dy, rad) in [(h + 1, 2i32), (h + 2, 1i32)] {
                for ox in -rad..=rad { for oz in -rad..=rad {
                    if ox.abs() == rad && oz.abs() == rad && r.next_boolean() { continue; }
                    set(blocks, cx0 + ox, base + dy, cz0 + oz, leaf, true);
                } }
            }
            for dy in 0..h { set(blocks, 0, base + dy, 0, log, false); }
        }
    }
}
