//! Player physics: a port of vanilla's 20 TPS movement (`LivingEntity.travel`, `Entity.move`, collisions, fluids).
use glam::{DVec3, Vec3};

use crate::blocks::{def, id_of, AIR};
use crate::world::World;

pub const HALF_W: f64 = 0.3;
pub const HEIGHT: f64 = 1.8;
pub const SNEAK_HEIGHT: f64 = 1.5;
pub const EYE: f64 = 1.62;
pub const SNEAK_EYE: f64 = 1.27;
const STEP_HEIGHT: f64 = 0.6;
const GRAVITY: f64 = 0.08;

#[derive(Default, Clone, Copy)]
pub struct Keys {
    pub forward: bool, pub back: bool, pub left: bool, pub right: bool,
    pub jump: bool, pub sneak: bool, pub sprint: bool,
}

pub struct Player {
    pub pos: DVec3,
    pub prev_pos: DVec3,
    /// Vanilla `deltaMovement` (blocks per tick).
    pub vel: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub flying: bool,
    pub on_ground: bool,
    pub sneaking: bool,
    pub sprinting: bool,
    pub horizontal_collision: bool,
    pub in_water: bool,
    pub eye_in_water: bool,
    pub no_jump_delay: i32,
    pub fall_distance: f64,
    pub eye_height: f64,
    pub prev_eye_height: f64,
    pub water_ticks: u32,
    pub walk_dist: f32,
    pub prev_walk_dist: f32,
    pub bob: f32,
    pub prev_bob: f32,
}

type Aabb = (DVec3, DVec3);

fn aabb(pos: DVec3, h: f64) -> Aabb { (DVec3::new(pos.x - HALF_W, pos.y, pos.z - HALF_W), DVec3::new(pos.x + HALF_W, pos.y + h, pos.z + HALF_W)) }

impl Player {
    pub fn new(pos: DVec3) -> Self {
        Self {
            pos, prev_pos: pos, vel: DVec3::ZERO, yaw: 0.0, pitch: 0.0, flying: false, on_ground: false, sneaking: false, sprinting: false,
            horizontal_collision: false, in_water: false, eye_in_water: false, no_jump_delay: 0, fall_distance: 0.0,
            eye_height: EYE, prev_eye_height: EYE, water_ticks: 0, walk_dist: 0.0, prev_walk_dist: 0.0, bob: 0.0, prev_bob: 0.0,
        }
    }

    pub fn height(&self) -> f64 { if self.sneaking { SNEAK_HEIGHT } else { HEIGHT } }
    pub fn eye(&self) -> DVec3 { self.pos + DVec3::new(0.0, self.eye_height, 0.0) }

    fn solid(w: &World, x: i32, y: i32, z: i32) -> bool { def(w.get(x, y, z)).solid }

    /// Clip `mv` against solid blocks; vanilla order: Y first, then the longer horizontal axis.
    pub fn collide_axes(w: &World, b: Aabb, mv: DVec3) -> DVec3 {
        let (mut lo, mut hi) = b;
        let mut out = DVec3::ZERO;
        let clip = |lo: DVec3, hi: DVec3, axis: usize, d: f64| -> f64 {
            if d == 0.0 { return 0.0; }
            let (mut min, mut max) = (lo, hi);
            if d > 0.0 { max[axis] += d; } else { min[axis] += d; }
            let (x0, x1) = ((min.x + 1e-7).floor() as i32, (max.x - 1e-7).floor() as i32);
            let (y0, y1) = ((min.y + 1e-7).floor() as i32, (max.y - 1e-7).floor() as i32);
            let (z0, z1) = ((min.z + 1e-7).floor() as i32, (max.z - 1e-7).floor() as i32);
            let mut r = d;
            for y in y0..=y1 { for z in z0..=z1 { for x in x0..=x1 {
                if !Self::solid(w, x, y, z) { continue; }
                let c = [x as f64, y as f64, z as f64];
                let others = [0usize, 1, 2].into_iter().filter(|a| *a != axis);
                let mut overlap = true;
                for a in others { if hi[a] <= c[a] + 1e-9 || lo[a] >= c[a] + 1.0 - 1e-9 { overlap = false; } }
                if !overlap { continue; }
                if d > 0.0 { let gap = c[axis] - hi[axis]; if gap >= -1e-9 && gap < r { r = gap.max(0.0); } }
                else { let gap = c[axis] + 1.0 - lo[axis]; if gap <= 1e-9 && gap > r { r = gap.min(0.0); } }
            } } }
            r
        };
        let dy = clip(lo, hi, 1, mv.y);
        lo.y += dy; hi.y += dy; out.y = dy;
        let x_first = mv.x.abs() >= mv.z.abs();
        let order: [usize; 2] = if x_first { [0, 2] } else { [2, 0] };
        for a in order {
            let d = clip(lo, hi, a, mv[a]);
            lo[a] += d; hi[a] += d; out[a] = d;
        }
        out
    }

    fn collide(&self, w: &World, mv: DVec3) -> DVec3 { collide_step(w, self.pos, HALF_W, self.height(), mv, self.on_ground) }

    fn back_off_edge(&self, w: &World, mut mv: DVec3) -> DVec3 {
        if mv.y > 0.0 || !self.sneaking || !(self.on_ground) { return mv; }
        let b = aabb(self.pos, self.height());
        let free = |d: DVec3| { let m: Aabb = (b.0 + d, b.1 + d); Self::collide_axes(w, m, DVec3::ZERO); Self::collide_axes(w, m, DVec3::new(0.0, 0.0, 0.0)); !Self::overlaps(w, m) };
        let (mut x, mut z) = (mv.x, mv.z);
        while x != 0.0 && free(DVec3::new(x, -STEP_HEIGHT, 0.0)) { if x.abs() <= 0.05 { x = 0.0 } else { x -= 0.05 * x.signum() } }
        while z != 0.0 && free(DVec3::new(0.0, -STEP_HEIGHT, z)) { if z.abs() <= 0.05 { z = 0.0 } else { z -= 0.05 * z.signum() } }
        while x != 0.0 && z != 0.0 && free(DVec3::new(x, -STEP_HEIGHT, z)) {
            if x.abs() <= 0.05 { x = 0.0 } else { x -= 0.05 * x.signum() }
            if z.abs() <= 0.05 { z = 0.0 } else { z -= 0.05 * z.signum() }
        }
        mv.x = x; mv.z = z;
        mv
    }

    fn overlaps(w: &World, b: Aabb) -> bool {
        let (x0, x1) = ((b.0.x + 1e-7).floor() as i32, (b.1.x - 1e-7).floor() as i32);
        let (y0, y1) = ((b.0.y + 1e-7).floor() as i32, (b.1.y - 1e-7).floor() as i32);
        let (z0, z1) = ((b.0.z + 1e-7).floor() as i32, (b.1.z - 1e-7).floor() as i32);
        for y in y0..=y1 { for z in z0..=z1 { for x in x0..=x1 { if Self::solid(w, x, y, z) { return true; } } } }
        false
    }

    /// `Entity.move`
    fn do_move(&mut self, w: &World, want: DVec3) {
        let want = self.back_off_edge(w, want);
        let r = self.collide(w, want);
        self.pos += r;
        let cx = (want.x - r.x).abs() > 1e-9;
        let cz = (want.z - r.z).abs() > 1e-9;
        let cy = (want.y - r.y).abs() > 1e-9;
        self.horizontal_collision = cx || cz;
        self.on_ground = cy && want.y < 0.0;
        if cx { self.vel.x = 0.0; }
        if cz { self.vel.z = 0.0; }
        if cy { self.vel.y = 0.0; }
    }

    /// Water depth over the feet (0 when dry), like `Entity.getFluidHeight` for water.
    pub fn fluid_height(&self, w: &World) -> f64 {
        let water = id_of("water");
        let b = aabb(self.pos, self.height());
        let (x0, x1) = ((b.0.x + 0.001).floor() as i32, (b.1.x - 0.001).floor() as i32);
        let (y0, y1) = ((b.0.y + 0.001).floor() as i32, (b.1.y - 0.001).floor() as i32);
        let (z0, z1) = ((b.0.z + 0.001).floor() as i32, (b.1.z - 0.001).floor() as i32);
        let mut best = 0.0f64;
        for y in y0..=y1 { for z in z0..=z1 { for x in x0..=x1 {
            if w.get(x, y, z) == water {
                let top = if w.get(x, y + 1, z) == water { y as f64 + 1.0 } else { y as f64 + 8.0 / 9.0 };
                if top > b.0.y { best = best.max(top - b.0.y); }
            }
        } } }
        best
    }

    fn eye_submerged(&self, w: &World) -> bool {
        let e = self.eye();
        let (x, y, z) = (e.x.floor() as i32, e.y.floor() as i32, e.z.floor() as i32);
        let water = id_of("water");
        if w.get(x, y, z) != water { return false; }
        let top = if w.get(x, y + 1, z) == water { y as f64 + 1.0 } else { y as f64 + 8.0 / 9.0 };
        e.y < top
    }

    fn block_friction(&self, w: &World) -> f64 {
        let (x, y, z) = (self.pos.x.floor() as i32, (self.pos.y - 0.500001).floor() as i32, self.pos.z.floor() as i32);
        match def(w.get(x, y, z)).name { "ice" | "packed_ice" => 0.98, _ => 0.6 }
    }

    fn move_relative(&mut self, speed: f64, input: DVec3) {
        let l = input.length_squared();
        if l < 1.0e-7 { return; }
        let v = if l > 1.0 { input.normalize() } else { input } * speed;
        let (s, c) = ((self.yaw as f64).to_radians().sin(), (self.yaw as f64).to_radians().cos());
        self.vel += DVec3::new(v.x * c - v.z * s, v.y, v.z * c + v.x * s);
    }

    /// One 20 Hz tick of `LocalPlayer.aiStep` + `Player.travel`.
    pub fn tick(&mut self, w: &World, keys: Keys, creative: bool) {
        if !w.loaded(self.pos.x.floor() as i32, self.pos.z.floor() as i32) { return; }
        self.prev_pos = self.pos;
        self.prev_eye_height = self.eye_height;
        self.prev_walk_dist = self.walk_dist;
        self.prev_bob = self.bob;
        self.sneaking = keys.sneak && !self.flying;
        // Eye height eases toward the pose target (Camera.tick).
        let target_eye = if self.sneaking { SNEAK_EYE } else { EYE };
        self.eye_height += (target_eye - self.eye_height) * 0.5;

        let fluid = self.fluid_height(w);
        self.in_water = fluid > 0.0;
        self.eye_in_water = self.eye_submerged(w);
        if self.eye_in_water { self.water_ticks = (self.water_ticks + 1).min(600); } else { self.water_ticks = self.water_ticks.saturating_sub(1); }

        // Input vector: forward is +z, left is +x; both scaled by 0.98 and slowed while sneaking.
        let mut fwd = (keys.forward as i32 - keys.back as i32) as f64;
        let mut lft = (keys.left as i32 - keys.right as i32) as f64;
        if self.sneaking { fwd *= 0.3; lft *= 0.3; }
        fwd *= 0.98;
        lft *= 0.98;

        // Sprinting: forward input, not sneaking; stops on collision or when input stops.
        let wants = keys.sprint && fwd >= 0.8 * 0.98 * if self.sneaking { 0.3 } else { 1.0 } && !self.sneaking;
        if wants { self.sprinting = true; }
        if !keys.forward || self.sneaking || (self.horizontal_collision && !self.in_water) { self.sprinting = false; }
        if self.flying && !keys.sprint { self.sprinting = false; }

        let speed = if self.sprinting && !self.flying { 0.1 * 1.3 } else { 0.1 };
        let fly_speed = 0.05 * if self.sprinting { 2.0 } else { 1.0 };

        if self.no_jump_delay > 0 { self.no_jump_delay -= 1; }
        let mut yaa = 0.0;
        if self.flying {
            if keys.sneak { self.vel.y -= 0.05 * 3.0; }
            if keys.jump { self.vel.y += 0.05 * 3.0; }
        } else if keys.jump {
            if self.in_water && fluid > 0.4 {
                self.vel.y += 0.04;
            } else if self.on_ground && self.no_jump_delay == 0 {
                self.vel.y = self.vel.y.max(0.42);
                if self.sprinting {
                    let f = (self.yaw as f64).to_radians();
                    self.vel += DVec3::new(-f.sin() * 0.2, 0.0, f.cos() * 0.2);
                }
                self.no_jump_delay = 10;
            }
        } else { self.no_jump_delay = 0; }
        if keys.sneak && self.in_water && !self.flying { self.vel.y -= 0.04; yaa = 0.0; }
        let _ = yaa;
        let input = DVec3::new(lft, 0.0, fwd);

        if self.in_water && !self.flying {
            self.travel_in_water(w, input, speed, fluid);
        } else {
            self.travel_in_air(w, input, speed, fly_speed);
        }
        let _ = creative;

        // Fall damage bookkeeping happens in the caller via `fall_distance`.
        // Walk-bob (Player.aiStep).
        let horiz = ((self.pos.x - self.prev_pos.x).powi(2) + (self.pos.z - self.prev_pos.z).powi(2)).sqrt() as f32;
        let target_bob = if self.on_ground && !self.flying { (horiz).min(0.1) } else { 0.0 };
        self.bob += (target_bob - self.bob) * 0.4;
        self.walk_dist += horiz * 0.6;
    }

    fn travel_in_air(&mut self, w: &World, input: DVec3, speed: f64, fly_speed: f64) {
        let friction = if self.on_ground { self.block_friction(w) } else { 1.0 };
        let f1 = friction * 0.91;
        let move_speed = if self.on_ground { speed * (0.21600002 / (friction * friction * friction)) } else if self.flying { fly_speed } else { 0.02 };
        let move_speed = if self.flying && self.on_ground { fly_speed } else { move_speed };
        self.move_relative(move_speed, input);
        let y_before = self.vel.y;
        let before_y_for_fall = self.pos.y;
        let mv = self.vel;
        self.do_move(w, mv);
        if !self.flying && !self.on_ground && self.pos.y < before_y_for_fall { self.fall_distance += before_y_for_fall - self.pos.y; }
        let _ = y_before;
        let mut vy = self.vel.y;
        if !self.flying { vy -= GRAVITY; }
        self.vel = DVec3::new(self.vel.x * f1, vy * 0.98, self.vel.z * f1);
        if self.flying {
            // Player.travel keeps vertical speed damped at 0.6 of the pre-travel value.
            self.vel.y = y_before * 0.6;
            self.fall_distance = 0.0;
        }
        if self.on_ground { self.fall_distance = self.fall_distance.max(0.0); }
    }

    fn travel_in_water(&mut self, w: &World, input: DVec3, speed: f64, fluid: f64) {
        let start_y = self.pos.y;
        let falling = self.vel.y <= 0.0;
        let mut f = if self.sprinting { 0.9 } else { 0.8 };
        let mut f1 = 0.02;
        let mut eff = 0.0f64; // water movement efficiency attribute (0 for the player)
        if !self.on_ground { eff *= 0.5; }
        if eff > 0.0 { f += (0.54600006 - f) * eff; f1 += (speed - f1) * eff; }
        self.move_relative(f1, input);
        // Swimming (sprinting with the head under water) steers vertically toward the look direction.
        if self.sprinting && self.eye_in_water {
            let look_y = -(self.pitch as f64).to_radians().sin();
            let d1 = if look_y < -0.2 { 0.085 } else { 0.06 };
            self.vel.y += (look_y - self.vel.y) * d1;
        }
        let mv = self.vel;
        self.do_move(w, mv);
        let mut v = self.vel;
        v = DVec3::new(v.x * f, v.y * 0.8, v.z * f);
        // getFluidFallingAdjustedMovement
        if !self.sprinting {
            let d0 = if falling && (v.y - 0.005).abs() >= 0.003 && (v.y - GRAVITY / 16.0).abs() < 0.003 { -0.003 } else { v.y - GRAVITY / 16.0 };
            v.y = d0;
        }
        self.vel = v;
        // jumpOutOfFluid: pop up onto a ledge.
        let b = aabb(self.pos, self.height());
        let probe: Aabb = (b.0 + DVec3::new(self.vel.x, self.vel.y + 0.6 - self.pos.y + start_y, self.vel.z), b.1 + DVec3::new(self.vel.x, self.vel.y + 0.6 - self.pos.y + start_y, self.vel.z));
        if self.horizontal_collision && !Self::overlaps(w, probe) && fluid > 0.0 { self.vel.y = 0.3; }
        self.fall_distance = 0.0;
    }

    pub fn look_dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        Vec3::new(-sy * cp, -sp, cy * cp)
    }

    /// Render position interpolated between ticks.
    pub fn lerp_pos(&self, a: f64) -> DVec3 { self.prev_pos + (self.pos - self.prev_pos) * a }
    pub fn lerp_eye(&self, a: f64) -> f64 { self.prev_eye_height + (self.eye_height - self.prev_eye_height) * a }
}


/// `Entity.collide` with step-up (shared by the player and mobs).
pub fn collide_step(w: &World, pos: DVec3, half_w: f64, height: f64, mv: DVec3, on_ground: bool) -> DVec3 {
    let b: Aabb = (DVec3::new(pos.x - half_w, pos.y, pos.z - half_w), DVec3::new(pos.x + half_w, pos.y + height, pos.z + half_w));
    let r = Player::collide_axes(w, b, mv);
    let (cx, cz) = (mv.x != r.x, mv.z != r.z);
    let cy = mv.y != r.y;
    if (on_ground || (cy && mv.y < 0.0)) && (cx || cz) {
        let mut up = Player::collide_axes(w, b, DVec3::new(mv.x, STEP_HEIGHT, mv.z));
        let mut ex = b;
        if mv.x > 0.0 { ex.1.x += mv.x } else { ex.0.x += mv.x }
        if mv.z > 0.0 { ex.1.z += mv.z } else { ex.0.z += mv.z }
        let up2 = Player::collide_axes(w, ex, DVec3::new(0.0, STEP_HEIGHT, 0.0));
        if up2.y < STEP_HEIGHT {
            let moved: Aabb = (b.0 + DVec3::new(0.0, up2.y, 0.0), b.1 + DVec3::new(0.0, up2.y, 0.0));
            let v = Player::collide_axes(w, moved, DVec3::new(mv.x, 0.0, mv.z)) + up2;
            if v.x * v.x + v.z * v.z > up.x * up.x + up.z * up.z { up = v; }
        }
        if up.x * up.x + up.z * up.z > r.x * r.x + r.z * r.z {
            let moved: Aabb = (b.0 + up, b.1 + up);
            return up + Player::collide_axes(w, moved, DVec3::new(0.0, -up.y + mv.y, 0.0));
        }
    }
    r
}

pub struct Hit {
    pub block: [i32; 3],
    pub normal: [i32; 3],
}

/// Voxel DDA; fluids are not targetable.
pub fn raycast(w: &World, origin: DVec3, dir: Vec3, reach: f64) -> Option<Hit> {
    let d = DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64).normalize();
    let mut cell = [origin.x.floor() as i32, origin.y.floor() as i32, origin.z.floor() as i32];
    let step = [d.x.signum() as i32, d.y.signum() as i32, d.z.signum() as i32];
    let inv = [1.0 / d.x.abs().max(1e-12), 1.0 / d.y.abs().max(1e-12), 1.0 / d.z.abs().max(1e-12)];
    let o = [origin.x, origin.y, origin.z];
    let mut t_max = [0.0; 3];
    for a in 0..3 {
        let next = if step[a] > 0 { cell[a] as f64 + 1.0 - o[a] } else { o[a] - cell[a] as f64 };
        t_max[a] = next * inv[a];
    }
    let mut normal = [0; 3];
    let mut t = 0.0;
    while t <= reach {
        let id = w.get(cell[0], cell[1], cell[2]);
        if id != AIR && def(id).solid { return Some(Hit { block: cell, normal }); }
        let a = if t_max[0] < t_max[1] && t_max[0] < t_max[2] { 0 } else if t_max[1] < t_max[2] { 1 } else { 2 };
        t = t_max[a];
        t_max[a] += inv[a];
        cell[a] += step[a];
        normal = [0; 3];
        normal[a] = -step[a];
    }
    None
}
