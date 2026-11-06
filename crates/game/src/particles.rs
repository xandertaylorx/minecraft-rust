//! Particles: block-break crumbs, explosion puffs/emitters, smoke (vanilla `TerrainParticle`, `HugeExplosion*`).
use glam::{DVec3, Vec3};
use worldgen::random::Xoro;

use crate::blocks::def;
use crate::entity_render::{EntityAtlas, ENT_ATLAS};
use crate::mesh::Vertex;
use crate::player::Player;
use crate::world::World;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PKind { Terrain, Explosion, Smoke, Flame }

pub struct Particle {
    pub kind: PKind,
    pub pos: DVec3,
    pub prev: DVec3,
    pub vel: DVec3,
    pub age: i32,
    pub max_age: i32,
    pub size: f32,
    pub gravity: f32,
    pub friction: f64,
    pub on_ground: bool,
    pub color: [f32; 3],
    /// Terrain: pixel rect of the 4x4 texel crumb; others pick a frame by age.
    pub rect: [f32; 4],
    pub tile_rgb: [f32; 3],
}

pub struct Emitter { pub pos: DVec3, pub age: i32, pub max_age: i32 }

#[derive(Default)]
pub struct Particles {
    pub list: Vec<Particle>,
    pub emitters: Vec<Emitter>,
}

fn rnd(r: &mut Xoro) -> f64 { r.next_double() }

impl Particles {
    /// `ParticleEngine.destroy`: 4x4x4 crumbs flying outward from the broken block.
    pub fn block_break(&mut self, r: &mut Xoro, block: [i32; 3], tile: u32, tint: [f32; 3]) {
        for i in 0..4 { for j in 0..4 { for k in 0..4 {
            if self.list.len() > 3000 { return; }
            let (x, y, z) = (block[0] as f64 + (i as f64 + 0.5) / 4.0, block[1] as f64 + (j as f64 + 0.5) / 4.0, block[2] as f64 + (k as f64 + 0.5) / 4.0);
            let (xa, ya, za) = (x - block[0] as f64 - 0.5, y - block[1] as f64 - 0.5, z - block[2] as f64 - 0.5);
            self.add_terrain(r, DVec3::new(x, y, z), DVec3::new(xa, ya, za), tile, tint);
        } } }
    }

    /// `ParticleEngine.crack`: one crumb from the mined face.
    pub fn crack(&mut self, r: &mut Xoro, block: [i32; 3], normal: [i32; 3], tile: u32, tint: [f32; 3]) {
        let (x, y, z) = (block[0] as f64, block[1] as f64, block[2] as f64);
        let mut p = DVec3::new(x + rnd(r) * 0.8 + 0.1, y + rnd(r) * 0.8 + 0.1, z + rnd(r) * 0.8 + 0.1);
        let s = 0.1;
        match (normal[0], normal[1], normal[2]) {
            (0, -1, 0) => p.y = y - s, (0, 1, 0) => p.y = y + 1.0 + s,
            (0, 0, -1) => p.z = z - s, (0, 0, 1) => p.z = z + 1.0 + s,
            (-1, 0, 0) => p.x = x - s, _ => p.x = x + 1.0 + s,
        }
        self.add_terrain(r, p, DVec3::ZERO, tile, tint);
        if let Some(l) = self.list.last_mut() { l.vel = l.vel * 0.2; l.size *= 0.6; }
    }

    fn add_terrain(&mut self, r: &mut Xoro, pos: DVec3, offset: DVec3, tile: u32, tint: [f32; 3]) {
        // Particle(level, x, y, z, xa, ya, za)
        let mut v = DVec3::new(offset.x + (rnd(r) * 2.0 - 1.0) * 0.4, offset.y + (rnd(r) * 2.0 - 1.0) * 0.4, offset.z + (rnd(r) * 2.0 - 1.0) * 0.4);
        let speed = (rnd(r) + rnd(r) + 1.0) * 0.15;
        let len = v.length().max(1e-6);
        v = DVec3::new(v.x / len * speed * 0.4, v.y / len * speed * 0.4 + 0.1, v.z / len * speed * 0.4);
        let size = 0.1 * (rnd(r) as f32 * 0.5 + 0.5) * 2.0 / 2.0;
        let (uo, vo) = (r.next_int_bound(4) as f32, r.next_int_bound(4) as f32);
        let (tx, ty) = ((tile % 16) as f32 * 16.0, (tile / 16) as f32 * 16.0);
        self.list.push(Particle {
            kind: PKind::Terrain, pos, prev: pos, vel: v, age: 0, max_age: (4.0 / (rnd(r) * 0.9 + 0.1)) as i32,
            size, gravity: 1.0, friction: 0.98, on_ground: false, color: [0.6 * tint[0], 0.6 * tint[1], 0.6 * tint[2]],
            rect: [tx + uo * 4.0, ty + vo * 4.0, 4.0, 4.0], tile_rgb: tint,
        });
    }

    /// `EXPLOSION_EMITTER` for power >= 2 (many puffs over 8 ticks) else a single puff.
    pub fn explosion(&mut self, r: &mut Xoro, pos: DVec3, big: bool) {
        if big { self.emitters.push(Emitter { pos, age: 0, max_age: 8 }); } else { self.puff(r, pos, 1.0); }
    }

    fn puff(&mut self, r: &mut Xoro, pos: DVec3, life_ratio: f32) {
        let f = (rnd(r) * 0.6 + 0.4) as f32;
        self.list.push(Particle {
            kind: PKind::Explosion, pos, prev: pos, vel: DVec3::ZERO, age: 0, max_age: 6 + r.next_int_bound(4),
            size: 1.0 - life_ratio * 0.5, gravity: 0.0, friction: 1.0, on_ground: false, color: [f, f, f], rect: [0.0; 4], tile_rgb: [1.0; 3],
        });
    }

    pub fn smoke(&mut self, r: &mut Xoro, pos: DVec3, vel: DVec3) {
        if self.list.len() > 3000 { return; }
        let g = (rnd(r) * 0.3 + 0.5) as f32;
        self.list.push(Particle {
            kind: PKind::Smoke, pos, prev: pos, vel, age: 0, max_age: 8 + r.next_int_bound(14),
            size: 0.3 + rnd(r) as f32 * 0.5, gravity: -0.1, friction: 0.96, on_ground: false, color: [g, g, g], rect: [0.0; 4], tile_rgb: [1.0; 3],
        });
    }

    pub fn tick(&mut self, w: &World, r: &mut Xoro) {
        let mut spawn: Vec<(DVec3, f32)> = Vec::new();
        for e in self.emitters.iter_mut() {
            for _ in 0..6 {
                let o = DVec3::new((rnd(r) - rnd(r)) * 4.0, (rnd(r) - rnd(r)) * 4.0, (rnd(r) - rnd(r)) * 4.0);
                spawn.push((e.pos + o, e.age as f32 / e.max_age as f32));
            }
            e.age += 1;
        }
        self.emitters.retain(|e| e.age < e.max_age);
        for (p, ratio) in spawn { self.puff(r, p, ratio); }
        self.list.retain_mut(|p| {
            p.prev = p.pos;
            p.age += 1;
            if p.age >= p.max_age { return false; }
            p.vel.y -= 0.04 * p.gravity as f64;
            let half = 0.1;
            let b = (DVec3::new(p.pos.x - half, p.pos.y - half, p.pos.z - half), DVec3::new(p.pos.x + half, p.pos.y + half, p.pos.z + half));
            let mv = if matches!(p.kind, PKind::Explosion) { p.vel } else { Player::collide_axes(w, b, p.vel) };
            p.on_ground = p.vel.y != mv.y && p.vel.y < 0.0;
            if !matches!(p.kind, PKind::Explosion) {
                if p.vel.x != mv.x { p.vel.x = 0.0; }
                if p.vel.z != mv.z { p.vel.z = 0.0; }
                if p.vel.y != mv.y { p.vel.y = 0.0; }
            }
            p.pos += mv;
            p.vel *= p.friction;
            if p.on_ground { p.vel.x *= 0.7; p.vel.z *= 0.7; }
            true
        });
        let _ = def;
    }

    /// Billboard quads facing the camera.
    pub fn build(&self, atlas: &EntityAtlas, cam: DVec3, forward: Vec3, a: f32, out: &mut (Vec<Vertex>, Vec<u32>)) {
        let at = ENT_ATLAS as f32;
        let up_ref = if forward.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
        let right = forward.cross(up_ref).normalize();
        let up = right.cross(forward).normalize();
        for p in &self.list {
            let pos = p.prev + (p.pos - p.prev) * a as f64;
            let o = (pos - cam).as_vec3();
            let (rect, size, color, light, alpha) = match p.kind {
                PKind::Terrain => (p.rect, p.size, p.color, [12.0, 0.0], 1.0),
                PKind::Explosion => {
                    let n = ((p.age as f32 + a) * 16.0 / p.max_age as f32) as usize;
                    (atlas.particle_rects[&format!("explosion_{}", n.min(15))], p.size, p.color, [15.0, 15.0], 1.0)
                }
                PKind::Smoke => {
                    let n = ((p.age as f32 / p.max_age as f32) * 12.0) as usize;
                    (atlas.particle_rects[&format!("big_smoke_{}", n.min(11))], p.size, p.color, [15.0, 4.0], 1.0 - (p.age as f32 / p.max_age as f32) * 0.3)
                }
                PKind::Flame => (atlas.particle_rects["flame"], p.size, [1.0; 3], [15.0, 15.0], 1.0),
            };
            let (r, u) = (right * size, up * size);
            let (u0, v0, u1, v1) = (rect[0] / at, rect[1] / at, (rect[0] + rect[2]) / at, (rect[1] + rect[3]) / at);
            crate::model::quad(out, [o - r + u, o + r + u, o + r - u, o - r - u], [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], [color[0], color[1], color[2], alpha], light);
        }
    }
}
