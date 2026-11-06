//! Box models (vanilla `ModelPart`/`CubeListBuilder`) and the entity mesh builder.
use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::mesh::Vertex;

pub struct BoxDef {
    pub o: [f32; 3],
    pub s: [f32; 3],
    pub uv: [f32; 2],
    pub mirror: bool,
    pub grow: f32,
}

pub struct PartDef {
    pub name: &'static str,
    pub boxes: Vec<BoxDef>,
    pub offset: [f32; 3],
    pub rot: [f32; 3],
    pub parent: Option<usize>,
}

pub struct ModelDef {
    pub tex: &'static str,
    pub tex_w: f32,
    pub tex_h: f32,
    pub parts: Vec<PartDef>,
}

pub fn bx(o: [f32; 3], s: [f32; 3], uv: [f32; 2]) -> BoxDef { BoxDef { o, s, uv, mirror: false, grow: 0.0 } }
pub fn bxm(o: [f32; 3], s: [f32; 3], uv: [f32; 2]) -> BoxDef { BoxDef { o, s, uv, mirror: true, grow: 0.0 } }
fn part(name: &'static str, boxes: Vec<BoxDef>, offset: [f32; 3], rot: [f32; 3], parent: Option<usize>) -> PartDef { PartDef { name, boxes, offset, rot, parent } }

impl ModelDef {
    pub fn index(&self, name: &str) -> usize { self.parts.iter().position(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}")) }
}

// ------------------------------------------------------------------ vanilla definitions

pub fn humanoid(tex: &'static str, tex_h: f32, thin_arms: bool) -> ModelDef {
    let (aw, ao_r, ao_l) = if thin_arms { (2.0, -1.0, -1.0) } else { (4.0, -3.0, -1.0) };
    let arm_off = if thin_arms { 5.0 } else { 5.0 };
    ModelDef {
        tex, tex_w: 64.0, tex_h,
        parts: vec![
            part("head", vec![bx([-4., -8., -4.], [8., 8., 8.], [0., 0.])], [0., 0., 0.], [0.; 3], None),
            part("hat", vec![BoxDef { grow: 0.5, ..bx([-4., -8., -4.], [8., 8., 8.], [32., 0.]) }], [0.; 3], [0.; 3], Some(0)),
            part("body", vec![bx([-4., 0., -2.], [8., 12., 4.], [16., 16.])], [0., 0., 0.], [0.; 3], None),
            part("right_arm", vec![bx([ao_r, -2., -2.], [aw, 12., 4.], [40., 16.])], [-arm_off, 2., 0.], [0.; 3], None),
            part("left_arm", vec![bxm([ao_l, -2., -2.], [aw, 12., 4.], [40., 16.])], [arm_off, 2., 0.], [0.; 3], None),
            part("right_leg", vec![bx([-2., 0., -2.], [if thin_arms { 2.0 } else { 4.0 }, 12., 4.], [0., 16.])], [-1.9, 12., 0.], [0.; 3], None),
            part("left_leg", vec![bxm([-2., 0., -2.], [if thin_arms { 2.0 } else { 4.0 }, 12., 4.], [0., 16.])], [1.9, 12., 0.], [0.; 3], None),
        ],
    }
}

pub fn creeper() -> ModelDef {
    let leg = || vec![bx([-2., 0., -2.], [4., 6., 4.], [0., 16.])];
    ModelDef {
        tex: "creeper", tex_w: 64.0, tex_h: 32.0,
        parts: vec![
            part("head", vec![bx([-4., -8., -4.], [8., 8., 8.], [0., 0.])], [0., 6., 0.], [0.; 3], None),
            part("body", vec![bx([-4., 0., -2.], [8., 12., 4.], [16., 16.])], [0., 6., 0.], [0.; 3], None),
            part("right_hind_leg", leg(), [-2., 18., 4.], [0.; 3], None),
            part("left_hind_leg", leg(), [2., 18., 4.], [0.; 3], None),
            part("right_front_leg", leg(), [-2., 18., -4.], [0.; 3], None),
            part("left_front_leg", leg(), [2., 18., -4.], [0.; 3], None),
        ],
    }
}

fn quad_legs(h: f32, ly: f32, front_z: f32, hind_z: f32, x: f32) -> Vec<PartDef> {
    let leg = |m: bool| vec![if m { bxm([-2., 0., -2.], [4., h, 4.], [0., 16.]) } else { bx([-2., 0., -2.], [4., h, 4.], [0., 16.]) }];
    vec![
        part("right_hind_leg", leg(false), [-x, ly, hind_z], [0.; 3], None),
        part("left_hind_leg", leg(true), [x, ly, hind_z], [0.; 3], None),
        part("right_front_leg", leg(false), [-x, ly, front_z], [0.; 3], None),
        part("left_front_leg", leg(true), [x, ly, front_z], [0.; 3], None),
    ]
}

pub fn pig() -> ModelDef {
    let mut parts = vec![
        part("head", vec![bx([-4., -4., -8.], [8., 8., 8.], [0., 0.]), bx([-2., 0., -9.], [4., 3., 1.], [16., 16.])], [0., 12., -6.], [0.; 3], None),
        part("body", vec![bx([-5., -10., -7.], [10., 16., 8.], [28., 8.])], [0., 11., 2.], [std::f32::consts::FRAC_PI_2, 0., 0.], None),
    ];
    parts.extend(quad_legs(6.0, 18.0, -5.0, 7.0, 3.0));
    ModelDef { tex: "pig", tex_w: 64.0, tex_h: 64.0, parts }
}

pub fn cow() -> ModelDef {
    let mut parts = vec![
        part("head", vec![
            bx([-4., -4., -6.], [8., 8., 6.], [0., 0.]), bx([-3., 1., -7.], [6., 3., 1.], [1., 33.]),
            bx([-5., -5., -5.], [1., 3., 1.], [22., 0.]), bx([4., -5., -5.], [1., 3., 1.], [22., 0.]),
        ], [0., 4., -8.], [0.; 3], None),
        part("body", vec![bx([-6., -10., -7.], [12., 18., 10.], [18., 4.]), bx([-2., 2., -8.], [4., 6., 1.], [52., 0.])], [0., 5., 2.], [std::f32::consts::FRAC_PI_2, 0., 0.], None),
    ];
    parts.extend(quad_legs(12.0, 12.0, -5.0, 7.0, 4.0));
    ModelDef { tex: "cow", tex_w: 64.0, tex_h: 64.0, parts }
}

pub fn sheep() -> ModelDef {
    let mut parts = vec![
        part("head", vec![bx([-3., -4., -6.], [6., 6., 8.], [0., 0.])], [0., 6., -8.], [0.; 3], None),
        part("body", vec![bx([-4., -10., -7.], [8., 16., 6.], [28., 8.])], [0., 5., 2.], [std::f32::consts::FRAC_PI_2, 0., 0.], None),
    ];
    parts.extend(quad_legs(12.0, 12.0, -5.0, 7.0, 3.0));
    ModelDef { tex: "sheep", tex_w: 64.0, tex_h: 32.0, parts }
}

pub fn chicken() -> ModelDef {
    let leg = || vec![bx([-1., 0., -3.], [3., 5., 3.], [26., 0.])];
    ModelDef {
        tex: "chicken", tex_w: 64.0, tex_h: 32.0,
        parts: vec![
            part("head", vec![bx([-2., -6., -2.], [4., 6., 3.], [0., 0.])], [0., 15., -4.], [0.; 3], None),
            part("beak", vec![bx([-2., -4., -4.], [4., 2., 2.], [14., 0.])], [0., 15., -4.], [0.; 3], None),
            part("red_thing", vec![bx([-1., -2., -3.], [2., 2., 2.], [14., 4.])], [0., 15., -4.], [0.; 3], None),
            part("body", vec![bx([-3., -4., -3.], [6., 8., 6.], [0., 9.])], [0., 16., 0.], [std::f32::consts::FRAC_PI_2, 0., 0.], None),
            part("right_leg", leg(), [-2., 19., 1.], [0.; 3], None),
            part("left_leg", leg(), [1., 19., 1.], [0.; 3], None),
            part("right_wing", vec![bx([0., 0., -3.], [1., 4., 6.], [24., 13.])], [-4., 13., 0.], [0.; 3], None),
            part("left_wing", vec![bx([-1., 0., -3.], [1., 4., 6.], [24., 13.])], [4., 13., 0.], [0.; 3], None),
        ],
    }
}

// ------------------------------------------------------------------ mesh building

/// Pose = per-part extra rotation (x, y, z radians) plus scale, light and tint for one draw.
pub struct Pose {
    pub rot: Vec<[f32; 3]>,
    /// Extra entity-space rotations after the model (death fall: roll about z).
    pub roll: f32,
    pub scale: [f32; 3],
    pub tint: [f32; 4],
    pub light: [f32; 2],
    pub hidden: Vec<usize>,
}

impl Pose {
    pub fn new(parts: usize) -> Self { Self { rot: vec![[0.0; 3]; parts], roll: 0.0, scale: [1.0; 3], tint: [1.0; 4], light: [15.0, 0.0], hidden: vec![] } }
}

/// Appends the model's quads to `out`. `origin` is the entity position relative to the camera, `yaw_deg` the body yaw.
pub fn build(model: &ModelDef, tex_rect: [f32; 4], atlas: f32, pose: &Pose, origin: Vec3, yaw_deg: f32, out: &mut (Vec<Vertex>, Vec<u32>)) {
    // model space (1/16 units, y down) -> world: T(origin) * RotY(180 - yaw) * S(-1,-1,1) * T(0,-1.501,0)
    let entity = Mat4::from_translation(origin)
        * Mat4::from_rotation_y((180.0 - yaw_deg).to_radians())
        * Mat4::from_rotation_z(pose.roll)
        * Mat4::from_scale(Vec3::new(-pose.scale[0], -pose.scale[1], pose.scale[2]))
        * Mat4::from_translation(Vec3::new(0.0, -1.501, 0.0));
    build_with(model, tex_rect, atlas, pose, entity, None, out);
}

/// Builds with an explicit model->output matrix; `only` restricts to one part (first-person arm).
pub fn build_with(model: &ModelDef, tex_rect: [f32; 4], atlas: f32, pose: &Pose, entity: Mat4, only: Option<usize>, out: &mut (Vec<Vertex>, Vec<u32>)) {
    let mut mats: Vec<Mat4> = Vec::with_capacity(model.parts.len());
    for (i, p) in model.parts.iter().enumerate() {
        let r = pose.rot[i];
        let rot = [p.rot[0] + r[0], p.rot[1] + r[1], p.rot[2] + r[2]];
        let local = Mat4::from_translation(Vec3::from(p.offset) / 16.0) * Mat4::from_quat(Quat::from_euler(EulerRot::ZYX, rot[2], rot[1], rot[0]));
        let m = match p.parent { Some(pi) => mats[pi] * local, None => local };
        mats.push(m);
    }
    let (tw, th) = (model.tex_w, model.tex_h);
    let (rx, ry, rw, rh) = (tex_rect[0], tex_rect[1], tex_rect[2], tex_rect[3]);
    for (i, p) in model.parts.iter().enumerate() {
        if pose.hidden.contains(&i) || only.map_or(false, |o| o != i) { continue; }
        let m = entity * mats[i] * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        for b in &p.boxes {
            let (w, h, d) = (b.s[0], b.s[1], b.s[2]);
            let (mut x0, mut x1) = (b.o[0] - b.grow, b.o[0] + w + b.grow);
            let (y0, y1) = (b.o[1] - b.grow, b.o[1] + h + b.grow);
            let (z0, z1) = (b.o[2] - b.grow, b.o[2] + d + b.grow);
            if b.mirror { std::mem::swap(&mut x0, &mut x1); }
            let (tx, ty) = (b.uv[0], b.uv[1]);
            let (u0, u1, u2, u3, u4, u5) = (tx, tx + d, tx + d + w, tx + d + w + w, tx + d + w + d, tx + d + w + d + w);
            let (v0, v1, v2) = (ty, ty + d, ty + d + h);
            // (corner positions, uv corners) per face; flipped horizontally for mirrored boxes.
            let faces: [([[f32; 3]; 4], [[f32; 2]; 4], [f32; 3]); 6] = [
                ([[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]], [[u1, v1], [u2, v1], [u2, v2], [u1, v2]], [0., 0., -1.]),
                ([[x1, y0, z1], [x0, y0, z1], [x0, y1, z1], [x1, y1, z1]], [[u4, v1], [u5, v1], [u5, v2], [u4, v2]], [0., 0., 1.]),
                ([[x0, y0, z1], [x0, y0, z0], [x0, y1, z0], [x0, y1, z1]], [[u0, v1], [u1, v1], [u1, v2], [u0, v2]], [-1., 0., 0.]),
                ([[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]], [[u2, v1], [u4, v1], [u4, v2], [u2, v2]], [1., 0., 0.]),
                ([[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]], [[u1, v0], [u2, v0], [u2, v1], [u1, v1]], [0., -1., 0.]),
                ([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], [[u2, v0], [u3, v0], [u3, v1], [u2, v1]], [0., 1., 0.]),
            ];
            for (fi, (corners, uvs, n)) in faces.iter().enumerate() {
                let mut uvs = *uvs;
                if b.mirror { uvs = [uvs[1], uvs[0], uvs[3], uvs[2]]; }
                let _ = fi;
                // Shade from the transformed normal: up 1.0, down 0.5, sides 0.6..0.8.
                let nm = (entity * mats[i]).transform_vector3(Vec3::from(*n)).normalize();
                let shade = if nm.y > 0.5 { 1.0 } else if nm.y < -0.5 { 0.5 } else if nm.z.abs() > nm.x.abs() { 0.8 } else { 0.6 };
                let base = out.0.len() as u32;
                for k in 0..4 {
                    let pw = m.transform_point3(Vec3::from(corners[k]));
                    let uv = [(rx + uvs[k][0] / tw * rw) / atlas, (ry + uvs[k][1] / th * rh) / atlas];
                    out.0.push(Vertex {
                        pos: pw.to_array(), uv,
                        color: [pose.tint[0] * shade, pose.tint[1] * shade, pose.tint[2] * shade, pose.tint[3]],
                        light: pose.light,
                    });
                }
                out.1.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}

/// A flat textured quad (item sprites) or crossed quads for arrows, in world space relative to the camera.
pub fn quad(out: &mut (Vec<Vertex>, Vec<u32>), c: [Vec3; 4], uv: [[f32; 2]; 4], color: [f32; 4], light: [f32; 2]) {
    let base = out.0.len() as u32;
    for k in 0..4 { out.0.push(Vertex { pos: c[k].to_array(), uv: uv[k], color, light }); }
    out.1.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
