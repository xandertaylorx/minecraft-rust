//! Chunk terrain: cell-interpolated final density like vanilla's `NoiseChunk`.
use std::collections::{HashMap, HashSet};

use std::cell::RefCell;

use crate::aquifer::{vein, Aquifer, Fluid};
use crate::biome::{obfuscate_seed, BiomeSource, BiomeTable};
use crate::surface::{load_surface_rule, Context, Palette, Rule, SurfaceSystem};
use crate::density::*;
use crate::random::Positional;

pub const MIN_Y: i32 = -64;
pub const HEIGHT: i32 = 384;

pub struct OverworldGen {
    pub router: Router,
    aquifer_random: Positional,
    ore_random: Positional,
    pub biomes: BiomeTable,
    zoom_seed: i64,
    rule: Rule,
    system: SurfaceSystem,
    pub palette: Vec<String>,
}

/// A generated chunk: block palette indices `(y*16+z)*16+x` and one biome id per column (`z*16+x`).
pub struct GenChunk {
    pub blocks: Vec<u16>,
    pub column_biome: Vec<u16>,
}

/// Solid (density > 0) flags for one chunk, indexed `(y * 16 + z) * 16 + x` with y from `MIN_Y`.
pub struct Terrain {
    pub solid: Vec<bool>,
}

impl OverworldGen {
    pub fn new(rs: &RandomState, seed: i64) -> Self {
        let mut pal = Palette::default();
        for n in ["air", "stone", "water", "lava", "granite", "tuff", "copper_ore", "raw_copper_block", "deepslate_iron_ore", "raw_iron_block"] { pal.id(n); }
        let biomes = BiomeTable::load(&rs.jar);
        let rule = load_surface_rule(&rs.jar, rs, &biomes, &mut pal);
        let system = SurfaceSystem::new(rs, &mut pal);
        Self {
            router: rs.router("overworld"),
            aquifer_random: rs.root.from_hash_of("minecraft:aquifer").fork_positional(),
            ore_random: rs.root.from_hash_of("minecraft:ore").fork_positional(),
            biomes, zoom_seed: obfuscate_seed(seed), rule, system, palette: pal.names,
        }
    }

    /// Biome id at a block position (vanilla BiomeManager zoom), for `/locate`.
    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> u16 {
        let src = BiomeSource { table: &self.biomes, router: &self.router, zoom_seed: self.zoom_seed, cache: Default::default() };
        src.biome_at(x, y, z)
    }

    fn pid(&self, name: &str) -> u16 { self.palette.iter().position(|n| n == name).unwrap() as u16 }

    fn eval_for_chunk(&self, cx: i32, cz: i32) -> Eval {
        let (x0, z0) = (cx * 16, cz * 16);
        let mut nodes: Vec<&Df> = Vec::new();
        let mut seen = HashSet::new();
        for f in [&self.router.final_density, &self.router.vein_toggle, &self.router.vein_ridged] {
            f.collect_interpolated(&mut nodes, &mut seen);
        }
        let mut ev = Eval { x0, z0, min_y: MIN_Y, grids: HashMap::new() };
        for node in nodes {
            let Df::Marker(_, inner) = node else { unreachable!() };
            let mut g = vec![0.0; GRID_NX * GRID_NX * GRID_NY];
            for i in 0..GRID_NX {
                for j in 0..GRID_NX {
                    for k in 0..GRID_NY {
                        let c = Ctx { x: x0 + i as i32 * CELL_W, y: MIN_Y + k as i32 * CELL_H, z: z0 + j as i32 * CELL_W };
                        g[(i * GRID_NX + j) * GRID_NY + k] = inner.eval(&c, Some(&ev));
                    }
                }
            }
            ev.grids.insert(node as *const Df as usize, g);
        }
        ev
    }

    pub fn terrain(&self, cx: i32, cz: i32) -> Terrain {
        let ev = self.eval_for_chunk(cx, cz);
        let mut solid = vec![false; 16 * 16 * HEIGHT as usize];
        for y in 0..HEIGHT {
            for z in 0..16 {
                for x in 0..16 {
                    let c = Ctx { x: cx * 16 + x, y: MIN_Y + y, z: cz * 16 + z };
                    solid[((y * 16 + z) * 16 + x) as usize] = self.router.final_density.eval(&c, Some(&ev)) > 0.0;
                }
            }
        }
        Terrain { solid }
    }
}

impl OverworldGen {
    /// Base blocks (stone/fluids/veins) for a chunk, before surface rules; index `(y*16+z)*16+x`.
    pub fn base_blocks(&self, cx: i32, cz: i32) -> Vec<u16> {
        let ev = self.eval_for_chunk(cx, cz);
        let mut aq = Aquifer::new(&self.router, &ev, self.aquifer_random, cx, cz, MIN_Y, HEIGHT);
        let mut out = vec![0u16; 16 * 16 * HEIGHT as usize];
        let (air, stone, water, lava) = (0u16, 1u16, 2u16, 3u16);
        for y in 0..HEIGHT {
            for z in 0..16 {
                for x in 0..16 {
                    let (wx, wy, wz) = (cx * 16 + x, MIN_Y + y, cz * 16 + z);
                    let d = self.router.final_density.eval(&Ctx { x: wx, y: wy, z: wz }, Some(&ev));
                    let b = match aq.compute_substance(wx, wy, wz, d) {
                        Some(Fluid::Air) => air,
                        Some(Fluid::Water) => water,
                        Some(Fluid::Lava) => lava,
                        None => match vein(&self.router, &ev, &self.ore_random, wx, wy, wz) {
                            Some(n) => self.pid(n),
                            None => stone,
                        },
                    };
                    out[((y * 16 + z) * 16 + x) as usize] = b;
                }
            }
        }
        out
    }
}

impl OverworldGen {
    /// Full chunk: noise terrain, aquifers, veins, then vanilla surface rules.
    pub fn generate(&self, cx: i32, cz: i32) -> GenChunk {
        let mut blocks = self.base_blocks(cx, cz);
        let h = HEIGHT as usize;
        let at = |b: &Vec<u16>, x: i32, y: i32, z: i32| -> u16 {
            if y < MIN_Y || y >= MIN_Y + HEIGHT { 0 } else { b[(((y - MIN_Y) * 16 + z) * 16 + x) as usize] }
        };
        // WORLD_SURFACE_WG: highest non-air block per column.
        let mut top = [0i32; 256];
        for z in 0..16 {
            for x in 0..16 {
                let mut y = MIN_Y + HEIGHT - 1;
                while y >= MIN_Y && at(&blocks, x, y, z) == 0 { y -= 1; }
                top[(z * 16 + x) as usize] = y;
            }
        }
        let flat = Eval { x0: 0, z0: 0, min_y: MIN_Y, grids: HashMap::new() };
        let cache: RefCell<HashMap<(i32, i32), i32>> = RefCell::new(HashMap::new());
        let prelim = |bx: i32, bz: i32| -> i32 {
            let (x, z) = ((bx >> 2) << 2, (bz >> 2) << 2);
            if let Some(v) = cache.borrow().get(&(x, z)) { return *v; }
            let v = self.router.preliminary_surface_level.eval(&Ctx { x, y: 0, z }, Some(&flat));
            let v = { let i = v as i32; if v < i as f64 { i - 1 } else { i } };
            cache.borrow_mut().insert((x, z), v);
            v
        };
        let src = BiomeSource { table: &self.biomes, router: &self.router, zoom_seed: self.zoom_seed, cache: Default::default() };
        let mut ctx = Context {
            sys: &self.system, biomes: &src, biome_names: &self.biomes.biomes, block_x: 0, block_z: 0, block_y: 0, surface_depth: 0,
            surface_secondary: None, min_surface_level: None, water_height: i32::MIN, stone_below: 0, stone_above: 0, biome: None,
            prelim: &prelim, steep: false, sea_level: 63,
        };
        let mut column_biome = vec![0u16; 256];
        let is_fluid = |b: u16| b == 2 || b == 3;
        for z in 0..16 {
            for x in 0..16 {
                let (wx, wz) = (cx * 16 + x, cz * 16 + z);
                let l1 = top[(z * 16 + x) as usize] + 1;
                let ht = |xx: i32, zz: i32| top[(zz * 16 + xx) as usize];
                let steep = {
                    let (i1, j1) = (ht(x, (z - 1).max(0)), ht(x, (z + 1).min(15)));
                    if j1 >= i1 + 4 { true } else { ht((x - 1).max(0), z) >= ht((x + 1).min(15), z) + 4 }
                };
                ctx.update_xz(wx, wz, steep);
                column_biome[(z * 16 + x) as usize] = src.biome_at(wx, l1, wz);
                let (mut i2, mut j2, mut k2) = (0, i32::MIN, i32::MAX);
                let mut y = l1;
                while y >= MIN_Y {
                    let b = at(&blocks, x, y, z);
                    if b == 0 {
                        i2 = 0;
                        j2 = i32::MIN;
                    } else if is_fluid(b) {
                        if j2 == i32::MIN { j2 = y + 1; }
                    } else {
                        if k2 >= y {
                            k2 = -4064 * 2;
                            let mut j3 = y - 1;
                            while j3 >= MIN_Y - 1 {
                                let b1 = at(&blocks, x, j3, z);
                                if b1 == 0 || is_fluid(b1) { k2 = j3 + 1; break; }
                                j3 -= 1;
                            }
                        }
                        i2 += 1;
                        ctx.update_y(i2, y - k2 + 1, j2, y);
                        if b == 1 {
                            if let Some(nb) = ctx.apply(&self.rule) {
                                if y >= MIN_Y && y < MIN_Y + HEIGHT { blocks[(((y - MIN_Y) * 16 + z) * 16 + x) as usize] = nb; }
                            }
                        }
                    }
                    y -= 1;
                }
            }
        }
        let _ = h;
        GenChunk { blocks, column_biome }
    }
}
