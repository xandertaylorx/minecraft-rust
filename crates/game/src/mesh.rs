//! Face-culled chunk meshing with Minecraft-style smooth lighting and ambient occlusion.
use bytemuck::{Pod, Zeroable};

use crate::blocks::{def, Tint, AIR};
use crate::world::{Light, Region, MAX_Y, MIN_Y};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// (sky, block) light in 0..15, interpolated.
    pub light: [f32; 2],
}

#[derive(Default)]
pub struct MeshData {
    pub opaque: (Vec<Vertex>, Vec<u32>),
    pub translucent: (Vec<Vertex>, Vec<u32>),
}

/// Direction, per-face brightness (vanilla), corner positions and UVs.
struct Face {
    n: [i32; 3],
    shade: f32,
    corners: [[i32; 3]; 4],
    uvs: [[f32; 2]; 4],
}

const UV: [[f32; 2]; 4] = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
static FACES: [Face; 6] = [
    Face { n: [0, 1, 0], shade: 1.0, corners: [[0, 1, 0], [1, 1, 0], [1, 1, 1], [0, 1, 1]], uvs: UV },
    Face { n: [0, -1, 0], shade: 0.5, corners: [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], uvs: UV },
    Face { n: [1, 0, 0], shade: 0.6, corners: [[1, 1, 1], [1, 1, 0], [1, 0, 0], [1, 0, 1]], uvs: UV },
    Face { n: [-1, 0, 0], shade: 0.6, corners: [[0, 1, 0], [0, 1, 1], [0, 0, 1], [0, 0, 0]], uvs: UV },
    Face { n: [0, 0, 1], shade: 0.8, corners: [[0, 1, 1], [1, 1, 1], [1, 0, 1], [0, 0, 1]], uvs: UV },
    Face { n: [0, 0, -1], shade: 0.8, corners: [[1, 1, 0], [0, 1, 0], [0, 0, 0], [1, 0, 0]], uvs: UV },
];

/// Per-biome tint colours (indexed by worldgen biome id).
pub struct BiomeColors {
    pub grass: Vec<[f32; 3]>,
    pub foliage: Vec<[f32; 3]>,
    pub water: Vec<[f32; 3]>,
}

pub struct Tiles<'a> {
    /// texture name -> atlas tile index
    pub lookup: &'a dyn Fn(&str) -> u32,
    pub colors: &'a BiomeColors,
}

/// Vanilla blends biome colours over a 5x5 area around each block column.
fn blended(region: &Region, rx: i32, rz: i32, table: &[[f32; 3]]) -> [f32; 3] {
    let mut s = [0f32; 3];
    for dz in -2..=2 {
        for dx in -2..=2 {
            let c = table[region.biome(rx + dx, rz + dz) as usize];
            for i in 0..3 { s[i] += c[i]; }
        }
    }
    [s[0] / 25.0, s[1] / 25.0, s[2] / 25.0]
}

fn tile_uv(tile: u32, uv: [f32; 2]) -> [f32; 2] {
    let (tx, ty) = ((tile % 16) as f32, (tile / 16) as f32);
    // Inset slightly to avoid sampling the neighbouring tile.
    let e = 0.002;
    [(tx + e + uv[0] * (1.0 - 2.0 * e)) / 16.0, (ty + e + uv[1] * (1.0 - 2.0 * e)) / crate::ATLAS_ROWS as f32]
}

pub fn mesh_chunk(region: &Region, light: &Light, tiles: &Tiles) -> MeshData {
    let mut out = MeshData::default();
    let (ox, oz) = (16, 16); // centre chunk offset inside the region
    let mut grass = Vec::with_capacity(256);
    let mut foliage = Vec::with_capacity(256);
    let mut water = Vec::with_capacity(256);
    for z in 0..16 {
        for x in 0..16 {
            grass.push(blended(region, x + ox, z + oz, &tiles.colors.grass));
            foliage.push(blended(region, x + ox, z + oz, &tiles.colors.foliage));
            water.push(blended(region, x + ox, z + oz, &tiles.colors.water));
        }
    }
    for y in MIN_Y..MAX_Y {
        for z in 0..16 {
            for x in 0..16 {
                let (rx, rz) = (x + ox, z + oz);
                let id = region.block(rx, y, rz);
                if id == AIR { continue; }
                let d = def(id);
                if d.name == "fire" {
                    // Vanilla picks fire_floor0/1 (fire_0 / fire_1 textures) per position.
                    let h = (rx.wrapping_mul(73856093) ^ y.wrapping_mul(19349663) ^ rz.wrapping_mul(83492791)) & 1;
                    let tile = (tiles.lookup)(if h == 0 { "fire_0" } else { "fire_1" });
                    emit_fire(&mut out.opaque, light, rx, y, rz, tile);
                    continue;
                }
                if d.cross {
                    let tile = (tiles.lookup)(d.top);
                    let col = (z * 16 + x) as usize;
                    let ct = if d.tint == Tint::Grass { grass[col] } else { [1.0; 3] };
                    emit_cross(&mut out.opaque, light, rx, y, rz, tile, ct);
                    continue;
                }
                for (fi, f) in FACES.iter().enumerate() {
                    let (nx, ny, nz) = (rx + f.n[0], y + f.n[1], rz + f.n[2]);
                    let nid = region.block(nx, ny, nz);
                    let nd = def(nid);
                    if nd.opaque { continue; }
                    if nid == id && d.cull_same { continue; }
                    // Fluids drop below the top: skip rendering sides against solid air-less cases handled above.
                    let tex = match fi { 0 => d.top, 1 => d.bottom, _ => d.side };
                    let tile = (tiles.lookup)(tex);
                    let col = (z * 16 + x) as usize;
                    let tint = match d.tint {
                        Tint::None => [1.0; 3],
                        Tint::Grass => if fi == 0 { grass[col] } else { [1.0; 3] },
                        Tint::Foliage => foliage[col],
                        Tint::Water => water[col],
                        Tint::Birch => [0.502, 0.655, 0.333],
                        Tint::Spruce => [0.380, 0.600, 0.380],
                    };
                    let target = if d.translucent { &mut out.translucent } else { &mut out.opaque };
                    let wh = if d.name == "water" && region.block(rx, y + 1, rz) != id { 8.0 / 9.0 } else { 1.0 };
                    emit(target, region, light, f, rx, y, rz, tile, tint, d.translucent, 0.0, wh, if d.name == "water" { 0.72 } else { 0.75 });
                    if let (Some(ov), true) = (d.overlay, fi >= 2) {
                        let t = (tiles.lookup)(ov);
                        emit(&mut out.opaque, region, light, f, rx, y, rz, t, grass[col], false, 0.001, 1.0, 1.0);
                    }
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit(
    target: &mut (Vec<Vertex>, Vec<u32>), region: &Region, light: &Light, f: &Face,
    rx: i32, y: i32, rz: i32, tile: u32, tint: [f32; 3], translucent: bool, push: f32, top: f32, alpha: f32,
) {
    let base = target.0.len() as u32;
    let (fx, fy, fz) = (rx + f.n[0], y + f.n[1], rz + f.n[2]);
    let front = light.at(fx, fy, fz);
    let mut ao_vals = [0f32; 4];
    for (i, c) in f.corners.iter().enumerate() {
        // Corner axes tangent to the face.
        let mut axes: Vec<usize> = (0..3).filter(|&a| f.n[a] == 0).collect();
        let (a, b) = (axes.remove(0), axes.remove(0));
        let sa = if c[a] == 1 { 1 } else { -1 };
        let sb = if c[b] == 1 { 1 } else { -1 };
        let mut off1 = [0; 3];
        let mut off2 = [0; 3];
        off1[a] = sa;
        off2[b] = sb;
        let p1 = (fx + off1[0], fy + off1[1], fz + off1[2]);
        let p2 = (fx + off2[0], fy + off2[1], fz + off2[2]);
        let p3 = (p1.0 + off2[0], p1.1 + off2[1], p1.2 + off2[2]);
        let o1 = def(region.block(p1.0, p1.1, p1.2)).opaque;
        let o2 = def(region.block(p2.0, p2.1, p2.2)).opaque;
        let o3 = def(region.block(p3.0, p3.1, p3.2)).opaque;
        let ao = if o1 && o2 { 0 } else { 3 - (o1 as i32 + o2 as i32 + o3 as i32) };
        ao_vals[i] = [0.5, 0.65, 0.8, 1.0][ao as usize];
        let l = |p: (i32, i32, i32), o: bool| if o { front } else { light.at(p.0, p.1, p.2) };
        let (l1, l2) = (l(p1, o1), l(p2, o2));
        let l3 = if o1 && o2 { front } else { l(p3, o3) };
        let sky = (front.0 as f32 + l1.0 as f32 + l2.0 as f32 + l3.0 as f32) / 4.0;
        let blk = (front.1 as f32 + l1.1 as f32 + l2.1 as f32 + l3.1 as f32) / 4.0;
        let ax = (rx - 16) as f32 + c[0] as f32 + f.n[0] as f32 * push;
        let ay = y as f32 + if c[1] == 1 { top } else { 0.0 } + f.n[1] as f32 * push;
        let az = (rz - 16) as f32 + c[2] as f32 + f.n[2] as f32 * push;
        let s = f.shade * ao_vals[i];
        target.0.push(Vertex {
            pos: [ax, ay, az],
            uv: tile_uv(tile, f.uvs[i]),
            color: [tint[0] * s, tint[1] * s, tint[2] * s, if translucent { alpha } else { 1.0 }],
            light: [sky, blk],
        });
    }
    // Flip the diagonal to avoid anisotropic AO artefacts.
    if ao_vals[0] + ao_vals[2] > ao_vals[1] + ao_vals[3] {
        target.1.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        target.1.extend_from_slice(&[base + 1, base + 2, base + 3, base + 1, base + 3, base]);
    }
}

/// Two diagonal quads (fire, plants), double sided.
fn emit_cross(target: &mut (Vec<Vertex>, Vec<u32>), light: &Light, rx: i32, y: i32, rz: i32, tile: u32, tint: [f32; 3]) {
    let (s, b) = light.at(rx, y, rz);
    let (lx, lz) = ((rx - 16) as f32, (rz - 16) as f32);
    let ly = y as f32;
    let planes = [([[0.0, 0.0], [1.0, 1.0]]), ([[0.0, 1.0], [1.0, 0.0]])];
    for pl in planes {
        let base = target.0.len() as u32;
        let (a, c) = (pl[0], pl[1]);
        let corners = [[a[0], 1.0, a[1]], [c[0], 1.0, c[1]], [c[0], 0.0, c[1]], [a[0], 0.0, a[1]]];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for k in 0..4 {
            target.0.push(Vertex { pos: [lx + corners[k][0], ly + corners[k][1], lz + corners[k][2]], uv: tile_uv(tile, uvs[k]), color: [tint[0], tint[1], tint[2], 1.0], light: [s as f32, b as f32] });
        }
        target.1.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// `block/template_fire_floor`: four double-sided planes tilted 22.5 degrees, rescaled.
fn emit_fire(target: &mut (Vec<Vertex>, Vec<u32>), light: &Light, rx: i32, y: i32, rz: i32, tile: u32) {
    let (s, b) = light.at(rx, y, rz);
    let (lx, lz) = ((rx - 16) as f32, (rz - 16) as f32);
    let ly = y as f32;
    // (axis, plane offset, angle) per element; coordinates in 1/16 units relative to the (8,8,8) pivot.
    let planes: [(char, f32, f32); 4] = [('x', 8.8, -22.5), ('x', 7.2, 22.5), ('z', 8.8, -22.5), ('z', 7.2, 22.5)];
    for (axis, off, ang) in planes {
        let (sn, cs) = (ang.to_radians().sin(), ang.to_radians().cos());
        let corners_2d = [(0.0f32, 22.4f32), (16.0, 22.4), (16.0, 0.0), (0.0, 0.0)]; // (along, up)
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let base = target.0.len() as u32;
        for k in 0..4 {
            let (along, up) = corners_2d[k];
            let (mut px, mut py, mut pz) = if axis == 'x' { (along, up, off) } else { (off, up, along) };
            // Rotate about the pivot, then rescale the rotated axes by 1/cos.
            let (cx, cy, cz) = (px - 8.0, py - 8.0, pz - 8.0);
            if axis == 'x' {
                let (ny, nz) = (cy * cs - cz * sn, cy * sn + cz * cs);
                py = ny / cs + 8.0; pz = nz / cs + 8.0;
            } else {
                let (nx, ny) = (cx * cs - cy * sn, cx * sn + cy * cs);
                px = nx / cs + 8.0; py = ny / cs + 8.0;
            }
            target.0.push(Vertex { pos: [lx + px / 16.0, ly + py / 16.0, lz + pz / 16.0], uv: tile_uv(tile, uvs[k]), color: [1.0; 4], light: [s as f32, b as f32] });
        }
        target.1.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
