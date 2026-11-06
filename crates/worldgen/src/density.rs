//! Data-driven density functions: loaded from Mojang's worldgen JSON and evaluated exactly.
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use zip::ZipArchive;

use crate::noise::{clamped_lerp, BlendedNoise, NormalNoise};
use crate::random::{Positional, Xoro};

pub struct Spline {
    coord: Arc<Df>,
    locs: Vec<f32>,
    vals: Vec<SplineVal>,
    ders: Vec<f32>,
}

pub enum SplineVal {
    Const(f32),
    Sub(Spline),
}

fn lerpf(t: f32, a: f32, b: f32) -> f32 { a + t * (b - a) }

impl SplineVal {
    fn apply(&self, c: &(Ctx, Option<&Eval>)) -> f32 {
        match self { SplineVal::Const(v) => *v, SplineVal::Sub(s) => s.apply(c) }
    }
}

impl Spline {
    pub fn coords(&self) -> Vec<&Arc<Df>> {
        let mut v = vec![&self.coord];
        for x in &self.vals { if let SplineVal::Sub(s) = x { v.extend(s.coords()); } }
        v
    }

    fn linear_extend(f: f32, locs: &[f32], v: f32, ders: &[f32], i: usize) -> f32 {
        let d = ders[i];
        if d == 0.0 { v } else { v + d * (f - locs[i]) }
    }

    pub fn apply(&self, c: &(Ctx, Option<&Eval>)) -> f32 {
        let f = self.coord.eval(&c.0, c.1) as f32;
        // first index whose location is greater than f, minus one
        let n = self.locs.len();
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if f < self.locs[mid] { hi = mid; } else { lo = mid + 1; }
        }
        let i = lo as i64 - 1;
        let j = n - 1;
        if i < 0 {
            Self::linear_extend(f, &self.locs, self.vals[0].apply(c), &self.ders, 0)
        } else if i as usize == j {
            Self::linear_extend(f, &self.locs, self.vals[j].apply(c), &self.ders, j)
        } else {
            let i = i as usize;
            let (f1, f2) = (self.locs[i], self.locs[i + 1]);
            let f3 = (f - f1) / (f2 - f1);
            let (f4, f5) = (self.ders[i], self.ders[i + 1]);
            let (f6, f7) = (self.vals[i].apply(c), self.vals[i + 1].apply(c));
            let f8 = f4 * (f2 - f1) - (f7 - f6);
            let f9 = -f5 * (f2 - f1) + (f7 - f6);
            lerpf(f3, f6, f7) + f3 * (1.0 - f3) * lerpf(f3, f8, f9)
        }
    }
}

#[derive(Clone, Copy)]
pub struct Ctx {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op { Add, Mul, Min, Max }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapKind { Abs, Square, Cube, HalfNegative, QuarterNegative, Invert, Squeeze }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerKind { Interpolated, FlatCache, Cache2D, CacheOnce, CacheAllInCell }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftKind { Both, A, B }

pub type OptNoise = Option<Arc<NormalNoise>>;

pub enum Df {
    Const(f64),
    Ap2(Op, Arc<Df>, Arc<Df>),
    Map(MapKind, Arc<Df>),
    Clamp(Arc<Df>, f64, f64),
    Marker(MarkerKind, Arc<Df>),
    Noise { noise: OptNoise, xz: f64, y: f64 },
    ShiftedNoise { sx: Arc<Df>, sy: Arc<Df>, sz: Arc<Df>, xz: f64, y: f64, noise: OptNoise },
    Shift(ShiftKind, OptNoise),
    Weird { input: Arc<Df>, noise: OptNoise, type2: bool },
    Range { input: Arc<Df>, min: f64, max: f64, inside: Arc<Df>, outside: Arc<Df> },
    Spline(Arc<Spline>),
    YGradient { from_y: i32, to_y: i32, from_v: f64, to_v: f64 },
    Blended(Arc<BlendedNoise>),
    FindTopSurface { density: Arc<Df>, upper: Arc<Df>, lower: i32, cell_h: i32 },
    BlendAlpha,
    BlendOffset,
    BlendDensity(Arc<Df>),
    Beardifier,
}

fn mth_floor(v: f64) -> i32 {
    let i = v as i32;
    if v < i as f64 { i - 1 } else { i }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 { if v < lo { lo } else { v.min(hi) } }

/// Per-chunk cell-corner grids for `interpolated` nodes (vanilla's `NoiseChunk`).
pub struct Eval {
    pub x0: i32,
    pub z0: i32,
    pub min_y: i32,
    pub grids: HashMap<usize, Vec<f64>>,
}

pub const CELL_W: i32 = 4;
pub const CELL_H: i32 = 8;
pub const GRID_NX: usize = 5;
pub const GRID_NY: usize = 49;

fn lerp(t: f64, a: f64, b: f64) -> f64 { a + t * (b - a) }

impl Eval {
    pub fn sample(&self, node: *const Df, c: &Ctx) -> Option<f64> {
        let g = self.grids.get(&(node as usize))?;
        let (lx, lz, ly) = (c.x - self.x0, c.z - self.z0, c.y - self.min_y);
        let (ci, cj, ck) = (lx.div_euclid(CELL_W) as usize, lz.div_euclid(CELL_W) as usize, ly.div_euclid(CELL_H) as usize);
        let (fx, fz, fy) = (lx.rem_euclid(CELL_W) as f64 / CELL_W as f64, lz.rem_euclid(CELL_W) as f64 / CELL_W as f64, ly.rem_euclid(CELL_H) as f64 / CELL_H as f64);
        let at = |i: usize, j: usize, k: usize| g[(i * GRID_NX + j) * GRID_NY + k];
        let v00 = lerp(fy, at(ci, cj, ck), at(ci, cj, ck + 1));
        let v10 = lerp(fy, at(ci + 1, cj, ck), at(ci + 1, cj, ck + 1));
        let v01 = lerp(fy, at(ci, cj + 1, ck), at(ci, cj + 1, ck + 1));
        let v11 = lerp(fy, at(ci + 1, cj + 1, ck), at(ci + 1, cj + 1, ck + 1));
        Some(lerp(fz, lerp(fx, v00, v10), lerp(fx, v01, v11)))
    }
}

impl Df {
    pub fn children(&self) -> Vec<&Arc<Df>> {
        match self {
            Df::Ap2(_, a, b) => vec![a, b],
            Df::Map(_, a) | Df::Clamp(a, _, _) | Df::Marker(_, a) | Df::BlendDensity(a) => vec![a],
            Df::ShiftedNoise { sx, sy, sz, .. } => vec![sx, sy, sz],
            Df::Weird { input, .. } => vec![input],
            Df::Range { input, inside, outside, .. } => vec![input, inside, outside],
            Df::FindTopSurface { density, upper, .. } => vec![density, upper],
            Df::Spline(s) => s.coords(),
            _ => vec![],
        }
    }

    /// All distinct `interpolated` marker nodes reachable from here.
    pub fn collect_interpolated<'a>(&'a self, out: &mut Vec<&'a Df>, seen: &mut std::collections::HashSet<usize>) {
        if !seen.insert(self as *const Df as usize) { return; }
        if let Df::Marker(MarkerKind::Interpolated, _) = self { out.push(self); }
        for c in self.children() { c.collect_interpolated(out, seen); }
    }
}

impl Df {
    pub fn compute(&self, c: &Ctx) -> f64 { self.eval(c, None) }

    pub fn eval(&self, c: &Ctx, ev: Option<&Eval>) -> f64 {
        match self {
            Df::Const(v) => *v,
            Df::Ap2(op, a, b) => {
                let d0 = a.eval(c, ev);
                match op {
                    Op::Add => d0 + b.eval(c, ev),
                    Op::Mul => if d0 == 0.0 { 0.0 } else { d0 * b.eval(c, ev) },
                    Op::Min => d0.min(b.eval(c, ev)),
                    Op::Max => d0.max(b.eval(c, ev)),
                }
            }
            Df::Map(k, a) => {
                let v = a.eval(c, ev);
                match k {
                    MapKind::Abs => v.abs(),
                    MapKind::Square => v * v,
                    MapKind::Cube => v * v * v,
                    MapKind::HalfNegative => if v > 0.0 { v } else { v * 0.5 },
                    MapKind::QuarterNegative => if v > 0.0 { v } else { v * 0.25 },
                    MapKind::Invert => 1.0 / v,
                    MapKind::Squeeze => { let d = clamp(v, -1.0, 1.0); d / 2.0 - d * d * d / 24.0 }
                }
            }
            Df::Clamp(a, lo, hi) => clamp(a.eval(c, ev), *lo, *hi),
            Df::Marker(kind, a) => match (kind, ev) {
                (MarkerKind::Interpolated, Some(e)) => match e.sample(self as *const Df, c) {
                    Some(v) => v,
                    None => a.eval(c, ev),
                },
                (MarkerKind::FlatCache, Some(_)) => a.eval(&Ctx { x: (c.x >> 2) << 2, y: 0, z: (c.z >> 2) << 2 }, ev),
                _ => a.eval(c, ev),
            },
            Df::BlendDensity(a) => a.eval(c, ev),
            Df::Noise { noise, xz, y } => match noise {
                Some(n) => n.get_value(c.x as f64 * xz, c.y as f64 * y, c.z as f64 * xz),
                None => 0.0,
            },
            Df::ShiftedNoise { sx, sy, sz, xz, y, noise } => {
                let d0 = c.x as f64 * xz + sx.eval(c, ev);
                let d1 = c.y as f64 * y + sy.eval(c, ev);
                let d2 = c.z as f64 * xz + sz.eval(c, ev);
                noise.as_ref().map_or(0.0, |n| n.get_value(d0, d1, d2))
            }
            Df::Shift(kind, noise) => {
                let (x, y, z) = match kind {
                    ShiftKind::Both => (c.x as f64, c.y as f64, c.z as f64),
                    ShiftKind::A => (c.x as f64, 0.0, c.z as f64),
                    ShiftKind::B => (c.z as f64, c.x as f64, 0.0),
                };
                noise.as_ref().map_or(0.0, |n| n.get_value(x * 0.25, y * 0.25, z * 0.25) * 4.0)
            }
            Df::Weird { input, noise, type2 } => {
                let v = input.eval(c, ev);
                let d0 = if *type2 {
                    if v < -0.75 { 0.5 } else if v < -0.5 { 0.75 } else if v < 0.5 { 1.0 } else if v < 0.75 { 2.0 } else { 3.0 }
                } else if v < -0.5 { 0.75 } else if v < 0.0 { 1.0 } else if v < 0.5 { 1.5 } else { 2.0 };
                d0 * noise.as_ref().map_or(0.0, |n| n.get_value(c.x as f64 / d0, c.y as f64 / d0, c.z as f64 / d0)).abs()
            }
            Df::Range { input, min, max, inside, outside } => {
                let v = input.eval(c, ev);
                if v >= *min && v < *max { inside.eval(c, ev) } else { outside.eval(c, ev) }
            }
            Df::Spline(s) => s.apply(&(*c, ev)) as f64,
            Df::YGradient { from_y, to_y, from_v, to_v } => {
                let t = (c.y as f64 - *from_y as f64) / (*to_y as f64 - *from_y as f64);
                clamped_lerp(t, *from_v, *to_v)
            }
            Df::Blended(b) => b.compute(c.x, c.y, c.z),
            Df::FindTopSurface { density, upper, lower, cell_h } => {
                let i = mth_floor(upper.eval(c, ev) / *cell_h as f64) * cell_h;
                if i <= *lower { return *lower as f64; }
                let mut j = i;
                while j >= *lower {
                    if density.eval(&Ctx { x: c.x, y: j, z: c.z }, ev) > 0.0 { return j as f64; }
                    j -= cell_h;
                }
                *lower as f64
            }
            Df::BlendAlpha => 1.0,
            Df::BlendOffset | Df::Beardifier => 0.0,
        }
    }
}

// ------------------------------------------------------------------ loading

pub struct Jar {
    zip: RefCell<ZipArchive<File>>,
}

impl Jar {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let f = File::open(path.as_ref()).unwrap_or_else(|e| panic!("cannot open {}: {e}", path.as_ref().display()));
        Self { zip: RefCell::new(ZipArchive::new(f).expect("not a zip")) }
    }
    pub fn json(&self, path: &str) -> Value {
        let mut z = self.zip.borrow_mut();
        let mut f = z.by_name(path).unwrap_or_else(|_| panic!("missing {path}"));
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        serde_json::from_str(&s).unwrap()
    }
}

fn strip_ns(id: &str) -> &str { id.strip_prefix("minecraft:").unwrap_or(id) }

/// `RandomState`: world seed + lazily created named noises and loaded functions.
pub struct RandomState {
    pub jar: Jar,
    pub root: Positional,
    noises: RefCell<HashMap<String, Option<Arc<NormalNoise>>>>,
    named: RefCell<HashMap<String, Arc<Df>>>,
    blended: RefCell<Option<Arc<BlendedNoise>>>,
}

impl RandomState {
    pub fn new(jar: Jar, seed: i64) -> Self {
        let root = Xoro::new(seed).fork_positional();
        Self { jar, root, noises: Default::default(), named: Default::default(), blended: RefCell::new(None) }
    }

    pub fn noise(&self, id: &str) -> OptNoise {
        let id = if id.contains(':') { id.to_string() } else { format!("minecraft:{id}") };
        if let Some(n) = self.noises.borrow().get(&id) { return n.clone(); }
        let v = self.jar.json(&format!("data/minecraft/worldgen/noise/{}.json", strip_ns(&id)));
        let first = v["firstOctave"].as_i64().unwrap() as i32;
        let amps: Vec<f64> = v["amplitudes"].as_array().unwrap().iter().map(|a| a.as_f64().unwrap()).collect();
        let mut r = self.root.from_hash_of(&id);
        let n = Some(Arc::new(NormalNoise::create(&mut r, first, &amps)));
        self.noises.borrow_mut().insert(id, n.clone());
        n
    }

    pub fn named(&self, id: &str) -> Arc<Df> {
        let key = strip_ns(id).to_string();
        if let Some(d) = self.named.borrow().get(&key) { return d.clone(); }
        let v = self.jar.json(&format!("data/minecraft/worldgen/density_function/{key}.json"));
        let d = self.load(&v);
        self.named.borrow_mut().insert(key, d.clone());
        d
    }

    fn num(v: &Value, k: &str) -> f64 { v[k].as_f64().unwrap_or_else(|| panic!("missing number {k} in {v}")) }

    pub fn load(&self, v: &Value) -> Arc<Df> {
        if let Some(n) = v.as_f64() { return Arc::new(Df::Const(n)); }
        if let Some(s) = v.as_str() { return self.named(s); }
        let ty = strip_ns(v["type"].as_str().unwrap_or_else(|| panic!("no type in {v}")));
        let arg = |k: &str| self.load(&v[k]);
        let df = match ty {
            "constant" => Df::Const(Self::num(v, "argument")),
            "add" => Df::Ap2(Op::Add, arg("argument1"), arg("argument2")),
            "mul" => Df::Ap2(Op::Mul, arg("argument1"), arg("argument2")),
            "min" => Df::Ap2(Op::Min, arg("argument1"), arg("argument2")),
            "max" => Df::Ap2(Op::Max, arg("argument1"), arg("argument2")),
            "abs" => Df::Map(MapKind::Abs, arg("argument")),
            "square" => Df::Map(MapKind::Square, arg("argument")),
            "cube" => Df::Map(MapKind::Cube, arg("argument")),
            "half_negative" => Df::Map(MapKind::HalfNegative, arg("argument")),
            "quarter_negative" => Df::Map(MapKind::QuarterNegative, arg("argument")),
            "invert" => Df::Map(MapKind::Invert, arg("argument")),
            "squeeze" => Df::Map(MapKind::Squeeze, arg("argument")),
            "clamp" => Df::Clamp(arg("input"), Self::num(v, "min"), Self::num(v, "max")),
            "interpolated" => Df::Marker(MarkerKind::Interpolated, arg("argument")),
            "flat_cache" => Df::Marker(MarkerKind::FlatCache, arg("argument")),
            "cache_2d" => Df::Marker(MarkerKind::Cache2D, arg("argument")),
            "cache_once" => Df::Marker(MarkerKind::CacheOnce, arg("argument")),
            "cache_all_in_cell" => Df::Marker(MarkerKind::CacheAllInCell, arg("argument")),
            "noise" => Df::Noise { noise: self.noise(v["noise"].as_str().unwrap()), xz: Self::num(v, "xz_scale"), y: Self::num(v, "y_scale") },
            "shifted_noise" => Df::ShiftedNoise {
                sx: arg("shift_x"), sy: arg("shift_y"), sz: arg("shift_z"),
                xz: Self::num(v, "xz_scale"), y: Self::num(v, "y_scale"), noise: self.noise(v["noise"].as_str().unwrap()),
            },
            "shift" => Df::Shift(ShiftKind::Both, self.noise(v["argument"].as_str().unwrap())),
            "shift_a" => Df::Shift(ShiftKind::A, self.noise(v["argument"].as_str().unwrap())),
            "shift_b" => Df::Shift(ShiftKind::B, self.noise(v["argument"].as_str().unwrap())),
            "weird_scaled_sampler" => Df::Weird {
                input: arg("input"), noise: self.noise(v["noise"].as_str().unwrap()),
                type2: v["rarity_value_mapper"].as_str().unwrap() == "type_2",
            },
            "range_choice" => Df::Range {
                input: arg("input"), min: Self::num(v, "min_inclusive"), max: Self::num(v, "max_exclusive"),
                inside: arg("when_in_range"), outside: arg("when_out_of_range"),
            },
            "y_clamped_gradient" => Df::YGradient {
                from_y: Self::num(v, "from_y") as i32, to_y: Self::num(v, "to_y") as i32,
                from_v: Self::num(v, "from_value"), to_v: Self::num(v, "to_value"),
            },
            "spline" => Df::Spline(Arc::new(self.load_spline(&v["spline"]))),
            "old_blended_noise" => {
                let mut b = self.blended.borrow_mut();
                let n = b.get_or_insert_with(|| {
                    let mut r = self.root.from_hash_of("minecraft:terrain");
                    Arc::new(BlendedNoise::new(&mut r, Self::num(v, "xz_scale"), Self::num(v, "y_scale"),
                        Self::num(v, "xz_factor"), Self::num(v, "y_factor"), Self::num(v, "smear_scale_multiplier")))
                });
                Df::Blended(n.clone())
            }
            "find_top_surface" => Df::FindTopSurface {
                density: arg("density"), upper: arg("upper_bound"),
                lower: Self::num(v, "lower_bound") as i32, cell_h: Self::num(v, "cell_height") as i32,
            },
            "blend_alpha" => Df::BlendAlpha,
            "blend_offset" => Df::BlendOffset,
            "blend_density" => Df::BlendDensity(arg("argument")),
            "beardifier" => Df::Beardifier,
            other => panic!("unsupported density function type {other}"),
        };
        Arc::new(df)
    }

    fn load_spline(&self, v: &Value) -> Spline {
        let coord = self.load(&v["coordinate"]);
        let pts = v["points"].as_array().expect("spline points");
        let (mut locs, mut vals, mut ders) = (vec![], vec![], vec![]);
        for p in pts {
            locs.push(p["location"].as_f64().unwrap() as f32);
            ders.push(p["derivative"].as_f64().unwrap() as f32);
            vals.push(match p["value"].as_f64() {
                Some(n) => SplineVal::Const(n as f32),
                None => SplineVal::Sub(self.load_spline(&p["value"])),
            });
        }
        Spline { coord, locs, vals, ders }
    }

    /// `noise_router` entries of a noise-settings file.
    pub fn router(&self, settings: &str) -> Router {
        let s = self.jar.json(&format!("data/minecraft/worldgen/noise_settings/{settings}.json"));
        let r = &s["noise_router"];
        let g = |k: &str| self.load(&r[k]);
        Router {
            barrier: g("barrier"), floodedness: g("fluid_level_floodedness"), spread: g("fluid_level_spread"), lava: g("lava"),
            temperature: g("temperature"), vegetation: g("vegetation"), continents: g("continents"), erosion: g("erosion"),
            depth: g("depth"), ridges: g("ridges"), preliminary_surface_level: g("preliminary_surface_level"),
            final_density: g("final_density"), vein_toggle: g("vein_toggle"), vein_ridged: g("vein_ridged"), vein_gap: g("vein_gap"),
        }
    }
}

pub struct Router {
    pub barrier: Arc<Df>,
    pub floodedness: Arc<Df>,
    pub spread: Arc<Df>,
    pub lava: Arc<Df>,
    pub temperature: Arc<Df>,
    pub vegetation: Arc<Df>,
    pub continents: Arc<Df>,
    pub erosion: Arc<Df>,
    pub depth: Arc<Df>,
    pub ridges: Arc<Df>,
    pub preliminary_surface_level: Arc<Df>,
    pub final_density: Arc<Df>,
    pub vein_toggle: Arc<Df>,
    pub vein_ridged: Arc<Df>,
    pub vein_gap: Arc<Df>,
}
