//! Rust port of the procedural universe (`net.minecraft.stripped.universe`).
//! Generation is bit-compatible with the Java version: same seeds give the same systems.

pub mod catalog;
pub mod flight;
pub mod java_random;
pub mod mth;
pub mod space;
pub mod terrain;
pub mod vec3;

pub use catalog::{Environment, Planet, PlanetId, StarSystem, SystemId, UniverseCatalog};
