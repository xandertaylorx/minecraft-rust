//! Router density functions and climate samples against the real Java `RandomState`.
use worldgen::density::{Ctx, Jar, RandomState};

const JAR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/client.jar");

fn near(a: f64, b: f64, ctx: &str) {
    assert!((a - b).abs() <= 1e-9 * (1.0 + b.abs()), "{a} vs {b} ({ctx})");
}

#[test]
fn router_matches_java() {
    if !std::path::Path::new(JAR).exists() { eprintln!("skipped: no client.jar"); return; }
    let golden = include_str!("noise_golden.txt");
    let mut states: std::collections::HashMap<i64, (RandomState, worldgen::density::Router)> = Default::default();
    let (mut rows, mut climate) = (0, 0);
    for line in golden.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let seed: i64 = f[1].parse().unwrap();
        let (x, y, z): (i32, i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap(), f[4].parse().unwrap());
        let (rs, r) = states.entry(seed).or_insert_with(|| {
            let rs = RandomState::new(Jar::open(JAR), seed);
            let r = rs.router("overworld");
            (RandomState::new(Jar::open(JAR), seed), r)
        });
        let _ = rs;
        let c = Ctx { x, y, z };
        if f[0] == "R" {
            let fns = [&r.continents, &r.erosion, &r.depth, &r.ridges, &r.temperature, &r.vegetation, &r.final_density,
                &r.preliminary_surface_level, &r.barrier, &r.floodedness, &r.spread, &r.lava, &r.vein_toggle, &r.vein_ridged, &r.vein_gap];
            let names = ["continents", "erosion", "depth", "ridges", "temperature", "vegetation", "final_density", "prelim", "barrier",
                "floodedness", "spread", "lava", "vein_toggle", "vein_ridged", "vein_gap"];
            for (i, d) in fns.iter().enumerate() {
                near(d.compute(&c), f[5 + i].parse().unwrap(), &format!("{} at {line}", names[i]));
            }
            rows += 1;
        } else {
            // Climate sample at quart coordinates.
            let ctx = Ctx { x: (x >> 2) << 2, y: (y >> 2) << 2, z: (z >> 2) << 2 };
            let q = |d: &std::sync::Arc<worldgen::density::Df>| (d.compute(&ctx) as f32 * 10000.0f32) as i64;
            let got = [q(&r.temperature), q(&r.vegetation), q(&r.continents), q(&r.erosion), q(&r.depth), q(&r.ridges)];
            let want: Vec<i64> = f[5..11].iter().map(|v| v.parse().unwrap()).collect();
            assert_eq!(got.to_vec(), want, "climate at {line}");
            climate += 1;
        }
    }
    assert!(rows >= 24 && climate >= 24, "{rows} {climate}");
}
