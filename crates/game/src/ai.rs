//! Goal implementations ported from `net.minecraft.world.entity.ai.goal` and per-mob goal setup.
use glam::DVec3;

use crate::blocks::def;
use crate::entity::*;

pub const F_JUMP: u8 = 4;

fn target_pos(c: &Ctx) -> DVec3 { c.player.pos }

fn gaussian(m: &mut Mob) -> f64 {
    // Box-Muller from two uniform doubles.
    let (u1, u2) = (m.rng.next_double().max(1e-12), m.rng.next_double());
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn random_stroll_pos(m: &mut Mob, c: &Ctx, horizontal: i32, vertical: i32) -> Option<DVec3> {
    // DefaultRandomPos.getPos: random offset, then settle on a standable cell near that height.
    for _ in 0..10 {
        let dx = m.rng.next_int_bound(2 * horizontal + 1) - horizontal;
        let dz = m.rng.next_int_bound(2 * horizontal + 1) - horizontal;
        let (x, z) = (m.pos.x.floor() as i32 + dx, m.pos.z.floor() as i32 + dz);
        if !c.world.loaded(x, z) { continue; }
        let y0 = m.pos.y.floor() as i32;
        for dy in 0..=vertical {
            for y in [y0 + dy, y0 - dy] {
                let ground = def(c.world.get(x, y - 1, z)).solid;
                if ground && !def(c.world.get(x, y, z)).solid && !def(c.world.get(x, y + 1, z)).solid
                    && c.world.get(x, y, z) != crate::blocks::id_of("water") && c.world.get(x, y - 1, z) != crate::blocks::id_of("lava") {
                    return Some(DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5));
                }
            }
        }
    }
    None
}

pub struct Float;
impl Goal for Float {
    fn flags(&self) -> u8 { F_JUMP }
    fn can_use(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { m.in_water }
    fn tick(&mut self, m: &mut Mob, _c: &mut Ctx) { if m.rng.next_float() < 0.8 { m.jump = true; } }
}

pub struct Stroll { speed: f64, interval: i32, wanted: DVec3 }
impl Stroll { pub fn new(speed: f64) -> Self { Self { speed, interval: 120, wanted: DVec3::ZERO } } }
impl Goal for Stroll {
    fn flags(&self) -> u8 { F_MOVE }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if m.no_action_time >= 100 { return false; }
        if m.rng.next_int_bound(self.interval) != 0 { return false; }
        match random_stroll_pos(m, c, 10, 7) { Some(p) => { self.wanted = p; true } None => false }
    }
    fn can_continue(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { !m.nav.is_done() }
    fn start(&mut self, m: &mut Mob, c: &mut Ctx) { m.move_to(c.world, self.wanted, self.speed); }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.nav.stop(); }
}

pub struct LookAtPlayer { range: f64, time: i32 }
impl LookAtPlayer { pub fn new(range: f64) -> Self { Self { range, time: 0 } } }
impl Goal for LookAtPlayer {
    fn flags(&self) -> u8 { F_LOOK }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if m.rng.next_float() >= 0.02 { return false; }
        c.player.alive && m.distance_sqr_to(c.player.pos) <= self.range * self.range
    }
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { c.player.alive && m.distance_sqr_to(c.player.pos) <= self.range * self.range && self.time > 0 }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { self.time = 40 + m.rng.next_int_bound(40); }
    fn tick(&mut self, m: &mut Mob, c: &mut Ctx) { m.look_at = Some((c.player.eye, 30.0)); self.time -= 1; }
}

pub struct RandomLookAround { rel: f64, time: i32 }
impl RandomLookAround { pub fn new() -> Self { Self { rel: 0.0, time: 0 } } }
impl Goal for RandomLookAround {
    fn flags(&self) -> u8 { F_MOVE | F_LOOK }
    fn can_use(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { m.rng.next_float() < 0.02 }
    fn can_continue(&mut self, _m: &mut Mob, _c: &mut Ctx) -> bool { self.time >= 0 }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { self.rel = std::f64::consts::TAU * m.rng.next_double(); self.time = 20 + m.rng.next_int_bound(20); }
    fn tick(&mut self, m: &mut Mob, _c: &mut Ctx) {
        self.time -= 1;
        let e = m.eye();
        m.look_at = Some((DVec3::new(m.pos.x + self.rel.cos(), e.y, m.pos.z + self.rel.sin()), 30.0));
    }
}

pub struct Panic { speed: f64, wanted: DVec3 }
impl Panic { pub fn new(speed: f64) -> Self { Self { speed, wanted: DVec3::ZERO } } }
impl Goal for Panic {
    fn flags(&self) -> u8 { F_MOVE }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if !(m.hurt_by || m.burning > 0) { return false; }
        match random_stroll_pos(m, c, 5, 4) { Some(p) => { self.wanted = p; true } None => false }
    }
    fn can_continue(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { !m.nav.is_done() }
    fn start(&mut self, m: &mut Mob, c: &mut Ctx) { m.move_to(c.world, self.wanted, self.speed); m.panic_ticks = 100; }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.hurt_by = false; m.nav.stop(); }
}

/// Distance test used by `isWithinMeleeAttackRange`.
fn in_melee_range(m: &Mob, c: &Ctx) -> bool {
    let reach = m.st().width / 2.0 + 0.3 + 0.83;
    let (dx, dz) = (c.player.pos.x - m.pos.x, c.player.pos.z - m.pos.z);
    let dy = c.player.pos.y - m.pos.y;
    (dx * dx + dz * dz).sqrt() <= reach && dy > -1.8 && dy < m.st().height
}

pub struct Melee { speed: f64, follow_unseen: bool, recalc: i32, next_attack: i32, last_check: i64, pathed: DVec3 }
impl Melee { pub fn new(speed: f64, follow_unseen: bool) -> Self { Self { speed, follow_unseen, recalc: 0, next_attack: 0, last_check: -100, pathed: DVec3::ZERO } } }
impl Goal for Melee {
    fn flags(&self) -> u8 { F_MOVE | F_LOOK }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if c.game_time - self.last_check < 20 { return false; }
        self.last_check = c.game_time;
        if !m.target || !c.player.alive { return false; }
        let from = [m.pos.x.floor() as i32, m.pos.y.floor() as i32, m.pos.z.floor() as i32];
        let to = [c.player.pos.x.floor() as i32, c.player.pos.y.floor() as i32, c.player.pos.z.floor() as i32];
        !find_path(c.world, from, to, 2, 200, true).is_empty() || in_melee_range(m, c)
    }
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if !m.target || !c.player.alive { return false; }
        if !self.follow_unseen { return !m.nav.is_done(); }
        c.player.targetable
    }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { m.aggressive = true; self.recalc = 0; self.next_attack = 0; }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { if m.target { /* target kept until target goal drops it */ } m.aggressive = false; m.nav.stop(); }
    fn tick(&mut self, m: &mut Mob, c: &mut Ctx) {
        m.look_at = Some((c.player.eye, 30.0));
        self.recalc = (self.recalc - 1).max(0);
        let see = line_of_sight(c.world, m.eye(), c.player.eye);
        if (self.follow_unseen || see) && self.recalc <= 0
            && (self.pathed == DVec3::ZERO || (c.player.pos - self.pathed).length_squared() >= 1.0 || m.rng.next_float() < 0.05)
        {
            self.pathed = c.player.pos;
            self.recalc = 4 + m.rng.next_int_bound(7);
            let d = m.distance_sqr_to(c.player.pos);
            if d > 1024.0 { self.recalc += 10 } else if d > 256.0 { self.recalc += 5 }
            if !m.move_to(c.world, c.player.pos, self.speed) { self.recalc += 15; }
        }
        self.next_attack = (self.next_attack - 1).max(0);
        if self.next_attack <= 0 && in_melee_range(m, c) && see {
            self.next_attack = 20;
            m.swing = 0;
            let dmg = m.st().attack;
            if dmg > 0.0 { c.events.push(Event::HurtPlayer { amount: dmg, from: m.pos, knockback: 0.0, by: m.st().name }); }
        }
    }
}

pub struct Swell;
impl Goal for Swell {
    fn flags(&self) -> u8 { F_MOVE }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { m.swell_dir > 0 || (m.target && m.distance_sqr_to(c.player.pos) < 9.0) }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { m.nav.stop(); }
    fn tick(&mut self, m: &mut Mob, c: &mut Ctx) {
        if !m.target || !c.player.alive { m.swell_dir = -1; }
        else if m.distance_sqr_to(c.player.pos) > 49.0 { m.swell_dir = -1; }
        else if !line_of_sight(c.world, m.eye(), c.player.eye) { m.swell_dir = -1; }
        else { m.swell_dir = 1; }
    }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.swell_dir = -1; }
}

pub struct BowAttack { speed: f64, min_interval: i32, radius_sqr: f64, attack_time: i32, see_time: i32, strafe_cw: bool, strafe_back: bool, strafe_time: i32 }
impl BowAttack {
    pub fn new(speed: f64, interval: i32, radius: f64) -> Self { Self { speed, min_interval: interval, radius_sqr: radius * radius, attack_time: -1, see_time: 0, strafe_cw: false, strafe_back: false, strafe_time: -1 } }
}
impl Goal for BowAttack {
    fn flags(&self) -> u8 { F_MOVE | F_LOOK }
    fn can_use(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { m.target }
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { self.can_use(m, c) || !m.nav.is_done() }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { m.aggressive = true; }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.aggressive = false; self.see_time = 0; self.attack_time = -1; m.bow_use = 0; }
    fn tick(&mut self, m: &mut Mob, c: &mut Ctx) {
        if !m.target || !c.player.alive { return; }
        let d0 = m.distance_sqr_to(c.player.pos);
        let see = line_of_sight(c.world, m.eye(), c.player.eye);
        if see != (self.see_time > 0) { self.see_time = 0; }
        if see { self.see_time += 1 } else { self.see_time -= 1 }
        if d0 <= self.radius_sqr && self.see_time >= 20 {
            m.nav.stop();
            self.strafe_time += 1;
        } else {
            m.move_to(c.world, c.player.pos, self.speed);
            self.strafe_time = -1;
        }
        if self.strafe_time >= 20 {
            if m.rng.next_float() < 0.3 { self.strafe_cw = !self.strafe_cw; }
            if m.rng.next_float() < 0.3 { self.strafe_back = !self.strafe_back; }
            self.strafe_time = 0;
        }
        if self.strafe_time > -1 {
            if d0 > self.radius_sqr * 0.75 { self.strafe_back = false; } else if d0 < self.radius_sqr * 0.25 { self.strafe_back = true; }
            m.zza = if self.strafe_back { -0.5 } else { 0.5 };
            m.xxa = if self.strafe_cw { 0.5 } else { -0.5 };
            // Face the target while strafing.
            let (dx, dz) = (c.player.pos.x - m.pos.x, c.player.pos.z - m.pos.z);
            m.yaw = rot_lerp(m.yaw, (dz.atan2(dx).to_degrees() as f32) - 90.0, 30.0);
            m.head_yaw = m.yaw;
        }
        m.look_at = Some((c.player.eye, 30.0));
        if m.bow_use > 0 {
            if !see && self.see_time < -60 { m.bow_use = 0; }
            else if see {
                m.bow_use += 1;
                if m.bow_use >= 20 {
                    m.bow_use = 0;
                    // performRangedAttack: aim slightly above the target with a distance-based lob.
                    let tgt = c.player.pos + DVec3::new(0.0, 1.8 * 0.6667, 0.0);
                    let origin = m.eye() - DVec3::new(0.0, 0.1, 0.0);
                    let d = tgt - origin;
                    let h = (d.x * d.x + d.z * d.z).sqrt();
                    let dir = DVec3::new(d.x, d.y + h * 0.2, d.z).normalize();
                    let inacc = 14.0 - 2.0 * 4.0; // normal difficulty
                    let k = 0.0172275 * inacc;
                    let v = (dir + DVec3::new(gaussian(m) * k, gaussian(m) * k, gaussian(m) * k)) * 1.6;
                    c.events.push(Event::Arrow { pos: origin, vel: v, owner: m.id });
                    self.attack_time = self.min_interval;
                }
            }
        } else {
            self.attack_time -= 1;
            if self.attack_time <= 0 && self.see_time >= -60 { m.bow_use = 1; }
        }
    }
}

pub struct TargetPlayer { interval: i32 }
impl TargetPlayer { pub fn new() -> Self { Self { interval: 10 } } }
impl Goal for TargetPlayer {
    fn flags(&self) -> u8 { F_TARGET }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if m.rng.next_int_bound(self.interval) != 0 { return false; }
        c.player.targetable && m.distance_sqr_to(c.player.pos) <= m.st().follow.powi(2) && line_of_sight(c.world, m.eye(), c.player.eye)
    }
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        c.player.targetable && m.distance_sqr_to(c.player.pos) <= (m.st().follow * 1.5).powi(2)
    }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { m.target = true; }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.target = false; }
}

pub struct HurtByTarget;
impl Goal for HurtByTarget {
    fn flags(&self) -> u8 { F_TARGET }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { m.hurt_by && c.player.targetable }
    fn can_continue(&mut self, m: &mut Mob, c: &mut Ctx) -> bool { c.player.targetable && m.distance_sqr_to(c.player.pos) <= (m.st().follow * 1.5).powi(2) }
    fn start(&mut self, m: &mut Mob, _c: &mut Ctx) { m.target = true; m.hurt_by = false; }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.target = false; }
}

/// FleeSunGoal: when burning in daylight, run to a nearby shaded spot.
pub struct FleeSun { speed: f64, wanted: DVec3 }
impl FleeSun { pub fn new(speed: f64) -> Self { Self { speed, wanted: DVec3::ZERO } } }
fn sky_open(c: &Ctx, x: i32, y: i32, z: i32) -> bool { (0..40).all(|d| !def(c.world.get(x, y + d, z)).solid) }
impl Goal for FleeSun {
    fn flags(&self) -> u8 { F_MOVE }
    fn can_use(&mut self, m: &mut Mob, c: &mut Ctx) -> bool {
        if m.target || c.daylight < 0.7 { return false; }
        let (x, y, z) = (m.pos.x.floor() as i32, (m.pos.y + 1.0).floor() as i32, m.pos.z.floor() as i32);
        if !sky_open(c, x, y, z) { return false; }
        for _ in 0..16 {
            let (dx, dz) = (m.rng.next_int_bound(13) - 6, m.rng.next_int_bound(13) - 6);
            let dy = m.rng.next_int_bound(5) - 2;
            let (tx, ty, tz) = (x + dx, y + dy, z + dz);
            if !c.world.loaded(tx, tz) { continue; }
            if !sky_open(c, tx, ty, tz) && def(c.world.get(tx, ty - 2, tz)).solid && !def(c.world.get(tx, ty - 1, tz)).solid {
                self.wanted = DVec3::new(tx as f64 + 0.5, (ty - 1) as f64, tz as f64 + 0.5);
                return true;
            }
        }
        false
    }
    fn can_continue(&mut self, m: &mut Mob, _c: &mut Ctx) -> bool { !m.nav.is_done() }
    fn start(&mut self, m: &mut Mob, c: &mut Ctx) { m.move_to(c.world, self.wanted, self.speed); }
    fn stop(&mut self, m: &mut Mob, _c: &mut Ctx) { m.nav.stop(); }
}

pub fn setup_goals(m: &mut Mob) {
    let _ = target_pos;
    match m.kind {
        Kind::Zombie => {
            m.goals.add(0, Box::new(Float));
            m.goals.add(2, Box::new(Melee::new(1.0, false)));
            m.goals.add(7, Box::new(Stroll::new(1.0)));
            m.goals.add(8, Box::new(LookAtPlayer::new(8.0)));
            m.goals.add(8, Box::new(RandomLookAround::new()));
            m.target_goals.add(1, Box::new(HurtByTarget));
            m.target_goals.add(2, Box::new(TargetPlayer::new()));
        }
        Kind::Skeleton => {
            m.goals.add(1, Box::new(Float));
            m.goals.add(3, Box::new(FleeSun::new(1.0)));
            m.goals.add(4, Box::new(BowAttack::new(1.0, 20, 15.0)));
            m.goals.add(5, Box::new(Stroll::new(1.0)));
            m.goals.add(6, Box::new(LookAtPlayer::new(8.0)));
            m.goals.add(6, Box::new(RandomLookAround::new()));
            m.target_goals.add(1, Box::new(HurtByTarget));
            m.target_goals.add(2, Box::new(TargetPlayer::new()));
        }
        Kind::Creeper => {
            m.goals.add(1, Box::new(Float));
            m.goals.add(2, Box::new(Swell));
            m.goals.add(4, Box::new(Melee::new(1.0, false)));
            m.goals.add(5, Box::new(Stroll::new(0.8)));
            m.goals.add(6, Box::new(LookAtPlayer::new(8.0)));
            m.goals.add(6, Box::new(RandomLookAround::new()));
            m.target_goals.add(1, Box::new(TargetPlayer::new()));
            m.target_goals.add(2, Box::new(HurtByTarget));
        }
        Kind::Pig | Kind::Sheep | Kind::Cow | Kind::Chicken => {
            let panic = match m.kind { Kind::Cow => 2.0, Kind::Chicken => 1.4, _ => 1.25 };
            m.goals.add(0, Box::new(Float));
            m.goals.add(1, Box::new(Panic::new(panic)));
            m.goals.add(6, Box::new(Stroll::new(1.0)));
            m.goals.add(7, Box::new(LookAtPlayer::new(6.0)));
            m.goals.add(8, Box::new(RandomLookAround::new()));
        }
    }
}
