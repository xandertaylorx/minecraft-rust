//! Entities: mobs (with vanilla-style attributes, physics and goal-based AI), arrows and dropped items.
use std::collections::BinaryHeap;

use glam::DVec3;
use worldgen::random::Xoro;

use crate::blocks::{def, id_of};
use crate::player::{collide_step, Player};
use crate::world::World;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind { Zombie, Skeleton, Creeper, Pig, Cow, Sheep, Chicken }

pub struct Stats {
    pub name: &'static str,
    pub health: f32,
    pub speed: f32,
    pub width: f64,
    pub height: f64,
    pub eye: f64,
    pub attack: f32,
    pub follow: f64,
    pub hostile: bool,
}

pub fn stats(k: Kind) -> Stats {
    match k {
        Kind::Zombie => Stats { name: "Zombie", health: 20.0, speed: 0.23, width: 0.6, height: 1.95, eye: 1.74, attack: 3.0, follow: 35.0, hostile: true },
        Kind::Skeleton => Stats { name: "Skeleton", health: 20.0, speed: 0.25, width: 0.6, height: 1.99, eye: 1.74, attack: 2.0, follow: 16.0, hostile: true },
        Kind::Creeper => Stats { name: "Creeper", health: 20.0, speed: 0.25, width: 0.6, height: 1.7, eye: 1.445, attack: 0.0, follow: 16.0, hostile: true },
        Kind::Pig => Stats { name: "Pig", health: 10.0, speed: 0.25, width: 0.9, height: 0.9, eye: 0.765, attack: 0.0, follow: 16.0, hostile: false },
        Kind::Cow => Stats { name: "Cow", health: 10.0, speed: 0.2, width: 0.9, height: 1.4, eye: 1.3, attack: 0.0, follow: 16.0, hostile: false },
        Kind::Sheep => Stats { name: "Sheep", health: 8.0, speed: 0.23, width: 0.9, height: 1.3, eye: 1.235, attack: 0.0, follow: 16.0, hostile: false },
        Kind::Chicken => Stats { name: "Chicken", health: 4.0, speed: 0.25, width: 0.4, height: 0.7, eye: 0.644, attack: 0.0, follow: 16.0, hostile: false },
    }
}

// ------------------------------------------------------------------ AI framework

pub const F_MOVE: u8 = 1;
pub const F_LOOK: u8 = 2;
pub const F_TARGET: u8 = 8;

pub struct PlayerInfo {
    pub pos: DVec3,
    pub eye: DVec3,
    pub alive: bool,
    pub targetable: bool,
}

pub enum Event {
    Arrow { pos: DVec3, vel: DVec3, owner: u64 },
    HurtPlayer { amount: f32, from: DVec3, knockback: f32, by: &'static str },
    Explode { pos: DVec3, power: f32 },
}

pub struct Ctx<'a> {
    pub world: &'a World,
    pub player: PlayerInfo,
    pub events: &'a mut Vec<Event>,
    pub game_time: i64,
    pub daylight: f32,
}

pub trait Goal: Send {
    fn flags(&self) -> u8;
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool;
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { self.can_use(m, c) }
    fn start(&mut self, _m: &mut Mob, _c: &mut Ctx) {}
    fn stop(&mut self, _m: &mut Mob, _c: &mut Ctx) {}
    fn tick(&mut self, _m: &mut Mob, _c: &mut Ctx) {}
}

#[derive(Default)]
pub struct GoalSelector {
    goals: Vec<(i32, Box<dyn Goal>, bool)>,
}

impl GoalSelector {
    pub fn add(&mut self, priority: i32, g: Box<dyn Goal>) { self.goals.push((priority, g, false)); self.goals.sort_by_key(|g| g.0); }

    pub fn tick(&mut self, m: &mut Mob, c: &mut Ctx) {
        for i in 0..self.goals.len() {
            if self.goals[i].2 {
                let (_, g, _) = &mut self.goals[i];
                if !g.can_continue(m, c) { g.stop(m, c); self.goals[i].2 = false; }
            }
        }
        for i in 0..self.goals.len() {
            if self.goals[i].2 { continue; }
            let prio = self.goals[i].0;
            let flags = self.goals[i].1.flags();
            // A goal may start if every running goal sharing a flag has a lower priority (higher number).
            let blocked = self.goals.iter().enumerate().any(|(j, (p, g, running))| j != i && *running && g.flags() & flags != 0 && *p <= prio);
            if blocked { continue; }
            if self.goals[i].1.can_use(m, c) {
                for j in 0..self.goals.len() {
                    if j != i && self.goals[j].2 && self.goals[j].1.flags() & flags != 0 { self.goals[j].1.stop(m, c); self.goals[j].2 = false; }
                }
                self.goals[i].1.start(m, c);
                self.goals[i].2 = true;
            }
        }
        for i in 0..self.goals.len() { if self.goals[i].2 { self.goals[i].1.tick(m, c); } }
    }
}

// ------------------------------------------------------------------ navigation (A* over walkable cells)

#[derive(Default)]
pub struct Nav {
    pub path: Vec<[i32; 3]>,
    pub idx: usize,
    pub speed: f64,
    pub stuck: i32,
    last_pos: DVec3,
}

impl Nav {
    pub fn is_done(&self) -> bool { self.idx >= self.path.len() }
    pub fn stop(&mut self) { self.path.clear(); self.idx = 0; }
}

fn passable(w: &World, x: i32, y: i32, z: i32) -> bool { !def(w.get(x, y, z)).solid }
fn is_water(w: &World, x: i32, y: i32, z: i32) -> bool { w.get(x, y, z) == id_of("water") }
fn standable(w: &World, x: i32, y: i32, z: i32, height: i32) -> bool {
    if (0..height).any(|d| !passable(w, x, y + d, z)) { return false; }
    def(w.get(x, y - 1, z)).solid || is_water(w, x, y - 1, z)
}

#[derive(PartialEq)]
struct Node { f: f32, g: f32, pos: [i32; 3] }
impl Eq for Node {}
impl PartialOrd for Node { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) } }
impl Ord for Node { fn cmp(&self, o: &Self) -> std::cmp::Ordering { o.f.partial_cmp(&self.f).unwrap_or(std::cmp::Ordering::Equal) } }

/// A* from `from` towards `to`; returns the best partial path when the goal is unreachable (vanilla behaviour).
pub fn find_path(w: &World, from: [i32; 3], to: [i32; 3], height: i32, max_nodes: usize, avoid_water: bool) -> Vec<[i32; 3]> {
    use std::collections::HashMap;
    let h = |p: [i32; 3]| (((p[0] - to[0]).pow(2) + (p[1] - to[1]).pow(2) + (p[2] - to[2]).pow(2)) as f32).sqrt();
    let mut open = BinaryHeap::new();
    let mut came: HashMap<[i32; 3], [i32; 3]> = HashMap::new();
    let mut gs: HashMap<[i32; 3], f32> = HashMap::new();
    open.push(Node { f: h(from), g: 0.0, pos: from });
    gs.insert(from, 0.0);
    let mut best = (h(from), from);
    let mut n = 0;
    while let Some(Node { g, pos, .. }) = open.pop() {
        if g > *gs.get(&pos).unwrap_or(&f32::MAX) { continue; }
        n += 1;
        let d = h(pos);
        if d < best.0 { best = (d, pos); }
        if d < 1.0 { best = (d, pos); break; }
        if n > max_nodes { break; }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            if dx != 0 && dz != 0 && !(passable(w, pos[0] + dx, pos[1], pos[2]) && passable(w, pos[0], pos[1], pos[2] + dz)) { continue; }
            let (nx, nz) = (pos[0] + dx, pos[2] + dz);
            let mut cand = None;
            for dy in [0, 1, -1, -2, -3] {
                let ny = pos[1] + dy;
                if dy == 1 && !(0..height).all(|k| passable(w, pos[0], pos[1] + height + k - height + 1 - 0, pos[2]) || k == 0) { }
                if !standable(w, nx, ny, nz, height) { continue; }
                if dy == 1 && !passable(w, pos[0], pos[1] + height, pos[2]) { continue; }
                if dy < 0 && !(0..-dy).all(|k| passable(w, nx, pos[1] - k, nz)) { continue; }
                cand = Some(ny);
                break;
            }
            let Some(ny) = cand else { continue };
            if avoid_water && is_water(w, nx, ny, nz) { continue; }
            if matches!(def(w.get(nx, ny - 1, nz)).name, "lava") || matches!(def(w.get(nx, ny, nz)).name, "lava" | "fire") { continue; }
            let np = [nx, ny, nz];
            let step = if dx != 0 && dz != 0 { 1.414 } else { 1.0 } + (ny - pos[1]).abs() as f32 * 0.5;
            let ng = g + step;
            if ng < *gs.get(&np).unwrap_or(&f32::MAX) {
                gs.insert(np, ng);
                came.insert(np, pos);
                open.push(Node { f: ng + h(np), g: ng, pos: np });
            }
        }
    }
    let mut path = vec![best.1];
    let mut cur = best.1;
    while let Some(p) = came.get(&cur) { path.push(*p); cur = *p; }
    path.reverse();
    path
}

// ------------------------------------------------------------------ the mob

#[derive(Default, Clone, Copy)]
pub struct MoveCtl { pub wanted: Option<DVec3>, pub speed_mod: f64, pub strafe: Option<(f32, f32)> }

pub struct Mob {
    pub id: u64,
    pub kind: Kind,
    pub pos: DVec3,
    pub prev_pos: DVec3,
    pub vel: DVec3,
    pub yaw: f32,
    pub prev_yaw: f32,
    pub head_yaw: f32,
    pub prev_head_yaw: f32,
    pub pitch: f32,
    pub prev_pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub in_water: bool,
    pub health: f32,
    pub max_health: f32,
    pub hurt_time: i32,
    pub invulnerable: i32,
    pub last_hurt: f32,
    pub death_time: i32,
    pub burning: i32,
    pub walk_pos: f32,
    pub walk_speed: f32,
    pub speed: f32,
    pub target: bool,
    pub aggressive: bool,
    pub swell: i32,
    pub old_swell: i32,
    pub swell_dir: i32,
    pub swing: i32,
    pub age: i32,
    pub no_action_time: i32,
    pub bow_use: i32,
    pub jump: bool,
    pub nav: Nav,
    pub move_ctl: MoveCtl,
    pub look_at: Option<(DVec3, f32)>,
    pub zza: f32,
    pub xxa: f32,
    pub goals: GoalSelector,
    pub target_goals: GoalSelector,
    pub rng: Xoro,
    pub hurt_by: bool,
    pub panic_ticks: i32,
    pub sun_check: i32,
    pub persistent: bool,
}

impl Mob {
    pub fn new(id: u64, kind: Kind, pos: DVec3, seed: i64) -> Self {
        let st = stats(kind);
        let mut m = Mob {
            id, kind, pos, prev_pos: pos, vel: DVec3::ZERO, yaw: 0.0, prev_yaw: 0.0, head_yaw: 0.0, prev_head_yaw: 0.0, pitch: 0.0, prev_pitch: 0.0,
            on_ground: false, horizontal_collision: false, in_water: false, health: st.health, max_health: st.health, hurt_time: 0, invulnerable: 0,
            last_hurt: 0.0, death_time: 0, burning: 0, walk_pos: 0.0, walk_speed: 0.0, speed: st.speed, target: false, aggressive: false, swell: 0,
            old_swell: 0, swell_dir: -1, swing: -1, age: 0, no_action_time: 0, bow_use: 0, jump: false, nav: Nav::default(),
            move_ctl: MoveCtl::default(), look_at: None, zza: 0.0, xxa: 0.0, goals: GoalSelector::default(), target_goals: GoalSelector::default(),
            rng: Xoro::new(seed), hurt_by: false, panic_ticks: 0, sun_check: 0, persistent: false,
        };
        m.yaw = (m.rng.next_float() * 360.0) - 180.0;
        m.prev_yaw = m.yaw;
        m.head_yaw = m.yaw;
        m.prev_head_yaw = m.yaw;
        crate::ai::setup_goals(&mut m);
        m
    }

    pub fn alive(&self) -> bool { self.health > 0.0 && self.death_time == 0 }
    pub fn st(&self) -> Stats { stats(self.kind) }
    pub fn eye(&self) -> DVec3 { self.pos + DVec3::new(0.0, self.st().eye, 0.0) }
    pub fn center(&self) -> DVec3 { self.pos + DVec3::new(0.0, self.st().height / 2.0, 0.0) }
    pub fn aabb(&self) -> (DVec3, DVec3) {
        let s = self.st();
        (DVec3::new(self.pos.x - s.width / 2.0, self.pos.y, self.pos.z - s.width / 2.0), DVec3::new(self.pos.x + s.width / 2.0, self.pos.y + s.height, self.pos.z + s.width / 2.0))
    }

    /// `LivingEntity.hurt`. Returns whether damage was dealt.
    pub fn hurt(&mut self, amount: f32, from: Option<DVec3>, knockback: f32) -> bool {
        if !self.alive() { return false; }
        let dealt;
        if self.invulnerable > 10 {
            if amount <= self.last_hurt { return false; }
            dealt = amount - self.last_hurt;
            self.last_hurt = amount;
        } else {
            self.last_hurt = amount;
            self.invulnerable = 20;
            self.hurt_time = 10;
            dealt = amount;
        }
        self.health -= dealt;
        if let Some(src) = from {
            self.knockback(0.4 + knockback * 0.5, src.x - self.pos.x, src.z - self.pos.z);
        }
        self.hurt_by = true;
        if self.health <= 0.0 { self.health = 0.0; }
        true
    }

    /// `LivingEntity.knockback(strength, dx, dz)`: pushes away from (dx, dz).
    pub fn knockback(&mut self, strength: f32, dx: f64, dz: f64) {
        let s = strength as f64;
        if s <= 0.0 { return; }
        let len = (dx * dx + dz * dz).sqrt();
        let (nx, nz) = if len < 1e-5 { (0.0, 0.0) } else { (dx / len * s, dz / len * s) };
        self.vel.x = self.vel.x / 2.0 - nx;
        self.vel.z = self.vel.z / 2.0 - nz;
        if self.on_ground { self.vel.y = (self.vel.y / 2.0 + s).min(0.4); }
    }

    pub fn distance_sqr_to(&self, p: DVec3) -> f64 { (self.pos - p).length_squared() }

    pub fn set_wanted(&mut self, p: DVec3, speed_mod: f64) { self.move_ctl.wanted = Some(p); self.move_ctl.speed_mod = speed_mod; }

    pub fn move_to(&mut self, w: &World, p: DVec3, speed: f64) -> bool {
        let from = [self.pos.x.floor() as i32, self.pos.y.floor() as i32, self.pos.z.floor() as i32];
        let to = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
        let h = (self.st().height.ceil()) as i32;
        let path = find_path(w, from, to, h, 400, !matches!(self.kind, Kind::Zombie | Kind::Skeleton | Kind::Creeper | Kind::Pig | Kind::Cow | Kind::Sheep | Kind::Chicken) || true);
        if path.len() < 2 { self.nav.stop(); return false; }
        self.nav = Nav { path, idx: 1, speed, stuck: 0, last_pos: self.pos };
        true
    }

    fn tick_nav(&mut self, w: &World) {
        if self.nav.is_done() { return; }
        let node = self.nav.path[self.nav.idx];
        let target = DVec3::new(node[0] as f64 + 0.5, node[1] as f64, node[2] as f64 + 0.5);
        let half = (self.st().width / 2.0).max(0.4);
        let (dx, dz, dy) = (target.x - self.pos.x, target.z - self.pos.z, (target.y - self.pos.y).abs());
        if dx.abs() < half * 0.9 && dz.abs() < half * 0.9 && dy < 1.0 { self.nav.idx += 1; self.nav.stuck = 0; }
        if self.nav.is_done() { self.move_ctl.wanted = None; return; }
        let node = self.nav.path[self.nav.idx];
        self.set_wanted(DVec3::new(node[0] as f64 + 0.5, node[1] as f64, node[2] as f64 + 0.5), self.nav.speed);
        // Stuck detection (PathNavigation.doStuckDetection).
        self.nav.stuck += 1;
        if self.nav.stuck > 100 { self.nav.stop(); self.move_ctl.wanted = None; }
        let _ = w;
    }

    fn tick_move_control(&mut self) {
        let Some(t) = self.move_ctl.wanted else { self.zza = 0.0; self.xxa = 0.0; return; };
        let (d0, d1, d2) = (t.x - self.pos.x, t.z - self.pos.z, t.y - self.pos.y);
        if d0 * d0 + d1 * d1 + d2 * d2 < 2.5e-7 { self.zza = 0.0; self.move_ctl.wanted = None; return; }
        let f9 = (d1.atan2(d0).to_degrees() as f32) - 90.0;
        self.yaw = rot_lerp(self.yaw, f9, 90.0);
        self.speed = (self.move_ctl.speed_mod * self.st().speed as f64) as f32;
        self.zza = 1.0;
        let width = self.st().width;
        if d2 > 0.6 && d0 * d0 + d1 * d1 < width.max(1.0) { self.jump = true; }
        self.move_ctl.wanted = None;
    }

    fn tick_look(&mut self) {
        if let Some((p, speed)) = self.look_at.take() {
            let (dx, dy, dz) = (p.x - self.pos.x, p.y - self.eye().y, p.z - self.pos.z);
            let h = (dx * dx + dz * dz).sqrt();
            let target_yaw = (dz.atan2(dx).to_degrees() as f32) - 90.0;
            let target_pitch = -(dy.atan2(h).to_degrees() as f32);
            self.head_yaw = rot_lerp(self.head_yaw, target_yaw, speed);
            self.pitch = rot_lerp(self.pitch, target_pitch, speed);
        } else {
            // Drift back to facing forward.
            self.head_yaw = rot_lerp(self.head_yaw, self.yaw, 10.0);
            self.pitch = rot_lerp(self.pitch, 0.0, 5.0);
        }
        // Head is limited to ±75° from the body; the body follows when it exceeds that.
        let diff = wrap_deg(self.head_yaw - self.yaw);
        if diff > 75.0 { self.yaw = self.head_yaw - 75.0 * diff.signum() * 0.0 - 75.0; } else if diff < -75.0 { self.yaw = self.head_yaw + 75.0; }
    }

    /// One 20 TPS update (`Mob.serverAiStep` + `LivingEntity.aiStep`).
    pub fn tick(&mut self, ctx: &mut Ctx) {
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
        self.prev_head_yaw = self.head_yaw;
        self.prev_pitch = self.pitch;
        self.age += 1;
        if self.invulnerable > 0 { self.invulnerable -= 1; }
        if self.hurt_time > 0 { self.hurt_time -= 1; }
        if self.swing >= 0 { self.swing += 1; if self.swing >= 6 { self.swing = -1; } }
        if self.health <= 0.0 {
            self.death_time += 1;
            self.physics(ctx.world, true);
            return;
        }
        // Fire: damage once per second, extinguished by water.
        if self.burning > 0 {
            self.burning -= 1;
            if self.in_water { self.burning = 0; }
            if self.burning % 20 == 0 { self.hurt(1.0, None, 0.0); self.invulnerable = self.invulnerable.min(20); }
        }
        let water = id_of("water");
        let lava = id_of("lava");
        let (bx, by, bz) = (self.pos.x.floor() as i32, (self.pos.y + 0.1).floor() as i32, self.pos.z.floor() as i32);
        self.in_water = ctx.world.get(bx, by, bz) == water;
        if ctx.world.get(bx, by, bz) == lava { self.burning = self.burning.max(300); if self.age % 10 == 0 { self.hurt(4.0, None, 0.0); } }
        if ctx.world.get(bx, by, bz) == id_of("fire") { self.burning = self.burning.max(160); if self.age % 10 == 0 { self.hurt(1.0, None, 0.0); } }
        // Goal selectors then controls.
        let mut tg = std::mem::take(&mut self.target_goals);
        tg.tick(self, ctx);
        self.target_goals = tg;
        let mut gs = std::mem::take(&mut self.goals);
        gs.tick(self, ctx);
        self.goals = gs;
        self.tick_nav(ctx.world);
        self.tick_move_control();
        self.tick_look();
        self.no_action_time += 1;
        self.physics(ctx.world, false);
        // Walk animation (LivingEntity.calculateEntityAnimation).
        let dist = ((self.pos.x - self.prev_pos.x).powi(2) + (self.pos.z - self.prev_pos.z).powi(2)).sqrt() as f32;
        let f = (dist * 4.0).min(1.0);
        self.walk_speed += (f - self.walk_speed) * 0.4;
        self.walk_pos += self.walk_speed;
    }

    fn physics(&mut self, w: &World, dead: bool) {
        let s = self.st();
        let (bx, by, bz) = (self.pos.x.floor() as i32, (self.pos.y + 0.1).floor() as i32, self.pos.z.floor() as i32);
        if !w.loaded(bx, bz) { return; }
        let in_water = w.get(bx, by, bz) == id_of("water");
        let jump = std::mem::take(&mut self.jump);
        let (zza, xxa) = if dead { (0.0, 0.0) } else { (self.zza as f64, self.xxa as f64) };
        if jump && !dead {
            if in_water { self.vel.y += 0.04; } else if self.on_ground { self.vel.y = 0.42; if self.aggressive { /* sprinting-less */ } }
        }
        let input = DVec3::new(xxa, 0.0, zza);
        let (sy, cy) = ((self.yaw as f64).to_radians().sin(), (self.yaw as f64).to_radians().cos());
        let mut move_relative = |me: &mut Mob, speed: f64| {
            let l = input.length_squared();
            if l < 1e-7 { return; }
            let v = if l > 1.0 { input.normalize() } else { input } * speed;
            me.vel += DVec3::new(v.x * cy - v.z * sy, v.y, v.z * cy + v.x * sy);
        };
        if in_water {
            let mut f = 0.8;
            if self.aggressive { f = 0.8; }
            move_relative(self, 0.02);
            self.do_move(w, s.width, s.height);
            let mut v = self.vel;
            v = DVec3::new(v.x * f, v.y * 0.8 - 0.08 / 16.0, v.z * f);
            self.vel = v;
        } else {
            let friction = if self.on_ground { 0.6 } else { 1.0 };
            let f1 = friction * 0.91;
            let sp = if self.on_ground { self.speed as f64 * (0.21600002 / (friction * friction * friction)) } else { 0.02 };
            move_relative(self, sp);
            self.do_move(w, s.width, s.height);
            let vy = (self.vel.y - 0.08) * 0.98;
            self.vel = DVec3::new(self.vel.x * f1, vy, self.vel.z * f1);
        }
        self.xxa = 0.0;
    }

    fn do_move(&mut self, w: &World, width: f64, height: f64) {
        let want = self.vel;
        let r = collide_step(w, self.pos, width / 2.0, height, want, self.on_ground);
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
}

pub fn wrap_deg(a: f32) -> f32 { let mut f = a % 360.0; if f >= 180.0 { f -= 360.0; } if f < -180.0 { f += 360.0; } f }
pub fn rot_lerp(cur: f32, target: f32, max: f32) -> f32 { let d = wrap_deg(target - cur).clamp(-max, max); cur + d }

// ------------------------------------------------------------------ projectiles and drops

pub struct Arrow {
    pub pos: DVec3,
    pub prev_pos: DVec3,
    pub vel: DVec3,
    pub from_player: bool,
    pub owner: u64,
    pub in_ground: bool,
    pub life: i32,
    pub crit: bool,
}

pub struct ItemEntity {
    pub pos: DVec3,
    pub prev_pos: DVec3,
    pub vel: DVec3,
    pub item: u16,
    pub count: u8,
    pub damage: u16,
    pub age: i32,
    pub pickup_delay: i32,
    pub on_ground: bool,
}

impl ItemEntity {
    pub fn new(pos: DVec3, vel: DVec3, item: u16, count: u8) -> Self {
        Self { pos, prev_pos: pos, vel, item, count, damage: 0, age: 0, pickup_delay: 10, on_ground: false }
    }

    pub fn tick(&mut self, w: &World) {
        self.prev_pos = self.pos;
        self.age += 1;
        if self.pickup_delay > 0 { self.pickup_delay -= 1; }
        let r = collide_step(w, self.pos, 0.125, 0.25, self.vel, false);
        self.pos += r;
        if (self.vel.y - r.y).abs() > 1e-9 { self.on_ground = self.vel.y < 0.0; self.vel.y = 0.0; } else { self.on_ground = false; }
        if (self.vel.x - r.x).abs() > 1e-9 { self.vel.x = 0.0; }
        if (self.vel.z - r.z).abs() > 1e-9 { self.vel.z = 0.0; }
        let friction = if self.on_ground { 0.6 * 0.98 } else { 0.98 };
        self.vel = DVec3::new(self.vel.x * friction, (self.vel.y - 0.04) * 0.98, self.vel.z * friction);
        let _ = Player::collide_axes;
    }
}

pub struct PrimedTnt {
    pub pos: DVec3,
    pub prev_pos: DVec3,
    pub vel: DVec3,
    pub fuse: i32,
    pub on_ground: bool,
}

#[derive(Default)]
pub struct Entities {
    pub tnts: Vec<PrimedTnt>,
    pub mobs: Vec<Mob>,
    pub arrows: Vec<Arrow>,
    pub items: Vec<ItemEntity>,
    pub next_id: u64,
}

/// Ray vs AABB (slab test); returns the entry distance along the unit direction.
pub fn ray_aabb(origin: DVec3, dir: DVec3, b: (DVec3, DVec3), max: f64) -> Option<f64> {
    let (mut tmin, mut tmax) = (0.0f64, max);
    for a in 0..3 {
        if dir[a].abs() < 1e-12 {
            if origin[a] < b.0[a] || origin[a] > b.1[a] { return None; }
        } else {
            let (mut t1, mut t2) = ((b.0[a] - origin[a]) / dir[a], (b.1[a] - origin[a]) / dir[a]);
            if t1 > t2 { std::mem::swap(&mut t1, &mut t2); }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax { return None; }
        }
    }
    Some(tmin)
}

/// Line of sight between two points (stepped ray against solid blocks).
pub fn line_of_sight(w: &World, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 1e-6 { return true; }
    let step = d / len * 0.25;
    let mut p = a;
    let mut t = 0.0;
    while t < len {
        if def(w.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)).solid { return false; }
        p += step;
        t += 0.25;
    }
    true
}
