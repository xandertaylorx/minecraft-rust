//! GUI atlas (vanilla sprites, bitmap font, block icons) and an immediate-mode quad builder.
use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::assets::Assets;
use crate::blocks::{def, BLOCKS};
use crate::render::{Atlas, HudVertex};

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const ATLAS_SIZE: u32 = 2048;
pub const ICON_PX: f32 = 64.0;
pub const ICON_X: f32 = 1024.0;

pub struct GuiAtlas {
    pub image: RgbaImage,
    pub sprites: HashMap<String, Rect>,
    /// Pixel width of each ASCII glyph (cell content), advance is width + 1.
    pub glyph_w: [u8; 256],
    pub font: Rect,
    pub white: Rect,
    pub icons: Rect,
}

/// Sprites copied into the atlas: (our name, path below `textures/gui/sprites/`).
const SPRITES: &[(&str, &str)] = &[
    ("hotbar", "hud/hotbar"), ("hotbar_selection", "hud/hotbar_selection"), ("crosshair", "hud/crosshair"),
    ("heart_container", "hud/heart/container"), ("heart_full", "hud/heart/full"), ("heart_half", "hud/heart/half"),
    ("food_empty", "hud/food_empty"), ("food_full", "hud/food_full"), ("food_half", "hud/food_half"),
    ("air", "hud/air"), ("air_empty", "hud/air_empty"),
    ("xp_bg", "hud/experience_bar_background"), ("xp_fg", "hud/experience_bar_progress"),
    ("button", "widget/button"), ("button_hl", "widget/button_highlighted"), ("button_off", "widget/button_disabled"),
    ("slider", "widget/slider"), ("slider_hl", "widget/slider_highlighted"),
    ("slider_handle", "widget/slider_handle"), ("slider_handle_hl", "widget/slider_handle_highlighted"),
    ("text_field", "widget/text_field"), ("text_field_hl", "widget/text_field_highlighted"),
    ("scroller", "container/creative_inventory/scroller"),
    ("tab_top_sel_1", "container/creative_inventory/tab_top_selected_1"),
    ("tab_top_1", "container/creative_inventory/tab_top_unselected_1"),
    ("tab_top_2", "container/creative_inventory/tab_top_unselected_2"),
    ("tab_bottom_sel_4", "container/creative_inventory/tab_bottom_selected_4"),
    ("tab_bottom_4", "container/creative_inventory/tab_bottom_unselected_4"),
];

/// Whole-texture images placed in the atlas: (our name, path below `textures/`).
const TEXTURES: &[(&str, &str)] = &[
    ("title_logo", "gui/title/minecraft.png"),
    ("title_edition", "gui/title/edition.png"),
    ("menu_bg", "gui/menu_background.png"),
    ("creative_bg", "gui/container/creative_inventory/tab_items.png"),
    ("inventory_bg", "gui/container/inventory.png"),
    ("clouds", "environment/clouds.png"),
    ("sun", "environment/celestial/sun.png"),
    ("moon", "environment/celestial/moon/full_moon.png"),
];

struct Shelf { x: u32, y: u32, row_h: u32 }

impl Shelf {
    fn place(&mut self, img: &RgbaImage, region_x: u32, region_w: u32) -> (u32, u32) {
        if self.x + img.width() > region_x + region_w { self.x = region_x; self.y += self.row_h; self.row_h = 0; }
        let pos = (self.x, self.y);
        self.x += img.width() + 1;
        self.row_h = self.row_h.max(img.height() + 1);
        pos
    }
}

pub fn build_gui_atlas(a: &mut Assets, atlas: &Atlas, grass: [f32; 3]) -> GuiAtlas {
    let mut image = RgbaImage::new(ATLAS_SIZE, ATLAS_SIZE);
    image::imageops::replace(&mut image, &atlas.image, 0, 0);
    let mut sprites = HashMap::new();
    // 1x1 white pixel for solid rectangles.
    image.put_pixel(256, 0, Rgba([255, 255, 255, 255]));
    let white = Rect { x: 256.0, y: 0.0, w: 1.0, h: 1.0 };
    // Font.
    let font_img = a.image("textures/font/ascii.png").expect("ascii font");
    image::imageops::replace(&mut image, &font_img, 512, 0);
    let font = Rect { x: 512.0, y: 0.0, w: font_img.width() as f32, h: font_img.height() as f32 };
    let cell = font_img.width() / 16;
    let mut glyph_w = [0u8; 256];
    for c in 0..256u32 {
        let (cx, cy) = ((c % 16) * cell, (c / 16) * cell);
        let mut w = 0;
        for x in 0..cell { for y in 0..cell { if font_img.get_pixel(cx + x, cy + y)[3] > 0 { w = w.max(x + 1); } } }
        glyph_w[c as usize] = w as u8;
    }
    glyph_w[b' ' as usize] = 3; // advance 4
    // Block icons.
    let icons = Rect { x: 1024.0, y: 0.0, w: 1024.0, h: 64.0 * ((BLOCKS.len() as f32 / 16.0).ceil()) };
    for id in 1..BLOCKS.len() as u16 {
        if def(id).name == "water" { continue; }
        let icon = crate::block_icon(atlas, id, grass);
        image::imageops::replace(&mut image, &icon, 1024 + (id as i64 % 16) * 64, (id as i64 / 16) * 64);
    }
    // Item sprites (nearest-upscaled 4x like the block icons).
    for (i, it) in crate::items::ITEMS.iter().enumerate() {
        let Some(img) = a.image(&format!("textures/item/{}.png", it.name)) else { continue };
        let up = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Nearest);
        let slot = BLOCKS.len() + i;
        image::imageops::replace(&mut image, &up, 1024 + (slot as i64 % 16) * 64, (slot as i64 / 16) * 64);
    }
    for n in 0..3 {
        if let Some(img) = a.image(&format!("textures/item/bow_pulling_{n}.png")) {
            let up = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Nearest);
            let slot = BLOCKS.len() + crate::items::ITEMS.len() + n;
            image::imageops::replace(&mut image, &up, 1024 + (slot as i64 % 16) * 64, (slot as i64 / 16) * 64);
        }
    }
    let mut shelf = Shelf { x: 512, y: 140, row_h: 0 };
    for (name, path) in SPRITES {
        let img = a.image(&format!("textures/gui/sprites/{path}.png")).unwrap_or_else(|| panic!("missing sprite {path}"));
        let (x, y) = shelf.place(&img, 512, 512);
        image::imageops::replace(&mut image, &img, x as i64, y as i64);
        sprites.insert(name.to_string(), Rect { x: x as f32, y: y as f32, w: img.width() as f32, h: img.height() as f32 });
    }
    let mut big = Shelf { x: 0, y: 260, row_h: 0 };
    for (name, path) in TEXTURES {
        let img = a.image(&format!("textures/{path}")).unwrap_or_else(|| panic!("missing texture {path}"));
        // Large textures go below the sprite shelf; they may exceed the shelf width.
        let (x, y) = big.place(&img, 0, 1100);
        if y + img.height() > ATLAS_SIZE { eprintln!("gui atlas full, skipping {name} ({}x{} at {x},{y})", img.width(), img.height()); continue; }
        image::imageops::replace(&mut image, &img, x as i64, y as i64);
        sprites.insert(name.to_string(), Rect { x: x as f32, y: y as f32, w: img.width() as f32, h: img.height() as f32 });
    }
    GuiAtlas { image, sprites, glyph_w, font, white, icons }
}

impl GuiAtlas {
    pub fn text_width(&self, s: &str) -> f32 {
        let mut w = 0.0;
        let mut it = s.chars();
        while let Some(c) = it.next() {
            if c == '§' { it.next(); continue; }
            w += self.glyph_w[(c as u32).min(255) as usize] as f32 + 1.0;
        }
        (w - 1.0).max(0.0)
    }
}

/// Minecraft chat/format colours `§0`..`§f`.
pub fn format_color(code: char) -> Option<[f32; 3]> {
    let hex = match code {
        '0' => 0x000000, '1' => 0x0000AA, '2' => 0x00AA00, '3' => 0x00AAAA, '4' => 0xAA0000, '5' => 0xAA00AA, '6' => 0xFFAA00,
        '7' => 0xAAAAAA, '8' => 0x555555, '9' => 0x5555FF, 'a' => 0x55FF55, 'b' => 0x55FFFF, 'c' => 0xFF5555, 'd' => 0xFF55FF,
        'e' => 0xFFFF55, 'f' => 0xFFFFFF, _ => return None,
    };
    Some([((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0])
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Normal, Invert, Icon }

pub struct Batch {
    pub kind: Kind,
    pub verts: Vec<HudVertex>,
}

pub struct Gui<'a> {
    pub atlas: &'a GuiAtlas,
    /// GUI scale (physical pixels per GUI pixel).
    pub scale: f32,
    /// Framebuffer size in physical pixels.
    pub fb: (f32, f32),
    pub batches: Vec<Batch>,
}

impl<'a> Gui<'a> {
    pub fn new(atlas: &'a GuiAtlas, fb: (u32, u32), requested_scale: u32) -> Self {
        let (w, h) = (fb.0 as f32, fb.1 as f32);
        let max = ((w / 320.0).floor().min((h / 240.0).floor())).max(1.0);
        let scale = if requested_scale == 0 { max } else { (requested_scale as f32).min(max).max(1.0) };
        Self { atlas, scale, fb: (w, h), batches: Vec::new() }
    }

    /// GUI-space size (320x240 at minimum).
    pub fn size(&self) -> (f32, f32) { (self.fb.0 / self.scale, self.fb.1 / self.scale) }

    fn quad(&mut self, kind: Kind, x: f32, y: f32, w: f32, h: f32, src: Rect, c: [f32; 4]) {
        let (sw, sh) = (ATLAS_SIZE as f32, ATLAS_SIZE as f32);
        let p = |gx: f32, gy: f32| [(gx * self.scale / self.fb.0) * 2.0 - 1.0, 1.0 - (gy * self.scale / self.fb.1) * 2.0];
        let uv = |ux: f32, uy: f32| [ux / sw, uy / sh];
        let (a, b, cc, d) = (p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h));
        let (ua, ub, uc, ud) = (uv(src.x, src.y), uv(src.x + src.w, src.y), uv(src.x + src.w, src.y + src.h), uv(src.x, src.y + src.h));
        if self.batches.last().map(|b| b.kind) != Some(kind) { self.batches.push(Batch { kind, verts: Vec::new() }); }
        let v = &mut self.batches.last_mut().unwrap().verts;
        for (pos, uv) in [(a, ua), (b, ub), (cc, uc), (a, ua), (cc, uc), (d, ud)] { v.push(HudVertex { pos, uv, color: c }); }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        let r = self.atlas.white;
        self.quad(Kind::Normal, x, y, w, h, r, c);
    }

    pub fn sprite(&mut self, name: &str, x: f32, y: f32) {
        let r = self.atlas.sprites[name];
        self.quad(Kind::Normal, x, y, r.w, r.h, r, [1.0; 4]);
    }

    pub fn sprite_tinted(&mut self, name: &str, x: f32, y: f32, c: [f32; 4]) {
        let r = self.atlas.sprites[name];
        self.quad(Kind::Normal, x, y, r.w, r.h, r, c);
    }

    pub fn sprite_tinted_scaled(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        let r = self.atlas.sprites[name];
        self.quad(Kind::Normal, x, y, w, h, r, c);
    }

    pub fn icon_slot_scaled(&mut self, slot: usize, x: f32, y: f32, size: f32) {
        let r = Rect { x: ICON_X + (slot % 16) as f32 * ICON_PX, y: (slot / 16) as f32 * ICON_PX, w: ICON_PX, h: ICON_PX };
        self.quad(Kind::Icon, x, y, size, size, r, [1.0; 4]);
    }

    pub fn icon_scaled(&mut self, id: u16, x: f32, y: f32, size: f32) {
        let slot = crate::items::icon_slot(id);
        let r = Rect { x: ICON_X + (slot % 16) as f32 * ICON_PX, y: (slot / 16) as f32 * ICON_PX, w: ICON_PX, h: ICON_PX };
        self.quad(Kind::Icon, x, y, size, size, r, [1.0; 4]);
    }

    pub fn sprite_scaled(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32) {
        let r = self.atlas.sprites[name];
        self.quad(Kind::Normal, x, y, w, h, r, [1.0; 4]);
    }

    /// A sub-rectangle of a sprite/texture (pixel coordinates within it).
    pub fn sprite_part(&mut self, name: &str, x: f32, y: f32, sx: f32, sy: f32, sw: f32, sh: f32) {
        let r = self.atlas.sprites[name];
        self.quad(Kind::Normal, x, y, sw, sh, Rect { x: r.x + sx, y: r.y + sy, w: sw, h: sh }, [1.0; 4]);
    }

    pub fn crosshair(&mut self, x: f32, y: f32) {
        let r = self.atlas.sprites["crosshair"];
        self.quad(Kind::Invert, x, y, r.w, r.h, r, [1.0; 4]);
    }

    /// Nine-slice stretch of a sprite whose source size is `(sw, sh)` with `border` px edges.
    pub fn nine_slice(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, border: f32) {
        let r = self.atlas.sprites[name];
        let b = border;
        let xs = [0.0, b, w - b, w];
        let ys = [0.0, b, h - b, h];
        let sxs = [0.0, b, r.w - b, r.w];
        let sys = [0.0, b, r.h - b, r.h];
        for j in 0..3 {
            for i in 0..3 {
                let (dw, dh) = (xs[i + 1] - xs[i], ys[j + 1] - ys[j]);
                let (sw, sh) = (sxs[i + 1] - sxs[i], sys[j + 1] - sys[j]);
                if dw <= 0.0 || dh <= 0.0 { continue; }
                self.quad(Kind::Normal, x + xs[i], y + ys[j], dw, dh, Rect { x: r.x + sxs[i], y: r.y + sys[j], w: sw, h: sh }, [1.0; 4]);
            }
        }
    }

    pub fn block_icon(&mut self, id: u16, x: f32, y: f32) { self.icon_scaled(id, x, y, 16.0); }

    pub fn text(&mut self, s: &str, x: f32, y: f32, color: [f32; 3], shadow: bool) -> f32 {
        if shadow { self.text_pass(s, x + 1.0, y + 1.0, color, true); }
        self.text_pass(s, x, y, color, false)
    }

    fn text_pass(&mut self, s: &str, x: f32, y: f32, base: [f32; 3], shadow: bool) -> f32 {
        let mut cx = x;
        let mut col = base;
        let mut it = s.chars();
        let cell = (self.atlas.font.w / 16.0).max(1.0);
        while let Some(c) = it.next() {
            if c == '§' {
                if let Some(code) = it.next() { if let Some(cc) = format_color(code) { col = cc; } else if code == 'r' { col = base; } }
                continue;
            }
            let idx = (c as u32).min(255);
            let gw = self.atlas.glyph_w[idx as usize] as f32;
            if c != ' ' && gw > 0.0 {
                let shade = if shadow { 0.25 } else { 1.0 };
                let src = Rect { x: self.atlas.font.x + (idx % 16) as f32 * cell, y: self.atlas.font.y + (idx / 16) as f32 * cell, w: gw, h: cell };
                self.quad(Kind::Normal, cx, y, gw, cell, src, [col[0] * shade, col[1] * shade, col[2] * shade, 1.0]);
            }
            cx += gw + 1.0;
        }
        cx - x
    }

    pub fn text_centered(&mut self, s: &str, cx: f32, y: f32, color: [f32; 3], shadow: bool) {
        let w = self.atlas.text_width(s);
        self.text(s, cx - w / 2.0, y, color, shadow);
    }

    /// Vanilla-style button; returns whether the mouse is over it.
    pub fn button(&mut self, label: &str, x: f32, y: f32, w: f32, h: f32, mouse: (f32, f32), enabled: bool) -> bool {
        let hover = enabled && mouse.0 >= x && mouse.0 < x + w && mouse.1 >= y && mouse.1 < y + h;
        let sprite = if !enabled { "button_off" } else if hover { "button_hl" } else { "button" };
        self.nine_slice(sprite, x, y, w, h, 3.0);
        let col = if !enabled { [0.627, 0.627, 0.627] } else if hover { [1.0, 1.0, 0.627] } else { [1.0; 3] };
        self.text_centered(label, x + w / 2.0, y + (h - 8.0) / 2.0, col, true);
        hover
    }
}
