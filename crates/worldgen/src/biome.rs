//! Biome selection: multi-noise parameter table (nearest match) and vanilla's `BiomeManager` zoom.
use std::collections::HashMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::density::{Ctx, Jar, Router};

const TABLE: &str = include_str!("../data/overworld_biome_params.txt");

struct Point {
    min: [i64; 6],
    max: [i64; 6],
    offset: i64,
    biome: u16,
}

#[derive(Clone, Debug)]
pub struct BiomeInfo {
    pub name: String,
    pub temperature: f32,
    pub downfall: f32,
    pub frozen: bool,
    pub water_color: u32,
    pub grass_color: Option<u32>,
    pub foliage_color: Option<u32>,
    pub grass_modifier: String,
}

pub struct BiomeTable {
    points: Vec<Point>,
    pub biomes: Vec<BiomeInfo>,
    index: HashMap<String, u16>,
}

fn hex(v: &Value) -> Option<u32> { u32::from_str_radix(v.as_str()?.trim_start_matches('#'), 16).ok() }

impl BiomeTable {
    pub fn load(jar: &Jar) -> Self {
        let mut index: HashMap<String, u16> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut points = Vec::new();
        for line in TABLE.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            let v: Vec<i64> = f[..13].iter().map(|x| x.parse().unwrap()).collect();
            let b = *index.entry(f[13].to_string()).or_insert_with(|| { names.push(f[13].to_string()); (names.len() - 1) as u16 });
            points.push(Point {
                min: [v[0], v[2], v[4], v[6], v[8], v[10]],
                max: [v[1], v[3], v[5], v[7], v[9], v[11]],
                offset: v[12],
                biome: b,
            });
        }
        let biomes = names.iter().map(|n| {
            let j = jar.json(&format!("data/minecraft/worldgen/biome/{n}.json"));
            let e = &j["effects"];
            BiomeInfo {
                name: n.clone(),
                temperature: j["temperature"].as_f64().unwrap_or(0.5) as f32,
                downfall: j["downfall"].as_f64().unwrap_or(0.5) as f32,
                frozen: j["temperature_modifier"].as_str() == Some("frozen"),
                water_color: hex(&e["water_color"]).unwrap_or(0x3f76e4),
                grass_color: hex(&e["grass_color"]),
                foliage_color: hex(&e["foliage_color"]),
                grass_modifier: e["grass_color_modifier"].as_str().unwrap_or("none").to_string(),
            }
        }).collect();
        Self { points, biomes, index }
    }

    pub fn id(&self, name: &str) -> Option<u16> { self.index.get(name.trim_start_matches("minecraft:")).copied() }

    /// Nearest parameter point to a quantized climate target `[temp, humidity, continents, erosion, depth, weirdness]`.
    pub fn nearest(&self, t: [i64; 6]) -> u16 {
        let mut best = (i64::MAX, 0u16);
        for p in &self.points {
            let mut d = p.offset * p.offset;
            for i in 0..6 {
                let a = t[i] - p.max[i];
                let b = p.min[i] - t[i];
                let dist = if a > 0 { a } else { b.max(0) };
                d += dist * dist;
                if d >= best.0 { break; }
            }
            if d < best.0 { best = (d, p.biome); }
        }
        best.1
    }
}

/// `BiomeManager.obfuscateSeed`: SHA-256 of the little-endian seed, first 8 bytes little-endian.
pub fn obfuscate_seed(seed: i64) -> i64 {
    let h = Sha256::digest(seed.to_le_bytes());
    i64::from_le_bytes(h[0..8].try_into().unwrap())
}

fn lcg_next(seed: i64, inc: i64) -> i64 {
    seed.wrapping_mul(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)).wrapping_add(inc)
}

fn fiddle(v: i64) -> f64 { (((v >> 24).rem_euclid(1024)) as f64 / 1024.0 - 0.5) * 0.9 }

fn fiddled_distance(seed: i64, x: i32, y: i32, z: i32, dx: f64, dy: f64, dz: f64) -> f64 {
    let mut s = lcg_next(seed, x as i64);
    s = lcg_next(s, y as i64);
    s = lcg_next(s, z as i64);
    s = lcg_next(s, x as i64);
    s = lcg_next(s, y as i64);
    s = lcg_next(s, z as i64);
    let d0 = fiddle(s);
    s = lcg_next(s, seed);
    let d1 = fiddle(s);
    s = lcg_next(s, seed);
    let d2 = fiddle(s);
    let sq = |v: f64| v * v;
    sq(dz + d2) + sq(dy + d1) + sq(dx + d0)
}

pub struct BiomeSource<'a> {
    pub table: &'a BiomeTable,
    pub router: &'a Router,
    pub zoom_seed: i64,
    pub cache: std::cell::RefCell<HashMap<(i32, i32, i32), u16>>,
}

impl BiomeSource<'_> {
    /// Biome at a quart (4x4x4 block) cell.
    pub fn noise_biome(&self, qx: i32, qy: i32, qz: i32) -> u16 {
        if let Some(b) = self.cache.borrow().get(&(qx, qy, qz)) { return *b; }
        let b = self.noise_biome_uncached(qx, qy, qz);
        self.cache.borrow_mut().insert((qx, qy, qz), b);
        b
    }

    fn noise_biome_uncached(&self, qx: i32, qy: i32, qz: i32) -> u16 {
        let c = Ctx { x: qx << 2, y: qy << 2, z: qz << 2 };
        let q = |d: &crate::density::Df| (d.compute(&c) as f32 * 10000.0f32) as i64;
        let r = self.router;
        self.table.nearest([q(&r.temperature), q(&r.vegetation), q(&r.continents), q(&r.erosion), q(&r.depth), q(&r.ridges)])
    }

    /// Biome at a block position (vanilla `BiomeManager.getBiome`, with its fuzzy zoom).
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> u16 {
        let (i, j, k) = (x - 2, y - 2, z - 2);
        let (l, i1, j1) = (i >> 2, j >> 2, k >> 2);
        let (d0, d1, d2) = ((i & 3) as f64 / 4.0, (j & 3) as f64 / 4.0, (k & 3) as f64 / 4.0);
        let (mut best, mut dist) = (0, f64::INFINITY);
        for n in 0..8 {
            let (fx, fy, fz) = (n & 4 == 0, n & 2 == 0, n & 1 == 0);
            let (a, b, c) = (if fx { l } else { l + 1 }, if fy { i1 } else { i1 + 1 }, if fz { j1 } else { j1 + 1 });
            let d = fiddled_distance(self.zoom_seed, a, b, c, if fx { d0 } else { d0 - 1.0 }, if fy { d1 } else { d1 - 1.0 }, if fz { d2 } else { d2 - 1.0 });
            if dist > d { best = n; dist = d; }
        }
        let l2 = if best & 4 == 0 { l } else { l + 1 };
        let i3 = if best & 2 == 0 { i1 } else { i1 + 1 };
        let j3 = if best & 1 == 0 { j1 } else { j1 + 1 };
        self.noise_biome(l2, i3, j3)
    }
}
