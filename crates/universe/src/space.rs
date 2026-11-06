//! Atmosphere, cloud field, surface layout and universe positions.
use crate::catalog::{mix, Planet, PlanetId, StarSystem, SystemId, UniverseCatalog, SECTOR_SIZE};
use crate::mth::clamp;
use crate::terrain::noise;
use crate::vec3::Vec3;

pub fn atmosphere_distance(body: &Planet, point: Vec3, top: f64) -> f64 {
    let c = body.native_center();
    let dx = ((point.x - c.x).abs() - body.radius).max(0.0);
    let dz = ((point.z - c.z).abs() - body.radius).max(0.0);
    let dy = (body.bottom() as f64 - point.y).max(point.y - top).max(0.0);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub fn atmosphere_density(body: &Planet, distance: f64) -> f64 {
    let thickness = 96.0 + body.environment.atmosphere * 240.0;
    let t = clamp((distance - 24.0) / thickness, 0.0, 1.0);
    body.environment.atmosphere * (1.0 - t * t * (3.0 - 2.0 * t))
}

pub fn cloud_height(body: &Planet) -> f32 {
    (224 + ((mix(body.seed + 92) as u64 >> 8) & 31) as i32) as f32
}

pub fn cloud_occupied(body: &Planet, x: f64, z: f64) -> bool {
    let c = body.native_center();
    let (dx, dz) = (x - c.x, z - c.z);
    let edge = dx.abs().max(dz.abs()) / body.radius;
    if edge > 0.96 { return false; }
    let field = noise(body.seed + 91, dx / 64.0, 0.0, dz / 64.0);
    field > 0.12 + (edge - 0.7).max(0.0) * 1.4
}

pub fn cloud_opacity(body: &Planet, camera: Vec3) -> f32 {
    let c = body.native_center();
    let edge = (camera.x - c.x).abs().max((camera.z - c.z).abs()) - body.radius;
    let horizontal = 1.0 - clamp(edge / 48.0, 0.0, 1.0);
    let above = 1.0 - clamp((camera.y - cloud_height(body) as f64 - 16.0) / 72.0, 0.0, 1.0);
    let below = 1.0 - clamp((body.bottom() as f64 - camera.y) / 24.0, 0.0, 1.0);
    (horizontal * above * below * (body.environment.atmosphere * 1.3).min(1.0)) as f32
}

/// Index 0..=6 into the eight planet biomes (7 = open space).
pub fn biome_index(p: &Planet) -> usize {
    let e = &p.environment;
    if e.temperature < 0.15 { 0 } else if e.temperature > 0.9 { 1 } else if e.radiation > 0.7 { 2 }
    else if e.water < 0.2 { 3 } else if e.water > 0.65 { 4 } else if e.vegetation > 0.45 { 5 } else { 6 }
}

/// `UniverseSurfaceLayout`.
pub struct SurfaceLayout { pub catalog: UniverseCatalog }

impl SurfaceLayout {
    pub fn new(seed: i64) -> Self { Self { catalog: UniverseCatalog::new(seed) } }
    pub fn bodies(&mut self) -> Vec<Planet> { self.catalog.system(SystemId::SOL).bodies.clone() }
    pub fn nearby(&mut self, x: f64, z: f64) -> Vec<Planet> {
        let cell = SystemId::at(x, z);
        let mut out = Vec::new();
        for dz in -1..=1 { for dx in -1..=1 {
            let s: &StarSystem = self.catalog.system(SystemId { x: cell.x + dx, y: 0, z: cell.z + dz });
            out.extend(s.bodies.iter().cloned());
        } }
        out
    }
    pub fn surface_at(&mut self, x: f64, z: f64) -> Option<Planet> {
        let sys = self.catalog.system(SystemId::at(x, z));
        for p in &sys.bodies {
            if p.parent_body == -3 { continue; }
            let c = p.native_center();
            if (x - c.x).abs() < p.radius && (z - c.z).abs() < p.radius { return Some(p.clone()); }
        }
        None
    }
    pub fn is_earth(&mut self, x: f64, z: f64) -> bool {
        self.surface_at(x, z).is_some_and(|p| p.id == PlanetId::EARTH)
    }
    pub fn second_planet(&mut self) -> Planet {
        self.bodies().into_iter().find(|p| p.parent_body < 0 && p.id != PlanetId::EARTH).unwrap()
    }
}

/// `UniversePosition`: sector + bounded local doubles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniversePosition { pub sector: SystemId, pub local: Vec3 }

impl UniversePosition {
    pub fn new(sector: SystemId, local: Vec3) -> Self {
        assert!(local.x.is_finite() && local.y.is_finite() && local.z.is_finite(), "Non-finite universe position");
        let carry = |v: f64| -> i64 {
            let cells = ((v + SECTOR_SIZE / 2.0) / SECTOR_SIZE).floor();
            assert!(cells >= -(2f64.powi(63)) && cells < 2f64.powi(63), "Position exceeds address range");
            cells as i64
        };
        let (dx, dy, dz) = (carry(local.x), carry(local.y), carry(local.z));
        Self {
            sector: SystemId {
                x: sector.x.checked_add(dx).unwrap(), y: sector.y.checked_add(dy).unwrap(), z: sector.z.checked_add(dz).unwrap(),
            },
            local: Vec3::new(local.x - dx as f64 * SECTOR_SIZE, local.y - dy as f64 * SECTOR_SIZE, local.z - dz as f64 * SECTOR_SIZE),
        }
    }
    pub fn mv(&self, d: Vec3) -> Self { Self::new(self.sector, self.local.add(d)) }
    pub fn relative_to(&self, o: &UniversePosition) -> Vec3 {
        Vec3::new(
            (self.sector.x.checked_sub(o.sector.x).unwrap()) as f64 * SECTOR_SIZE + self.local.x - o.local.x,
            (self.sector.y.checked_sub(o.sector.y).unwrap()) as f64 * SECTOR_SIZE + self.local.y - o.local.y,
            (self.sector.z.checked_sub(o.sector.z).unwrap()) as f64 * SECTOR_SIZE + self.local.z - o.local.z,
        )
    }
}
