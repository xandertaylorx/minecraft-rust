//! Chunk storage, vanilla overworld generation (via the worldgen crate), and light.
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::blocks::{self, def, AIR};

pub const MIN_Y: i32 = -64;
pub const HEIGHT: usize = 320;
pub const MAX_Y: i32 = MIN_Y + HEIGHT as i32;

pub type Blocks = Arc<Vec<u16>>;

#[inline]
pub fn idx(x: usize, y: usize, z: usize) -> usize { (y * 16 + z) * 16 + x }

pub fn generate(wgen: &worldgen::chunk::OverworldGen, map: &[u16], cx: i32, cz: i32) -> (Blocks, Arc<Vec<u16>>) {
    let g = wgen.generate(cx, cz);
    let mut blocks: Vec<u16> = g.blocks.iter().map(|&i| map[i as usize]).collect();
    crate::features::decorate(&mut blocks, &g.column_biome, &wgen.biomes.biomes, cx, cz);
    (Arc::new(blocks), Arc::new(g.column_biome))
}

pub struct Chunk {
    pub blocks: Blocks,
    pub biomes: Arc<Vec<u16>>,
}

#[derive(Default)]
pub struct World {
    pub chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    pub fn get(&self, x: i32, y: i32, z: i32) -> u16 {
        if y < MIN_Y || y >= MAX_Y { return AIR; }
        match self.chunks.get(&(x.div_euclid(16), z.div_euclid(16))) {
            Some(c) => c.blocks[idx(x.rem_euclid(16) as usize, (y - MIN_Y) as usize, z.rem_euclid(16) as usize)],
            None => AIR,
        }
    }

    /// Approximate client light at a block (computed on demand for the debug screen).
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        let Some(r) = self.region(cx, cz) else { return (0, 0) };
        let l = r.light();
        l.at(x.rem_euclid(16) + 16, y, z.rem_euclid(16) + 16)
    }

    pub fn loaded(&self, x: i32, z: i32) -> bool { self.chunks.contains_key(&(x.div_euclid(16), z.div_euclid(16))) }

    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u16) -> bool {
        if y < MIN_Y || y >= MAX_Y { return false; }
        let Some(c) = self.chunks.get_mut(&(x.div_euclid(16), z.div_euclid(16))) else { return false };
        Arc::make_mut(&mut c.blocks)[idx(x.rem_euclid(16) as usize, (y - MIN_Y) as usize, z.rem_euclid(16) as usize)] = id;
        true
    }

    /// Gather the 3x3 chunk neighbourhood around a chunk, if fully loaded.
    pub fn region(&self, cx: i32, cz: i32) -> Option<Region> {
        let mut parts: Vec<Blocks> = Vec::with_capacity(9);
        let mut biomes: Vec<Arc<Vec<u16>>> = Vec::with_capacity(9);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let c = self.chunks.get(&(cx + dx, cz + dz))?;
                parts.push(c.blocks.clone());
                biomes.push(c.biomes.clone());
            }
        }
        Some(Region { parts, biomes, cx, cz })
    }
}

/// 3x3 chunks of blocks plus computed light, indexed in region-local coordinates (0..48).
pub struct Region {
    parts: Vec<Blocks>,
    biomes: Vec<Arc<Vec<u16>>>,
    pub cx: i32,
    pub cz: i32,
}

pub const RW: usize = 48;

impl Region {
    #[inline]
    pub fn block(&self, rx: i32, y: i32, rz: i32) -> u16 {
        if y < MIN_Y || y >= MAX_Y || rx < 0 || rz < 0 || rx >= RW as i32 || rz >= RW as i32 { return AIR; }
        let (px, pz) = (rx as usize / 16, rz as usize / 16);
        self.parts[pz * 3 + px][idx(rx as usize % 16, (y - MIN_Y) as usize, rz as usize % 16)]
    }

    /// Biome of the column at region-local (rx, rz).
    pub fn biome(&self, rx: i32, rz: i32) -> u16 {
        let (rx, rz) = (rx.clamp(0, RW as i32 - 1), rz.clamp(0, RW as i32 - 1));
        self.biomes[(rz as usize / 16) * 3 + rx as usize / 16][(rz as usize % 16) * 16 + rx as usize % 16]
    }

    /// Computes (sky, block) light for the whole region.
    pub fn light(&self) -> Light {
        let n = RW * RW * HEIGHT;
        let mut sky = vec![0u8; n];
        let mut blk = vec![0u8; n];
        let li = |x: usize, y: usize, z: usize| (y * RW + z) * RW + x;
        let mut q: VecDeque<(u16, u16, u16)> = VecDeque::new();
        let mut qb: VecDeque<(u16, u16, u16)> = VecDeque::new();
        for z in 0..RW {
            for x in 0..RW {
                for y in (0..HEIGHT).rev() {
                    if def(self.block(x as i32, y as i32 + MIN_Y, z as i32)).opaque { break; }
                    sky[li(x, y, z)] = 15;
                    q.push_back((x as u16, y as u16, z as u16));
                }
                for y in 0..HEIGHT {
                    let e = def(self.block(x as i32, y as i32 + MIN_Y, z as i32)).emit;
                    if e > 0 {
                        blk[li(x, y, z)] = e;
                        qb.push_back((x as u16, y as u16, z as u16));
                    }
                }
            }
        }
        let dirs: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
        for (queue, arr) in [(&mut q, &mut sky), (&mut qb, &mut blk)] {
            while let Some((x, y, z)) = queue.pop_front() {
                let l = arr[li(x as usize, y as usize, z as usize)];
                if l <= 1 { continue; }
                for (dx, dy, dz) in dirs {
                    let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                    if nx < 0 || nz < 0 || ny < 0 || nx >= RW as i32 || nz >= RW as i32 || ny >= HEIGHT as i32 { continue; }
                    if def(self.block(nx, ny + MIN_Y, nz)).opaque { continue; }
                    let i = li(nx as usize, ny as usize, nz as usize);
                    if arr[i] + 1 < l {
                        arr[i] = l - 1;
                        queue.push_back((nx as u16, ny as u16, nz as u16));
                    }
                }
            }
        }
        Light { sky, block: blk }
    }
}

pub struct Light {
    pub sky: Vec<u8>,
    pub block: Vec<u8>,
}

impl Light {
    #[inline]
    pub fn at(&self, rx: i32, y: i32, rz: i32) -> (u8, u8) {
        if y < MIN_Y { return (0, 0); }
        if y >= MAX_Y { return (15, 0); }
        if rx < 0 || rz < 0 || rx >= RW as i32 || rz >= RW as i32 { return (15, 0); }
        let i = ((y - MIN_Y) as usize * RW + rz as usize) * RW + rx as usize;
        (self.sky[i], self.block[i])
    }
}
