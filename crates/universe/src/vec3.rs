#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const fn new(x: f64, y: f64, z: f64) -> Self { Self { x, y, z } }
    pub fn add(self, o: Vec3) -> Vec3 { Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z) }
    pub fn add_xyz(self, x: f64, y: f64, z: f64) -> Vec3 { Vec3::new(self.x + x, self.y + y, self.z + z) }
    pub fn subtract(self, o: Vec3) -> Vec3 { Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z) }
    pub fn scale(self, s: f64) -> Vec3 { Vec3::new(self.x * s, self.y * s, self.z * s) }
    pub fn dot(self, o: Vec3) -> f64 { self.x * o.x + self.y * o.y + self.z * o.z }
    pub fn length_sqr(self) -> f64 { self.x * self.x + self.y * self.y + self.z * self.z }
    pub fn length(self) -> f64 { self.length_sqr().sqrt() }
    pub fn normalize(self) -> Vec3 {
        let d = self.length();
        if d < 1.0E-5f32 as f64 { Vec3::ZERO } else { Vec3::new(self.x / d, self.y / d, self.z / d) }
    }
    pub fn lerp(self, to: Vec3, t: f64) -> Vec3 {
        use crate::mth::lerp;
        Vec3::new(lerp(t, self.x, to.x), lerp(t, self.y, to.y), lerp(t, self.z, to.z))
    }
    /// `Vec3.directionFromRotation(pitch, yaw)`, in single precision exactly like Java.
    pub fn direction_from_rotation(pitch: f32, yaw: f32) -> Vec3 {
        use crate::mth::{cos, sin};
        let deg = (std::f64::consts::PI / 180.0) as f32;
        let pi = std::f64::consts::PI as f32;
        let f = cos((-yaw * deg - pi) as f64);
        let f1 = sin((-yaw * deg - pi) as f64);
        let f2 = -cos((-pitch * deg) as f64);
        let f3 = sin((-pitch * deg) as f64);
        Vec3::new((f1 * f2) as f64, f3 as f64, (f * f2) as f64)
    }
}
