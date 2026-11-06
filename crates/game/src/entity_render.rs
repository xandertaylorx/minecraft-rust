//! Builds the per-frame entity mesh (mobs, arrows, dropped items) from game state.
use std::collections::HashMap;
use std::sync::OnceLock;

use glam::{DVec3, Vec3};
use image::RgbaImage;

use crate::assets::Assets;
use crate::blocks::{def, BLOCKS};
use crate::entity::{wrap_deg, Kind, Mob};
use crate::game::Game;
use crate::items::{self, ITEMS};
use crate::mesh::Vertex;
use crate::model::{self, ModelDef, Pose};

pub const ENT_ATLAS: u32 = 1024;

pub struct EntityAtlas {
    pub image: RgbaImage,
    /// name -> pixel rect (x, y, w, h)
    pub rects: HashMap<&'static str, [f32; 4]>,
    pub item_rects: Vec<[f32; 4]>,
    pub particle_rects: HashMap<String, [f32; 4]>,
    pub item_images: Vec<RgbaImage>,
}

pub fn build_entity_atlas(a: &mut Assets, block_atlas: &RgbaImage) -> EntityAtlas {
    let mut image = RgbaImage::new(ENT_ATLAS, ENT_ATLAS);
    image::imageops::replace(&mut image, block_atlas, 0, 0);
    let mut rects = HashMap::new();
    let list: &[(&'static str, &str)] = &[
        ("zombie", "entity/zombie/zombie"), ("skeleton", "entity/skeleton/skeleton"), ("creeper", "entity/creeper/creeper"),
        ("pig", "entity/pig/temperate_pig"), ("cow", "entity/cow/temperate_cow"), ("sheep", "entity/sheep/sheep"),
        ("sheep_wool", "entity/sheep/sheep_wool"), ("steve", "entity/player/wide/steve"), ("chicken", "entity/chicken/temperate_chicken"), ("arrow", "entity/projectiles/arrow"),
    ];
    let (mut x, mut y, mut row) = (256u32, 0u32, 0u32);
    for (name, path) in list {
        let img = a.image(&format!("textures/{path}.png")).unwrap_or_else(|| panic!("missing {path}"));
        if x + img.width() > ENT_ATLAS { x = 256; y += row + 1; row = 0; }
        image::imageops::replace(&mut image, &img, x as i64, y as i64);
        rects.insert(*name, [x as f32, y as f32, img.width() as f32, img.height() as f32]);
        x += img.width() + 1;
        row = row.max(img.height());
    }
    // Fire (first frame of the animated strip).
    let fire = a.block_tile("fire_0");
    y += row + 1;
    image::imageops::replace(&mut image, &fire, 256, y as i64);
    rects.insert("fire", [256.0, y as f32, 16.0, 16.0]);
    // Item sprites in a strip.
    let mut item_rects = Vec::new();
    let mut item_images: Vec<RgbaImage> = Vec::new();
    let mut bow_rects: Vec<[f32; 4]> = Vec::new();
    let iy = 512u32;
    for (i, it) in ITEMS.iter().enumerate() {
        let img = a.image(&format!("textures/item/{}.png", it.name)).unwrap_or_else(|| RgbaImage::new(16, 16));
        let (px, py) = (260 + ((i as u32 % 40) * 17) as i64, (iy + (i as u32 / 40) * 17) as i64);
        image::imageops::replace(&mut image, &img, px, py);
        item_rects.push([px as f32, py as f32, 16.0, 16.0]);
        item_images.push(img);
    }
    for n in 0..3u32 {
        let img = a.image(&format!("textures/item/bow_pulling_{n}.png")).unwrap_or_else(|| RgbaImage::new(16, 16));
        let i = (ITEMS.len() as u32) + n;
        let (px, py) = (260 + ((i % 40) * 17) as i64, (iy + (i / 40) * 17) as i64);
        image::imageops::replace(&mut image, &img, px, py);
        bow_rects.push([px as f32, py as f32, 16.0, 16.0]);
        item_images.push(img);
    }
    let mut particle_rects = HashMap::new();
    let (mut px, mut py) = (260u32, 700u32);
    let mut names: Vec<String> = (0..16).map(|i| format!("explosion_{i}")).collect();
    names.extend((0..12).map(|i| format!("big_smoke_{i}")));
    names.push("flame".to_string());
    for n in names {
        let Some(img) = a.image(&format!("textures/particle/{n}.png")) else { continue };
        if px + img.width() > ENT_ATLAS { px = 260; py += 34; }
        image::imageops::replace(&mut image, &img, px as i64, py as i64);
        particle_rects.insert(n, [px as f32, py as f32, img.width() as f32, img.height() as f32]);
        px += img.width() + 1;
    }
    for (n, r) in bow_rects.iter().enumerate() { particle_rects.insert(format!("bow_pulling_{n}"), *r); }
    EntityAtlas { image, rects, item_rects, particle_rects, item_images }
}

pub struct Models { pub player: ModelDef, zombie: ModelDef, skeleton: ModelDef, creeper: ModelDef, pig: ModelDef, cow: ModelDef, sheep: ModelDef, sheep_wool: ModelDef, chicken: ModelDef }

pub fn models() -> &'static Models {
    static M: OnceLock<Models> = OnceLock::new();
    M.get_or_init(|| {
        let mut sheep_wool = model::sheep();
        sheep_wool.tex = "sheep_wool";
        for p in sheep_wool.parts.iter_mut() {
            for b in p.boxes.iter_mut() { b.grow = if p.name == "body" { 1.75 } else if p.name == "head" { 0.6 } else { 0.5 }; }
        }
        // Wool uses the same layout but its own box sizes for the body/head (vanilla SheepFurModel).
        sheep_wool.parts[0].boxes[0] = model::BoxDef { o: [-3., -4., -4.], s: [6., 6., 6.], uv: [0., 0.], mirror: false, grow: 0.6 };
        sheep_wool.parts[1].boxes[0] = model::BoxDef { o: [-4., -10., -7.], s: [8., 16., 6.], uv: [28., 8.], mirror: false, grow: 1.75 };
        for p in sheep_wool.parts.iter_mut().skip(2) { p.boxes[0].s = [4., 6., 4.]; }
        Models { player: model::humanoid("steve", 64.0, false), zombie: model::humanoid("zombie", 64.0, false), skeleton: model::humanoid("skeleton", 32.0, true), creeper: model::creeper(), pig: model::pig(), cow: model::cow(), sheep: model::sheep(), sheep_wool, chicken: model::chicken() }
    })
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 { a + wrap_deg(b - a) * t }

fn light_at(game: &Game, p: DVec3) -> [f32; 2] {
    let (x, y, z) = (p.x.floor() as i32, (p.y + 0.5).floor() as i32, p.z.floor() as i32);
    let open = (0..48).all(|d| !def(game.world.get(x, y + 1 + d, z)).solid);
    [if open { 15.0 } else { 3.0 }, 0.0]
}

fn pose_for(m: &Mob, md: &ModelDef, a: f32) -> Pose {
    let mut pose = Pose::new(md.parts.len());
    let ws = lerp(m.walk_speed * 0.0 + m.walk_speed, m.walk_speed, a);
    let wp = m.walk_pos + m.walk_speed * (a - 1.0).max(-1.0) * 0.0 + (a - 1.0) * m.walk_speed;
    let yaw = lerp_angle(m.prev_yaw, m.yaw, a);
    let head_yaw = lerp_angle(m.prev_head_yaw, m.head_yaw, a);
    let head_rel = wrap_deg(head_yaw - yaw).clamp(-75.0, 75.0).to_radians();
    let pitch = lerp(m.prev_pitch, m.pitch, a).to_radians();
    let cosw = |off: f32| (wp * 0.6662 + off).cos();
    let idx = |n: &str| md.parts.iter().position(|p| p.name == n);
    if let Some(i) = idx("head") { pose.rot[i] = [pitch, head_rel, 0.0]; }
    let age = m.age as f32 + a;
    match m.kind {
        Kind::Zombie | Kind::Skeleton => {
            let (ra, la) = (idx("right_arm").unwrap(), idx("left_arm").unwrap());
            let (rl, ll) = (idx("right_leg").unwrap(), idx("left_leg").unwrap());
            pose.rot[ra] = [cosw(std::f32::consts::PI) * 2.0 * ws * 0.5, 0.0, 0.0];
            pose.rot[la] = [cosw(0.0) * 2.0 * ws * 0.5, 0.0, 0.0];
            pose.rot[rl] = [cosw(0.0) * 1.4 * ws, 0.0, 0.0];
            pose.rot[ll] = [cosw(std::f32::consts::PI) * 1.4 * ws, 0.0, 0.0];
            if m.kind == Kind::Zombie {
                // AnimationUtils.animateZombieArms
                let att = if m.swing >= 0 { (m.swing as f32 + a) / 6.0 } else { 0.0 };
                let f1 = (att * std::f32::consts::PI).sin();
                let f2 = ((1.0 - (1.0 - att) * (1.0 - att)) * std::f32::consts::PI).sin();
                let f3 = -std::f32::consts::PI / if m.aggressive { 1.5 } else { 2.25 };
                pose.rot[ra] = [f3 + f1 * 1.2 - f2 * 0.4 + (age * 0.067).sin() * 0.05, -(0.1 - f1 * 0.6), (age * 0.09).cos() * 0.05 + 0.05];
                pose.rot[la] = [f3 + f1 * 1.2 - f2 * 0.4 - (age * 0.067).sin() * 0.05, 0.1 - f1 * 0.6, -((age * 0.09).cos() * 0.05 + 0.05)];
            } else if m.aggressive {
                pose.rot[ra] = [-std::f32::consts::FRAC_PI_2 + pitch, -0.1 + head_rel, 0.0];
                pose.rot[la] = [-std::f32::consts::FRAC_PI_2 + pitch, 0.1 + head_rel + 0.4, 0.0];
            }
            if m.swing >= 0 && m.kind == Kind::Skeleton { /* bow skeleton has no melee swing */ }
        }
        Kind::Creeper | Kind::Pig | Kind::Cow | Kind::Sheep => {
            let set = |pose: &mut Pose, n: &str, v: f32| if let Some(i) = md.parts.iter().position(|p| p.name == n) { pose.rot[i][0] += v; };
            set(&mut pose, "right_hind_leg", cosw(0.0) * 1.4 * ws);
            set(&mut pose, "left_hind_leg", cosw(std::f32::consts::PI) * 1.4 * ws);
            set(&mut pose, "right_front_leg", cosw(std::f32::consts::PI) * 1.4 * ws);
            set(&mut pose, "left_front_leg", cosw(0.0) * 1.4 * ws);
        }
        Kind::Chicken => {
            let set = |pose: &mut Pose, n: &str, v: f32| if let Some(i) = md.parts.iter().position(|p| p.name == n) { pose.rot[i][0] += v; };
            set(&mut pose, "right_leg", cosw(0.0) * 1.4 * ws);
            set(&mut pose, "left_leg", cosw(std::f32::consts::PI) * 1.4 * ws);
            for n in ["beak", "red_thing"] { if let Some(i) = idx(n) { pose.rot[i] = [pitch, head_rel, 0.0]; } }
            let flap = if m.on_ground { 0.0 } else { age.sin().abs() * 1.0 + 0.6 };
            if let Some(i) = idx("right_wing") { pose.rot[i][2] = flap; }
            if let Some(i) = idx("left_wing") { pose.rot[i][2] = -flap; }
        }
    }
    pose
}

pub fn build(game: &Game, atlas: &EntityAtlas, cam: DVec3, a: f32) -> (Vec<Vertex>, Vec<u32>) {
    let mut out: (Vec<Vertex>, Vec<u32>) = (Vec::new(), Vec::new());
    let ms = models();
    let at = ENT_ATLAS as f32;
    for m in &game.ents.mobs {
        let md = match m.kind { Kind::Zombie => &ms.zombie, Kind::Skeleton => &ms.skeleton, Kind::Creeper => &ms.creeper, Kind::Pig => &ms.pig, Kind::Cow => &ms.cow, Kind::Sheep => &ms.sheep, Kind::Chicken => &ms.chicken };
        let p = m.prev_pos + (m.pos - m.prev_pos) * a as f64;
        let origin = (p - cam).as_vec3();
        let yaw = lerp_angle(m.prev_yaw, m.yaw, a);
        let mut pose = pose_for(m, md, a);
        pose.light = light_at(game, p);
        if m.hurt_time > 0 || m.death_time > 0 { pose.tint = [1.0, 0.45, 0.45, 1.0]; }
        if m.death_time > 0 {
            let f = ((m.death_time as f32 + a - 1.0) / 20.0 * 1.6).sqrt().min(1.0);
            pose.roll = f * std::f32::consts::FRAC_PI_2;
        }
        if m.kind == Kind::Creeper {
            let sw = lerp(m.old_swell as f32, m.swell as f32, a) / 28.0;
            let f2 = 1.0 + (sw.clamp(0.0, 1.0) * 100.0).sin() * sw.clamp(0.0, 1.0) * 0.01;
            let mut f = sw.clamp(0.0, 1.0);
            f = f * f; f = f * f;
            pose.scale = [(1.0 + f * 0.4) * f2, (1.0 + f * 0.1) / f2, (1.0 + f * 0.4) * f2];
            if ((sw.clamp(0.0, 1.0) * 10.0) as i32) % 2 == 0 && sw > 0.05 { pose.tint = [2.2, 2.2, 2.2, 1.0]; }
        }
        let rect = atlas.rects[md.tex];
        model::build(md, rect, at, &pose, origin, yaw, &mut out);
        if m.kind == Kind::Sheep {
            let wr = atlas.rects["sheep_wool"];
            model::build(&ms.sheep_wool, wr, at, &pose, origin, yaw, &mut out);
        }
        if m.burning > 0 && m.death_time == 0 {
            let r = atlas.rects["fire"];
            let (u0, v0, u1, v1) = (r[0] / at, r[1] / at, (r[0] + 16.0) / at, (r[1] + 16.0) / at);
            let s = m.st();
            let (w, h) = (s.width as f32 * 0.7, s.height as f32 * 0.9);
            for k in 0..2 {
                let ang = k as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
                let (dx, dz) = (ang.cos() * w, ang.sin() * w);
                model::quad(&mut out, [origin + Vec3::new(-dx, h, -dz), origin + Vec3::new(dx, h, dz), origin + Vec3::new(dx, 0.0, dz), origin + Vec3::new(-dx, 0.0, -dz)], [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], [1.0; 4], [15.0, 15.0]);
            }
        }
    }
    // Primed TNT: a pulsing block that flashes white before exploding.
    for t in &game.ents.tnts {
        let p = t.prev_pos + (t.pos - t.prev_pos) * a as f64;
        let o = (p - cam).as_vec3() + Vec3::new(0.0, 0.49, 0.0);
        let f = t.fuse as f32 - a + 1.0;
        let mut scale = 1.0;
        if f < 10.0 { let f1 = (1.0 - f / 10.0).clamp(0.0, 1.0); scale = 1.0 + f1 * f1 * f1 * f1 * 0.3; }
        let flash = (t.fuse / 5) % 2 == 0;
        let tint = if flash { 3.0 } else { 1.0 };
        let light = light_at(game, p);
        let h = 0.5 * scale;
        let d = def(crate::blocks::id_of("tnt"));
        let faces: [(&str, [Vec3; 4], f32); 6] = [
            (d.top, [Vec3::new(-h, h, -h), Vec3::new(h, h, -h), Vec3::new(h, h, h), Vec3::new(-h, h, h)], 1.0),
            (d.bottom, [Vec3::new(-h, -h, h), Vec3::new(h, -h, h), Vec3::new(h, -h, -h), Vec3::new(-h, -h, -h)], 0.5),
            (d.side, [Vec3::new(-h, h, -h), Vec3::new(h, h, -h), Vec3::new(h, -h, -h), Vec3::new(-h, -h, -h)], 0.8),
            (d.side, [Vec3::new(h, h, h), Vec3::new(-h, h, h), Vec3::new(-h, -h, h), Vec3::new(h, -h, h)], 0.8),
            (d.side, [Vec3::new(h, h, -h), Vec3::new(h, h, h), Vec3::new(h, -h, h), Vec3::new(h, -h, -h)], 0.6),
            (d.side, [Vec3::new(-h, h, h), Vec3::new(-h, h, -h), Vec3::new(-h, -h, -h), Vec3::new(-h, -h, h)], 0.6),
        ];
        for (tex, c, shade) in faces {
            let Some(&tile) = game.names.get(tex) else { continue };
            let (tx, ty) = ((tile % 16) as f32 * 16.0, (tile / 16) as f32 * 16.0);
            let s = shade * tint;
            model::quad(&mut out, [o + c[0], o + c[1], o + c[2], o + c[3]], [[tx / at, ty / at], [(tx + 16.0) / at, ty / at], [(tx + 16.0) / at, (ty + 16.0) / at], [tx / at, (ty + 16.0) / at]], [s, s, s, 1.0], light);
        }
    }
    // Particles.
    game.particles.build(atlas, cam, game.camera(a).forward(), a, &mut out);
    // Arrows.
    let ar = atlas.rects["arrow"];
    for ar_e in &game.ents.arrows {
        let p = ar_e.prev_pos + (ar_e.pos - ar_e.prev_pos) * a as f64;
        let o = (p - cam).as_vec3();
        let v = if ar_e.in_ground { (ar_e.pos - ar_e.prev_pos).normalize_or_zero() } else { ar_e.vel.normalize_or_zero() };
        let dir = if v.length_squared() > 0.0 { v.as_vec3() } else { Vec3::Y };
        let up = if dir.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
        let side = dir.cross(up).normalize() * 0.06;
        let side2 = dir.cross(side).normalize() * 0.06;
        let (len0, len1) = (-0.45, 0.45);
        let light = light_at(game, p);
        // Shaft: uv rect (0,0)-(16,5) of the arrow texture.
        let (u0, v0, u1, v1) = (ar[0] / at, ar[1] / at, (ar[0] + 16.0) / at, (ar[1] + 5.0) / at);
        for s in [side, side2] {
            model::quad(&mut out, [o + dir * len0 - s, o + dir * len1 - s, o + dir * len1 + s, o + dir * len0 + s], [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], [1.0; 4], light);
        }
    }
    // Dropped items.
    for it in &game.ents.items {
        let p = it.prev_pos + (it.pos - it.prev_pos) * a as f64;
        let bob = ((it.age as f32 + a) / 10.0).sin() * 0.1 + 0.1;
        let o = (p - cam).as_vec3() + Vec3::new(0.0, bob + 0.125, 0.0);
        let spin = (it.age as f32 + a) / 20.0;
        let (s, c) = spin.sin_cos();
        let light = light_at(game, p);
        let rot = |v: Vec3| Vec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c);
        if items::is_block(it.item) && it.item < BLOCKS.len() as u16 {
            let d = def(it.item);
            let h = 0.125f32;
            let faces: [(&str, [Vec3; 4]); 6] = [
                (d.top, [Vec3::new(-h, h, -h), Vec3::new(h, h, -h), Vec3::new(h, h, h), Vec3::new(-h, h, h)]),
                (d.bottom, [Vec3::new(-h, -h, h), Vec3::new(h, -h, h), Vec3::new(h, -h, -h), Vec3::new(-h, -h, -h)]),
                (d.side, [Vec3::new(-h, h, -h), Vec3::new(h, h, -h), Vec3::new(h, -h, -h), Vec3::new(-h, -h, -h)]),
                (d.side, [Vec3::new(h, h, h), Vec3::new(-h, h, h), Vec3::new(-h, -h, h), Vec3::new(h, -h, h)]),
                (d.side, [Vec3::new(h, h, -h), Vec3::new(h, h, h), Vec3::new(h, -h, h), Vec3::new(h, -h, -h)]),
                (d.side, [Vec3::new(-h, h, h), Vec3::new(-h, h, -h), Vec3::new(-h, -h, -h), Vec3::new(-h, -h, h)]),
            ];
            for (fi, (tex, corners)) in faces.iter().enumerate() {
                let Some(&tile) = game.names.get(*tex) else { continue };
                let (tx, ty) = ((tile % 16) as f32 * 16.0, (tile / 16) as f32 * 16.0);
                let shade = [1.0, 0.5, 0.8, 0.8, 0.6, 0.6][fi];
                model::quad(&mut out, [o + rot(corners[0]), o + rot(corners[1]), o + rot(corners[2]), o + rot(corners[3])],
                    [[tx / at, ty / at], [(tx + 16.0) / at, ty / at], [(tx + 16.0) / at, (ty + 16.0) / at], [tx / at, (ty + 16.0) / at]], [shade, shade, shade, 1.0], light);
            }
        } else if let Some(idx) = it.item.checked_sub(items::ITEM_BASE) {
            let r = atlas.item_rects[idx as usize];
            let (u0, v0, u1, v1) = (r[0] / at, r[1] / at, (r[0] + 16.0) / at, (r[1] + 16.0) / at);
            let h = 0.25f32;
            let c = [Vec3::new(-h, h * 2.0, 0.0), Vec3::new(h, h * 2.0, 0.0), Vec3::new(h, 0.0, 0.0), Vec3::new(-h, 0.0, 0.0)];
            let o = o - Vec3::new(0.0, 0.1, 0.0);
            model::quad(&mut out, [o + rot(c[0]), o + rot(c[1]), o + rot(c[2]), o + rot(c[3])], [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], [1.0; 4], light);
        }
    }
    out
}
