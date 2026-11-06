//! `ShipFlightPhysics`, `PulseNavigation` and `SurfaceStreamPlanner`.
use std::collections::HashSet;

use crate::catalog::Planet;
use crate::mth::{approach, approach_degrees, clamp, clamp_f, lerp, wrap_degrees};
use crate::vec3::Vec3;

pub const ATMOSPHERE_TOP: f64 = 512.0;
pub const SPACE_CEILING: f64 = 4096.0;
pub const SURFACE_SPEED: f64 = 1.8;
pub const CRUISE_SPEED: f64 = 12.0;
pub const PULSE_SPEED: f64 = 80.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Input { pub forward: bool, pub backward: bool, pub left: bool, pub right: bool, pub jump: bool, pub shift: bool, pub sprint: bool }

#[derive(Clone, Copy, Debug)]
pub struct Flight { pub velocity: Vec3, pub yaw: f32, pub pitch: f32, pub pulse: bool }

pub fn step(velocity: Vec3, yaw: f32, pitch: f32, pilot_yaw: f32, pilot_pitch: f32, input: Input,
            altitude: f64, piloted: bool, clearance: f64) -> Flight {
    let yaw = wrap_degrees(approach_degrees(yaw, pilot_yaw, 6.0));
    let pitch = approach(pitch, clamp_f(pilot_pitch, -85.0, 85.0), 4.0);
    let pulse = piloted && input.sprint && input.forward && altitude > 360.0 && pitch <= 10.0 && !input.shift;
    let mut transition = clamp(clearance / 360.0, 0.0, 1.0);
    transition = transition * transition * (3.0 - 2.0 * transition);
    let speed = if pulse { PULSE_SPEED } else { lerp(transition, SURFACE_SPEED, CRUISE_SPEED) };
    let desired = if piloted {
        let mut d = Vec3::ZERO;
        let forward = Vec3::direction_from_rotation(pitch, yaw);
        let sideways = Vec3::direction_from_rotation(0.0, yaw - 90.0);
        if input.forward { d = d.add(forward); }
        if input.backward { d = d.subtract(forward.scale(0.2)); }
        if input.left { d = d.add(sideways.scale(0.5)); }
        if input.right { d = d.subtract(sideways.scale(0.5)); }
        if input.jump { d = d.add_xyz(0.0, 1.0, 0.0); }
        if input.shift { d = d.add_xyz(0.0, -0.85, 0.0); }
        if d.length_sqr() > 1.0 { d = d.normalize(); }
        d.scale(speed)
    } else {
        Vec3::new(0.0, -0.2, 0.0)
    };
    let mut next = velocity.lerp(desired, if pulse { 0.12 } else if input.backward { 0.32 } else { 0.14 });
    if piloted && !input.forward && !input.backward && !input.left && !input.right {
        let k = if input.jump || input.shift { 0.22 } else { 0.75 };
        next = Vec3::new(velocity.x * k, next.y, velocity.z * k);
    }
    if next.length_sqr() > PULSE_SPEED * PULSE_SPEED { next = next.normalize().scale(PULSE_SPEED); }
    if altitude >= SPACE_CEILING && next.y > 0.0 { next = Vec3::new(next.x, 0.0, next.z); }
    Flight { velocity: next, yaw, pitch, pulse }
}

pub fn acquire<'a>(position: Vec3, look: Vec3, bodies: &'a [Planet]) -> Option<&'a Planet> {
    let (mut best, mut score) = (None, 0.0);
    for body in bodies {
        if body.parent_body < -1 { continue; }
        let c = body.native_center();
        let delta = Vec3::new(c.x, 80.0, c.z).subtract(position);
        let distance = delta.length();
        if distance < body.radius + 320.0 || distance > 40000.0 { continue; }
        let alignment = look.dot(delta.scale(1.0 / distance));
        if alignment < 0.975 { continue; }
        let candidate = alignment - 0.0000001 * distance;
        if candidate > score { score = candidate; best = Some(body); }
    }
    best
}

pub fn pulse_speed(distance: f64, radius: f64) -> f64 {
    PULSE_SPEED.min(CRUISE_SPEED.max((distance - radius - 140.0) * 0.11))
}

pub fn pulse_direction(position: Vec3, body: &Planet) -> Vec3 {
    let c = body.native_center();
    Vec3::new(c.x, 480f64.max(position.y), c.z).subtract(position).normalize()
}

pub const MAX_TICKETS: usize = 16;
pub const MAX_LOOKAHEAD_BLOCKS: f64 = 128.0;

pub fn predict_chunks(position: Vec3, velocity: Vec3, view_distance: i32) -> Vec<(i32, i32)> {
    let horizontal = velocity.x.hypot(velocity.z);
    if !horizontal.is_finite() || horizontal < 0.5 || !position.x.is_finite() || !position.z.is_finite() { return vec![]; }
    let ahead = MAX_LOOKAHEAD_BLOCKS.min(horizontal * 20.0);
    let (dx, dz) = (velocity.x / horizontal, velocity.z / horizontal);
    let view_edge = view_distance.max(0) as f64 * 16.0 / dx.abs().max(dz.abs());
    let (cur_x, cur_z) = ((position.x / 16.0).floor() as i32, (position.z / 16.0).floor() as i32);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for step in 1..=4 {
        let distance = view_edge + ahead * step as f64 / 4.0;
        let cx = ((position.x + dx * distance) / 16.0).floor() as i32;
        let cz = ((position.z + dz * distance) / 16.0).floor() as i32;
        for z in -1..=1 { for x in -1..=1 {
            let (ccx, ccz) = (cx + x, cz + z);
            if (ccx as i64).abs() > 1_874_999 || (ccz as i64).abs() > 1_874_999 { continue; }
            if (ccx - cur_x).abs().max((ccz - cur_z).abs()) <= view_distance { continue; }
            if seen.insert((ccx, ccz)) {
                out.push((ccx, ccz));
                if out.len() == MAX_TICKETS { return out; }
            }
        } }
    }
    out
}
