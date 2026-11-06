mod assets;
mod blocks;
mod ai;
mod entity;
mod entity_render;
mod game;
mod gameplay;
mod gui;
mod hand;
mod items;
mod mesh;
mod particles;
mod model;
mod player;
mod render;
mod ui;
mod world;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::DVec3;
use image::{Rgba, RgbaImage};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};
use worldgen::chunk::OverworldGen;
use worldgen::density::{Jar, RandomState};

use assets::Assets;
use blocks::{def, BLOCKS};
use game::{Game, Mode, Screen};
use gui::{build_gui_atlas, Gui, GuiAtlas};
use mesh::{mesh_chunk, BiomeColors, Tiles};
use render::{Atlas, Renderer, SkyState, WorldState};
use ui::{Action, Click, FrameInfo};
use world::generate;

pub const SEED: i64 = 4_815_162_342;

fn jar_path() -> &'static str {
    ["assets/client.jar", "assets/minecraft-1.21.11-resources.jar"]
        .into_iter().find(|p| std::path::Path::new(p).exists()).unwrap_or("assets/client.jar")
}

// ---------------------------------------------------------------- assets

fn build_atlas(a: &mut Assets) -> Atlas {
    let mut names: Vec<&str> = Vec::new();
    for b in BLOCKS.iter().skip(1) {
        for n in [Some(b.top), Some(b.side), Some(b.bottom), b.overlay].into_iter().flatten() {
            if !n.is_empty() && !names.contains(&n) { names.push(n); }
        }
    }
    names.push("fire_1");
    for i in 0..10 { names.push(Box::leak(format!("destroy_stage_{i}").into_boxed_str())); }
    assert!(names.len() <= 256);
    let mut image = RgbaImage::new(256, 256);
    let mut map = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        let tile = a.block_tile(n);
        image::imageops::replace(&mut image, &tile, (i as i64 % 16) * 16, (i as i64 / 16) * 16);
        map.insert(n.to_string(), i as u32);
    }
    Atlas { image, names: map }
}

fn colormap(a: &mut Assets, name: &str, temp: f32, down: f32) -> [f32; 3] {
    let img = a.image(&format!("textures/colormap/{name}.png")).expect("colormap");
    let x = ((1.0 - temp) * 255.0) as u32;
    let y = ((1.0 - temp * down) * 255.0) as u32;
    let p = img.get_pixel(x.min(255), y.min(255));
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
}

/// Isometric inventory icon, drawn like vanilla's GUI block rendering.
pub fn block_icon(atlas: &Atlas, id: u16, grass: [f32; 3]) -> RgbaImage {
    const S: u32 = 64;
    let d = def(id);
    let tile = |name: &str| -> RgbaImage {
        let t = atlas.names[name];
        image::imageops::crop_imm(&atlas.image, (t % 16) * 16, (t / 16) * 16, 16, 16).to_image()
    };
    let (top, side) = (tile(d.top), tile(d.side));
    let overlay = d.overlay.map(tile);
    let mut out = RgbaImage::new(S, S);
    let k = 16.0 / S as f32;
    for py in 0..S {
        for px in 0..S {
            let (x, y) = ((px as f32 + 0.5) * k, (py as f32 + 0.5) * k);
            let (u, v) = (((x - 8.0) / 8.0 + y / 4.0) / 2.0, (y / 4.0 - (x - 8.0) / 8.0) / 2.0);
            let (s, t) = (x / 8.0, (y - 4.0 - 4.0 * (x / 8.0)) / 8.0);
            let (s2, t2) = ((x - 8.0) / 8.0, (y - 8.0 + 4.0 * ((x - 8.0) / 8.0)) / 8.0);
            let sample = |img: &RgbaImage, a: f32, b: f32| *img.get_pixel(((a * 16.0) as u32).min(15), ((b * 16.0) as u32).min(15));
            let (mut c, shade, tinted) = if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                (sample(&top, u, v), 1.0f32, d.tint == blocks::Tint::Grass)
            } else if (0.0..1.0).contains(&s) && (0.0..1.0).contains(&t) {
                (sample(&side, s, t), 0.8, false)
            } else if (0.0..1.0).contains(&s2) && (0.0..1.0).contains(&t2) {
                (sample(&side, s2, t2), 0.6, false)
            } else { continue };
            if tinted { for i in 0..3 { c[i] = (c[i] as f32 * grass[i]) as u8; } }
            if let (Some(ov), false, true) = (&overlay, tinted, shade < 1.0) {
                let o = if shade == 0.8 { sample(ov, s, t) } else { sample(ov, s2, t2) };
                if o[3] > 0 { for i in 0..3 { c[i] = (o[i] as f32 * grass[i]) as u8; } }
            }
            for i in 0..3 { c[i] = (c[i] as f32 * shade) as u8; }
            out.put_pixel(px, py, Rgba([c[0], c[1], c[2], 255]));
        }
    }
    out
}

fn sample_colormap(img: &RgbaImage, temp: f32, down: f32) -> [f32; 3] {
    let t = temp.clamp(0.0, 1.0);
    let d = down.clamp(0.0, 1.0) * t;
    let (x, y) = (((1.0 - t) * 255.0) as u32, ((1.0 - d) * 255.0) as u32);
    let p = img.get_pixel(x.min(255), y.min(255));
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
}

fn rgb(c: u32) -> [f32; 3] { [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0] }

fn biome_colors(a: &mut Assets, wgen: &OverworldGen) -> BiomeColors {
    let grass_map = a.image("textures/colormap/grass.png").expect("grass colormap");
    let foliage_map = a.image("textures/colormap/foliage.png").expect("foliage colormap");
    let mut out = BiomeColors { grass: vec![], foliage: vec![], water: vec![] };
    for b in &wgen.biomes.biomes {
        let mut g = b.grass_color.map(rgb).unwrap_or_else(|| sample_colormap(&grass_map, b.temperature, b.downfall));
        if b.grass_modifier == "dark_forest" && b.grass_color.is_none() {
            let c = sample_colormap(&grass_map, b.temperature, b.downfall);
            let ci = ((c[0] * 255.0) as u32) << 16 | ((c[1] * 255.0) as u32) << 8 | (c[2] * 255.0) as u32;
            g = rgb(((ci & 0xFEFEFE) + 0x28340A) >> 1);
        } else if b.grass_modifier == "swamp" {
            g = rgb(0x6A7039);
        }
        out.grass.push(g);
        out.foliage.push(b.foliage_color.map(rgb).unwrap_or_else(|| sample_colormap(&foliage_map, b.temperature, b.downfall)));
        out.water.push(rgb(b.water_color));
    }
    out
}

fn make_generator() -> (Arc<OverworldGen>, Arc<Vec<u16>>) {
    let rs = RandomState::new(Jar::open(jar_path()), SEED);
    let wgen = OverworldGen::new(&rs, SEED);
    let map: Vec<u16> = wgen.palette.iter().map(|n| {
        BLOCKS.iter().position(|b| b.name == n).unwrap_or_else(|| { eprintln!("no block '{n}' in the game table; using stone"); 1 }) as u16
    }).collect();
    (Arc::new(wgen), Arc::new(map))
}


fn setup_assets() -> (Atlas, GuiAtlas, Assets, entity_render::EntityAtlas) {
    let mut a = Assets::open(jar_path());
    let atlas = build_atlas(&mut a);
    let grass = colormap(&mut a, "grass", 0.8, 0.4);
    let gui = build_gui_atlas(&mut a, &atlas, grass);
    let ent = entity_render::build_entity_atlas(&mut a, &atlas.image);
    (atlas, gui, a, ent)
}

/// Underwater: dark-blue fog with short, growing visibility, no sun/clouds (FogRenderer water fog).
fn apply_underwater(game: &Game, sky: &mut SkyState, r: &mut Renderer, day: f32) {
    if game.in_world && game.player.eye_in_water {
        let f = 0.3 + 0.7 * day;
        let c = [0.02 * f, 0.02 * f + 0.03, 0.2 * f + 0.05];
        sky.zenith = c;
        sky.horizon = c;
        sky.night = 0.0;
        sky.sun_dir = [0.0, -1.0, 0.0];
        sky.cloud_color = [0.0; 3];
        let vision = game.player.water_ticks as f32 / 600.0;
        r.fog_override = Some((-8.0, 24.0 + 72.0 * vision));
    } else { r.fog_override = None; }
}

fn load_anims(a: &mut Assets, names: &HashMap<String, u32>) -> Vec<assets::TileAnim> {
    let mut list: Vec<_> = names.iter().filter(|(n, _)| !n.starts_with("destroy_stage")).filter_map(|(n, t)| assets::TileAnim::load(a, n, *t)).collect();
    list.sort_by_key(|a| a.tile);
    list
}

fn crack_tile(game: &Game, atlas_names: &HashMap<String, u32>) -> Option<([i32; 3], u32)> {
    let m = game.mining.as_ref()?;
    let stage = ((m.progress * 10.0) as u32).min(9);
    Some((m.pos, *atlas_names.get(&format!("destroy_stage_{stage}"))?))
}

// ---------------------------------------------------------------- headless screenshot

fn screenshot_mode(args: &[String]) {
    let get = |k: &str| args.iter().position(|a| a == k);
    let val = |k: &str| get(k).map(|i| args[i + 1].clone());
    let out = val("--shot").unwrap();
    let nums = |k: &str, n: usize| -> Option<Vec<f64>> { get(k).map(|i| (0..n).map(|j| args[i + 1 + j].parse().unwrap()).collect()) };
    let size = nums("--size", 2).unwrap_or(vec![960.0, 540.0]);
    let (w, h) = (size[0] as u32, size[1] as u32);
    let radius = nums("--radius", 1).map(|v| v[0] as i32).unwrap_or(6);
    let (atlas, gui_atlas, mut assets, ent_atlas) = setup_assets();
    let (wgen, pal_map) = make_generator();
    let colors = Arc::new(biome_colors(&mut assets, &wgen));
    let names = Arc::new(atlas.names.clone());
    let mut game = Game::new(wgen, pal_map, names.clone(), colors);
    game.settings.render_distance = radius;
    if val("--mode").as_deref() == Some("survival") { game.mode = Mode::Survival; }
    game.in_world = true;
    game.spawned = true;
    game.screen = Screen::Playing;
    game.player.flying = game.mode == Mode::Creative;
    game.player.pos = DVec3::new(8.5, 100.0, 8.5);
    if let Some(p) = nums("--pos", 3) { game.player.pos = DVec3::new(p[0], p[1], p[2]); }
    if args.iter().any(|a| a == "--ground") {
        // generate chunk (0,0) neighbourhood first so we can stand on the surface
        let (px, pz) = (game.player.pos.x.floor() as i32, game.player.pos.z.floor() as i32);
        let (cx, cz) = (px.div_euclid(16), pz.div_euclid(16));
        let (b, bi) = generate(&game.wgen, &game.pal_map, cx, cz);
        game.world.chunks.insert((cx, cz), world::Chunk { blocks: b, biomes: bi });
        let mut y = 300; while y > -60 && !blocks::def(game.world.get(px, y, pz)).solid { y -= 1; }
        game.player.pos.y = y as f64 + 1.0;
    }
    let look = nums("--look", 2);
    game.player.yaw = look.as_ref().map(|l| l[0] as f32).unwrap_or(0.0);
    game.player.pitch = look.as_ref().map(|l| l[1] as f32).unwrap_or(20.0);
    if let Some(t) = nums("--time", 1) { game.time_ticks = t[0]; }
    match val("--screen").as_deref() {
        Some("title") => { game.screen = Screen::Title; game.in_world = false; }
        Some("pause") => game.screen = Screen::Pause,
        Some("options") => game.screen = Screen::Options,
        Some("creative") => game.screen = Screen::Creative,
        Some("inventory") => { game.screen = Screen::Inventory; game.mode = Mode::Survival; }
        Some("chat") => { game.screen = Screen::Chat; game.chat_input = "/gamemode creative".into(); }
        _ => {}
    }
    if args.iter().any(|a| a == "--f3") { game.show_debug = true; game.debug_light = (15, 0); }
    if args.iter().any(|a| a == "--chat") {
        for l in ["§eWelcome to Minecraft (Rust)!", "Set the time to 13000", "<Steve> hello world"] { game.say(l); }
    }
    let (pcx, pcz) = game.player_chunk();
    let want_mobs = args.iter().any(|a| a == "--mobs");
    let keys: Vec<(i32, i32)> = (-(radius + 1)..=(radius + 1)).flat_map(|z| (-(radius + 1)..=(radius + 1)).map(move |x| (pcx + x, pcz + z))).collect();
    use rayon::prelude::*;
    let (g2, m2) = (game.wgen.clone(), game.pal_map.clone());
    let gens: Vec<_> = keys.par_iter().map(|k| (*k, generate(&g2, &m2, k.0, k.1))).collect();
    for (k, (b, bi)) in gens { game.world.chunks.insert(k, world::Chunk { blocks: b, biomes: bi }); }

    if args.iter().any(|a| a == "--arena") {
        let grass = blocks::id_of("grass_block");
        for x in -2..20 { for z in -4..30 { for y in 101..112 { game.world.set(x, y, z, 0); } game.world.set(x, 100, z, grass); } }
        game.player.pos = DVec3::new(8.5, 101.0, 8.5);
    }
    if args.iter().any(|a| a == "--fire") {
        for (i, x) in [5, 7, 9, 11].iter().enumerate() { game.world.set(*x, 101, 12, blocks::id_of("fire")); game.world.set(*x, 100, 12, if i == 1 { blocks::id_of("oak_planks") } else { blocks::id_of("grass_block") }); }
        game.world.set(13, 101, 12, blocks::id_of("tnt"));
        game.world.set(13, 101, 14, blocks::id_of("water"));
    }
    if args.iter().any(|a| a == "--dive") {
        let water = blocks::id_of("water");
        'find: for r in 0..(radius * 16) {
            for dz in -r..=r { for dx in -r..=r {
                if dx.abs().max(dz.abs()) != r { continue; }
                let (x, z) = (8 + dx, 8 + dz);
                if game.world.get(x, 62, z) == water && game.world.get(x, 57, z) == water {
                    game.player.pos = DVec3::new(x as f64 + 0.5, 58.0, z as f64 + 0.5);
                    game.player.eye_in_water = true;
                    game.player.in_water = true;
                    game.player.water_ticks = 300;
                    println!("dive at {x} {z}");
                    break 'find;
                }
            } }
        }
    }
    if args.iter().any(|a| a == "--boom") {
        for (i, x) in [5, 8, 11].iter().enumerate() { game.world.set(*x, 101, 14, blocks::id_of("tnt")); let _ = i; }
        game.prime_tnt(DVec3::new(8.5, 101.0, 12.5), nums("--fuse", 1).map(|v| v[0] as i32).unwrap_or(40));
        let after = nums("--after", 1).map(|v| v[0] as i32).unwrap_or(0);
        game.mode = Mode::Creative;
        for _ in 0..after { game.tick(0.05); }
        println!("tnts={} particles={} emitters={}", game.ents.tnts.len(), game.particles.list.len(), game.particles.emitters.len());
    }
    if args.iter().any(|a| a == "--sim") {
        // Headless AI/combat simulation: flat arena, one mob of each hostile kind attacking a stationary survival player.
        game.mode = Mode::Survival;
        game.player.flying = false;
        let grass = blocks::id_of("grass_block");
        for x in -30..50 { for z in -30..50 { for y in 101..115 { game.world.set(x, y, z, 0); } game.world.set(x, 100, z, grass); } }
        game.player.pos = DVec3::new(8.5, 101.0, 8.5);
        game.player.prev_pos = game.player.pos;
        game.time_ticks = 18000.0;
        game.spawn = DVec3::new(8.5, 101.0, 8.5);
        let which = val("--sim").unwrap_or_default();
        let kind = match which.as_str() { "skeleton" => entity::Kind::Skeleton, "creeper" => entity::Kind::Creeper, "pig" => entity::Kind::Pig, _ => entity::Kind::Zombie };
        game.spawn_mob(kind, DVec3::new(20.5, 101.0, 8.5));
        if args.iter().any(|a| a == "--sword") { game.slots[0] = Some(game::Stack::new(items::item_id("iron_sword").unwrap(), 1)); game.selected = 0; }
        if args.iter().any(|a| a == "--attack") { if let Some(m) = game.ents.mobs.last_mut() { m.pos = DVec3::new(10.0, 101.0, 8.5); m.prev_pos = m.pos; } }
        for t in 0..600 {
            if args.iter().any(|a| a == "--attack") && t % 13 == 0 {
                game.player.yaw = 90.0; // face +x? ensure target by direct attack
                if let Some(id) = game.ents.mobs.iter().find(|m| m.kind == kind && m.alive() && (m.pos - game.player.pos).length() < 3.2).map(|m| m.id) { game.attack_mob(id); }
            }
            game.tick(0.05);
            if t % 20 == 0 {
                let m = game.ents.mobs.iter().find(|m| m.kind == kind);
                println!("t={t:3} hp={:.1} hunger={:.1} mob={:?} arrows={} items={}", game.health,
                    game.hunger,
                    m.map(|m| format!("({:.1},{:.1},{:.1}) hp={} tgt={} agg={} nav={} burn={} wet={} inv={}", m.pos.x, m.pos.y, m.pos.z, m.health, m.target, m.aggressive, m.nav.path.len(), m.burning, m.in_water, m.invulnerable)),
                    game.ents.arrows.len(), game.ents.items.len());
            }
        }
        return;
    }
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("no GPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).expect("device");
    let adapter_name = adapter.get_info().name;
    let mut r = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm, (w, h), &atlas, &gui_atlas, &ent_atlas.image);
    let (mut sky, day) = game.sky();
    r.day = day;
    r.fog_end = (radius as f32 * 16.0).max(48.0);
    apply_underwater(&game, &mut sky, &mut r, day);
    let meshes: Vec<_> = (-radius..=radius).flat_map(|z| (-radius..=radius).map(move |x| (pcx + x, pcz + z))).collect::<Vec<_>>()
        .par_iter().filter_map(|k| {
            let region = game.world.region(k.0, k.1)?;
            let light = region.light();
            let lookup = |n: &str| atlas.names[n];
            let tiles = Tiles { lookup: &lookup, colors: &game.colors };
            Some((*k, mesh_chunk(&region, &light, &tiles)))
        }).collect();
    for (k, m) in meshes { r.set_chunk(k, m); }
    {
        let ticks = nums("--ticks", 1).map(|v| v[0] as i32).unwrap_or(0);
        let mut anims = load_anims(&mut assets, &atlas.names);
        for _ in 0..ticks { for an in anims.iter_mut() { an.step(); } }
        for an in anims.iter() { r.update_tile(an.tile, an.current()); }
    }
    game.player.prev_pos = game.player.pos;
    game.player.prev_eye_height = game.player.eye_height;
    game.update_target();
    if args.iter().any(|a| a == "--mine") {
        if let Some(t) = &game.target { game.mining = Some(game::Mining { pos: t.block, progress: 0.45 }); }
    }
    if args.iter().any(|a| a == "--swing") { game.swing_ticks = 3; game.prev_swing = 0.5; }
    apply_underwater(&game, &mut sky, &mut r, day);
    if want_mobs {
        let p = game.player.pos;
        let fwd = game.player.look_dir();
        let (fx, fz) = (fwd.x as f64, fwd.z as f64);
        let n = (fx * fx + fz * fz).sqrt().max(1e-6);
        let (fx, fz) = (fx / n, fz / n);
        let kinds = [entity::Kind::Zombie, entity::Kind::Skeleton, entity::Kind::Creeper, entity::Kind::Pig, entity::Kind::Cow, entity::Kind::Sheep, entity::Kind::Chicken];
        for (i, k) in kinds.iter().enumerate() {
            let side = (i as f64 - 3.0) * 1.6;
            let (px, pz) = (p.x + fx * 5.0 - fz * side, p.z + fz * 5.0 + fx * side);
            let mut y = 120;
            while y > 0 && !blocks::def(game.world.get(px.floor() as i32, y, pz.floor() as i32)).solid { y -= 1; }
            game.spawn_mob(*k, DVec3::new(px, y as f64 + 1.0, pz));
            let m = game.ents.mobs.last_mut().unwrap();
            m.yaw = (-fz.atan2(fx).to_degrees() as f32) + 0.0 + 180.0 + 90.0;
            m.prev_yaw = m.yaw; m.head_yaw = m.yaw; m.prev_head_yaw = m.yaw;
            if args.iter().any(|a| a == "--burn") && i == 0 { m.burning = 100; }
            if args.iter().any(|a| a == "--hurt") && i == 1 { m.hurt_time = 5; }
            if args.iter().any(|a| a == "--walk") { m.walk_pos = 3.0; m.walk_speed = 0.8; }
        }
        let cam_pos = game.camera(0.0).pos;
        let (v, i) = entity_render::build(&game, &ent_atlas, cam_pos, 0.0);
        println!("mobs: {} verts: {}", game.ents.mobs.len(), v.len());
        r.set_entities(&v, &i);
    }
    { let cp = game.camera(0.0).pos; let (v, i) = entity_render::build(&game, &ent_atlas, cp, 0.0); r.set_entities(&v, &i); }
    if game.screen == Screen::Playing && !args.iter().any(|a| a == "--nohand") {
        if let Some(n) = nums("--slot", 1) { game.selected = n[0] as usize; }
        if let Some(n) = nums("--draw", 1) { game.use_ticks = n[0] as i32; }
        game.hand_item = game.slots[game.selected].map(|s| s.id).unwrap_or(0);
        game.hand_height = 1.0; game.hand_prev_height = 1.0;
        let mut hm = (Vec::new(), Vec::new());
        hand::build(&game, &ent_atlas, &mut hm);
        r.set_hand(&hm.0, &hm.1);
    }
    let info = FrameInfo { fps: 144, mouse: (w as f32 / 2.0 / 2.0, h as f32 / 2.0 / 2.0), adapter: adapter_name, loaded: game.world.chunks.len(), meshed: r.chunk_count(), scroll: 0 };
    let mut gui = Gui::new(&gui_atlas, (w, h), game.settings.gui_scale);
    let _clicks = ui::draw(&game, &mut gui, &info);
    let ws = WorldState { target: game.target.as_ref().map(|h| h.block), crack: crack_tile(&game, &atlas.names) };
    let img = r.screenshot(&game.camera(0.0), &ws, &sky, &gui.batches);
    img.save(&out).unwrap();
    println!("saved {out} ({} chunks)", r.chunk_count());
}

// ---------------------------------------------------------------- windowed app

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    game: Game,
    atlas_names: HashMap<String, u32>,
    ent_atlas: entity_render::EntityAtlas,
    anims: Vec<assets::TileAnim>,
    anim_acc: f64,
    gui_atlas: GuiAtlas,
    adapter_name: String,
    grabbed: bool,
    last: Instant,
    frames: u32,
    fps: u32,
    fps_timer: Instant,
    mouse: (f32, f32),
    clicks: Vec<Click>,
    dragging: Option<usize>,
    creative_scroll: usize,
    sprint_tap: bool,
    sprint_ctrl: bool,
    gui_scale: f32,
    history_idx: Option<usize>,
    want_screenshot: bool,
}

#[derive(Default)]
struct App { state: Option<State> }

impl State {
    fn grab(&mut self, on: bool) {
        self.grabbed = on;
        let mode = if on { CursorGrabMode::Locked } else { CursorGrabMode::None };
        if self.window.set_cursor_grab(mode).is_err() && on { let _ = self.window.set_cursor_grab(CursorGrabMode::Confined); }
        self.window.set_cursor_visible(!on);
    }

    fn set_screen(&mut self, s: Screen) {
        self.game.screen = s;
        self.game.keys = Default::default();
        self.game.left_down = false;
        self.game.right_down = false;
        self.sprint_tap = false;
        self.grab(s == Screen::Playing);
    }

    fn gui_mouse(&self) -> (f32, f32) { (self.mouse.0 / self.gui_scale, self.mouse.1 / self.gui_scale) }

    fn apply_slider(&mut self, idx: usize) {
        let c = &self.clicks[idx];
        let t = ((self.gui_mouse().0 - c.x - 4.0) / (c.w - 8.0)).clamp(0.0, 1.0);
        let s = &mut self.game.settings;
        match c.action {
            Action::Fov => s.fov = (30.0 + t * 80.0).round(),
            Action::RenderDistance => s.render_distance = (2.0 + t * 30.0).round() as i32,
            Action::Sensitivity => s.sensitivity = t,
            Action::Brightness => { s.brightness = t; self.renderer.gamma = 0.15 + t * 0.7; }
            _ => {}
        }
    }

    fn click(&mut self) {
        let (mx, my) = self.gui_mouse();
        let Some(idx) = self.clicks.iter().position(|c| mx >= c.x && mx < c.x + c.w && my >= c.y && my < c.y + c.h) else {
            // Dropping the cursor stack outside the window of a container.
            if matches!(self.game.screen, Screen::Inventory | Screen::Creative) { self.game.cursor = None; }
            return;
        };
        let action = self.clicks[idx].action;
        match action {
            Action::Singleplayer => { self.game.start_world(&mut self.renderer); self.set_screen(Screen::Playing); }
            Action::Quit => std::process::exit(0),
            Action::Resume => self.set_screen(Screen::Playing),
            Action::OpenOptions => { self.game.prev_screen = self.game.screen; self.game.screen = Screen::Options; }
            Action::CloseOptions => { let p = self.game.prev_screen; self.game.screen = p; }
            Action::SaveQuit => {
                for k in self.game.world.chunks.keys().copied().collect::<Vec<_>>() { self.renderer.drop_chunk(k); }
                self.game.in_world = false;
                self.game.world = Default::default();
                self.game.screen = Screen::Title;
                self.grab(false);
            }
            Action::Fov | Action::RenderDistance | Action::Sensitivity | Action::Brightness => { self.dragging = Some(idx); self.apply_slider(idx); }
            Action::GuiScale => { let s = &mut self.game.settings; s.gui_scale = (s.gui_scale + 1) % 5; }
            Action::Clouds => self.game.settings.clouds = !self.game.settings.clouds,
            Action::Bobbing => self.game.settings.bobbing = !self.game.settings.bobbing,
            Action::Slot(i) => { let g = &mut self.game; std::mem::swap(&mut g.cursor, &mut g.slots[i]); }
            Action::CreativeItem(id) => self.game.cursor = Some(game::Stack::new(id, if items::max_stack(id) == 1 { 1 } else { 64 })),
            Action::CreativeScroll => {}
        }
    }

    fn key(&mut self, code: KeyCode, down: bool, repeat: bool, text: Option<&str>) {
        let now = self.game.clock;
        if self.game.screen == Screen::Chat {
            if !down { return; }
            match code {
                KeyCode::Escape => self.set_screen(Screen::Playing),
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    let line = std::mem::take(&mut self.game.chat_input);
                    if !line.is_empty() {
                        self.game.chat_history.push(line.clone());
                        if line.starts_with('/') { self.game.run_command(&line); } else { self.game.say(&format!("<Player> {line}")); }
                    }
                    self.history_idx = None;
                    self.set_screen(Screen::Playing);
                }
                KeyCode::Backspace => { self.game.chat_input.pop(); }
                KeyCode::ArrowUp => {
                    let n = self.game.chat_history.len();
                    if n > 0 { let i = self.history_idx.map(|i| i.saturating_sub(1)).unwrap_or(n - 1); self.history_idx = Some(i); self.game.chat_input = self.game.chat_history[i].clone(); }
                }
                KeyCode::ArrowDown => {
                    let n = self.game.chat_history.len();
                    match self.history_idx { Some(i) if i + 1 < n => { self.history_idx = Some(i + 1); self.game.chat_input = self.game.chat_history[i + 1].clone(); } _ => { self.history_idx = None; self.game.chat_input.clear(); } }
                }
                _ => if let Some(t) = text { for c in t.chars().filter(|c| !c.is_control()) { if self.game.chat_input.len() < 256 { self.game.chat_input.push(c); } } },
            }
            return;
        }
        match code {
            KeyCode::F3 if down && !repeat => self.game.show_debug = !self.game.show_debug,
            KeyCode::F1 if down && !repeat => self.game.hide_hud = !self.game.hide_hud,
            KeyCode::F2 if down && !repeat => self.want_screenshot = true,
            KeyCode::F11 if down && !repeat => {
                let fs = self.window.fullscreen().is_some();
                self.window.set_fullscreen(if fs { None } else { Some(Fullscreen::Borderless(None)) });
            }
            KeyCode::Escape if down => match self.game.screen {
                Screen::Playing => self.set_screen(Screen::Pause),
                Screen::Pause | Screen::Inventory | Screen::Creative => self.set_screen(Screen::Playing),
                Screen::Options => { let p = self.game.prev_screen; self.game.screen = p; }
                _ => {}
            },
            _ if self.game.screen == Screen::Playing => {
                let k = &mut self.game.keys;
                match code {
                    KeyCode::KeyW => {
                        if down && !repeat {
                            if now - self.game.last_w < 0.3 { self.sprint_tap = true; }
                            self.game.last_w = now;
                        }
                        k.forward = down;
                        if !down { self.sprint_tap = false; }
                    }
                    KeyCode::KeyS => k.back = down,
                    KeyCode::KeyA => k.left = down,
                    KeyCode::KeyD => k.right = down,
                    KeyCode::Space => {
                        if down && !repeat && self.game.mode == Mode::Creative {
                            if now - self.game.last_space < 0.3 { self.game.player.flying = !self.game.player.flying; }
                            self.game.last_space = now;
                        }
                        self.game.keys.jump = down;
                    }
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => k.sneak = down,
                    KeyCode::ControlLeft => self.sprint_ctrl = down,
                    KeyCode::KeyE if down && !repeat => { let s = if self.game.mode == Mode::Creative { Screen::Creative } else { Screen::Inventory }; self.set_screen(s); }
                    KeyCode::KeyT if down && !repeat => { self.set_screen(Screen::Chat); self.game.chat_input.clear(); }
                    KeyCode::Slash if down && !repeat => { self.set_screen(Screen::Chat); self.game.chat_input = "/".into(); }
                    c if down => {
                        let n = match c { KeyCode::Digit1 => 0, KeyCode::Digit2 => 1, KeyCode::Digit3 => 2, KeyCode::Digit4 => 3, KeyCode::Digit5 => 4,
                            KeyCode::Digit6 => 5, KeyCode::Digit7 => 6, KeyCode::Digit8 => 7, KeyCode::Digit9 => 8, _ => 9 };
                        if n < 9 { self.game.selected = n; self.game.item_name_timer = 2.0; }
                    }
                    _ => {}
                }
                self.game.keys.sprint = self.sprint_ctrl || self.sprint_tap;
            }
            KeyCode::KeyE if down && !repeat && matches!(self.game.screen, Screen::Inventory | Screen::Creative) => self.set_screen(Screen::Playing),
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() { return; }
        let window = Arc::new(el.create_window(Window::default_attributes().with_title("Minecraft (Rust)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))).unwrap());
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface), power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default()
        })).expect("no adapter");
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).expect("device");
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format, width: size.width.max(1), height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, alpha_mode: caps.alpha_modes[0], view_formats: vec![], desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let (atlas, gui_atlas, mut assets, ent_atlas) = setup_assets();
        let renderer = Renderer::new(device, queue, format, (config.width, config.height), &atlas, &gui_atlas, &ent_atlas.image);
        let (wgen, pal_map) = make_generator();
        let colors = Arc::new(biome_colors(&mut assets, &wgen));
        let game = Game::new(wgen, pal_map, Arc::new(atlas.names.clone()), colors);
        self.state = Some(State {
            window, surface, config, renderer, game, atlas_names: atlas.names.clone(), ent_atlas, anims: load_anims(&mut assets, &atlas.names), anim_acc: 0.0, gui_atlas, adapter_name, grabbed: false,
            last: Instant::now(), frames: 0, fps: 0, fps_timer: Instant::now(), mouse: (0.0, 0.0), clicks: Vec::new(), dragging: None,
            creative_scroll: 0, sprint_tap: false, sprint_ctrl: false, gui_scale: 2.0, history_idx: None, want_screenshot: false,
        });
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, ev: DeviceEvent) {
        let Some(s) = self.state.as_mut() else { return };
        if let (DeviceEvent::MouseMotion { delta }, true) = (ev, s.grabbed) {
            // Vanilla sensitivity curve: (0.6 * s + 0.2)^3 * 8 degrees per count... scaled to our units.
            let f = s.game.settings.sensitivity * 0.6 + 0.2;
            let k = f * f * f * 8.0 * 0.15;
            s.game.player.yaw += delta.0 as f32 * k;
            s.game.player.pitch = (s.game.player.pitch + delta.1 as f32 * k).clamp(-90.0, 90.0);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                s.config.width = sz.width.max(1);
                s.config.height = sz.height.max(1);
                s.surface.configure(&s.renderer.device, &s.config);
                s.renderer.resize(s.config.width, s.config.height);
            }
            WindowEvent::CursorMoved { position, .. } => {
                s.mouse = (position.x as f32, position.y as f32);
                if let Some(i) = s.dragging { s.apply_slider(i); }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let text = event.text.as_ref().map(|t| t.to_string());
                    s.key(code, event.state == ElementState::Pressed, event.repeat, text.as_deref());
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                if s.game.screen == Screen::Playing && s.game.in_world {
                    if !s.grabbed && down { s.grab(true); return; }
                    match button {
                        MouseButton::Left => { s.game.left_down = down; if down { s.game.left_clicked = true; } }
                        MouseButton::Right => s.game.right_down = down,
                        MouseButton::Middle if down => s.game.pick_block(),
                        _ => {}
                    }
                } else if down && button == MouseButton::Left {
                    s.click();
                } else if !down && button == MouseButton::Left {
                    s.dragging = None;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta { MouseScrollDelta::LineDelta(_, y) => y, MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0 };
                match s.game.screen {
                    Screen::Playing => {
                        s.game.selected = (s.game.selected as i32 - d.signum() as i32).rem_euclid(9) as usize;
                        s.game.item_name_timer = 2.0;
                    }
                    Screen::Creative => { s.creative_scroll = (s.creative_scroll as i32 - d.signum() as i32).max(0) as usize; }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - s.last).as_secs_f64();
                s.last = now;
                s.game.tick(dt);
                s.anim_acc += dt.min(0.25);
                while s.anim_acc >= 0.05 {
                    s.anim_acc -= 0.05;
                    for an in s.anims.iter_mut() { if an.step() { s.renderer.update_tile(an.tile, an.current()); } }
                }
                s.game.stream(&mut s.renderer, 24);
                let (mut sky, day) = s.game.sky();
                s.renderer.day = day;
                apply_underwater(&s.game, &mut sky, &mut s.renderer, day);
                s.renderer.fog_end = (s.game.settings.render_distance as f32 * 16.0).max(48.0);
                { let cam_pos = s.game.camera(0.0).pos; let (v, i) = entity_render::build(&s.game, &s.ent_atlas, cam_pos, s.game.partial); s.renderer.set_entities(&v, &i); }
                {
                    let mut hm = (Vec::new(), Vec::new());
                    if s.game.in_world && !s.game.hide_hud && matches!(s.game.screen, Screen::Playing | Screen::Chat | Screen::Pause) { hand::build(&s.game, &s.ent_atlas, &mut hm); }
                    s.renderer.set_hand(&hm.0, &hm.1);
                }
                match s.surface.get_current_texture() {
                    Ok(frame) => {
                        let view = frame.texture.create_view(&Default::default());
                        let cam = s.game.camera(0.0);
                        let (pcx, pcz) = s.game.player_chunk();
                        let vd = s.game.settings.render_distance;
                        let fb = (s.config.width, s.config.height);
                        let mut gui = Gui::new(&s.gui_atlas, fb, s.game.settings.gui_scale);
                        s.gui_scale = gui.scale;
                        let info = FrameInfo { fps: s.fps, mouse: s.gui_mouse(), adapter: s.adapter_name.clone(), loaded: s.game.world.chunks.len(), meshed: s.renderer.chunk_count(), scroll: s.creative_scroll };
                        s.clicks = ui::draw(&s.game, &mut gui, &info);
                        let ws = if s.game.in_world { WorldState { target: s.game.target.as_ref().map(|h| h.block), crack: crack_tile(&s.game, &s.atlas_names) } } else { WorldState { target: None, crack: None } };
                        let sky = if s.game.in_world { sky } else { SkyState { cloud_color: [0.0; 3], ..sky } };
                        s.renderer.render(&view, &cam, &ws, &sky, &gui.batches, &|k| (k.0 - pcx).abs() <= vd && (k.1 - pcz).abs() <= vd && s.game.in_world);
                        frame.present();
                        if s.want_screenshot {
                            s.want_screenshot = false;
                            let img = s.renderer.screenshot(&cam, &ws, &sky, &gui.batches);
                            let _ = std::fs::create_dir_all("screenshots");
                            let name = format!("screenshots/{}.png", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
                            if img.save(&name).is_ok() { s.game.say(&format!("Saved screenshot as {name}")); }
                        }
                    }
                    Err(_) => s.surface.configure(&s.renderer.device, &s.config),
                }
                s.frames += 1;
                if s.fps_timer.elapsed().as_secs_f32() >= 1.0 {
                    s.fps = s.frames;
                    s.frames = 0;
                    s.fps_timer = Instant::now();
                }
                s.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--shot") {
        screenshot_mode(&args);
        return;
    }
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App::default()).unwrap();
}
