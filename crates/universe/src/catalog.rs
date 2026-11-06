//! Port of `SystemId`, `PlanetId`, `Planet` and `UniverseCatalog`.

use std::collections::{HashMap, VecDeque};
use std::f64::consts::PI;

use crate::java_random::JavaRandom;
use crate::vec3::Vec3;

pub const GENERATOR_VERSION: u32 = 5;
pub const CACHE_LIMIT: usize = 128;
pub const SECTOR_SIZE: f64 = 16384.0;
pub const EARTH_RADIUS: f64 = 512.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SystemId {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl SystemId {
    pub const SOL: SystemId = SystemId { x: 0, y: 0, z: 0 };

    pub fn seed(&self, universe_seed: i64) -> i64 {
        mix(mix(mix(universe_seed ^ self.x) ^ self.y.rotate_left(21)) ^ self.z.rotate_left(42))
    }

    /// Star-system cell containing native world coordinates (x, z).
    pub fn at(x: f64, z: f64) -> SystemId {
        SystemId {
            x: ((x + SECTOR_SIZE / 2.0) / SECTOR_SIZE).floor() as i64,
            y: 0,
            z: ((z + SECTOR_SIZE / 2.0) / SECTOR_SIZE).floor() as i64,
        }
    }
}

impl std::fmt::Display for SystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{},{}", self.x, self.y, self.z)
    }
}

/// SplitMix64 finalizer, as used by the Java version.
pub fn mix(value: i64) -> i64 {
    let mut v = value as u64;
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    (v ^ (v >> 31)) as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlanetId {
    pub system: SystemId,
    pub body: u8,
}

impl PlanetId {
    pub const EARTH: PlanetId = PlanetId { system: SystemId::SOL, body: 0 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub temperature: f64,
    pub atmosphere: f64,
    pub radiation: f64,
    pub roughness: f64,
    pub water: f64,
    pub vegetation: f64,
    pub sky_color: i32,
    pub ground_color: i32,
}

impl Environment {
    pub fn description(&self) -> &'static str {
        if self.temperature < 0.15 {
            "frozen"
        } else if self.temperature > 0.9 {
            "volcanic"
        } else if self.radiation > 0.7 {
            "radioactive"
        } else if self.water > 0.8 {
            "ocean"
        } else if self.atmosphere < 0.15 {
            "barren"
        } else if self.water < 0.2 {
            "desert"
        } else if self.vegetation > 0.65 {
            "lush"
        } else {
            "temperate"
        }
    }
}

/// `parent_body`: >= 0 moon of that body, -1 planet, -2 asteroid, -3 star.
#[derive(Clone, Debug, PartialEq)]
pub struct Planet {
    pub id: PlanetId,
    pub name: String,
    pub seed: i64,
    pub parent_body: i32,
    pub center: Vec3,
    pub radius: f64,
    pub environment: Environment,
}

impl Planet {
    pub fn native_center(&self) -> Vec3 {
        Vec3::new(
            self.center.x + self.id.system.x as f64 * SECTOR_SIZE,
            self.center.y,
            self.center.z + self.id.system.z as f64 * SECTOR_SIZE,
        )
    }

    pub fn bottom(&self) -> i32 {
        (self.native_center().y - self.radius).floor() as i32
    }
}

#[derive(Clone, Debug)]
pub struct StarSystem {
    pub id: SystemId,
    pub seed: i64,
    pub star_color: i32,
    pub bodies: Vec<Planet>,
    pub asteroid_seed: i64,
}

pub struct UniverseCatalog {
    seed: i64,
    cache: HashMap<SystemId, StarSystem>,
    order: VecDeque<SystemId>,
}

impl UniverseCatalog {
    pub fn new(seed: i64) -> Self {
        Self { seed, cache: HashMap::new(), order: VecDeque::new() }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    pub fn cached_systems(&self) -> usize {
        self.cache.len()
    }

    /// Compact (current) layout, memoised in a bounded LRU of descriptors.
    pub fn system(&mut self, id: SystemId) -> &StarSystem {
        if self.cache.contains_key(&id) {
            self.order.retain(|k| *k != id);
        } else {
            self.cache.insert(id, generate(self.seed, id, true));
            if self.cache.len() > CACHE_LIMIT {
                if let Some(old) = self.order.pop_front() {
                    self.cache.remove(&old);
                }
            }
        }
        self.order.push_back(id);
        &self.cache[&id]
    }

    pub fn earth(&mut self) -> Planet {
        self.system(SystemId::SOL).bodies[0].clone()
    }

    /// The pre-compaction layout, kept for reading older saves.
    pub fn legacy_system(&self, id: SystemId) -> StarSystem {
        generate(self.seed, id, false)
    }
}

const FIRST: [&str; 12] = ["Ner", "Vel", "Ast", "Lum", "Cal", "Iri", "Tal", "Vey", "Or", "Eos", "Mir", "Zel"];
const LAST: [&str; 8] = ["ara", "ion", "ea", "os", "is", "en", "ora", "eus"];

fn name(seed: i64) -> String {
    let u = seed as u64;
    let a = ((u >> 32) as i32).rem_euclid(FIRST.len() as i32) as usize;
    let b = ((u >> 16) as i32).rem_euclid(LAST.len() as i32) as usize;
    format!("{}{}-{}", FIRST[a], LAST[b], to_base36((seed & 4095) as u64).to_uppercase())
}

fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn environment(r: &mut JavaRandom) -> Environment {
    let temperature = r.next_double();
    let atmosphere = r.next_double();
    let radiation = r.next_double().powf(4.0);
    let water = r.next_double() * atmosphere * (1.0 - (temperature - 0.7).max(0.0));
    let vegetation = r.next_double() * atmosphere * (1.0 - radiation);
    let roughness = r.next_double();
    Environment {
        temperature,
        atmosphere,
        radiation,
        roughness,
        water,
        vegetation,
        sky_color: r.next_int_bound(0x1000000),
        ground_color: r.next_int_bound(0x1000000),
    }
}

fn clear_position(mut candidate: Vec3, radius: f64, bodies: &[Planet]) -> Vec3 {
    let distance = candidate.x.hypot(candidate.z);
    let mut angle = candidate.z.atan2(candidate.x);
    for _ in 0..96 {
        let check = candidate;
        if check.x.abs() + radius < 8000.0
            && check.z.abs() + radius < 8000.0
            && bodies.iter().all(|p| {
                !((p.center.x - check.x).abs() < p.radius + radius + 64.0
                    && (p.center.z - check.z).abs() < p.radius + radius + 64.0)
            })
        {
            return candidate;
        }
        angle += 2.399963229728653;
        candidate = Vec3::new(angle.cos() * distance, -64.0, angle.sin() * distance);
    }
    panic!("Cannot place non-overlapping compact planet");
}

fn generate(universe_seed: i64, id: SystemId, compact: bool) -> StarSystem {
    let system_seed = id.seed(universe_seed);
    let mut random = JavaRandom::new(system_seed);
    let mut bodies: Vec<Planet> = Vec::new();
    let planet_count = 3 + random.next_int_bound(6);
    let sol = id == SystemId::SOL;

    for orbit in 0..planet_count {
        let index = bodies.len() as i64;
        let mut body_seed = mix(system_seed ^ (index + 1).wrapping_mul(0x9e3779b97f4a7c15u64 as i64));
        let mut br = JavaRandom::new(body_seed);
        let angle = br.next_double() * PI * 2.0;
        let mut distance = 100_000.0 + orbit as f64 * 150_000.0 + br.next_double() * 40_000.0;
        let mut center = Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance);
        let radius_roll = br.next_double();
        let mut radius = if compact { 256.0 + radius_roll * 128.0 } else { 4096.0 + radius_roll * 8192.0 };
        if compact {
            distance = 1600.0 + orbit as f64 * 650.0 + (distance - 100_000.0 - orbit as f64 * 150_000.0) / 400.0;
            center = Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance);
        }
        let mut env = environment(&mut br);
        let earth = sol && orbit == 0;
        if earth {
            body_seed = universe_seed;
            radius = if compact { EARTH_RADIUS } else { 32768.0 };
            center = Vec3::new(0.0, -radius, 0.0);
            env = Environment {
                temperature: 0.5, atmosphere: 1.0, radiation: 0.0, roughness: 0.5,
                water: 0.6, vegetation: 0.7, sky_color: 0x78a7ff, ground_color: 0x79c05a,
            };
        }
        if !earth {
            if sol && orbit == 1 {
                center = Vec3::new(if compact { 4096.0 } else { 50000.0 }, 0.0, 0.0);
            }
            center = Vec3::new(center.x, -radius, center.z);
        }
        if compact {
            if !(sol && orbit <= 1) {
                center = clear_position(center, radius, &bodies);
            }
            center = Vec3::new(center.x, 80.0 - radius, center.z);
        }
        bodies.push(Planet {
            id: PlanetId { system: id, body: index as u8 },
            name: if earth { "Earth".into() } else { name(body_seed) },
            seed: body_seed,
            parent_body: -1,
            center,
            radius,
            environment: env,
        });

        let moons = br.next_int_bound(3);
        for moon in 0..moons {
            let moon_index = bodies.len() as i64;
            let moon_seed = mix(body_seed ^ (moon as i64 + 1).wrapping_mul(0x632be59bd9b4e019));
            let mut mr = JavaRandom::new(moon_seed);
            let moon_roll = mr.next_double();
            let moon_radius = if compact { 64.0 + moon_roll * 64.0 } else { 1024.0 + moon_roll * 2048.0 };
            let mut mc = Vec3::new(
                center.x + radius * (3 + moon) as f64,
                -moon_radius,
                center.z + radius * (moon + 1) as f64,
            );
            if compact {
                mc = clear_position(mc, moon_radius, &bodies);
                mc = Vec3::new(mc.x, 80.0 - moon_radius, mc.z);
            }
            let env = environment(&mut mr);
            bodies.push(Planet {
                id: PlanetId { system: id, body: moon_index as u8 },
                name: name(moon_seed),
                seed: moon_seed,
                parent_body: index as i32,
                center: mc,
                radius: moon_radius,
                environment: env,
            });
        }
    }

    let star_color = 0xffc080 | random.next_int_bound(128);
    let asteroid_seed = mix(system_seed ^ 0x41535445524f4944);
    if compact {
        let mut field = JavaRandom::new(asteroid_seed);
        for asteroid in 0..12 {
            let index = bodies.len() as i64;
            let angle = field.next_double() * PI * 2.0;
            let distance = 6000.0 + field.next_double() * 800.0;
            let radius = 12.0 + field.next_double() * 12.0;
            let mut center = clear_position(
                Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance), radius, &bodies);
            center = Vec3::new(center.x, (220 + field.next_int_bound(60)) as f64, center.z);
            bodies.push(Planet {
                id: PlanetId { system: id, body: index as u8 },
                name: format!("Asteroid {}", asteroid + 1),
                seed: mix(asteroid_seed ^ index),
                parent_body: -2,
                center,
                radius,
                environment: Environment {
                    temperature: 0.3, atmosphere: 0.0, radiation: 0.0, roughness: 0.8,
                    water: 0.0, vegetation: 0.0, sky_color: 0x101420, ground_color: 0x77736c,
                },
            });
        }
        let index = bodies.len() as u8;
        bodies.push(Planet {
            id: PlanetId { system: id, body: index },
            name: "Star".into(),
            seed: mix(system_seed ^ 0x53544152),
            parent_body: -3,
            center: Vec3::new(0.0, 1200.0, 0.0),
            radius: 96.0,
            environment: Environment {
                temperature: 1.0, atmosphere: 0.0, radiation: 1.0, roughness: 0.0,
                water: 0.0, vegetation: 0.0, sky_color: star_color, ground_color: star_color,
            },
        });
    }
    StarSystem { id, seed: system_seed, star_color, bodies, asteroid_seed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sol_layout_matches_documented_facts() {
        let mut c = UniverseCatalog::new(12345);
        let earth = c.earth();
        assert_eq!(earth.name, "Earth");
        assert_eq!(earth.radius, 512.0);
        assert_eq!(earth.center, Vec3::new(0.0, 80.0 - 512.0, 0.0));
        let sol = c.system(SystemId::SOL);
        let second = sol.bodies.iter().find(|b| b.parent_body == -1 && b.id != PlanetId::EARTH).unwrap();
        assert_eq!(second.center.x, 4096.0);
        assert_eq!(second.center.z, 0.0);
        assert_eq!(sol.bodies.last().unwrap().name, "Star");
        assert_eq!(sol.bodies.iter().filter(|b| b.parent_body == -2).count(), 12);
    }

    #[test]
    fn generation_is_deterministic_and_cache_bounded() {
        let mut c = UniverseCatalog::new(-99);
        let a = c.system(SystemId { x: 3, y: 0, z: -7 }).clone();
        for x in 0..300 {
            c.system(SystemId { x, y: 0, z: 1 });
        }
        assert!(c.cached_systems() <= CACHE_LIMIT);
        let b = UniverseCatalog::new(-99).system(SystemId { x: 3, y: 0, z: -7 }).clone();
        assert_eq!(a.bodies, b.bodies);
    }

    #[test]
    fn system_cell_lookup() {
        assert_eq!(SystemId::at(0.0, 0.0), SystemId::SOL);
        assert_eq!(SystemId::at(8192.0, 0.0).x, 1);
        assert_eq!(SystemId::at(-8193.0, 0.0).x, -1);
    }
}
