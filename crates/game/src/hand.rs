//! First-person hand: vanilla `ItemInHandRenderer` transforms, swing/equip/bow/eat animations, extruded item sprites.
use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::blocks::def;
use crate::entity_render::{models, EntityAtlas, ENT_ATLAS};
use crate::game::{Game, Mode};
use crate::items::{self, Kind as IK};
use crate::mesh::Vertex;
use crate::model::{self, Pose};

fn rx(d: f32) -> Mat4 { Mat4::from_rotation_x(d.to_radians()) }
fn ry(d: f32) -> Mat4 { Mat4::from_rotation_y(d.to_radians()) }
fn rz(d: f32) -> Mat4 { Mat4::from_rotation_z(d.to_radians()) }
fn tr(x: f32, y: f32, z: f32) -> Mat4 { Mat4::from_translation(Vec3::new(x, y, z)) }

/// `ItemTransform.apply` for the right hand.
fn display(rot: [f32; 3], t: [f32; 3], s: f32) -> Mat4 {
    tr(t[0] / 16.0, t[1] / 16.0, t[2] / 16.0)
        * Mat4::from_quat(Quat::from_euler(EulerRot::ZYX, rot[2].to_radians(), rot[1].to_radians(), rot[0].to_radians()))
        * Mat4::from_scale(Vec3::splat(s))
}

fn attack_transform(swing: f32, side: f32) -> Mat4 {
    let f = (swing * swing * std::f32::consts::PI).sin();
    let f1 = (swing.sqrt() * std::f32::consts::PI).sin();
    ry(side * (45.0 + f * -20.0)) * rz(side * f1 * -20.0) * rx(f1 * -80.0) * ry(side * -45.0)
}

fn swing_arm(swing: f32, side: f32) -> Mat4 {
    let f = -0.4 * (swing.sqrt() * std::f32::consts::PI).sin();
    let f1 = 0.2 * (swing.sqrt() * std::f32::consts::TAU).sin();
    let f2 = -0.2 * (swing * std::f32::consts::PI).sin();
    tr(side * f, f1, f2) * attack_transform(swing, side)
}

fn arm_transform(equip: f32, side: f32) -> Mat4 { tr(side * 0.56, -0.52 + equip * -0.6, -0.72) }

/// Extruded 16x16 sprite (`ItemModelGenerator`): front/back planes plus edge quads; model space 0..1.
fn sprite_mesh(out: &mut (Vec<Vertex>, Vec<u32>), m: Mat4, img: &image::RgbaImage, rect: [f32; 4], light: [f32; 2], tint: f32) {
    let at = ENT_ATLAS as f32;
    let (zf, zb) = (7.5 / 16.0, 8.5 / 16.0);
    let uv = |px: f32, py: f32| [(rect[0] + px) / at, (rect[1] + py) / at];
    let p = |x: f32, y: f32, z: f32| m.transform_point3(Vec3::new(x, y, z));
    let c = [tint; 3];
    // Front and back.
    for (z, flip) in [(zb, false), (zf, true)] {
        let (a, b, c2, d) = (p(0.0, 1.0, z), p(1.0, 1.0, z), p(1.0, 0.0, z), p(0.0, 0.0, z));
        let (u0, u1) = if flip { (16.0, 0.0) } else { (0.0, 16.0) };
        model::quad(out, [a, b, c2, d], [uv(u0, 0.0), uv(u1, 0.0), uv(u1, 16.0), uv(u0, 16.0)], [c[0], c[1], c[2], 1.0], light);
    }
    let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && x < 16 && y < 16 && img.get_pixel(x as u32, y as u32)[3] > 0;
    for y in 0..16i32 {
        for x in 0..16i32 {
            if !opaque(x, y) { continue; }
            let (fx, fy) = (x as f32 / 16.0, 1.0 - (y as f32 + 1.0) / 16.0);
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let t = uv(cx - 0.01, cy - 0.01);
            let t2 = uv(cx + 0.01, cy + 0.01);
            let uvs = [t, t2, t2, t];
            let s = 0.8;
            let col = [c[0] * s, c[1] * s, c[2] * s, 1.0];
            if !opaque(x - 1, y) { model::quad(out, [p(fx, fy + 1.0 / 16.0, zf), p(fx, fy + 1.0 / 16.0, zb), p(fx, fy, zb), p(fx, fy, zf)], uvs, col, light); }
            if !opaque(x + 1, y) { let x1 = fx + 1.0 / 16.0; model::quad(out, [p(x1, fy + 1.0 / 16.0, zb), p(x1, fy + 1.0 / 16.0, zf), p(x1, fy, zf), p(x1, fy, zb)], uvs, col, light); }
            if !opaque(x, y - 1) { let y1 = fy + 1.0 / 16.0; model::quad(out, [p(fx, y1, zf), p(fx + 1.0 / 16.0, y1, zf), p(fx + 1.0 / 16.0, y1, zb), p(fx, y1, zb)], uvs, col, light); }
            if !opaque(x, y + 1) { model::quad(out, [p(fx, fy, zb), p(fx + 1.0 / 16.0, fy, zb), p(fx + 1.0 / 16.0, fy, zf), p(fx, fy, zf)], uvs, col, light); }
        }
    }
}

fn cube_mesh(out: &mut (Vec<Vertex>, Vec<u32>), m: Mat4, id: u16, game: &Game, light: [f32; 2]) {
    let at = ENT_ATLAS as f32;
    let d = def(id);
    let p = |x: f32, y: f32, z: f32| m.transform_point3(Vec3::new(x, y, z));
    let faces: [(&str, [Vec3; 4], f32); 6] = [
        (d.top, [Vec3::new(0., 1., 0.), Vec3::new(1., 1., 0.), Vec3::new(1., 1., 1.), Vec3::new(0., 1., 1.)], 1.0),
        (d.bottom, [Vec3::new(0., 0., 1.), Vec3::new(1., 0., 1.), Vec3::new(1., 0., 0.), Vec3::new(0., 0., 0.)], 0.5),
        (d.side, [Vec3::new(0., 1., 0.), Vec3::new(1., 1., 0.), Vec3::new(1., 0., 0.), Vec3::new(0., 0., 0.)], 0.8),
        (d.side, [Vec3::new(1., 1., 1.), Vec3::new(0., 1., 1.), Vec3::new(0., 0., 1.), Vec3::new(1., 0., 1.)], 0.8),
        (d.side, [Vec3::new(1., 1., 0.), Vec3::new(1., 1., 1.), Vec3::new(1., 0., 1.), Vec3::new(1., 0., 0.)], 0.6),
        (d.side, [Vec3::new(0., 1., 1.), Vec3::new(0., 1., 0.), Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.)], 0.6),
    ];
    for (tex, c, shade) in faces {
        let Some(&tile) = game.names.get(tex) else { continue };
        let (tx, ty) = ((tile % 16) as f32 * 16.0, (tile / 16) as f32 * 16.0);
        let tint = if d.tint == crate::blocks::Tint::Grass && tex == d.top { [0.57, 0.74, 0.35] } else { [1.0; 3] };
        model::quad(out, [p(c[0].x, c[0].y, c[0].z), p(c[1].x, c[1].y, c[1].z), p(c[2].x, c[2].y, c[2].z), p(c[3].x, c[3].y, c[3].z)],
            [[tx / at, ty / at], [(tx + 16.0) / at, ty / at], [(tx + 16.0) / at, (ty + 16.0) / at], [tx / at, (ty + 16.0) / at]],
            [shade * tint[0], shade * tint[1], shade * tint[2], 1.0], light);
    }
}

pub fn build(game: &Game, atlas: &EntityAtlas, out: &mut (Vec<Vertex>, Vec<u32>)) {
    let a = game.partial;
    let swing = game.swing_lerp(a);
    let equip = 1.0 - (game.hand_prev_height + (game.hand_height - game.hand_prev_height) * a);
    let shown = game.hand_item;
    let p = &game.player;
    let light = {
        let (x, y, z) = (p.pos.x.floor() as i32, (p.pos.y + 1.0).floor() as i32, p.pos.z.floor() as i32);
        let open = (0..48).all(|d| !def(game.world.get(x, y + 1 + d, z)).solid);
        [if open { 15.0 } else { 4.0 }, game.debug_light.1 as f32]
    };
    let mut m = Mat4::IDENTITY;
    // GameRenderer.bobView
    if game.settings.bobbing {
        let wd = p.prev_walk_dist + (p.walk_dist - p.prev_walk_dist) * a;
        let bob = p.prev_bob + (p.bob - p.prev_bob) * a;
        let t = wd * std::f32::consts::PI;
        m = m * tr(t.sin() * bob * 0.5, -(t.cos().abs() * bob), 0.0) * rz(t.sin() * bob * 3.0) * rx((t - 0.2).cos().abs() * bob * 5.0);
    }
    // Hand sway from view-rotation lag.
    let x_bob = game.prev_x_bob + (game.x_bob - game.prev_x_bob) * a;
    let y_bob = game.prev_y_bob + (game.y_bob - game.prev_y_bob) * a;
    m = m * rx((p.pitch - x_bob) * 0.1) * ry((p.yaw - y_bob) * 0.1);

    let side = 1.0f32;
    let at = crate::entity_render::ENT_ATLAS as f32;
    if shown == 0 {
        // Empty hand: the player's arm (`ItemInHandRenderer.renderPlayerArm`).
        if game.mode == Mode::Survival || game.mode == Mode::Creative {
            let f1 = swing.sqrt();
            let f2 = -0.3 * (f1 * std::f32::consts::PI).sin();
            let f3 = 0.4 * (f1 * std::f32::consts::TAU).sin();
            let f4 = -0.4 * (swing * std::f32::consts::PI).sin();
            let mut am = m * tr(side * (f2 + 0.64000005), f3 - 0.6 + equip * -0.6, f4 - 0.71999997) * ry(side * 45.0);
            let f5 = (swing * swing * std::f32::consts::PI).sin();
            let f6 = (f1 * std::f32::consts::PI).sin();
            am = am * ry(side * f6 * 70.0) * rz(side * f5 * -20.0);
            am = am * tr(side * -1.0, 3.6, 3.5) * rz(side * 120.0) * rx(200.0) * ry(side * -135.0) * tr(side * 5.6, 0.0, 0.0);
            let md = &models().player;
            let ri = md.index("right_arm");
            let pose = Pose { light, ..Pose::new(md.parts.len()) };
            // Arm local matrix includes its own offset; the 1/16 scale is applied by build_with.
            model::build_with(md, atlas.rects["steve"], at, &pose, am, Some(ri), out);
        }
        return;
    }
    let kind = items::item_def(shown).map(|d| d.kind);
    let is_block = items::is_block(shown);
    let using = game.use_ticks > 0;
    let mut hm = m;
    match kind {
        Some(IK::Bow) if using => {
            let j = side;
            hm = hm * arm_transform(equip, side) * tr(j * -0.2785682, 0.18344387, 0.15731531) * rx(-13.935) * ry(j * 35.3) * rz(j * -9.785);
            let f7 = game.use_ticks as f32 + a;
            let mut f9 = f7 / 20.0;
            f9 = ((f9 * f9 + f9 * 2.0) / 3.0).min(1.0);
            if f9 > 0.1 { let f11 = ((f7 - 0.1) * 1.3).sin(); let f15 = f11 * (f9 - 0.1); hm = hm * tr(f15 * 0.0, f15 * 0.004, f15 * 0.0); }
            hm = hm * tr(0.0, 0.0, f9 * 0.04) * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + f9 * 0.2)) * ry(-j * 45.0);
        }
        Some(IK::Food(_)) if using => {
            let f = (32.0 - game.use_ticks as f32 - a) + 1.0;
            let f1 = f / 32.0;
            if f1 < 0.8 { hm = hm * tr(0.0, ((f / 4.0) * std::f32::consts::PI).cos().abs() * 0.1, 0.0); }
            let f3 = 1.0 - f1.max(0.0).powf(27.0);
            hm = hm * tr(f3 * 0.6 * side, f3 * -0.5, 0.0) * ry(side * f3 * 90.0) * rx(f3 * 10.0) * rz(side * f3 * 30.0);
            hm = hm * arm_transform(equip, side);
        }
        _ => {
            hm = hm * arm_transform(equip, side) * swing_arm(swing, side);
        }
    }
    if is_block {
        // block/block display: firstperson_righthand rotation [0,45,0], scale 0.4.
        let mm = hm * display([0.0, 45.0, 0.0], [0.0; 3], 0.4) * tr(-0.5, -0.5, -0.5);
        cube_mesh(out, mm, shown, game, light);
    } else if let Some(idx) = shown.checked_sub(items::ITEM_BASE) {
        // handheld / generated firstperson_righthand: rotation [0,-90,25], translation [1.13,3.2,1.13], scale 0.68.
        let mm = hm * display([0.0, -90.0, 25.0], [1.13, 3.2, 1.13], 0.68) * tr(-0.5, -0.5, -0.5);
        let mut ii = idx as usize;
        let mut rect = atlas.item_rects[ii];
        if kind == Some(IK::Bow) && using {
            let n = if game.use_ticks >= 18 { 2 } else if game.use_ticks >= 9 { 1 } else { 0 };
            ii = items::ITEMS.len() + n;
            rect = atlas.particle_rects[&format!("bow_pulling_{n}")];
        }
        let img = &atlas.item_images[ii];
        sprite_mesh(out, mm, img, rect, light, 1.0);
    }
}
