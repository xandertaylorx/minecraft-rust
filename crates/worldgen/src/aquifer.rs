//! Port of `Aquifer.NoiseBasedAquifer` and `OreVeinifier`.
use std::cell::RefCell;
use std::collections::HashMap;

use crate::density::*;
use crate::random::Positional;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fluid { Air, Water, Lava }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FluidStatus {
    pub level: i32,
    pub fluid: Fluid,
}

impl FluidStatus {
    fn at(&self, y: i32) -> Fluid { if y < self.level { self.fluid } else { Fluid::Air } }
}

const WAY_BELOW_MIN_Y: i32 = -4064 * 2 + 0;
const SEA_LEVEL: i32 = 63;

fn global_fluid(y: i32) -> FluidStatus {
    if y < (-54i32).min(SEA_LEVEL) { FluidStatus { level: -54, fluid: Fluid::Lava } } else { FluidStatus { level: SEA_LEVEL, fluid: Fluid::Water } }
}

fn grid_x(v: i32) -> i32 { v >> 4 }
fn from_grid_x(g: i32, o: i32) -> i32 { (g << 4) + o }
fn grid_y(v: i32) -> i32 { v.div_euclid(12) }
fn from_grid_y(g: i32, o: i32) -> i32 { g * 12 + o }

fn similarity(a: i32, b: i32) -> f64 { 1.0 - (b - a) as f64 / 25.0 }

fn clamp(v: f64, lo: f64, hi: f64) -> f64 { if v < lo { lo } else { v.min(hi) } }
fn clamped_map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let t = (v - a) / (b - a);
    if t < 0.0 { c } else if t > 1.0 { d } else { c + t * (d - c) }
}
fn map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 { c + ((v - a) / (b - a)) * (d - c) }

pub struct Aquifer<'a> {
    router: &'a Router,
    ev: &'a Eval,
    flat: Eval,
    prelim_cache: RefCell<HashMap<(i32, i32), i32>>,
    random: Positional,
    loc: Vec<Option<(i32, i32, i32)>>,
    status: Vec<Option<FluidStatus>>,
    skip_above: i32,
    min_gx: i32,
    min_gy: i32,
    min_gz: i32,
    size_x: i32,
    size_z: i32,
}

const OFFSETS: [(i32, i32); 13] = [(0, 0), (-2, -1), (-1, -1), (0, -1), (1, -1), (-3, 0), (-2, 0), (-1, 0), (1, 0), (-2, 1), (-1, 1), (0, 1), (1, 1)];

impl<'a> Aquifer<'a> {
    pub fn new(router: &'a Router, ev: &'a Eval, random: Positional, cx: i32, cz: i32, min_y: i32, height: i32) -> Self {
        let (min_bx, max_bx) = (cx * 16, cx * 16 + 15);
        let (min_bz, max_bz) = (cz * 16, cz * 16 + 15);
        let min_gx = grid_x(min_bx - 5);
        let max_gx = grid_x(max_bx - 5) + 1;
        let size_x = max_gx - min_gx + 1;
        let min_gy = grid_y(min_y + 1) - 1;
        let max_gy = grid_y(min_y + height + 1) + 1;
        let size_y = max_gy - min_gy + 1;
        let min_gz = grid_x(min_bz - 5);
        let max_gz = grid_x(max_bz - 5) + 1;
        let size_z = max_gz - min_gz + 1;
        let n = (size_x * size_y * size_z) as usize;
        let mut a = Aquifer {
            router, ev, flat: Eval { x0: 0, z0: 0, min_y: -64, grids: HashMap::new() }, prelim_cache: RefCell::new(HashMap::new()),
            random, loc: vec![None; n], status: vec![None; n], skip_above: 0, min_gx, min_gy, min_gz, size_x, size_z,
        };
        let top = a.max_prelim(from_grid_x(min_gx, 0), from_grid_x(min_gz, 0), from_grid_x(max_gx, 9), from_grid_x(max_gz, 9));
        let j1 = top + 8;
        let k1 = grid_y(j1 + 12) + 1;
        a.skip_above = from_grid_y(k1, 11) - 1;
        a
    }

    fn prelim(&self, bx: i32, bz: i32) -> i32 {
        let (x, z) = ((bx >> 2) << 2, (bz >> 2) << 2);
        if let Some(v) = self.prelim_cache.borrow().get(&(x, z)) { return *v; }
        let v = self.router.preliminary_surface_level.eval(&Ctx { x, y: 0, z }, Some(&self.flat));
        let v = {
            let i = v as i32;
            if v < i as f64 { i - 1 } else { i }
        };
        self.prelim_cache.borrow_mut().insert((x, z), v);
        v
    }

    fn max_prelim(&self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> i32 {
        let mut best = i32::MIN;
        let mut j = min_z;
        while j <= max_z {
            let mut k = min_x;
            while k <= max_x {
                best = best.max(self.prelim(k, j));
                k += 4;
            }
            j += 4;
        }
        best
    }

    fn index(&self, gx: i32, gy: i32, gz: i32) -> usize {
        let (i, j, k) = (gx - self.min_gx, gy - self.min_gy, gz - self.min_gz);
        ((j * self.size_z + k) * self.size_x + i) as usize
    }

    fn status_of(&mut self, idx: usize) -> FluidStatus {
        if let Some(s) = self.status[idx] { return s; }
        let (x, y, z) = self.loc[idx].unwrap();
        let s = self.compute_fluid(x, y, z);
        self.status[idx] = Some(s);
        s
    }

    /// `None` means "use the default block" (solid or aquifer barrier).
    pub fn compute_substance(&mut self, x: i32, y: i32, z: i32, density: f64) -> Option<Fluid> {
        if density > 0.0 { return None; }
        let global = global_fluid(y);
        if y > self.skip_above { return Some(global.at(y)); }
        if global.at(y) == Fluid::Lava { return Some(Fluid::Lava); }
        let (l, i1, j1) = (grid_x(x - 5), grid_y(y + 1), grid_x(z - 5));
        let (mut k1, mut l1, mut i2, mut j2) = (i32::MAX, i32::MAX, i32::MAX, i32::MAX);
        let (mut k2, mut l2, mut i3, mut _j3) = (0usize, 0usize, 0usize, 0usize);
        for k3 in 0..=1 {
            for l3 in -1..=1 {
                for i4 in 0..=1 {
                    let (j4, k4, l4) = (l + k3, i1 + l3, j1 + i4);
                    let i5 = self.index(j4, k4, l4);
                    let p = match self.loc[i5] {
                        Some(p) => p,
                        None => {
                            let mut r = self.random.at(j4, k4, l4);
                            let p = (
                                from_grid_x(j4, r.next_int_bound(10)),
                                from_grid_y(k4, r.next_int_bound(9)),
                                from_grid_x(l4, r.next_int_bound(10)),
                            );
                            self.loc[i5] = Some(p);
                            p
                        }
                    };
                    let (dx, dy, dz) = (p.0 - x, p.1 - y, p.2 - z);
                    let j6 = dx * dx + dy * dy + dz * dz;
                    if k1 >= j6 {
                        _j3 = i3; i3 = l2; l2 = k2; k2 = i5; j2 = i2; i2 = l1; l1 = k1; k1 = j6;
                    } else if l1 >= j6 {
                        _j3 = i3; i3 = l2; l2 = i5; j2 = i2; i2 = l1; l1 = j6;
                    } else if i2 >= j6 {
                        _j3 = i3; i3 = i5; j2 = i2; i2 = j6;
                    } else if j2 >= j6 {
                        _j3 = i5; j2 = j6;
                    }
                }
            }
        }
        let s1 = self.status_of(k2);
        let d1 = similarity(k1, l1);
        let block = s1.at(y);
        if d1 <= 0.0 { return Some(block); }
        if block == Fluid::Water && global_fluid(y - 1).at(y - 1) == Fluid::Lava { return Some(block); }
        let mut barrier = f64::NAN;
        let s3 = self.status_of(l2);
        let d2 = d1 * self.pressure(x, y, z, &mut barrier, s1, s3);
        if density + d2 > 0.0 { return None; }
        let s4 = self.status_of(i3);
        let d0 = similarity(k1, i2);
        if d0 > 0.0 {
            let d3 = d1 * d0 * self.pressure(x, y, z, &mut barrier, s1, s4);
            if density + d3 > 0.0 { return None; }
        }
        let d4 = similarity(l1, i2);
        if d4 > 0.0 {
            let d5 = d1 * d4 * self.pressure(x, y, z, &mut barrier, s3, s4);
            if density + d5 > 0.0 { return None; }
        }
        Some(block)
    }

    fn pressure(&self, x: i32, y: i32, z: i32, memo: &mut f64, a: FluidStatus, b: FluidStatus) -> f64 {
        let (fa, fb) = (a.at(y), b.at(y));
        if (fa == Fluid::Lava && fb == Fluid::Water) || (fa == Fluid::Water && fb == Fluid::Lava) { return 2.0; }
        let j = (a.level - b.level).abs();
        if j == 0 { return 0.0; }
        let d0 = 0.5 * (a.level + b.level) as f64;
        let d1 = y as f64 + 0.5 - d0;
        let d2 = j as f64 / 2.0;
        let d9 = d2 - d1.abs();
        let d10 = if d1 > 0.0 {
            let d11 = 0.0 + d9;
            if d11 > 0.0 { d11 / 1.5 } else { d11 / 2.5 }
        } else {
            let d15 = 3.0 + d9;
            if d15 > 0.0 { d15 / 3.0 } else { d15 / 10.0 }
        };
        let d12 = if !(d10 < -2.0) && !(d10 > 2.0) {
            if memo.is_nan() { *memo = self.router.barrier.eval(&Ctx { x, y, z }, Some(self.ev)); }
            *memo
        } else { 0.0 };
        2.0 * (d12 + d10)
    }

    fn compute_fluid(&self, x: i32, y: i32, z: i32) -> FluidStatus {
        let global = global_fluid(y);
        let mut i = i32::MAX;
        let (j, k) = (y + 12, y - 12);
        let mut flag = false;
        for (ox, oz) in OFFSETS {
            let (l, i1) = (x + ox * 16, z + oz * 16);
            let j1 = self.prelim(l, i1);
            let k1 = j1 + 8;
            let flag1 = ox == 0 && oz == 0;
            if flag1 && k > k1 { return global; }
            let flag2 = j > k1;
            if flag2 || flag1 {
                let g = global_fluid(k1);
                if g.at(k1) != Fluid::Air {
                    if flag1 { flag = true; }
                    if flag2 { return g; }
                }
            }
            i = i.min(j1);
        }
        let l1 = self.surface_level(x, y, z, global, i, flag);
        FluidStatus { level: l1, fluid: self.fluid_type(x, y, z, global, l1) }
    }

    fn surface_level(&self, x: i32, y: i32, z: i32, global: FluidStatus, i: i32, flag: bool) -> i32 {
        let c = Ctx { x, y, z };
        let (d0, d1);
        let erosion = self.router.erosion.eval(&c, Some(&self.flat));
        let depth = self.router.depth.eval(&c, Some(&self.flat));
        if erosion < -0.225f32 as f64 && depth > 0.9f32 as f64 {
            d0 = -1.0;
            d1 = -1.0;
        } else {
            let ii = i + 8 - y;
            let d2 = if flag { clamped_map(ii as f64, 0.0, 64.0, 1.0, 0.0) } else { 0.0 };
            let d3 = clamp(self.router.floodedness.eval(&c, Some(self.ev)), -1.0, 1.0);
            let d4 = map(d2, 1.0, 0.0, -0.3, 0.8);
            let d5 = map(d2, 1.0, 0.0, -0.8, 0.4);
            d0 = d3 - d5;
            d1 = d3 - d4;
        }
        if d1 > 0.0 { global.level }
        else if d0 > 0.0 { self.randomized_level(x, y, z, i) }
        else { WAY_BELOW_MIN_Y }
    }

    fn randomized_level(&self, x: i32, y: i32, z: i32, i: i32) -> i32 {
        let (k, l, i1) = (x.div_euclid(16), y.div_euclid(40), z.div_euclid(16));
        let j1 = l * 40 + 20;
        let d0 = self.router.spread.eval(&Ctx { x: k, y: l, z: i1 }, Some(self.ev)) * 10.0;
        let l1 = (d0 / 3.0).floor() as i32 * 3;
        i.min(j1 + l1)
    }

    fn fluid_type(&self, x: i32, y: i32, z: i32, global: FluidStatus, level: i32) -> Fluid {
        let mut f = global.fluid;
        if level <= -10 && level != WAY_BELOW_MIN_Y && global.fluid != Fluid::Lava {
            let (k, l, i1) = (x.div_euclid(64), y.div_euclid(40), z.div_euclid(64));
            let d0 = self.router.lava.eval(&Ctx { x: k, y: l, z: i1 }, Some(self.ev));
            if d0.abs() > 0.3 { f = Fluid::Lava; }
        }
        f
    }
}

/// `OreVeinifier.create(...)` for one voxel.
pub fn vein(router: &Router, ev: &Eval, ore_random: &Positional, x: i32, y: i32, z: i32) -> Option<&'static str> {
    let c = Ctx { x, y, z };
    let d0 = router.vein_toggle.eval(&c, Some(ev));
    let copper = d0 > 0.0;
    let (min_y, max_y) = if copper { (0, 50) } else { (-60, -8) };
    let d1 = d0.abs();
    let (j, k) = (max_y - y, y - min_y);
    if k >= 0 && j >= 0 {
        let l = j.min(k);
        let d2 = clamped_map(l as f64, 0.0, 20.0, -0.2, 0.0);
        if d1 + d2 < 0.4f32 as f64 { return None; }
        let mut r = ore_random.at(x, y, z);
        if r.next_float() > 0.7f32 { return None; }
        if router.vein_ridged.eval(&c, Some(ev)) >= 0.0 { return None; }
        let d3 = clamped_map(d1, 0.4f32 as f64, 0.6f32 as f64, 0.1f32 as f64, 0.3f32 as f64);
        if r.next_float() < d3 as f32 && router.vein_gap.eval(&c, Some(ev)) > -0.3f32 as f64 {
            return Some(match (r.next_float() < 0.02f32, copper) {
                (true, true) => "raw_copper_block", (false, true) => "copper_ore",
                (true, false) => "raw_iron_block", (false, false) => "deepslate_iron_ore",
            });
        }
        return Some(if copper { "granite" } else { "tuff" });
    }
    None
}
