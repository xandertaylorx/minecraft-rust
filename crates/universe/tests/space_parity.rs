//! Atmosphere, clouds, layout, flight physics, pulse navigation, stream planner and
//! positions against the real Java classes (golden from tests/GoldSpace.java.txt).
use universe::flight::*;
use universe::space::*;
use universe::vec3::Vec3;
use universe::{SystemId, UniverseCatalog};

fn near(a: f64, b: &str, ctx: &str) {
    let b: f64 = b.parse().unwrap();
    assert!((a - b).abs() <= 1e-6 * (1.0 + b.abs()), "{a} vs {b} in {ctx}");
}

fn vec_from(s: &str) -> Vec<f64> {
    s.trim_matches(|c| c == '(' || c == ')').split(", ").map(|v| v.parse().unwrap()).collect()
}

#[test]
fn space_matches_java_golden() {
    let golden = include_str!("space_golden.txt");
    let mut layout = SurfaceLayout::new(12345);
    let mut cat = UniverseCatalog::new(12345);
    let sol = cat.system(SystemId::SOL).clone();
    let cams = [(4096.0, 100.0, 0.0), (4096.0, 700.0, 0.0), (5000.0, 300.0, 800.0), (4096.0, -2000.0, 0.0), (4600.0, 235.0, 10.0), (0.0, 50.0, 0.0)];
    let (mut v, mut yaw, mut pitch) = (Vec3::ZERO, 10f32, 0f32);
    let mut atm_count = 0;
    let mut cam_idx = 0;
    let mut fly = 0;
    let mut stream = 0;
    let streams = [((100.0, 50.0), (5.0, 3.0)), ((-4000.5, 70.0), (-9.0, 0.2)), ((0.0, 0.0), (0.1, 0.0)), ((33.0, 33.0), (0.0, -12.0))];
    let mut acq = 0;
    for line in golden.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "SURF" => {
                let (x, z): (f64, f64) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let s = layout.surface_at(x, z);
                let got = s.map(|p| format!("{}/{}", p.id.system, p.id.body)).unwrap_or("none".into());
                assert_eq!(got, f[3], "{line}");
                assert_eq!(layout.nearby(x, z).len().to_string(), f[4], "{line}");
                assert_eq!(layout.is_earth(x, z).to_string(), f[5], "{line}");
            }
            "SECOND" => assert_eq!(layout.second_planet().id.body.to_string(), f[1]),
            "ATM" => {
                let b = &sol.bodies[f[1].parse::<usize>().unwrap()];
                let (cx, cy, cz) = cams[cam_idx % cams.len()];
                cam_idx += 1;
                let cam = Vec3::new(cx, cy, cz);
                let d = atmosphere_distance(b, cam, 120.0);
                near(d, f[5], line);
                near(atmosphere_density(b, d), f[6], line);
                near(cloud_opacity(b, cam) as f64, f[7], line);
                atm_count += 1;
            }
            "CLOUD" => {
                let b = &sol.bodies[f[1].parse::<usize>().unwrap()];
                assert_eq!(cloud_height(b), f[2].parse::<f32>().unwrap());
            }
            "CELLS" => {
                let b = &sol.bodies[f[1].parse::<usize>().unwrap()];
                let c = b.native_center();
                let got: String = (0..64i64).map(|i| {
                    let (ox, oz) = ((i * 37 % 200 - 100) as f64, (i * 53 % 200 - 100) as f64);
                    if cloud_occupied(b, c.x + ox * b.radius / 100.0, c.z + oz * b.radius / 100.0) { '1' } else { '0' }
                }).collect();
                assert_eq!(got, f[2], "{line}");
            }
            "FLY" => {
                let i = fly * 0 + f[1].parse::<i32>().unwrap();
                // Re-run the whole scenario once at the first FLY line.
                if fly == 0 {
                    for k in 0..400 {
                        let inp = Input { forward: (k / 17) % 2 == 0, backward: (k / 31) % 5 == 0, left: (k / 13) % 7 == 0,
                            right: (k / 11) % 9 == 0, jump: (k / 23) % 4 == 0, shift: (k / 29) % 6 == 0, sprint: (k / 19) % 3 != 0 };
                        let (py, pp) = (((k * 3) % 360 - 180) as f32, ((k * 5) % 170 - 85) as f32);
                        let alt = 100.0 + k as f64 * 15.0;
                        let r = step(v, yaw, pitch, py, pp, inp, alt, (k / 97) % 4 != 3, (alt - 140.0 - (k % 50) as f64).max(0.0));
                        v = r.velocity; yaw = r.yaw; pitch = r.pitch;
                        if k % 5 == 0 {
                            let g = golden.lines().find(|l| l.starts_with(&format!("FLY {k} "))).unwrap();
                            let g: Vec<&str> = g.split(' ').collect();
                            near(v.x, g[2], g[0]); near(v.y, g[3], g[0]); near(v.z, g[4], g[0]);
                            assert_eq!(yaw, g[5].parse::<f32>().unwrap(), "yaw at {k}");
                            assert_eq!(pitch, g[6].parse::<f32>().unwrap(), "pitch at {k}");
                            assert_eq!(r.pulse.to_string(), g[7], "pulse at {k}");
                        }
                    }
                }
                let _ = i;
                fly += 1;
            }
            "ACQ" => {
                let yawd: f64 = f[1].parse().unwrap();
                let pos = Vec3::new(-3000.0, 600.0, 20.0);
                let look = Vec3::direction_from_rotation(0.0, (-90.0 + yawd) as f32);
                let t = acquire(pos, look, &sol.bodies);
                assert_eq!(t.map(|p| p.id.body as i32).unwrap_or(-1).to_string(), f[2], "{line}");
                near(pulse_speed(5000.0 + yawd * 100.0, 300.0), f[3], line);
                if let Some(t) = t {
                    let d = pulse_direction(pos, t);
                    let g = vec_from(&f[4..].join(" "));
                    near(d.x, &g[0].to_string(), line); near(d.y, &g[1].to_string(), line); near(d.z, &g[2].to_string(), line);
                }
                acq += 1;
            }
            "STREAM" => {
                let ((px, pz), (vx, vz)) = streams[(stream / 2) % 4];
                let vd: i32 = f[1].parse().unwrap();
                let got: String = predict_chunks(Vec3::new(px, 0.0, pz), Vec3::new(vx, 0.0, vz), vd)
                    .iter().map(|(x, z)| format!("{x},{z};")).collect();
                assert_eq!(got, f.get(2).copied().unwrap_or(""), "{line}");
                stream += 1;
            }
            "POS" => {
                let p = UniversePosition::new(SystemId { x: 2, y: 0, z: -1 }, Vec3::new(9000.0, -20000.0, 5.0));
                assert_eq!(p.sector.to_string(), f[1]);
                near(p.local.x, f[2], line); near(p.local.y, f[3], line); near(p.local.z, f[4], line);
                let r = p.relative_to(&UniversePosition::new(SystemId::SOL, Vec3::new(1.0, 2.0, 3.0)));
                let g = golden.lines().find(|l| l.starts_with("REL ")).unwrap().split(' ').collect::<Vec<_>>();
                near(r.x, g[1], "rel"); near(r.y, g[2], "rel"); near(r.z, g[3], "rel");
            }
            _ => {}
        }
    }
    assert!(atm_count >= 60 && fly == 80 && stream == 8 && acq == 6, "{atm_count} {fly} {stream} {acq}");
}
