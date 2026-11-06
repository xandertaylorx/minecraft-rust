//! The parts of `net.minecraft.util.Mth` the universe code relies on (bit-exact).
use std::sync::OnceLock;

const SIN_SCALE: f64 = 10430.378350470453;

fn table() -> &'static [f32; 65536] {
    static T: OnceLock<Box<[f32; 65536]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f64 / SIN_SCALE).sin() as f32;
        }
        t
    })
}

pub fn sin(x: f64) -> f32 { table()[((x * SIN_SCALE) as i64 & 65535) as usize] }
pub fn cos(x: f64) -> f32 { table()[((x * SIN_SCALE + 16384.0) as i64 & 65535) as usize] }
pub fn lerp(t: f64, a: f64, b: f64) -> f64 { a + t * (b - a) }
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 { if v < lo { lo } else { v.min(hi) } }
pub fn clamp_f(v: f32, lo: f32, hi: f32) -> f32 { if v < lo { lo } else { v.min(hi) } }

pub fn wrap_degrees(v: f32) -> f32 {
    let mut f = v % 360.0;
    if f >= 180.0 { f -= 360.0; }
    if f < -180.0 { f += 360.0; }
    f
}
pub fn approach(cur: f32, target: f32, step: f32) -> f32 {
    let step = step.abs();
    if cur < target { clamp_f(cur + step, cur, target) } else { clamp_f(cur - step, target, cur) }
}
pub fn approach_degrees(cur: f32, target: f32, step: f32) -> f32 {
    let d = wrap_degrees(target - cur);
    approach(cur, cur + d, step)
}
