//! Reads Mojang's vanilla resources from the pinned resources jar (never committed).
use std::{fs::File, io::Read, path::Path};

use image::RgbaImage;
use zip::ZipArchive;

pub struct Assets {
    zip: ZipArchive<File>,
}

impl Assets {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let f = File::open(path.as_ref()).unwrap_or_else(|e| {
            panic!("cannot open {} ({e}); run scripts/fetch-assets.sh", path.as_ref().display())
        });
        Self { zip: ZipArchive::new(f).expect("resources jar is not a zip") }
    }

    pub fn bytes(&mut self, path: &str) -> Option<Vec<u8>> {
        let mut f = self.zip.by_name(&format!("assets/minecraft/{path}")).ok()?;
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok()?;
        Some(v)
    }

    pub fn image(&mut self, path: &str) -> Option<RgbaImage> {
        let b = self.bytes(path)?;
        Some(image::load_from_memory(&b).ok()?.to_rgba8())
    }

    /// A block texture; animated strips (taller than wide) use their first frame.
    pub fn block_tile(&mut self, name: &str) -> RgbaImage {
        let img = self.image(&format!("textures/block/{name}.png")).unwrap_or_else(|| panic!("missing texture {name}"));
        if img.height() > img.width() {
            image::imageops::crop_imm(&img, 0, 0, img.width(), img.width()).to_image()
        } else {
            img
        }
    }
}

/// One animated block texture (`.mcmeta` animation): per-tick frame stepping.
pub struct TileAnim {
    pub tile: u32,
    frames: Vec<RgbaImage>,
    seq: Vec<(usize, u32)>,
    idx: usize,
    left: u32,
}

impl TileAnim {
    /// Builds the animation for `textures/block/{name}.png` if it has one.
    pub fn load(a: &mut Assets, name: &str, tile: u32) -> Option<TileAnim> {
        let img = a.image(&format!("textures/block/{name}.png"))?;
        if img.height() <= img.width() { return None; }
        let meta = a.bytes(&format!("textures/block/{name}.png.mcmeta"))?;
        let v: serde_json::Value = serde_json::from_slice(&meta).ok()?;
        let anim = v.get("animation")?;
        let default_time = anim.get("frametime").and_then(|t| t.as_u64()).unwrap_or(1) as u32;
        let n = (img.height() / img.width()) as usize;
        let frames: Vec<RgbaImage> = (0..n).map(|i| image::imageops::crop_imm(&img, 0, i as u32 * img.width(), img.width(), img.width()).to_image()).collect();
        let seq: Vec<(usize, u32)> = match anim.get("frames").and_then(|f| f.as_array()) {
            Some(list) => list.iter().filter_map(|e| {
                if let Some(i) = e.as_u64() { Some((i as usize, default_time)) }
                else { Some((e.get("index")?.as_u64()? as usize, e.get("time").and_then(|t| t.as_u64()).unwrap_or(default_time as u64) as u32)) }
            }).collect(),
            None => (0..n).map(|i| (i, default_time)).collect(),
        };
        let left = seq.first()?.1;
        Some(TileAnim { tile, frames, seq, idx: 0, left })
    }

    pub fn current(&self) -> &RgbaImage { &self.frames[self.seq[self.idx].0] }

    /// Advance one tick; returns true when the displayed frame changed.
    pub fn step(&mut self) -> bool {
        if self.left > 1 { self.left -= 1; return false; }
        let before = self.seq[self.idx].0;
        self.idx = (self.idx + 1) % self.seq.len();
        self.left = self.seq[self.idx].1;
        self.seq[self.idx].0 != before
    }
}
