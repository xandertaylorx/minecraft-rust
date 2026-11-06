//! Gameplay systems: combat, projectiles, drops, fire, explosions, spawning, item use.
use glam::DVec3;
use worldgen::random::Xoro;

use crate::blocks::{self, def, id_of};
use crate::entity::*;
use crate::game::{Game, Mode, Screen, Stack};
use crate::items::{self, Kind as IK};

const REACH_ENTITY: f64 = 3.0;

fn blast_resistance(name: &str) -> f32 {
    match name {
        "bedrock" => 3_600_000.0,
        "stone" | "cobblestone" | "deepslate" | "tuff" | "granite" | "basalt" | "blackstone" | "end_stone" | "bricks" => 6.0,
        "oak_planks" | "oak_log" => 3.0,
        "dirt" | "sand" | "grass_block" | "gravel" | "clay" => 0.5,
        "oak_leaves" => 0.2,
        "glass" | "ice" => 0.3,
        "water" | "lava" => 100.0,
        n => blocks::hardness(n).max(0.1) * 4.0,
    }
}

impl Game {
    fn particle_tile(&self, id: u16) -> (u32, [f32; 3]) {
        let d = def(id);
        let tex = if d.name == "grass_block" { d.bottom } else { d.side };
        let tile = self.names.get(tex).copied().unwrap_or(0);
        let tint = match d.tint { crate::blocks::Tint::Foliage => [0.47, 0.67, 0.19], crate::blocks::Tint::Water => [0.25, 0.46, 0.89], _ => [1.0; 3] };
        (tile, tint)
    }

    pub fn break_particles(&mut self, p: [i32; 3], id: u16) {
        let (tile, tint) = self.particle_tile(id);
        let mut r = std::mem::replace(&mut self.rng, Xoro::new(1));
        self.particles.block_break(&mut r, p, tile, tint);
        self.rng = r;
    }

    pub fn crack_particles(&mut self, p: [i32; 3], normal: [i32; 3], id: u16) {
        let (tile, tint) = self.particle_tile(id);
        let mut r = std::mem::replace(&mut self.rng, Xoro::new(1));
        self.particles.crack(&mut r, p, normal, tile, tint);
        self.rng = r;
    }

    pub fn prime_tnt(&mut self, pos: DVec3, fuse: i32) {
        let a = self.rng.next_double() * std::f64::consts::TAU;
        self.ents.tnts.push(PrimedTnt { pos, prev_pos: pos, vel: DVec3::new(-a.sin() * 0.02, 0.2, -a.cos() * 0.02), fuse, on_ground: false });
    }

    fn tick_tnts(&mut self) {
        let mut tnts = std::mem::take(&mut self.ents.tnts);
        let mut boom: Vec<DVec3> = Vec::new();
        for t in tnts.iter_mut() {
            t.prev_pos = t.pos;
            t.vel.y -= 0.04;
            let r = crate::player::collide_step(&self.world, t.pos, 0.49, 0.98, t.vel, false);
            t.pos += r;
            t.on_ground = (t.vel.y - r.y).abs() > 1e-9 && t.vel.y < 0.0;
            if (t.vel.x - r.x).abs() > 1e-9 { t.vel.x = 0.0; }
            if (t.vel.z - r.z).abs() > 1e-9 { t.vel.z = 0.0; }
            if (t.vel.y - r.y).abs() > 1e-9 { t.vel.y = 0.0; }
            t.vel *= 0.98;
            if t.on_ground { t.vel = DVec3::new(t.vel.x * 0.7, t.vel.y * -0.5, t.vel.z * 0.7); }
            t.fuse -= 1;
            if t.fuse <= 0 { boom.push(t.pos + DVec3::new(0.0, 0.98 / 16.0, 0.0)); }
        }
        tnts.retain(|t| t.fuse > 0);
        let mut added = std::mem::take(&mut self.ents.tnts);
        tnts.append(&mut added);
        self.ents.tnts = tnts;
        for c in boom { self.explode(c, 4.0); }
    }

    pub fn held(&self) -> Option<Stack> { self.slots[self.selected] }

    fn player_info(&self) -> PlayerInfo {
        PlayerInfo { pos: self.player.pos, eye: self.player.eye(), alive: self.health > 0.0, targetable: self.mode == Mode::Survival && self.health > 0.0 }
    }

    // ------------------------------------------------------------ targeting

    /// Nearest mob under the crosshair within reach; compared against the block hit by the caller.
    pub fn update_entity_target(&mut self) -> Option<(u64, f64)> {
        if self.screen != Screen::Playing { return None; }
        let dir = self.player.look_dir();
        let d = DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64).normalize();
        let eye = self.player.eye();
        let mut best: Option<(u64, f64)> = None;
        for m in &self.ents.mobs {
            if !m.alive() { continue; }
            let (lo, hi) = m.aabb();
            let b = (lo - DVec3::splat(0.1), hi + DVec3::splat(0.1));
            if let Some(t) = ray_aabb(eye, d, b, REACH_ENTITY) {
                if best.map_or(true, |(_, bt)| t < bt) { best = Some((m.id, t)); }
            }
        }
        best
    }

    // ------------------------------------------------------------ player health

    pub fn hurt_player(&mut self, amount: f32, from: Option<DVec3>, knockback: f32, by: &str) {
        if self.mode == Mode::Creative || self.health <= 0.0 { return; }
        if self.player_invuln > 10 { return; }
        self.player_invuln = 20;
        self.player_hurt = 10;
        self.health -= amount;
        if let Some(src) = from {
            let (dx, dz) = (src.x - self.player.pos.x, src.z - self.player.pos.z);
            let len = (dx * dx + dz * dz).sqrt().max(1e-5);
            let s = 0.4 + knockback as f64 * 0.5;
            self.player.vel.x = self.player.vel.x / 2.0 - dx / len * s;
            self.player.vel.z = self.player.vel.z / 2.0 - dz / len * s;
            if self.player.on_ground { self.player.vel.y = (self.player.vel.y / 2.0 + s).min(0.4); }
        }
        if self.health <= 0.0 {
            let msg = if by.is_empty() { "Player died".to_string() } else { format!("Player was slain by {by}") };
            self.say(&format!("§c{msg}"));
            // Respawn at the world spawn with a full inventory kept (vanilla would drop it; no death screen yet).
            self.health = 20.0;
            self.hunger = 20.0;
            self.player.pos = self.spawn;
            self.player.prev_pos = self.spawn;
            self.player.vel = DVec3::ZERO;
            self.player_burning = 0;
        }
    }

    // ------------------------------------------------------------ attacking

    pub fn attack_mob(&mut self, id: u64) {
        let held = self.held();
        let (bonus, speed) = held.map(|s| items::weapon_stats(s.id)).unwrap_or((0.0, 4.0));
        let cooldown = (20.0 / speed).max(1.0);
        let scale = ((self.attack_ticker as f32 + 0.5) / cooldown).clamp(0.0, 1.0);
        let mut dmg = (1.0 + bonus) * (0.2 + scale * scale * 0.8);
        let crit = self.player.fall_distance > 0.0 && !self.player.on_ground && !self.player.in_water && scale > 0.9 && !self.player.sprinting;
        if crit { dmg *= 1.5; }
        let kb = if self.player.sprinting && scale > 0.9 { 1.0 } else { 0.0 };
        let ppos = self.player.pos;
        let mut hit = false;
        if let Some(m) = self.ents.mobs.iter_mut().find(|m| m.id == id) {
            hit = m.hurt(dmg, Some(ppos), kb);
        }
        self.attack_ticker = 0;
        self.swing_ticks = 0;
        if hit {
            if kb > 0.0 { self.player.sprinting = false; }
            if self.mode == Mode::Survival { self.damage_held(if held.map_or(false, |s| matches!(items::item_def(s.id).map(|d| d.kind), Some(IK::Sword))) { 1 } else { 2 }); }
        }
    }

    pub fn damage_held(&mut self, amount: u16) {
        let Some(s) = self.slots[self.selected].as_mut() else { return };
        let max = items::max_damage(s.id);
        if max == 0 { return; }
        s.damage += amount;
        if s.damage >= max { self.slots[self.selected] = None; }
    }

    // ------------------------------------------------------------ drops

    pub fn spawn_item(&mut self, pos: DVec3, item: u16, count: u8) {
        if count == 0 { return; }
        let v = DVec3::new((self.rng.next_double() - 0.5) * 0.2, 0.2, (self.rng.next_double() - 0.5) * 0.2);
        self.ents.items.push(ItemEntity::new(pos, v, item, count));
    }

    fn mob_loot(&mut self, kind: Kind, pos: DVec3, burning: bool) {
        let r = |g: &mut Game, lo: i32, hi: i32| lo + g.rng.next_int_bound(hi - lo + 1);
        let mut drops: Vec<(&str, i32)> = vec![];
        match kind {
            Kind::Zombie => drops.push(("rotten_flesh", r(self, 0, 2))),
            Kind::Skeleton => { drops.push(("bone", r(self, 0, 2))); drops.push(("arrow", r(self, 0, 2))); }
            Kind::Creeper => drops.push(("gunpowder", r(self, 0, 2))),
            Kind::Pig => drops.push((if burning { "cooked_porkchop" } else { "porkchop" }, r(self, 1, 3))),
            Kind::Cow => { drops.push((if burning { "cooked_beef" } else { "beef" }, r(self, 1, 3))); drops.push(("leather", r(self, 0, 2))); }
            Kind::Sheep => drops.push(("mutton", r(self, 1, 2))),
            Kind::Chicken => { drops.push(("chicken", 1)); drops.push(("feather", r(self, 0, 2))); }
        }
        for (n, c) in drops { if let Some(id) = items::item_id(n) { self.spawn_item(pos + DVec3::new(0.0, 0.3, 0.0), id, c.max(0) as u8); } }
    }

    // ------------------------------------------------------------ explosions

    pub fn explode(&mut self, center: DVec3, power: f32) {
        let mut destroyed: Vec<(i32, i32, i32)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for i in 0..16 { for j in 0..16 { for k in 0..16 {
            if !(i == 0 || i == 15 || j == 0 || j == 15 || k == 0 || k == 15) { continue; }
            let mut d = DVec3::new(i as f64 / 15.0 * 2.0 - 1.0, j as f64 / 15.0 * 2.0 - 1.0, k as f64 / 15.0 * 2.0 - 1.0);
            d = d.normalize();
            let mut p = center;
            let mut intensity = power * (0.7 + self.rng.next_float() * 0.6);
            while intensity > 0.0 {
                let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                let b = self.world.get(bx, by, bz);
                if b != 0 {
                    intensity -= (blast_resistance(def(b).name) + 0.3) * 0.3;
                    if intensity > 0.0 && seen.insert((bx, by, bz)) { destroyed.push((bx, by, bz)); }
                }
                p += d * 0.3;
                intensity -= 0.225;
            }
        } } }
        for (x, y, z) in destroyed {
            let id = self.world.get(x, y, z);
            if self.world.set(x, y, z, 0) {
                self.mark_dirty_pub(x, z);
                if def(id).name == "tnt" {
                    let fuse = self.rng.next_int_bound(20) + 10;
                    self.prime_tnt(DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5), fuse);
                    continue;
                }
                if def(id).name == "fire" { self.fires.remove(&(x, y, z)); }
                if self.mode == Mode::Survival && self.rng.next_float() < 1.0 / power {
                    if let Some(d) = items::drop_for(def(id).name) { self.spawn_item(DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5), d, 1); }
                }
            }
        }
        // Explosion visuals: puff emitter plus smoke at a sample of destroyed blocks.
        {
            let mut r = std::mem::replace(&mut self.rng, Xoro::new(1));
            self.particles.explosion(&mut r, center, power >= 2.0);
            for (n, (x, y, z)) in seen.iter().enumerate() {
                if n % 4 != 0 { continue; }
                let p = DVec3::new(*x as f64 + r.next_double(), *y as f64 + r.next_double(), *z as f64 + r.next_double());
                let d = (p - center).normalize_or_zero() * (0.5 / (((p - center).length() / power as f64) + 0.1)) * (r.next_double() * r.next_double() + 0.3) * 0.1;
                self.particles.smoke(&mut r, (p + center) / 2.0, d);
            }
            self.rng = r;
        }
        for t in self.ents.tnts.iter_mut() {
            let c = t.pos + DVec3::new(0.0, 0.49, 0.0);
            let dist = (c - center).length();
            if dist < power as f64 * 2.0 { let impact = 1.0 - dist / (power as f64 * 2.0); t.vel += (c - center).normalize_or_zero() * impact; }
        }
        // Entities within twice the radius.
        let q = (power * 2.0) as f64;
        let ppos = self.player.pos + DVec3::new(0.0, 0.9, 0.0);
        let dist = (ppos - center).length();
        if dist < q {
            let impact = (1.0 - dist / q) * 1.0;
            let dmg = ((impact * impact + impact) / 2.0 * 7.0 * q + 1.0) as f32;
            self.hurt_player(dmg, Some(center), 0.0, "Creeper");
            let dir = (ppos - center).normalize_or_zero();
            self.player.vel += dir * impact;
        }
        for m in self.ents.mobs.iter_mut() {
            let c = m.center();
            let dist = (c - center).length();
            if dist < q && m.alive() {
                let impact = (1.0 - dist / q) * 1.0;
                let dmg = ((impact * impact + impact) / 2.0 * 7.0 * q + 1.0) as f32;
                m.hurt(dmg, None, 0.0);
                m.vel += (c - center).normalize_or_zero() * impact;
            }
        }
    }

    // ------------------------------------------------------------ arrows

    pub fn shoot_arrow(&mut self, pos: DVec3, vel: DVec3, from_player: bool, crit: bool, owner: u64) {
        self.ents.arrows.push(Arrow { pos, prev_pos: pos, vel, from_player, owner, in_ground: false, life: 0, crit });
    }

    fn tick_arrows(&mut self) {
        let mut arrows = std::mem::take(&mut self.ents.arrows);
        let water = id_of("water");
        arrows.retain_mut(|a| {
            a.prev_pos = a.pos;
            a.life += 1;
            if a.in_ground {
                let (x, y, z) = (a.pos.x.floor() as i32, a.pos.y.floor() as i32, a.pos.z.floor() as i32);
                if !def(self.world.get(x, y, z)).solid { a.in_ground = false; } else {
                    // Players pick their own arrows back up.
                    if a.from_player && (a.pos - self.player.pos).length() < 1.2 {
                        if self.mode == Mode::Survival { if let Some(id) = items::item_id("arrow") { self.add_to_inventory(id, 1); } }
                        return false;
                    }
                    return a.life < 1200;
                }
            }
            let next = a.pos + a.vel;
            let len = a.vel.length();
            let dir = if len > 1e-9 { a.vel / len } else { DVec3::Y };
            // Entities first (closest along the segment).
            let mut hit_mob: Option<(usize, f64)> = None;
            for (i, m) in self.ents.mobs.iter().enumerate() {
                if !m.alive() || m.id == a.owner { continue; }
                let (lo, hi) = m.aabb();
                if let Some(t) = ray_aabb(a.pos, dir, (lo - DVec3::splat(0.3), hi + DVec3::splat(0.3)), len) { if hit_mob.map_or(true, |(_, bt)| t < bt) { hit_mob = Some((i, t)); } }
            }
            let mut hit_player = false;
            if !a.from_player && self.mode == Mode::Survival && self.health > 0.0 {
                let lo = DVec3::new(self.player.pos.x - 0.6, self.player.pos.y - 0.3, self.player.pos.z - 0.6);
                let hi = DVec3::new(self.player.pos.x + 0.6, self.player.pos.y + 2.1, self.player.pos.z + 0.6);
                if ray_aabb(a.pos, dir, (lo, hi), len).is_some() { hit_player = true; }
            }
            // Blocks: step along the segment.
            let mut block_hit: Option<f64> = None;
            let steps = (len / 0.2).ceil().max(1.0) as i32;
            for s in 1..=steps {
                let p = a.pos + a.vel * (s as f64 / steps as f64);
                if def(self.world.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)).solid { block_hit = Some(len * (s - 1) as f64 / steps as f64); break; }
            }
            let mob_t = hit_mob.map(|h| h.1).unwrap_or(f64::MAX);
            if let Some((i, t)) = hit_mob {
                if block_hit.map_or(true, |bt| t <= bt) {
                    let speed = a.vel.length();
                    let mut dmg = (speed * 2.0).ceil() as i32;
                    if a.crit { dmg += self.rng.next_int_bound(dmg / 2 + 2); }
                    let m = &mut self.ents.mobs[i];
                    let src = m.pos - a.vel.normalize_or_zero();
                    if m.hurt(dmg as f32, Some(src), 0.0) { m.hurt_by = true; }
                    return false;
                }
            }
            if hit_player && block_hit.map_or(true, |bt| bt > 0.0) && mob_t == f64::MAX {
                let speed = a.vel.length();
                let dmg = (speed * 2.0).ceil() as f32;
                self.hurt_player(dmg, Some(a.pos - a.vel), 0.0, "Skeleton");
                return false;
            }
            if let Some(t) = block_hit {
                a.pos += dir * t;
                a.vel = DVec3::ZERO;
                a.in_ground = true;
                a.life = 0;
                return true;
            }
            a.pos = next;
            let in_water = self.world.get(a.pos.x.floor() as i32, a.pos.y.floor() as i32, a.pos.z.floor() as i32) == water;
            let drag = if in_water { 0.6 } else { 0.99 };
            a.vel = a.vel * drag - DVec3::new(0.0, 0.05, 0.0);
            a.life < 1200 && a.pos.y > -128.0
        });
        self.ents.arrows.extend(arrows);
    }

    // ------------------------------------------------------------ inventory

    pub fn add_to_inventory(&mut self, id: u16, count: u8) -> bool {
        let max = items::max_stack(id);
        let mut left = count;
        for s in self.slots.iter_mut().flatten() {
            if s.id == id && s.count < max && s.damage == 0 { let add = (max - s.count).min(left); s.count += add; left -= add; if left == 0 { return true; } }
        }
        if let Some(e) = self.slots.iter_mut().find(|s| s.is_none()) { *e = Some(Stack::new(id, left)); return true; }
        false
    }

    pub fn consume_arrow(&mut self) -> bool {
        if self.mode == Mode::Creative { return true; }
        let Some(arrow) = items::item_id("arrow") else { return false };
        for s in self.slots.iter_mut() {
            if let Some(st) = s { if st.id == arrow { st.count -= 1; if st.count == 0 { *s = None; } return true; } }
        }
        false
    }

    // ------------------------------------------------------------ using items (right mouse)

    pub fn use_item_tick(&mut self, playing: bool) {
        let held = self.held();
        let kind = held.and_then(|s| items::item_def(s.id)).map(|d| d.kind);
        let down = self.right_down && playing;
        match kind {
            Some(IK::Bow) => {
                if down { self.use_ticks += 1; }
                else if self.use_ticks > 0 {
                    let t = self.use_ticks as f32;
                    self.use_ticks = 0;
                    let f = t / 20.0;
                    let power = ((f * f + f * 2.0) / 3.0).min(1.0);
                    if power >= 0.1 && self.consume_arrow() {
                        let d = self.player.look_dir();
                        let dir = DVec3::new(d.x as f64, d.y as f64, d.z as f64).normalize();
                        let origin = self.player.eye() + dir * 0.3 - DVec3::new(0.0, 0.1, 0.0);
                        self.shoot_arrow(origin, dir * (power as f64 * 3.0), true, power >= 1.0, 0);
                        if self.mode == Mode::Survival { self.damage_held(1); }
                    }
                }
            }
            Some(IK::Food(n)) => {
                if down && self.hunger < 20.0 { self.use_ticks += 1; if self.use_ticks >= 32 {
                    self.use_ticks = 0;
                    self.hunger = (self.hunger + n as f32).min(20.0);
                    if self.mode == Mode::Survival { if let Some(s) = self.slots[self.selected].as_mut() { s.count -= 1; if s.count == 0 { self.slots[self.selected] = None; } } }
                } } else { self.use_ticks = 0; }
            }
            _ => { self.use_ticks = 0; }
        }
    }

    /// Right click (fresh press) with the held item on the targeted block.
    pub fn use_on_block(&mut self) -> bool {
        let held = self.held();
        let Some(h) = &self.target else { return false };
        let kind = held.and_then(|s| items::item_def(s.id)).map(|d| d.kind);
        if kind == Some(IK::FlintSteel) {
            let hb = h.block;
            if def(self.world.get(hb[0], hb[1], hb[2])).name == "tnt" {
                self.world.set(hb[0], hb[1], hb[2], 0);
                self.mark_dirty_pub(hb[0], hb[2]);
                self.prime_tnt(DVec3::new(hb[0] as f64 + 0.5, hb[1] as f64, hb[2] as f64 + 0.5), 80);
                self.damage_held(1);
                return true;
            }
            let (x, y, z) = (h.block[0] + h.normal[0], h.block[1] + h.normal[1], h.block[2] + h.normal[2]);
            if self.world.get(x, y, z) == 0 && self.world.set(x, y, z, id_of("fire")) {
                self.mark_dirty_pub(x, z);
                self.fires.insert((x, y, z), 0);
                self.damage_held(1);
            }
            return true;
        }
        false
    }

    // ------------------------------------------------------------ fire blocks

    fn tick_fires(&mut self) {
        if self.time_ticks as i64 % 10 != 0 { return; }
        let fire = id_of("fire");
        let keys: Vec<(i32, i32, i32)> = self.fires.keys().copied().collect();
        for k in keys {
            if self.world.get(k.0, k.1, k.2) != fire { self.fires.remove(&k); continue; }
            let age = self.fires.get_mut(&k).unwrap();
            *age += 1;
            let age = *age;
            let flammable = |g: &Game, x: i32, y: i32, z: i32| matches!(def(g.world.get(x, y, z)).name, "oak_planks" | "oak_log" | "oak_leaves");
            let around = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)];
            let fuel = around.iter().any(|d| flammable(self, k.0 + d.0, k.1 + d.1, k.2 + d.2));
            let burn_out = if fuel { age > 14 && self.rng.next_int_bound(4) == 0 } else { age > 2 + self.rng.next_int_bound(4) };
            if burn_out {
                self.world.set(k.0, k.1, k.2, 0);
                self.mark_dirty_pub(k.0, k.2);
                self.fires.remove(&k);
                continue;
            }
            for d in around {
                let (x, y, z) = (k.0 + d.0, k.1 + d.1, k.2 + d.2);
                if def(self.world.get(x, y, z)).name == "tnt" {
                    self.world.set(x, y, z, 0);
                    self.mark_dirty_pub(x, z);
                    self.prime_tnt(DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5), 80);
                }
                if flammable(self, x, y, z) && self.rng.next_int_bound(3) == 0 {
                    self.world.set(x, y, z, fire);
                    self.mark_dirty_pub(x, z);
                    self.fires.insert((x, y, z), 0);
                }
            }
        }
    }

    // ------------------------------------------------------------ spawning

    pub fn populate_chunk(&mut self, cx: i32, cz: i32) {
        if !self.populated.insert((cx, cz)) { return; }
        let mut r = Xoro::new(crate::SEED ^ (cx as i64).wrapping_mul(341873128712) ^ (cz as i64).wrapping_mul(132897987541));
        if r.next_float() > 0.10 { return; }
        let (lx, lz) = (r.next_int_bound(16), r.next_int_bound(16));
        let (x, z) = (cx * 16 + lx, cz * 16 + lz);
        let mut y = crate::world::MAX_Y - 1;
        while y > crate::world::MIN_Y && !def(self.world.get(x, y, z)).solid && self.world.get(x, y, z) != id_of("water") { y -= 1; }
        if self.world.get(x, y, z) != id_of("grass_block") { return; }
        let kinds = [(Kind::Pig, 12), (Kind::Sheep, 12), (Kind::Cow, 8), (Kind::Chicken, 10)];
        let total: i32 = kinds.iter().map(|k| k.1).sum();
        let mut pick = r.next_int_bound(total);
        let mut kind = Kind::Pig;
        for (k, w) in kinds { if pick < w { kind = k; break; } pick -= w; }
        let n = 2 + r.next_int_bound(3);
        for _ in 0..n {
            let (ox, oz) = (x + r.next_int_bound(5) - 2, z + r.next_int_bound(5) - 2);
            let mut yy = y + 4;
            while yy > y - 6 && !def(self.world.get(ox, yy - 1, oz)).solid { yy -= 1; }
            if def(self.world.get(ox, yy, oz)).solid || def(self.world.get(ox, yy + 1, oz)).solid { continue; }
            self.spawn_mob(kind, DVec3::new(ox as f64 + 0.5, yy as f64, oz as f64 + 0.5));
        }
    }

    pub fn spawn_mob(&mut self, kind: Kind, pos: DVec3) {
        self.ents.next_id += 1;
        let id = self.ents.next_id;
        let seed = (id as i64).wrapping_mul(0x9E3779B97F4A7C15u64 as i64) ^ self.time_ticks as i64;
        let mut m = Mob::new(id, kind, pos, seed);
        if !stats(kind).hostile { m.persistent = true; }
        self.ents.mobs.push(m);
    }

    fn try_spawn_hostile(&mut self) {
        let hostile = self.ents.mobs.iter().filter(|m| m.st().hostile).count();
        if hostile >= 20 { return; }
        let (_, day) = self.sky();
        let night = day < 0.45;
        for _ in 0..4 {
            let ang = self.rng.next_double() * std::f64::consts::TAU;
            let dist = 24.0 + self.rng.next_double() * 36.0;
            let (x, z) = ((self.player.pos.x + ang.cos() * dist).floor() as i32, (self.player.pos.z + ang.sin() * dist).floor() as i32);
            if !self.world.loaded(x, z) { continue; }
            let mut top = crate::world::MAX_Y - 1;
            while top > crate::world::MIN_Y && !def(self.world.get(x, top, z)).solid && self.world.get(x, top, z) != id_of("water") { top -= 1; }
            if self.world.get(x, top, z) == id_of("water") { continue; }
            // Night: surface; any time: caves (a pocket well below the surface).
            let y = if night { top + 1 } else {
                let y = (top - 12).max(-50) + self.rng.next_int_bound(8);
                y
            };
            if !(!def(self.world.get(x, y, z)).solid && !def(self.world.get(x, y + 1, z)).solid && def(self.world.get(x, y - 1, z)).solid) { continue; }
            if self.world.get(x, y, z) == id_of("water") { continue; }
            let kind = match self.rng.next_int_bound(10) { 0..=4 => Kind::Zombie, 5..=7 => Kind::Skeleton, _ => Kind::Creeper };
            self.spawn_mob(kind, DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5));
            return;
        }
    }

    // ------------------------------------------------------------ main per-tick entry

    pub fn gameplay_tick(&mut self, playing: bool) {
        self.attack_ticker = (self.attack_ticker + 1).min(1000);
        if self.player_invuln > 0 { self.player_invuln -= 1; }
        if self.player_hurt > 0 { self.player_hurt -= 1; }
        // Player burning.
        let (bx, by, bz) = (self.player.pos.x.floor() as i32, (self.player.pos.y + 0.1).floor() as i32, self.player.pos.z.floor() as i32);
        let here = def(self.world.get(bx, by, bz)).name;
        if here == "lava" { self.player_burning = 300; if self.time_ticks as i64 % 10 == 0 { self.hurt_player(4.0, None, 0.0, "lava"); } }
        if here == "fire" { self.player_burning = self.player_burning.max(160); if self.time_ticks as i64 % 10 == 0 { self.hurt_player(1.0, None, 0.0, "fire"); } }
        if self.player_burning > 0 {
            self.player_burning -= 1;
            if self.player.in_water || self.mode == Mode::Creative { self.player_burning = 0; }
            if self.player_burning % 20 == 0 && self.player_burning > 0 { self.player_invuln = 0; self.hurt_player(1.0, None, 0.0, "burning"); }
        }
        // Hunger.
        if self.mode == Mode::Survival {
            let drain = if self.player.sprinting { 0.0013 } else { 0.0004 };
            self.hunger = (self.hunger - drain).max(0.0);
            if self.hunger <= 0.0 && self.time_ticks as i64 % 80 == 0 && self.health > 1.0 { self.health -= 1.0; }
        }
        self.use_item_tick(playing);

        // Mobs.
        let mut events: Vec<Event> = Vec::new();
        let (_, day) = self.sky();
        let mut mobs = std::mem::take(&mut self.ents.mobs);
        {
            let mut ctx = Ctx { world: &self.world, player: self.player_info(), events: &mut events, game_time: self.time_ticks as i64, daylight: day };
            for m in mobs.iter_mut() {
                m.tick(&mut ctx);
                // Sunburn for undead.
                if matches!(m.kind, Kind::Zombie | Kind::Skeleton) && day > 0.7 && !m.in_water && m.burning == 0 {
                    if m.rng.next_float() * 30.0 < (day - 0.4) * 2.0 {
                        let (x, y, z) = (m.pos.x.floor() as i32, (m.pos.y + m.st().height).floor() as i32, m.pos.z.floor() as i32);
                        if (0..40).all(|d| !def(ctx.world.get(x, y + d, z)).solid) { m.burning = 160; }
                    }
                }
                // Creeper fuse.
                if m.kind == Kind::Creeper && m.alive() {
                    m.old_swell = m.swell;
                    m.swell = (m.swell + m.swell_dir).clamp(0, 30);
                    if m.swell >= 30 {
                        ctx.events.push(Event::Explode { pos: m.center(), power: 3.0 });
                        m.health = 0.0;
                        m.death_time = 19;
                    }
                }
            }
        }
        // Entity.push: overlapping living entities shove each other apart.
        for i in 0..mobs.len() {
            for j in (i + 1)..mobs.len() {
                if !mobs[i].alive() || !mobs[j].alive() { continue; }
                let (wi, wj) = (mobs[i].st().width, mobs[j].st().width);
                let (mut dx, mut dz) = (mobs[j].pos.x - mobs[i].pos.x, mobs[j].pos.z - mobs[i].pos.z);
                if (mobs[i].pos.y - mobs[j].pos.y).abs() > 1.0 { continue; }
                let mut d = dx.abs().max(dz.abs());
                if d < (wi + wj) / 2.0 && d >= 0.01 {
                    d = d.sqrt();
                    dx /= d; dz /= d;
                    let mult = (1.0 / d).min(1.0);
                    let (px, pz) = (dx * mult * 0.05, dz * mult * 0.05);
                    mobs[i].vel.x -= px; mobs[i].vel.z -= pz;
                    mobs[j].vel.x += px; mobs[j].vel.z += pz;
                } else if d < 0.01 && d < (wi + wj) / 2.0 {
                    mobs[j].vel.x += 0.02; mobs[i].vel.x -= 0.02;
                }
            }
        }
        // Deaths: drop loot once, then remove after the 20 tick fall animation.
        let mut loot: Vec<(Kind, DVec3, bool)> = Vec::new();
        for m in mobs.iter_mut() {
            if m.health <= 0.0 && m.death_time == 0 { loot.push((m.kind, m.pos, m.burning > 0)); m.death_time = 1; m.nav.stop(); }
        }
        mobs.retain(|m| m.death_time < 20);
        // Despawn far mobs (hostile, or unpersistent).
        let pp = self.player.pos;
        mobs.retain(|m| { let d = m.distance_sqr_to(pp); !(d > 128.0 * 128.0 || (!m.persistent && d > 32.0 * 32.0 && m.age > 600 && m.rng.clone().next_int_bound(800) == 0)) });
        self.ents.mobs = mobs;
        for (k, p, b) in loot { self.mob_loot(k, p, b); }
        for e in events {
            match e {
                Event::HurtPlayer { amount, from, knockback, by } => self.hurt_player(amount, Some(from), knockback, by),
                Event::Arrow { pos, vel, owner } => self.shoot_arrow(pos, vel, false, false, owner),
                Event::Explode { pos, power } => self.explode(pos, power),
            }
        }
        self.tick_arrows();
        self.tick_fires();
        self.tick_tnts();
        // Dropped items: physics, pickup, despawn.
        let mut items_v = std::mem::take(&mut self.ents.items);
        let pp = self.player.pos;
        items_v.retain_mut(|it| {
            it.tick(&self.world);
            if it.age > 6000 { return false; }
            if it.pickup_delay == 0 && (it.pos.x - pp.x).abs() < 1.0 && (it.pos.z - pp.z).abs() < 1.0 && it.pos.y > pp.y - 0.5 && it.pos.y < pp.y + 2.0 {
                let (id, n) = (it.item, it.count);
                if self.add_to_inventory(id, n) { return false; }
            }
            true
        });
        let mut extra = std::mem::take(&mut self.ents.items);
        items_v.append(&mut extra);
        self.ents.items = items_v;
        // Spawning.
        self.spawn_timer -= 1;
        if self.spawn_timer <= 0 { self.spawn_timer = 40; self.try_spawn_hostile(); }
        if self.time_ticks as i64 % 20 == 0 {
            let keys: Vec<(i32, i32)> = self.world.chunks.keys().copied().filter(|k| !self.populated.contains(k)).collect();
            let (pcx, pcz) = self.player_chunk();
            for k in keys { if (k.0 - pcx).abs() <= self.settings.render_distance && (k.1 - pcz).abs() <= self.settings.render_distance && self.world.region(k.0, k.1).is_some() { self.populate_chunk(k.0, k.1); } }
        }
        self.debug_ents = self.ents.mobs.len();
    }
}
