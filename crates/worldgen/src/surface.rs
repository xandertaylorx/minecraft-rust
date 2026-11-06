//! Surface rules (data-driven from noise settings) and `SurfaceSystem`.
use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::biome::BiomeSource;
use crate::density::{Jar, RandomState};
use crate::noise::NormalNoise;
use crate::random::{Positional, Xoro};

pub enum Anchor { Absolute(i32), AboveBottom(i32), BelowTop(i32) }

impl Anchor {
    fn from_json(v: &Value) -> Anchor {
        if let Some(n) = v["absolute"].as_i64() { Anchor::Absolute(n as i32) }
        else if let Some(n) = v["above_bottom"].as_i64() { Anchor::AboveBottom(n as i32) }
        else { Anchor::BelowTop(v["below_top"].as_i64().unwrap() as i32) }
    }
    fn resolve(&self) -> i32 {
        match self { Anchor::Absolute(n) => *n, Anchor::AboveBottom(n) => -64 + n, Anchor::BelowTop(n) => 319 - n }
    }
}

pub enum Cond {
    StoneDepth { offset: i32, add_surface_depth: bool, secondary: i32, ceiling: bool },
    YAbove { y: i32, mult: i32, add_stone: bool },
    Water { offset: i32, mult: i32, add_stone: bool },
    Biome(HashSet<u16>),
    Noise { noise: Arc<NormalNoise>, min: f64, max: f64 },
    Not(Box<Cond>),
    Steep,
    Hole,
    Gradient { random: Positional, below: i32, above: i32 },
    AbovePrelim,
    Temperature,
}

pub enum Rule {
    Block(u16),
    Seq(Vec<Rule>),
    Test(Cond, Box<Rule>),
    Bandlands,
}

/// Interned block names produced by the generator.
#[derive(Default)]
pub struct Palette {
    pub names: Vec<String>,
}

impl Palette {
    pub fn id(&mut self, name: &str) -> u16 {
        let n = name.trim_start_matches("minecraft:");
        if let Some(i) = self.names.iter().position(|x| x == n) { return i as u16; }
        self.names.push(n.to_string());
        (self.names.len() - 1) as u16
    }
}

pub fn parse_rule(v: &Value, rs: &RandomState, biomes: &crate::biome::BiomeTable, pal: &mut Palette) -> Rule {
    match v["type"].as_str().unwrap().trim_start_matches("minecraft:") {
        "block" => Rule::Block(pal.id(v["result_state"]["Name"].as_str().unwrap())),
        "sequence" => Rule::Seq(v["sequence"].as_array().unwrap().iter().map(|r| parse_rule(r, rs, biomes, pal)).collect()),
        "condition" => Rule::Test(parse_cond(&v["if_true"], rs, biomes), Box::new(parse_rule(&v["then_run"], rs, biomes, pal))),
        "bandlands" => Rule::Bandlands,
        t => panic!("unsupported surface rule {t}"),
    }
}

fn parse_cond(v: &Value, rs: &RandomState, biomes: &crate::biome::BiomeTable) -> Cond {
    match v["type"].as_str().unwrap().trim_start_matches("minecraft:") {
        "stone_depth" => Cond::StoneDepth {
            offset: v["offset"].as_i64().unwrap() as i32,
            add_surface_depth: v["add_surface_depth"].as_bool().unwrap(),
            secondary: v["secondary_depth_range"].as_i64().unwrap() as i32,
            ceiling: v["surface_type"].as_str().unwrap() == "ceiling",
        },
        "y_above" => Cond::YAbove {
            y: Anchor::from_json(&v["anchor"]).resolve(),
            mult: v["surface_depth_multiplier"].as_i64().unwrap() as i32,
            add_stone: v["add_stone_depth"].as_bool().unwrap(),
        },
        "water" => Cond::Water {
            offset: v["offset"].as_i64().unwrap() as i32,
            mult: v["surface_depth_multiplier"].as_i64().unwrap() as i32,
            add_stone: v["add_stone_depth"].as_bool().unwrap(),
        },
        "biome" => Cond::Biome(v["biome_is"].as_array().unwrap().iter().filter_map(|b| biomes.id(b.as_str().unwrap())).collect()),
        "noise_threshold" => Cond::Noise {
            noise: rs.noise(v["noise"].as_str().unwrap()).unwrap(),
            min: v["min_threshold"].as_f64().unwrap(),
            max: v["max_threshold"].as_f64().unwrap(),
        },
        "not" => Cond::Not(Box::new(parse_cond(&v["invert"], rs, biomes))),
        "steep" => Cond::Steep,
        "hole" => Cond::Hole,
        "vertical_gradient" => {
            let name = v["random_name"].as_str().unwrap();
            Cond::Gradient {
                random: rs.root.from_hash_of(name).fork_positional(),
                below: Anchor::from_json(&v["true_at_and_below"]).resolve(),
                above: Anchor::from_json(&v["false_at_and_above"]).resolve(),
            }
        }
        "above_preliminary_surface" => Cond::AbovePrelim,
        "temperature" => Cond::Temperature,
        t => panic!("unsupported surface condition {t}"),
    }
}

pub struct SurfaceSystem {
    surface: Arc<NormalNoise>,
    secondary: Arc<NormalNoise>,
    band_offset: Arc<NormalNoise>,
    pub bands: Vec<u16>,
    noise_random: Positional,
}

impl SurfaceSystem {
    pub fn new(rs: &RandomState, pal: &mut Palette) -> Self {
        let noise_random = rs.root;
        let mut r = noise_random.from_hash_of("minecraft:clay_bands");
        let bands = generate_bands(&mut r, pal);
        Self {
            surface: rs.noise("surface").unwrap(),
            secondary: rs.noise("surface_secondary").unwrap(),
            band_offset: rs.noise("clay_bands_offset").unwrap(),
            bands,
            noise_random,
        }
    }

    pub fn surface_depth(&self, x: i32, z: i32) -> i32 {
        let d0 = self.surface.get_value(x as f64, 0.0, z as f64);
        (d0 * 2.75 + 3.0 + self.noise_random.at(x, 0, z).next_double() * 0.25) as i32
    }

    pub fn band(&self, x: i32, y: i32, z: i32) -> u16 {
        let i = (self.band_offset.get_value(x as f64, 0.0, z as f64) * 4.0).round() as i32;
        let n = self.bands.len() as i32;
        self.bands[((y + i + n) % n) as usize]
    }
}

fn next_between(r: &mut Xoro, lo: i32, hi: i32) -> i32 { r.next_int_bound(hi - lo + 1) + lo }

fn generate_bands(r: &mut Xoro, pal: &mut Palette) -> Vec<u16> {
    let terracotta = pal.id("terracotta");
    let mut b = vec![terracotta; 192];
    let mut k = 0usize;
    let orange = pal.id("orange_terracotta");
    while k < b.len() {
        k += r.next_int_bound(5) as usize + 1;
        if k < b.len() { b[k] = orange; }
        k += 1;
    }
    let mut make = |r: &mut Xoro, b: &mut Vec<u16>, base: i32, id: u16| {
        let i = next_between(r, 6, 15);
        for _ in 0..i {
            let k = base + r.next_int_bound(3);
            let l = r.next_int_bound(b.len() as i32) as usize;
            let mut i1 = 0;
            while l + i1 < b.len() && (i1 as i32) < k { b[l + i1] = id; i1 += 1; }
        }
    };
    let (yel, bro, red) = (pal.id("yellow_terracotta"), pal.id("brown_terracotta"), pal.id("red_terracotta"));
    make(r, &mut b, 1, yel);
    make(r, &mut b, 2, bro);
    make(r, &mut b, 1, red);
    let l = next_between(r, 9, 15);
    let (white, gray) = (pal.id("white_terracotta"), pal.id("light_gray_terracotta"));
    let (mut i, mut j) = (0, 0usize);
    while i < l && j < b.len() {
        b[j] = white;
        if j > 1 && r.next_boolean() { b[j - 1] = gray; }
        if j + 1 < b.len() && r.next_boolean() { b[j + 1] = gray; }
        i += 1;
        j += r.next_int_bound(16) as usize + 4;
    }
    b
}

/// Per-column evaluation state (vanilla `SurfaceRules.Context`).
pub struct Context<'a> {
    pub sys: &'a SurfaceSystem,
    pub biomes: &'a BiomeSource<'a>,
    pub biome_names: &'a [crate::biome::BiomeInfo],
    pub block_x: i32,
    pub block_z: i32,
    pub block_y: i32,
    pub surface_depth: i32,
    pub surface_secondary: Option<f64>,
    pub min_surface_level: Option<i32>,
    pub water_height: i32,
    pub stone_below: i32,
    pub stone_above: i32,
    pub biome: Option<u16>,
    pub prelim: &'a dyn Fn(i32, i32) -> i32,
    pub steep: bool,
    pub sea_level: i32,
}

impl Context<'_> {
    fn biome(&mut self) -> u16 {
        if let Some(b) = self.biome { return b; }
        let b = self.biomes.biome_at(self.block_x, self.block_y, self.block_z);
        self.biome = Some(b);
        b
    }
    pub fn update_xz(&mut self, x: i32, z: i32, steep: bool) {
        self.block_x = x;
        self.block_z = z;
        self.surface_depth = self.sys.surface_depth(x, z);
        self.surface_secondary = None;
        self.min_surface_level = None;
        self.steep = steep;
    }
    pub fn update_y(&mut self, above: i32, below: i32, water: i32, y: i32) {
        self.biome = None;
        self.block_y = y;
        self.water_height = water;
        self.stone_below = below;
        self.stone_above = above;
    }
    fn secondary(&mut self) -> f64 {
        *self.surface_secondary.get_or_insert_with(|| self.sys.secondary.get_value(self.block_x as f64, 0.0, self.block_z as f64))
    }
    fn min_surface(&mut self) -> i32 {
        if let Some(v) = self.min_surface_level { return v; }
        let (i, j) = (self.block_x >> 4, self.block_z >> 4);
        let c = [(self.prelim)(i << 4, j << 4), (self.prelim)((i + 1) << 4, j << 4), (self.prelim)(i << 4, (j + 1) << 4), (self.prelim)((i + 1) << 4, (j + 1) << 4)];
        let (fx, fz) = ((self.block_x & 15) as f32 / 16.0f32, (self.block_z & 15) as f32 / 16.0f32);
        let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
        let (fx, fz) = (fx as f64, fz as f64);
        let v = lerp(fz, lerp(fx, c[0] as f64, c[1] as f64), lerp(fx, c[2] as f64, c[3] as f64));
        let fl = { let i = v as i32; if v < i as f64 { i - 1 } else { i } };
        let r = fl + self.surface_depth - 8;
        self.min_surface_level = Some(r);
        r
    }

    fn cond(&mut self, c: &Cond) -> bool {
        match c {
            Cond::StoneDepth { offset, add_surface_depth, secondary, ceiling } => {
                let i = if *ceiling { self.stone_below } else { self.stone_above };
                let j = if *add_surface_depth { self.surface_depth } else { 0 };
                let k = if *secondary == 0 { 0 } else {
                    let s = self.secondary();
                    (0.0 + ((s - -1.0) / 2.0) * *secondary as f64) as i32
                };
                i <= 1 + offset + j + k
            }
            Cond::YAbove { y, mult, add_stone } =>
                self.block_y + if *add_stone { self.stone_above } else { 0 } >= y + self.surface_depth * mult,
            Cond::Water { offset, mult, add_stone } =>
                self.water_height == i32::MIN
                    || self.block_y + if *add_stone { self.stone_above } else { 0 } >= self.water_height + offset + self.surface_depth * mult,
            Cond::Biome(set) => { let b = self.biome(); set.contains(&b) }
            Cond::Noise { noise, min, max } => {
                let d = noise.get_value(self.block_x as f64, 0.0, self.block_z as f64);
                d >= *min && d <= *max
            }
            Cond::Not(c) => !self.cond(c),
            Cond::Steep => self.steep,
            Cond::Hole => self.surface_depth <= 0,
            Cond::Gradient { random, below, above } => {
                let k = self.block_y;
                if k <= *below { true } else if k >= *above { false } else {
                    let d0 = 1.0 + ((k - below) as f64 / (above - below) as f64) * (0.0 - 1.0);
                    random.at(self.block_x, k, self.block_z).next_float() < d0 as f32
                }
            }
            Cond::AbovePrelim => self.block_y >= self.min_surface(),
            Cond::Temperature => {
                let b = self.biome();
                let info = &self.biome_names[b as usize];
                let t = info.temperature;
                // Height adjustment above the snow line (the legacy temperature noise term is omitted).
                let i = self.sea_level + 17;
                let t = if self.block_y > i { t - (self.block_y - i) as f32 * 0.05 / 40.0 } else { t };
                t < 0.15
            }
        }
    }

    pub fn apply(&mut self, r: &Rule) -> Option<u16> {
        match r {
            Rule::Block(b) => Some(*b),
            Rule::Seq(v) => v.iter().find_map(|x| self.apply(x)),
            Rule::Test(c, then) => if self.cond(c) { self.apply(then) } else { None },
            Rule::Bandlands => Some(self.sys.band(self.block_x, self.block_y, self.block_z)),
        }
    }
}

pub fn load_surface_rule(jar: &Jar, rs: &RandomState, biomes: &crate::biome::BiomeTable, pal: &mut Palette) -> Rule {
    let s = jar.json("data/minecraft/worldgen/noise_settings/overworld.json");
    parse_rule(&s["surface_rule"], rs, biomes, pal)
}
