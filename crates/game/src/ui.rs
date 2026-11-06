//! All GUI screens: HUD, F3, chat, title, pause, options and inventories (immediate mode).
use crate::blocks::{self, def, display_name, BLOCKS};
use crate::game::{Game, Mode, Screen};
use crate::gui::Gui;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Singleplayer, Quit, Resume, OpenOptions, CloseOptions, SaveQuit,
    Fov, RenderDistance, Sensitivity, GuiScale, Brightness, Clouds, Bobbing,
    Slot(usize), CreativeItem(u16), CreativeScroll,
}

pub struct Click {
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
    pub action: Action,
}

pub struct FrameInfo {
    pub fps: u32,
    pub mouse: (f32, f32),
    pub adapter: String,
    pub loaded: usize,
    pub meshed: usize,
    pub scroll: usize,
}

const WHITE: [f32; 3] = [1.0; 3];
const GRAY: [f32; 3] = [0.627; 3];

fn dim(g: &mut Gui, a: f32) {
    let (w, h) = g.size();
    g.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, a]);
}

fn tile_background(g: &mut Gui) {
    let (w, h) = g.size();
    let tile = 32.0;
    let r = g.atlas.sprites["menu_bg"];
    let _ = r;
    let (cols, rows) = ((w / tile).ceil() as i32, (h / tile).ceil() as i32);
    for j in 0..rows { for i in 0..cols {
        let (x, y) = (i as f32 * tile, j as f32 * tile);
        g.sprite_tinted_scaled("menu_bg", x, y, tile, tile, [0.25, 0.25, 0.25, 1.0]);
    } }
}

fn slider(g: &mut Gui, clicks: &mut Vec<Click>, mouse: (f32, f32), label: &str, value: f32, x: f32, y: f32, w: f32, action: Action) {
    let hover = mouse.0 >= x && mouse.0 < x + w && mouse.1 >= y && mouse.1 < y + 20.0;
    g.nine_slice(if hover { "slider_hl" } else { "slider" }, x, y, w, 20.0, 3.0);
    let hx = x + value.clamp(0.0, 1.0) * (w - 8.0);
    g.nine_slice(if hover { "slider_handle_hl" } else { "slider_handle" }, hx, y, 8.0, 20.0, 3.0);
    g.text_centered(label, x + w / 2.0, y + 6.0, if hover { [1.0, 1.0, 0.627] } else { WHITE }, true);
    clicks.push(Click { x, y, w, h: 20.0, action });
}

pub fn draw(game: &Game, g: &mut Gui, info: &FrameInfo) -> Vec<Click> {
    let mut clicks = Vec::new();
    let (w, h) = g.size();
    match game.screen {
        Screen::Title => title(game, g, info, &mut clicks),
        Screen::Options => {
            if game.in_world { dim(g, 0.6); } else { tile_background(g); }
            options(game, g, info, &mut clicks);
        }
        _ => {}
    }
    if game.in_world && !matches!(game.screen, Screen::Title) {
        if game.player_burning > 0 { let (w, h) = g.size(); g.rect(0.0, 0.0, w, h, [1.0, 0.45, 0.0, 0.18]); }
        if game.player_hurt > 0 { let (w, h) = g.size(); g.rect(0.0, 0.0, w, h, [1.0, 0.0, 0.0, 0.2 * game.player_hurt as f32 / 10.0]); }
        if game.player.eye_in_water { let (w, h) = g.size(); g.rect(0.0, 0.0, w, h, [0.05, 0.15, 0.5, 0.25]); }
        if !game.hide_hud && game.screen != Screen::Options { hud(game, g); }
        if game.show_debug && game.screen != Screen::Options { debug(game, g, info); }
        if game.screen != Screen::Options { chat(game, g); }
        match game.screen {
            Screen::Pause => pause(game, g, info, &mut clicks),
            Screen::Inventory => inventory(game, g, info, &mut clicks),
            Screen::Creative => creative(game, g, info, &mut clicks),
            _ => {}
        }
        // Stack held by the cursor follows the mouse.
        if matches!(game.screen, Screen::Inventory | Screen::Creative) {
            if let Some(c) = game.cursor { draw_stack(g, c.id, c.count, info.mouse.0 - 8.0, info.mouse.1 - 8.0); }
        }
    }
    let _ = (w, h);
    clicks
}

fn title(game: &Game, g: &mut Gui, info: &FrameInfo, clicks: &mut Vec<Click>) {
    let _ = game;
    tile_background(g);
    let (w, h) = g.size();
    let cx = w / 2.0;
    if let Some(r) = g.atlas.sprites.get("title_logo").copied() {
        let lw = 256.0;
        let lh = r.h * lw / r.w;
        g.sprite_scaled("title_logo", cx - lw / 2.0, 30.0, lw, lh);
        if let Some(e) = g.atlas.sprites.get("title_edition").copied() {
            let ew = 128.0;
            g.sprite_scaled("title_edition", cx - 64.0, 30.0 + lh - 7.0, ew, e.h * ew / e.w);
        }
    }
    let y0 = (h / 4.0 + 48.0).floor();
    let bw = 200.0;
    for (label, y, action, on) in [("Singleplayer", y0, Some(Action::Singleplayer), true), ("Multiplayer", y0 + 24.0, None, false)] {
        g.button(label, cx - bw / 2.0, y, bw, 20.0, info.mouse, on);
        if let Some(a) = action { clicks.push(Click { x: cx - bw / 2.0, y, w: bw, h: 20.0, action: a }); }
    }
    g.button("Options...", cx - 100.0, y0 + 72.0, 98.0, 20.0, info.mouse, true);
    clicks.push(Click { x: cx - 100.0, y: y0 + 72.0, w: 98.0, h: 20.0, action: Action::OpenOptions });
    g.button("Quit Game", cx + 2.0, y0 + 72.0, 98.0, 20.0, info.mouse, true);
    clicks.push(Click { x: cx + 2.0, y: y0 + 72.0, w: 98.0, h: 20.0, action: Action::Quit });
    g.text("Minecraft 1.21.11 (Rust)", 2.0, h - 10.0, WHITE, true);
    let cp = "Not affiliated with Mojang";
    g.text(cp, w - g.atlas.text_width(cp) - 2.0, h - 10.0, WHITE, true);
}

fn pause(game: &Game, g: &mut Gui, info: &FrameInfo, clicks: &mut Vec<Click>) {
    let _ = game;
    dim(g, 0.55);
    let (w, h) = g.size();
    let cx = w / 2.0;
    g.text_centered("Game Menu", cx, 40.0, WHITE, true);
    let y0 = (h / 4.0 + 24.0).floor();
    let items = [("Back to Game", Action::Resume, 0.0), ("Options...", Action::OpenOptions, 48.0), ("Save and Quit to Title", Action::SaveQuit, 96.0)];
    for (label, a, dy) in items {
        g.button(label, cx - 102.0, y0 + dy, 204.0, 20.0, info.mouse, true);
        clicks.push(Click { x: cx - 102.0, y: y0 + dy, w: 204.0, h: 20.0, action: a });
    }
}

fn options(game: &Game, g: &mut Gui, info: &FrameInfo, clicks: &mut Vec<Click>) {
    let (w, h) = g.size();
    let cx = w / 2.0;
    g.text_centered("Options", cx, 15.0, WHITE, true);
    let s = &game.settings;
    let mouse = info.mouse;
    let (lx, rx, bw) = (cx - 155.0, cx + 5.0, 150.0);
    let y = |row: f32| 40.0 + row * 24.0;
    slider(g, clicks, mouse, &format!("FOV: {}", s.fov as i32), (s.fov - 30.0) / 80.0, lx, y(0.0), bw, Action::Fov);
    slider(g, clicks, mouse, &format!("Render Distance: {} chunks", s.render_distance), (s.render_distance - 2) as f32 / 30.0, rx, y(0.0), bw, Action::RenderDistance);
    slider(g, clicks, mouse, &format!("Sensitivity: {}%", (s.sensitivity * 200.0) as i32), s.sensitivity, lx, y(1.0), bw, Action::Sensitivity);
    slider(g, clicks, mouse, &format!("Brightness: {}%", (s.brightness * 100.0) as i32), s.brightness, rx, y(1.0), bw, Action::Brightness);
    let gs = if s.gui_scale == 0 { "Auto".to_string() } else { s.gui_scale.to_string() };
    g.button(&format!("GUI Scale: {gs}"), lx, y(2.0), bw, 20.0, mouse, true);
    clicks.push(Click { x: lx, y: y(2.0), w: bw, h: 20.0, action: Action::GuiScale });
    g.button(&format!("Clouds: {}", if s.clouds { "ON" } else { "OFF" }), rx, y(2.0), bw, 20.0, mouse, true);
    clicks.push(Click { x: rx, y: y(2.0), w: bw, h: 20.0, action: Action::Clouds });
    g.button(&format!("View Bobbing: {}", if s.bobbing { "ON" } else { "OFF" }), lx, y(3.0), bw, 20.0, mouse, true);
    clicks.push(Click { x: lx, y: y(3.0), w: bw, h: 20.0, action: Action::Bobbing });
    let dy = (h - 28.0).max(y(5.0));
    g.button("Done", cx - 100.0, dy, 200.0, 20.0, mouse, true);
    clicks.push(Click { x: cx - 100.0, y: dy, w: 200.0, h: 20.0, action: Action::CloseOptions });
}

fn draw_stack(g: &mut Gui, id: u16, count: u8, x: f32, y: f32) { draw_stack_d(g, id, count, 0, x, y); }

fn draw_stack_d(g: &mut Gui, id: u16, count: u8, damage: u16, x: f32, y: f32) {
    if id == 0 { return; }
    g.block_icon(id, x, y);
    let max = crate::items::max_damage(id);
    if max > 0 && damage > 0 {
        let frac = 1.0 - damage as f32 / max as f32;
        let w = (13.0 * frac).round().max(1.0);
        let c = if frac > 0.5 { [(1.0 - frac) * 2.0, 1.0, 0.0, 1.0] } else { [1.0, frac * 2.0, 0.0, 1.0] };
        g.rect(x + 2.0, y + 13.0, 13.0, 2.0, [0.0, 0.0, 0.0, 1.0]);
        g.rect(x + 2.0, y + 13.0, w, 1.0, c);
    }
    if count > 1 {
        let s = count.to_string();
        let w = g.atlas.text_width(&s);
        g.text(&s, x + 17.0 - w, y + 9.0, WHITE, true);
    }
}

fn hud(game: &Game, g: &mut Gui) {
    let (w, h) = g.size();
    if game.screen == Screen::Playing || matches!(game.screen, Screen::Chat | Screen::Pause) {
        g.crosshair(((w - 15.0) / 2.0).floor(), ((h - 15.0) / 2.0).floor());
    }
    let (hx, hy) = ((w - 182.0) / 2.0, h - 22.0);
    g.sprite("hotbar", hx, hy);
    if game.mode == Mode::Survival {
        let hp = game.health.ceil() as i32;
        for i in 0..10 {
            let x = hx + i as f32 * 8.0;
            g.sprite("heart_container", x, hy - 17.0);
            if hp >= (i + 1) * 2 { g.sprite("heart_full", x, hy - 17.0); } else if hp == i * 2 + 1 { g.sprite("heart_half", x, hy - 17.0); }
        }
        let fd = game.hunger.ceil() as i32;
        for i in 0..10 {
            let x = hx + 182.0 - 9.0 - i as f32 * 8.0;
            g.sprite("food_empty", x, hy - 17.0);
            if fd >= (i + 1) * 2 { g.sprite("food_full", x, hy - 17.0); } else if fd == i * 2 + 1 { g.sprite("food_half", x, hy - 17.0); }
        }
    }
    g.sprite("hotbar_selection", hx + 20.0 * game.selected as f32 - 1.0, hy - 1.0);
    // Held-item swing offsets the selected icon slightly like a first-person arm swing.
    for i in 0..9 {
        if let Some(s) = game.slots[i] { draw_stack_d(g, s.id, s.count, s.damage, hx + 3.0 + 20.0 * i as f32, hy + 3.0); }
    }
    if game.item_name_timer > 0.0 {
        if let Some(s) = game.slots[game.selected] {
            let a = (game.item_name_timer.min(1.0)).clamp(0.0, 1.0);
            let n = display_name(crate::items::name_of(s.id));
            let y = hy - if game.mode == Mode::Survival { 36.0 } else { 22.0 };
            g.text_centered(&n, w / 2.0, y, [a; 3], true);
        }
    }
}

/// Bottom-right held block with the vanilla-style swing / mining animation.
fn held_item(game: &Game, g: &mut Gui, w: f32, h: f32) {
    if game.screen != Screen::Playing { return; }
    let Some(s) = game.slots[game.selected] else { return };
    let t = game.swing_lerp(game.partial);
    let swing = if t > 0.0 { (t * std::f32::consts::PI).sin() } else { 0.0 };
    let bob = (game.player.walk_dist * std::f32::consts::PI).sin() * 2.0 * game.player.bob * 10.0;
    let size = 64.0;
    let (x, y) = (w - size - 8.0 - swing * 14.0 + bob, h - size * 0.9 + 10.0 + (game.player.walk_dist * std::f32::consts::PI * 2.0).cos().abs() * 2.0 * game.player.bob * 10.0 - swing * 18.0);
    let bow_slot = if crate::items::item_def(s.id).map_or(false, |d| d.kind == crate::items::Kind::Bow) && game.use_ticks > 0 {
        let n = if game.use_ticks >= 18 { 2 } else if game.use_ticks >= 9 { 1 } else { 0 };
        Some(BLOCKS.len() + crate::items::ITEMS.len() + n)
    } else { None };
    match bow_slot { Some(slot) => g.icon_slot_scaled(slot, x, y, size), None => g.icon_scaled(s.id, x, y, size) }
}

fn chat(game: &Game, g: &mut Gui) {
    let (_, h) = g.size();
    let open = game.screen == Screen::Chat;
    let base = h - 40.0;
    let lines: Vec<&(String, f32)> = game.chat.iter().rev().filter(|c| open || c.1 < 10.0).take(10).collect();
    for (i, (text, age)) in lines.iter().enumerate() {
        let a = if open { 1.0 } else { (1.0 - ((age - 8.0) / 2.0).clamp(0.0, 1.0)).clamp(0.0, 1.0) };
        let y = base - i as f32 * 9.0;
        let width = g.atlas.text_width(text) + 4.0;
        g.rect(2.0, y - 1.0, width, 9.0, [0.0, 0.0, 0.0, 0.4 * a]);
        g.text(text, 4.0, y, [a; 3], false);
    }
    if open {
        let (w, h) = g.size();
        g.rect(2.0, h - 14.0, w - 4.0, 12.0, [0.0, 0.0, 0.0, 0.55]);
        let caret = if (game.clock * 2.0) as i32 % 2 == 0 { "_" } else { "" };
        g.text(&format!("{}{}", game.chat_input, caret), 4.0, h - 12.0, WHITE, true);
    }
}

fn debug(game: &Game, g: &mut Gui, info: &FrameInfo) {
    let (w, _) = g.size();
    let p = game.player.pos;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let facing = {
        let y = game.player.yaw.rem_euclid(360.0);
        match y { y if !(45.0..315.0).contains(&y) => "south (Towards positive Z)", y if y < 135.0 => "west (Towards negative X)", y if y < 225.0 => "north (Towards negative Z)", _ => "east (Towards positive X)" }
    };
    let biome = {
        let col = ((bz.rem_euclid(16)) * 16 + bx.rem_euclid(16)) as usize;
        game.world.chunks.get(&(bx.div_euclid(16), bz.div_euclid(16)))
            .and_then(|c| c.biomes.get(col)).map(|b| game.wgen.biomes.biomes[*b as usize].name.clone()).unwrap_or_default()
    };
    let (sky, blk) = game.debug_light;
    let mut left = vec![
        "Minecraft 1.21.11 (Rust)".to_string(),
        format!("{} fps", info.fps),
        format!("C: {}/{} chunks loaded/meshed  D: {}", info.loaded, info.meshed, game.settings.render_distance),
        String::new(),
        format!("XYZ: {:.3} / {:.5} / {:.3}", p.x, p.y, p.z),
        format!("Block: {} {} {}", bx, by, bz),
        format!("Chunk: {} {} {} in {} {}", bx.rem_euclid(16), by.rem_euclid(16), bz.rem_euclid(16), bx.div_euclid(16), bz.div_euclid(16)),
        format!("Facing: {} ({:.1} / {:.1})", facing, wrap180(game.player.yaw), game.player.pitch),
        format!("Client Light: {} sky, {} block", sky, blk),
        format!("Biome: minecraft:{}", biome),
        format!("Day {}  Time {}", (game.time_ticks / 24000.0) as i64, (game.time_ticks % 24000.0) as i64),
        format!("Mode: {:?}{}", game.mode, if game.player.flying { " (flying)" } else { "" }),
        format!("E: {} mobs, {} arrows, {} items", game.debug_ents, game.ents.arrows.len(), game.ents.items.len()),
    ];
    if let Some(t) = &game.target {
        let id = game.world.get(t.block[0], t.block[1], t.block[2]);
        left.push(String::new());
        left.push(format!("Targeted Block: {}, {}, {}", t.block[0], t.block[1], t.block[2]));
        left.push(format!("minecraft:{}", def(id).name));
    }
    for (i, l) in left.iter().enumerate() {
        if l.is_empty() { continue; }
        let y = 2.0 + i as f32 * 10.0;
        let tw = g.atlas.text_width(l);
        g.rect(1.0, y - 1.0, tw + 2.0, 9.0, [0.31, 0.31, 0.31, 0.56]);
        g.text(l, 2.0, y, [0.88; 3], false);
    }
    let right = vec![
        format!("Rust (wgpu) {}", env!("CARGO_PKG_VERSION")),
        format!("GPU: {}", info.adapter),
        format!("Seed: {}", crate::SEED),
        format!("Blocks: {}", BLOCKS.len()),
    ];
    for (i, l) in right.iter().enumerate() {
        let tw = g.atlas.text_width(l);
        let y = 2.0 + i as f32 * 10.0;
        g.rect(w - tw - 3.0, y - 1.0, tw + 2.0, 9.0, [0.31, 0.31, 0.31, 0.56]);
        g.text(l, w - tw - 2.0, y, [0.88; 3], false);
    }
}

fn wrap180(a: f32) -> f32 { let v = a.rem_euclid(360.0); if v >= 180.0 { v - 360.0 } else { v } }

fn slot(g: &mut Gui, clicks: &mut Vec<Click>, info: &FrameInfo, x: f32, y: f32, idx: usize, game: &Game) {
    if let Some(s) = game.slots[idx] { draw_stack_d(g, s.id, s.count, s.damage, x, y); }
    let hover = info.mouse.0 >= x && info.mouse.0 < x + 16.0 && info.mouse.1 >= y && info.mouse.1 < y + 16.0;
    if hover { g.rect(x, y, 16.0, 16.0, [1.0, 1.0, 1.0, 0.45]); }
    clicks.push(Click { x, y, w: 16.0, h: 16.0, action: Action::Slot(idx) });
}

fn inventory(game: &Game, g: &mut Gui, info: &FrameInfo, clicks: &mut Vec<Click>) {
    dim(g, 0.45);
    let (w, h) = g.size();
    let (x, y) = ((w - 176.0).floor() / 2.0, ((h - 166.0) / 2.0).floor());
    if g.atlas.sprites.contains_key("inventory_bg") { g.sprite_part("inventory_bg", x, y, 0.0, 0.0, 176.0, 166.0); }
    g.text("Crafting", x + 97.0, y + 8.0, [0.25; 3], false);
    for r in 0..3 { for c in 0..9 { slot(g, clicks, info, x + 8.0 + c as f32 * 18.0, y + 84.0 + r as f32 * 18.0, 9 + r * 9 + c, game); } }
    for c in 0..9 { slot(g, clicks, info, x + 8.0 + c as f32 * 18.0, y + 142.0, c, game); }
    tooltip_for_slots(game, g, info, x, y);
}

fn tooltip_for_slots(game: &Game, g: &mut Gui, info: &FrameInfo, _x: f32, _y: f32) {
    let _ = (game, g, info);
}

fn creative(game: &Game, g: &mut Gui, info: &FrameInfo, clicks: &mut Vec<Click>) {
    dim(g, 0.45);
    let (w, h) = g.size();
    let (x, y) = ((w - 195.0).floor() / 2.0, ((h - 136.0) / 2.0).floor());
    if g.atlas.sprites.contains_key("creative_bg") { g.sprite_part("creative_bg", x, y, 0.0, 0.0, 195.0, 136.0); }
    if g.atlas.sprites.contains_key("tab_top_sel_1") { g.sprite("tab_top_sel_1", x, y - 28.0); }
    g.text("Building Blocks", x + 8.0, y + 6.0, [0.25; 3], false);
    let mut items = blocks::creative_items();
    items.extend(crate::items::all_item_ids());
    let rows_total = items.len().div_ceil(9);
    let scroll = info.scroll.min(rows_total.saturating_sub(5));
    let mut hover_item: Option<u16> = None;
    for r in 0..5 {
        for c in 0..9 {
            let i = (scroll + r) * 9 + c;
            let (sx, sy) = (x + 9.0 + c as f32 * 18.0, y + 18.0 + r as f32 * 18.0);
            if let Some(&id) = items.get(i) {
                g.block_icon(id, sx, sy);
                let hov = info.mouse.0 >= sx && info.mouse.0 < sx + 16.0 && info.mouse.1 >= sy && info.mouse.1 < sy + 16.0;
                if hov { g.rect(sx, sy, 16.0, 16.0, [1.0, 1.0, 1.0, 0.45]); hover_item = Some(id); }
                clicks.push(Click { x: sx, y: sy, w: 16.0, h: 16.0, action: Action::CreativeItem(id) });
            }
        }
    }
    // Scrollbar.
    let track = 90.0 - 15.0;
    let frac = if rows_total > 5 { scroll as f32 / (rows_total - 5) as f32 } else { 0.0 };
    if g.atlas.sprites.contains_key("scroller") { g.sprite("scroller", x + 175.0, y + 18.0 + frac * track); }
    clicks.push(Click { x: x + 174.0, y: y + 17.0, w: 14.0, h: 92.0, action: Action::CreativeScroll });
    for c in 0..9 { slot(g, clicks, info, x + 9.0 + c as f32 * 18.0, y + 112.0, c, game); }
    if let Some(id) = hover_item {
        let n = display_name(crate::items::name_of(id));
        let tw = g.atlas.text_width(&n);
        let (tx, ty) = (info.mouse.0 + 8.0, info.mouse.1 - 14.0);
        g.rect(tx - 2.0, ty - 2.0, tw + 4.0, 12.0, [0.06, 0.0, 0.06, 0.9]);
        g.text(&n, tx, ty, WHITE, true);
    }
}
