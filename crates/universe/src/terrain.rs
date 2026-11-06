//! Port of `PlanetTerrain`: deterministic height, material, cave, ore and crust functions.

use crate::catalog::{mix, Planet, SystemId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Air, Stone, Basalt, Tuff, EndStone, Deepslate, SnowBlock, Blackstone, Sand, GreenTerracotta,
    GrassBlock, Dirt, Sandstone, RedSandstone, IronOre, GoldOre, RedstoneOre, DiamondOre, CoalOre,
    Lava, Water, Ice, PackedIce, MagmaBlock, WarpedWartBlock, RootedDirt, MossBlock, Clay,
    /// `OAK_LEAVES` with `persistent=true`.
    PersistentOakLeaves,
}

impl Block {
    /// Minecraft registry path (used to compare against the Java golden output).
    pub fn name(self) -> &'static str {
        use Block::*;
        match self {
            Air => "air", Stone => "stone", Basalt => "basalt", Tuff => "tuff", EndStone => "end_stone",
            Deepslate => "deepslate", SnowBlock => "snow_block", Blackstone => "blackstone", Sand => "sand",
            GreenTerracotta => "green_terracotta", GrassBlock => "grass_block", Dirt => "dirt",
            Sandstone => "sandstone", RedSandstone => "red_sandstone", IronOre => "iron_ore",
            GoldOre => "gold_ore", RedstoneOre => "redstone_ore", DiamondOre => "diamond_ore",
            CoalOre => "coal_ore", Lava => "lava", Water => "water", Ice => "ice", PackedIce => "packed_ice",
            MagmaBlock => "magma_block", WarpedWartBlock => "warped_wart_block", RootedDirt => "rooted_dirt",
            MossBlock => "moss_block", Clay => "clay", PersistentOakLeaves => "oak_leaves",
        }
    }
}

const K1: i64 = 0x632be59bd9b4e019;
const K2: i64 = 0x9e3779b97f4a7c15u64 as i64;
const K3: i64 = 0x94d049bb133111ebu64 as i64;

fn is_first_destination(p: &Planet) -> bool {
    let c = p.native_center();
    p.id.system == SystemId::SOL && p.parent_body == -1 && c.x == 4096.0 && c.z == 0.0
}

pub fn height(p: &Planet, x: i32, z: i32) -> i32 {
    let c = p.native_center();
    let (lx, lz) = (x as f64 - c.x, z as f64 - c.z);
    if p.parent_body == -2 {
        let sq = p.radius * p.radius - lx * lx - lz * lz;
        return (c.y + if sq < 0.0 { -p.radius } else { sq.sqrt() }).floor() as i32;
    }
    let broad = noise(p.seed, lx / 180.0, 0.0, lz / 180.0);
    let fine = noise(p.seed + 1, lx / 37.0, 0.0, lz / 37.0);
    let mountain = noise(p.seed + 2, lx / 360.0, 0.0, lz / 360.0).abs();
    let mut terrain = 55.0 + broad * (18.0 + p.environment.roughness * 70.0) + fine * 8.0 + mountain * 28.0;
    if is_first_destination(p) {
        let shore = p.radius * 0.7 - lx.hypot(lz);
        if shore > 0.0 {
            terrain = terrain.max(sea_level(p) as f64 + 16f64.min(shore * 0.4) + fine * 2.0);
        }
    }
    let edge = (p.radius - lx.abs()).min(p.radius - lz.abs());
    terrain -= (48.0 - edge).max(0.0) * 2.0;
    let lo = (-55).max(p.bottom() + 2) as f64;
    lo.max(220f64.min(terrain)) as i32
}

pub fn surface_height(p: &Planet, x: i32, z: i32) -> i32 {
    let t = height(p, x, z);
    if p.parent_body >= -1 && p.environment.water >= 0.07 { t.max(sea_level(p)) } else { t }
}

pub fn sea_level(p: &Planet) -> i32 {
    (35.0 + p.environment.water * 70.0) as i32
}

pub fn rock(p: &Planet) -> Block {
    let e = &p.environment;
    if e.temperature > 0.9 { Block::Basalt }
    else if e.radiation > 0.7 { Block::Tuff }
    else if e.atmosphere < 0.15 { Block::EndStone }
    else { Block::Stone }
}

pub fn surface(p: &Planet) -> Block {
    let e = &p.environment;
    if e.temperature < 0.15 { Block::SnowBlock }
    else if e.temperature > 0.9 { Block::Blackstone }
    else if e.water < 0.2 { Block::Sand }
    else if e.radiation > 0.7 { Block::GreenTerracotta }
    else if e.vegetation > 0.15 { Block::GrassBlock }
    else { rock(p) }
}

pub fn block(p: &Planet, x: i32, y: i32, z: i32, height: i32) -> Block {
    let c = p.native_center();
    if p.parent_body == -2 {
        let (dx, dy, dz) = (x as f64 - c.x, y as f64 - c.y, z as f64 - c.z);
        if dx * dx + dy * dy + dz * dz > p.radius * p.radius {
            return Block::Air;
        }
        let ore = mix(p.seed ^ (x as i64).wrapping_mul(K1) ^ (y as i64).wrapping_mul(K2) ^ z as i64) & 127;
        return if ore < 5 { Block::IronOre } else if ore < 8 { Block::GoldOre }
            else if ore < 10 { Block::RedstoneOre } else { Block::Stone };
    }
    let bottom = p.bottom();
    if y < bottom {
        return Block::Air;
    }
    let shell = shell_depth(p, x as f64, y as f64, z as f64, height as f64);
    if shell < 10.0 && y < height - 4 {
        return crust(p, x, y, z, shell);
    }
    if y <= bottom + 12 {
        return rock(p);
    }
    if y < -64 {
        let layer = (((y - bottom) / 48) as i64 + (p.seed & 7)) & 7;
        return if layer == 0 { Block::Tuff } else if p.environment.temperature > 0.9 { Block::Basalt } else { Block::Deepslate };
    }
    let sea = sea_level(p);
    let lx = (x as f64 - c.x).floor() as i32;
    let lz = (z as f64 - c.z).floor() as i32;
    let e = &p.environment;
    if y > height {
        if y > sea || e.water < 0.07 { return Block::Air; }
        if e.temperature > 0.9 { return Block::Lava; }
        if e.temperature < 0.15 && y == sea { return Block::Ice; }
        return Block::Water;
    }
    if y == height { return surface(p); }
    if y > height - 4 { return if e.water < 0.2 { Block::Sandstone } else { Block::Dirt }; }
    if y > -55 && y < height - 7
        && noise(p.seed + 17, lx as f64 / 25.0, y as f64 / 18.0, lz as f64 / 25.0).abs() < 0.055
    {
        return if y < -48 && e.temperature > 0.9 { Block::Lava } else { Block::Air };
    }
    let sample = mix(p.seed ^ (lx as i64).wrapping_mul(K1) ^ (y as i64).wrapping_mul(K2) ^ (lz as i64).wrapping_mul(K3));
    let ore = (sample & 2047) as i32;
    if ore < 5 && y < 15 { Block::DiamondOre }
    else if ore < 18 && y < 40 { Block::RedstoneOre }
    else if ore < 35 { Block::IronOre }
    else if ore < 60 { Block::CoalOre }
    else { rock(p) }
}

pub fn shell_depth(p: &Planet, x: f64, y: f64, z: f64, top: f64) -> f64 {
    let c = p.native_center();
    (p.radius - (x - c.x).abs()).min(p.radius - (z - c.z).abs()).min((y - p.bottom() as f64).min(top - y))
}

pub fn crust(p: &Planet, x: i32, y: i32, z: i32, depth: f64) -> Block {
    let e = &p.environment;
    if depth > 3.0 {
        return if e.water < 0.2 { Block::Sandstone } else { Block::Dirt };
    }
    let c = p.native_center();
    let patch = noise(p.seed + 83, (x as f64 - c.x) / 31.0, (y as f64 - c.y) / 31.0, (z as f64 - c.z) / 31.0);
    if e.temperature < 0.15 { return if patch > 0.35 { Block::PackedIce } else { Block::SnowBlock }; }
    if e.temperature > 0.9 { return if patch > 0.3 { Block::MagmaBlock } else { Block::Blackstone }; }
    if e.atmosphere < 0.15 { return if patch > 0.25 { Block::Tuff } else { Block::EndStone }; }
    if e.water < 0.2 { return if patch > 0.1 { Block::RedSandstone } else { Block::Sandstone }; }
    if e.radiation > 0.7 { return if patch > 0.25 { Block::WarpedWartBlock } else { Block::GreenTerracotta }; }
    if e.vegetation > 0.15 {
        if patch > 0.32 - e.vegetation * 0.25 { return Block::PersistentOakLeaves; }
        return if patch < -0.35 { Block::RootedDirt } else { Block::MossBlock };
    }
    if patch > 0.3 { Block::Clay } else { Block::Dirt }
}

pub fn noise(seed: i64, x: f64, y: f64, z: f64) -> f64 {
    let (ix, iy, iz) = (x.floor() as i64, y.floor() as i64, z.floor() as i64);
    let (fx, fy, fz) = (fade(x - ix as f64), fade(y - iy as f64), fade(z - iz as f64));
    let a = lerp(fx, sample(seed, ix, iy, iz), sample(seed, ix + 1, iy, iz));
    let b = lerp(fx, sample(seed, ix, iy + 1, iz), sample(seed, ix + 1, iy + 1, iz));
    let c = lerp(fx, sample(seed, ix, iy, iz + 1), sample(seed, ix + 1, iy, iz + 1));
    let d = lerp(fx, sample(seed, ix, iy + 1, iz + 1), sample(seed, ix + 1, iy + 1, iz + 1));
    lerp(fz, lerp(fy, a, b), lerp(fy, c, d))
}

fn fade(t: f64) -> f64 { t * t * (3.0 - 2.0 * t) }
fn lerp(t: f64, a: f64, b: f64) -> f64 { a + t * (b - a) }
fn sample(seed: i64, x: i64, y: i64, z: i64) -> f64 {
    let h = mix(seed ^ x.wrapping_mul(K1) ^ y.wrapping_mul(K2) ^ z.wrapping_mul(K3)) as u64;
    (h >> 11) as f64 * (1.0 / (1u64 << 53) as f64) * 2.0 - 1.0
}
