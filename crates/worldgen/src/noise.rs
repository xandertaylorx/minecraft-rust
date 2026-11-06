//! ImprovedNoise, PerlinNoise, NormalNoise and BlendedNoise (bit-exact ports).
use crate::random::{Positional, Xoro};

const GRADIENT: [[i32; 3]; 16] = [
    [1, 1, 0], [-1, 1, 0], [1, -1, 0], [-1, -1, 0], [1, 0, 1], [-1, 0, 1], [1, 0, -1], [-1, 0, -1],
    [0, 1, 1], [0, -1, 1], [0, 1, -1], [0, -1, -1], [1, 1, 0], [0, -1, 1], [-1, 1, 0], [0, -1, -1],
];

fn floor(v: f64) -> i32 {
    let i = v as i32;
    if v < i as f64 { i - 1 } else { i }
}
fn lfloor(v: f64) -> i64 {
    let i = v as i64;
    if v < i as f64 { i - 1 } else { i }
}
fn smoothstep(t: f64) -> f64 { t * t * t * (t * (t * 6.0 - 15.0) + 10.0) }
fn lerp(t: f64, a: f64, b: f64) -> f64 { a + t * (b - a) }
fn lerp2(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> f64 { lerp(b, lerp(a, c, d), lerp(a, e, f)) }
pub fn clamped_lerp(t: f64, a: f64, b: f64) -> f64 { if t < 0.0 { a } else if t > 1.0 { b } else { lerp(t, a, b) } }

pub struct ImprovedNoise {
    p: [u8; 256],
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
}

impl ImprovedNoise {
    pub fn new(r: &mut Xoro) -> Self {
        let xo = r.next_double() * 256.0;
        let yo = r.next_double() * 256.0;
        let zo = r.next_double() * 256.0;
        let mut p = [0u8; 256];
        for (i, v) in p.iter_mut().enumerate() { *v = i as u8; }
        for k in 0..256 {
            let j = r.next_int_bound(256 - k as i32) as usize;
            p.swap(k, k + j);
        }
        Self { p, xo, yo, zo }
    }

    pub fn noise(&self, x: f64, y: f64, z: f64) -> f64 { self.noise5(x, y, z, 0.0, 0.0) }

    pub fn noise5(&self, x: f64, y: f64, z: f64, y_scale: f64, y_max: f64) -> f64 {
        let (d0, d1, d2) = (x + self.xo, y + self.yo, z + self.zo);
        let (i, j, k) = (floor(d0), floor(d1), floor(d2));
        let (d3, d4, d5) = (d0 - i as f64, d1 - j as f64, d2 - k as f64);
        let d6 = if y_scale != 0.0 {
            let d7 = if y_max >= 0.0 && y_max < d4 { y_max } else { d4 };
            floor(d7 / y_scale + 1.0E-7f32 as f64) as f64 * y_scale
        } else { 0.0 };
        self.sample_and_lerp(i, j, k, d3, d4 - d6, d5, d4)
    }

    fn pp(&self, i: i32) -> i32 { self.p[(i & 0xFF) as usize] as i32 }

    fn grad_dot(h: i32, x: f64, y: f64, z: f64) -> f64 {
        let g = GRADIENT[(h & 15) as usize];
        g[0] as f64 * x + g[1] as f64 * y + g[2] as f64 * z
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_and_lerp(&self, x: i32, y: i32, z: i32, fx: f64, fy: f64, fz: f64, fy_unshifted: f64) -> f64 {
        let i = self.pp(x);
        let j = self.pp(x + 1);
        let k = self.pp(i + y);
        let l = self.pp(i + y + 1);
        let i1 = self.pp(j + y);
        let j1 = self.pp(j + y + 1);
        let d0 = Self::grad_dot(self.pp(k + z), fx, fy, fz);
        let d1 = Self::grad_dot(self.pp(i1 + z), fx - 1.0, fy, fz);
        let d2 = Self::grad_dot(self.pp(l + z), fx, fy - 1.0, fz);
        let d3 = Self::grad_dot(self.pp(j1 + z), fx - 1.0, fy - 1.0, fz);
        let d4 = Self::grad_dot(self.pp(k + z + 1), fx, fy, fz - 1.0);
        let d5 = Self::grad_dot(self.pp(i1 + z + 1), fx - 1.0, fy, fz - 1.0);
        let d6 = Self::grad_dot(self.pp(l + z + 1), fx, fy - 1.0, fz - 1.0);
        let d7 = Self::grad_dot(self.pp(j1 + z + 1), fx - 1.0, fy - 1.0, fz - 1.0);
        let (a, b, c) = (smoothstep(fx), smoothstep(fy_unshifted), smoothstep(fz));
        lerp(c, lerp2(a, b, d0, d1, d2, d3), lerp2(a, b, d4, d5, d6, d7))
    }
}

pub fn wrap(v: f64) -> f64 { v - lfloor(v / 3.3554432E7 + 0.5) as f64 * 3.3554432E7 }

pub struct PerlinNoise {
    levels: Vec<Option<ImprovedNoise>>,
    amplitudes: Vec<f64>,
    lowest_freq_value_factor: f64,
    lowest_freq_input_factor: f64,
    max_value: f64,
}

impl PerlinNoise {
    /// `PerlinNoise.create(random, firstOctave, amplitudes)`.
    pub fn create(random: &mut Xoro, first_octave: i32, amplitudes: &[f64]) -> Self {
        let pos: Positional = random.fork_positional();
        let levels = amplitudes.iter().enumerate().map(|(k, a)| {
            (*a != 0.0).then(|| ImprovedNoise::new(&mut pos.from_hash_of(&format!("octave_{}", first_octave + k as i32))))
        }).collect();
        Self::finish(levels, amplitudes.to_vec(), first_octave)
    }

    /// Legacy construction used by `BlendedNoise`: octaves `lo..=hi` all amplitude 1, sequential seeding.
    pub fn legacy_for_blended(random: &mut Xoro, lo: i32, hi: i32) -> Self {
        let i = -lo;
        let n = (i + hi + 1) as usize;
        let amplitudes = vec![1.0; n];
        let j = i as usize; // index of octave 0
        let mut levels: Vec<Option<ImprovedNoise>> = (0..n).map(|_| None).collect();
        levels[j] = Some(ImprovedNoise::new(random));
        for k in (0..j).rev() { levels[k] = Some(ImprovedNoise::new(random)); }
        Self::finish(levels, amplitudes, -i)
    }

    fn finish(levels: Vec<Option<ImprovedNoise>>, amplitudes: Vec<f64>, first_octave: i32) -> Self {
        let i = amplitudes.len() as i32;
        let j = -first_octave;
        let mut s = Self {
            levels, amplitudes,
            lowest_freq_input_factor: 2f64.powi(-j),
            lowest_freq_value_factor: 2f64.powi(i - 1) / (2f64.powi(i) - 1.0),
            max_value: 0.0,
        };
        s.max_value = s.edge_value(2.0);
        s
    }

    pub fn max_value(&self) -> f64 { self.max_value }
    pub fn max_broken_value(&self, y: f64) -> f64 { self.edge_value(y + 2.0) }

    fn edge_value(&self, v: f64) -> f64 {
        let mut d0 = 0.0;
        let mut d1 = self.lowest_freq_value_factor;
        for (i, l) in self.levels.iter().enumerate() {
            if l.is_some() { d0 += self.amplitudes[i] * v * d1; }
            d1 /= 2.0;
        }
        d0
    }

    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut d0 = 0.0;
        let mut d1 = self.lowest_freq_input_factor;
        let mut d2 = self.lowest_freq_value_factor;
        for (i, l) in self.levels.iter().enumerate() {
            if let Some(n) = l {
                d0 += self.amplitudes[i] * n.noise5(wrap(x * d1), wrap(y * d1), wrap(z * d1), 0.0, 0.0) * d2;
            }
            d1 *= 2.0;
            d2 /= 2.0;
        }
        d0
    }

    /// `getOctaveNoise(i)` counts from the highest-frequency octave.
    pub fn octave(&self, i: usize) -> Option<&ImprovedNoise> { self.levels[self.levels.len() - 1 - i].as_ref() }
}

pub struct NormalNoise {
    first: PerlinNoise,
    second: PerlinNoise,
    value_factor: f64,
    pub max_value: f64,
}

impl NormalNoise {
    pub fn create(random: &mut Xoro, first_octave: i32, amplitudes: &[f64]) -> Self {
        let first = PerlinNoise::create(random, first_octave, amplitudes);
        let second = PerlinNoise::create(random, first_octave, amplitudes);
        let (mut j, mut k) = (i32::MAX, i32::MIN);
        for (l, a) in amplitudes.iter().enumerate() {
            if *a != 0.0 { j = j.min(l as i32); k = k.max(l as i32); }
        }
        let value_factor = 0.16666666666666666 / (0.1 * (1.0 + 1.0 / ((k - j) as f64 + 1.0)));
        let max_value = (first.max_value() + second.max_value()) * value_factor;
        Self { first, second, value_factor, max_value }
    }

    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        const F: f64 = 1.0181268882175227;
        (self.first.get_value(x, y, z) + self.second.get_value(x * F, y * F, z * F)) * self.value_factor
    }
}

pub struct BlendedNoise {
    min_limit: PerlinNoise,
    max_limit: PerlinNoise,
    main: PerlinNoise,
    xz_multiplier: f64,
    y_multiplier: f64,
    xz_factor: f64,
    y_factor: f64,
    smear: f64,
    pub max_value: f64,
}

impl BlendedNoise {
    pub fn new(r: &mut Xoro, xz_scale: f64, y_scale: f64, xz_factor: f64, y_factor: f64, smear: f64) -> Self {
        let min_limit = PerlinNoise::legacy_for_blended(r, -15, 0);
        let max_limit = PerlinNoise::legacy_for_blended(r, -15, 0);
        let main = PerlinNoise::legacy_for_blended(r, -7, 0);
        let y_multiplier = 684.412 * y_scale;
        let max_value = min_limit.max_broken_value(y_multiplier);
        Self { min_limit, max_limit, main, xz_multiplier: 684.412 * xz_scale, y_multiplier, xz_factor, y_factor, smear, max_value }
    }

    pub fn compute(&self, bx: i32, by: i32, bz: i32) -> f64 {
        let d0 = bx as f64 * self.xz_multiplier;
        let d1 = by as f64 * self.y_multiplier;
        let d2 = bz as f64 * self.xz_multiplier;
        let (d3, d4, d5) = (d0 / self.xz_factor, d1 / self.y_factor, d2 / self.xz_factor);
        let d6 = self.y_multiplier * self.smear;
        let d7 = d6 / self.y_factor;
        let (mut d8, mut d9, mut d10) = (0.0, 0.0, 0.0);
        let mut d11 = 1.0;
        for i in 0..8 {
            if let Some(n) = self.main.octave(i) {
                d10 += n.noise5(wrap(d3 * d11), wrap(d4 * d11), wrap(d5 * d11), d7 * d11, d4 * d11) / d11;
            }
            d11 /= 2.0;
        }
        let d16 = (d10 / 10.0 + 1.0) / 2.0;
        let (flag1, flag2) = (d16 >= 1.0, d16 <= 0.0);
        d11 = 1.0;
        for j in 0..16 {
            let (d12, d13, d14) = (wrap(d0 * d11), wrap(d1 * d11), wrap(d2 * d11));
            let d15 = d6 * d11;
            if !flag1 {
                if let Some(n) = self.min_limit.octave(j) { d8 += n.noise5(d12, d13, d14, d15, d1 * d11) / d11; }
            }
            if !flag2 {
                if let Some(n) = self.max_limit.octave(j) { d9 += n.noise5(d12, d13, d14, d15, d1 * d11) / d11; }
            }
            d11 /= 2.0;
        }
        clamped_lerp(d16, d8 / 512.0, d9 / 512.0) / 128.0
    }
}
