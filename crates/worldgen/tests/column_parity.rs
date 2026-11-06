//! Base columns (stone, fluids via the aquifer, ore veins) vs real `NoiseBasedChunkGenerator.getBaseColumn`.
use worldgen::chunk::{OverworldGen, HEIGHT, MIN_Y};
use worldgen::density::{Jar, RandomState};

const JAR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/client.jar");

#[test]
fn columns_match_java() {
    if !std::path::Path::new(JAR).exists() { eprintln!("skipped: no client.jar"); return; }
    let rs = RandomState::new(Jar::open(JAR), 12345);
    let g = OverworldGen::new(&rs, 12345);
    let (mut total, mut bad) = (0, 0);
    for line in include_str!("column_golden.txt").lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let (x, z): (i32, i32) = (f[0].parse().unwrap(), f[1].parse().unwrap());
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        let blocks = g.base_blocks(cx, cz);
        let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
        let want = f[2].as_bytes();
        for y in 0..320.min(HEIGHT) {
            let name = g.palette[blocks[((y * 16 + lz) * 16 + lx) as usize] as usize].as_str();
            let got = match name { "air" => b'A', "water" => b'W', "lava" => b'L', _ => b'S' };
            total += 1;
            if got != want[y as usize] {
                bad += 1;
                if bad < 8 { eprintln!("mismatch at {x},{} {z}: got {} want {}", y + MIN_Y, got as char, want[y as usize] as char); }
            }
        }
    }
    assert_eq!(bad, 0, "{bad} of {total} voxels differ");
}
