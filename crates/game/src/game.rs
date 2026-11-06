//! Game state: world streaming, player, inventory, mining, time, commands.
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use glam::DVec3;
use worldgen::chunk::OverworldGen;

use crate::blocks::{self, def, id_of, HOTBAR};
use crate::mesh::{mesh_chunk, BiomeColors, MeshData, Tiles};
use crate::player::{raycast, Hit, Keys, Player};
use crate::render::{Camera, Renderer, SkyState};
use crate::world::{self, generate, Blocks, World};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode { Survival, Creative }

#[derive(Clone, Copy, Debug)]
pub struct Stack { pub id: u16, pub count: u8, pub damage: u16 }

impl Stack { pub fn new(id: u16, count: u8) -> Self { Self { id, count, damage: 0 } } }

pub struct Settings {
    pub render_distance: i32,
    pub fov: f32,
    pub sensitivity: f32,
    pub gui_scale: u32,
    pub brightness: f32,
    pub clouds: bool,
    pub bobbing: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { render_distance: 10, fov: 70.0, sensitivity: 0.5, gui_scale: 0, brightness: 0.5, clouds: true, bobbing: true }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen { Title, Playing, Pause, Options, Inventory, Creative, Chat }

pub struct Mining { pub pos: [i32; 3], pub progress: f32 }

pub struct Game {
    pub wgen: Arc<OverworldGen>,
    pub pal_map: Arc<Vec<u16>>,
    pub names: Arc<HashMap<String, u32>>,
    pub colors: Arc<BiomeColors>,
    pub settings: Settings,
    pub screen: Screen,
    pub prev_screen: Screen,
    pub in_world: bool,
    pub spawned: bool,
    pub spawn: DVec3,
    pub world: World,
    pub player: Player,
    pub keys: Keys,
    pub mode: Mode,
    pub slots: [Option<Stack>; 36],
    pub cursor: Option<Stack>,
    pub selected: usize,
    pub health: f32,
    pub hunger: f32,
    pub time_ticks: f64,
    pub target: Option<Hit>,
    pub mining: Option<Mining>,
    pub swing_ticks: i32,
    pub prev_swing: f32,
    pub break_cooldown: i32,
    pub place_cooldown: i32,
    pub tick_acc: f64,
    pub partial: f32,
    pub left_down: bool,
    pub right_down: bool,
    pub chat: Vec<(String, f32)>,
    pub chat_input: String,
    pub chat_history: Vec<String>,
    pub show_debug: bool,
    pub hide_hud: bool,
    pub item_name_timer: f32,
    pub last_w: f32,
    pub last_space: f32,
    pub clock: f32,
    pub debug_light: (u8, u8),
    pub ents: crate::entity::Entities,
    pub particles: crate::particles::Particles,
    pub creative_tab: usize,
    pub difficulty: u8,
    pub hurt_dir: f32,
    pub rules: HashMap<String, String>,
    pub tab_cycle: Option<(String, usize)>,
    pub chat_scroll: usize,
    pub creative_search: String,
    pub hand_item: u16,
    pub hand_height: f32,
    pub hand_prev_height: f32,
    pub x_bob: f32,
    pub y_bob: f32,
    pub prev_x_bob: f32,
    pub prev_y_bob: f32,
    pub populated: std::collections::HashSet<(i32, i32)>,
    pub fires: HashMap<(i32, i32, i32), i32>,
    pub attack_ticker: i32,
    pub use_ticks: i32,
    pub left_clicked: bool,
    pub player_invuln: i32,
    pub player_hurt: i32,
    pub player_burning: i32,
    pub spawn_timer: i32,
    pub rng: worldgen::random::Xoro,
    pub debug_ents: usize,
    pub target_mob: Option<(u64, f64)>,
    debug_timer: f32,
    gen_tx: Sender<((i32, i32), Blocks, Arc<Vec<u16>>)>,
    gen_rx: Receiver<((i32, i32), Blocks, Arc<Vec<u16>>)>,
    mesh_tx: Sender<((i32, i32), u64, MeshData)>,
    mesh_rx: Receiver<((i32, i32), u64, MeshData)>,
    gen_inflight: HashSet<(i32, i32)>,
    mesh_inflight: HashSet<(i32, i32)>,
    meshed: HashMap<(i32, i32), u64>,
    dirty: HashSet<(i32, i32)>,
    version: u64,
}

impl Game {
    pub fn new(wgen: Arc<OverworldGen>, pal_map: Arc<Vec<u16>>, names: Arc<HashMap<String, u32>>, colors: Arc<BiomeColors>) -> Self {
        let (gen_tx, gen_rx) = channel();
        let (mesh_tx, mesh_rx) = channel();
        let mut slots = [None; 36];
        for (i, n) in HOTBAR.iter().enumerate() { slots[i] = Some(Stack::new(id_of(n), 64)); }
        let kit = ["iron_sword", "bow", "iron_pickaxe", "iron_axe", "iron_shovel", "arrow", "flint_and_steel"];
        for (i, n) in kit.iter().enumerate() {
            if let Some(id) = crate::items::item_id(n) { slots[i] = Some(Stack::new(id, if *n == "arrow" { 64 } else { 1 })); }
        }
        slots[7] = Some(Stack::new(id_of("oak_planks"), 64));
        slots[8] = Some(Stack::new(id_of("cobblestone"), 64));
        Self {
            wgen, pal_map, names, colors, settings: Settings::default(), screen: Screen::Title, prev_screen: Screen::Title,
            in_world: false, spawned: false, spawn: DVec3::new(8.5, 100.0, 8.5), world: World::default(),
            player: Player::new(DVec3::new(8.5, 200.0, 8.5)), keys: Keys::default(), mode: Mode::Creative, slots, cursor: None,
            selected: 0, health: 20.0, hunger: 20.0, time_ticks: 11000.0, target: None, mining: None, swing_ticks: -1, prev_swing: 0.0,
            break_cooldown: 0, place_cooldown: 0, tick_acc: 0.0, partial: 0.0, left_down: false, right_down: false, chat: Vec::new(), chat_input: String::new(),
            chat_history: Vec::new(), show_debug: false, hide_hud: false, item_name_timer: 0.0, 
            last_w: -10.0, last_space: -10.0, clock: 0.0, debug_light: (0, 0), debug_ents: 0, target_mob: None, debug_timer: 0.0, ents: Default::default(), particles: Default::default(), creative_tab: 0, difficulty: 2, hurt_dir: 0.0, rules: HashMap::new(), tab_cycle: None, chat_scroll: 0, creative_search: String::new(), hand_item: 0, hand_height: 1.0, hand_prev_height: 1.0, x_bob: 0.0, y_bob: 0.0, prev_x_bob: 0.0, prev_y_bob: 0.0, populated: HashSet::new(), fires: HashMap::new(), attack_ticker: 100, use_ticks: 0, left_clicked: false, player_invuln: 0, player_hurt: 0, player_burning: 0, spawn_timer: 0, rng: worldgen::random::Xoro::new(0xC0FFEE),
            gen_tx, gen_rx, mesh_tx, mesh_rx, gen_inflight: HashSet::new(), mesh_inflight: HashSet::new(), meshed: HashMap::new(),
            dirty: HashSet::new(), version: 0,
        }
    }

    /// Begin (or restart) a single-player world.
    pub fn start_world(&mut self, renderer: &mut Renderer) {
        for k in self.world.chunks.keys().copied().collect::<Vec<_>>() { renderer.drop_chunk(k); }
        self.world = World::default();
        self.meshed.clear();
        self.dirty.clear();
        self.gen_inflight.clear();
        self.mesh_inflight.clear();
        // Results of work started before the reset are ignored by draining stale channels.
        while self.gen_rx.try_recv().is_ok() {}
        while self.mesh_rx.try_recv().is_ok() {}
        self.player = Player::new(DVec3::new(8.5, 200.0, 8.5));
        self.spawned = false;
        self.in_world = true;
        self.screen = Screen::Playing;
        self.health = 20.0;
        self.hunger = 20.0;
        self.time_ticks = 1000.0;
        self.player.flying = self.mode == Mode::Creative;
        self.say("§eWelcome to Minecraft (Rust)! Press T to chat, / for commands.");
    }

    pub fn say(&mut self, s: &str) { self.chat.push((s.to_string(), 0.0)); }

    pub fn selected_stack(&self) -> Option<Stack> { self.slots[self.selected] }

    pub fn player_chunk(&self) -> (i32, i32) {
        ((self.player.pos.x / 16.0).floor() as i32, (self.player.pos.z / 16.0).floor() as i32)
    }

    // ------------------------------------------------------------ streaming

    pub fn stream(&mut self, renderer: &mut Renderer, max_new: usize) {
        if !self.in_world { return; }
        let (pcx, pcz) = self.player_chunk();
        let vd = self.settings.render_distance;
        while let Ok((k, b, bi)) = self.gen_rx.try_recv() {
            self.gen_inflight.remove(&k);
            if self.in_world { self.world.chunks.insert(k, world::Chunk { blocks: b, biomes: bi }); }
        }
        let mut uploads = 0;
        while let Ok((k, ver, data)) = self.mesh_rx.try_recv() {
            self.mesh_inflight.remove(&k);
            if self.world.chunks.contains_key(&k) && ver >= *self.meshed.get(&k).unwrap_or(&0) {
                renderer.set_chunk(k, data);
                self.meshed.insert(k, ver);
                uploads += 1;
            }
            if uploads >= 24 { break; }
        }
        let mut want: Vec<(i32, i32)> = Vec::new();
        for dz in -(vd + 1)..=(vd + 1) {
            for dx in -(vd + 1)..=(vd + 1) {
                let k = (pcx + dx, pcz + dz);
                if !self.world.chunks.contains_key(&k) && !self.gen_inflight.contains(&k) { want.push(k); }
            }
        }
        want.sort_by_key(|k| (k.0 - pcx).pow(2) + (k.1 - pcz).pow(2));
        for k in want.into_iter().take(max_new.saturating_sub(self.gen_inflight.len())) {
            self.gen_inflight.insert(k);
            let (tx, wgen, map) = (self.gen_tx.clone(), self.wgen.clone(), self.pal_map.clone());
            rayon::spawn(move || { let (b, bi) = generate(&wgen, &map, k.0, k.1); let _ = tx.send((k, b, bi)); });
        }
        let mut todo: Vec<(i32, i32)> = Vec::new();
        for dz in -vd..=vd {
            for dx in -vd..=vd {
                let k = (pcx + dx, pcz + dz);
                if self.mesh_inflight.contains(&k) { continue; }
                if (!self.meshed.contains_key(&k) || self.dirty.contains(&k)) && self.world.region(k.0, k.1).is_some() { todo.push(k); }
            }
        }
        todo.sort_by_key(|k| (k.0 - pcx).pow(2) + (k.1 - pcz).pow(2));
        for k in todo.into_iter().take(max_new.max(8)) {
            self.dirty.remove(&k);
            self.spawn_mesh(k);
        }
        let far: Vec<_> = self.world.chunks.keys().copied().filter(|k| (k.0 - pcx).abs() > vd + 3 || (k.1 - pcz).abs() > vd + 3).collect();
        for k in far {
            self.world.chunks.remove(&k);
            self.meshed.remove(&k);
            self.dirty.remove(&k);
            renderer.drop_chunk(k);
        }
    }

    fn spawn_mesh(&mut self, k: (i32, i32)) {
        let Some(region) = self.world.region(k.0, k.1) else { return };
        self.mesh_inflight.insert(k);
        self.version += 1;
        let (tx, ver, names, colors) = (self.mesh_tx.clone(), self.version, self.names.clone(), self.colors.clone());
        rayon::spawn(move || {
            let light = region.light();
            let lookup = |n: &str| names[n];
            let tiles = Tiles { lookup: &lookup, colors: &colors };
            let _ = tx.send((k, ver, mesh_chunk(&region, &light, &tiles)));
        });
    }

    pub fn mark_dirty_pub(&mut self, x: i32, z: i32) { self.mark_dirty(x, z); }

    fn mark_dirty(&mut self, x: i32, z: i32) {
        let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
        let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
        let dxs: Vec<i32> = std::iter::once(0).chain((lx <= 2).then_some(-1)).chain((lx >= 13).then_some(1)).collect();
        let dzs: Vec<i32> = std::iter::once(0).chain((lz <= 2).then_some(-1)).chain((lz >= 13).then_some(1)).collect();
        for dx in &dxs { for dz in &dzs { self.dirty.insert((cx + dx, cz + dz)); } }
    }

    // ------------------------------------------------------------ gameplay

    pub fn update_target(&mut self) {
        let reach = if self.mode == Mode::Creative { 5.0 } else { 4.5 };
        self.target = if self.screen == Screen::Playing { raycast(&self.world, self.player.eye(), self.player.look_dir(), reach) } else { None };
        self.target_mob = self.update_entity_target();
        // A mob closer than the block hides the block target.
        if let (Some((_, t)), Some(h)) = (self.target_mob, &self.target) {
            let c = DVec3::new(h.block[0] as f64 + 0.5, h.block[1] as f64 + 0.5, h.block[2] as f64 + 0.5);
            if t < (c - self.player.eye()).length() - 0.5 { self.target = None; }
        }
    }

    fn give(&mut self, id: u16, count: u8) -> bool {
        // Stack onto existing, then first empty slot (hotbar first).
        for s in self.slots.iter_mut().flatten() {
            if s.id == id && s.count < 64 { let add = (64 - s.count).min(count); s.count += add; if add == count { return true; } }
        }
        if let Some(e) = self.slots.iter_mut().find(|s| s.is_none()) { *e = Some(Stack::new(id, count)); return true; }
        false
    }

    pub fn break_block_at(&mut self, p: [i32; 3]) {
        let id = self.world.get(p[0], p[1], p[2]);
        let name = def(id).name;
        if id == 0 || name == "bedrock" && self.mode != Mode::Creative { return; }
        if self.world.set(p[0], p[1], p[2], 0) {
            self.mark_dirty(p[0], p[2]);
            self.break_particles(p, id);
            if name == "fire" { self.fires.remove(&(p[0], p[1], p[2])); }
            if self.mode == Mode::Survival {
                let held = self.slots[self.selected].map(|s| s.id);
                let (_, harvest) = crate::items::destroy_progress(held, name);
                if harvest {
                    if let Some(d) = crate::items::drop_for(name) {
                        self.spawn_item(DVec3::new(p[0] as f64 + 0.5, p[1] as f64 + 0.5, p[2] as f64 + 0.5), d, 1);
                    }
                }
                let is_tool = held.and_then(crate::items::item_def).map_or(false, |d| matches!(d.kind, crate::items::Kind::Pickaxe | crate::items::Kind::Axe | crate::items::Kind::Shovel | crate::items::Kind::Hoe | crate::items::Kind::Sword));
                if is_tool { self.damage_held(if held.and_then(crate::items::item_def).map_or(false, |d| d.kind == crate::items::Kind::Sword) { 2 } else { 1 }); }
            }
        }
    }

    pub fn place_block(&mut self) {
        let Some(h) = &self.target else { return };
        let (x, y, z) = (h.block[0] + h.normal[0], h.block[1] + h.normal[1], h.block[2] + h.normal[2]);
        let Some(stack) = self.slots[self.selected] else { return };
        if !crate::items::is_block(stack.id) { return; }
        let cur = self.world.get(x, y, z);
        if cur != 0 && def(cur).solid { return; }
        let p = self.player.pos;
        let overlaps = def(stack.id).solid
            && (x as f64) < p.x + crate::player::HALF_W && (x as f64 + 1.0) > p.x - crate::player::HALF_W
            && (z as f64) < p.z + crate::player::HALF_W && (z as f64 + 1.0) > p.z - crate::player::HALF_W
            && (y as f64) < p.y + crate::player::HEIGHT && (y as f64 + 1.0) > p.y;
        if overlaps || !self.world.set(x, y, z, stack.id) { return; }
        self.mark_dirty(x, z);
        self.swing_ticks = 0;
        if self.mode == Mode::Survival {
            let s = self.slots[self.selected].as_mut().unwrap();
            s.count -= 1;
            if s.count == 0 { self.slots[self.selected] = None; }
        }
    }

    pub fn pick_block(&mut self) {
        let Some(h) = &self.target else { return };
        let id = self.world.get(h.block[0], h.block[1], h.block[2]);
        if self.mode != Mode::Creative || id == 0 { return; }
        if let Some(i) = (0..9).find(|&i| self.slots[i].map(|s| s.id) == Some(id)) { self.selected = i; }
        else { self.slots[self.selected] = Some(Stack::new(id, 64)); }
        self.item_name_timer = 2.0;
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.mode = m;
        self.player.flying = m == Mode::Creative;
        self.health = 20.0;
    }

    /// Per-frame entry: advances the 20 TPS simulation with an accumulator.
    pub fn tick(&mut self, dt: f64) {
        let dtf = dt as f32;
        self.clock += dtf;
        for c in self.chat.iter_mut() { c.1 += dtf; }
        self.chat.retain(|c| c.1 < 30.0);
        if !self.in_world { return; }
        if !self.spawned {
            let ready = (-2..=2).all(|dz| (-2..=2).all(|dx| self.world.chunks.contains_key(&(dx, dz))));
            if !ready { return; }
            // Find dry land near the origin (spiral outward), like the vanilla spawn search.
            let water = id_of("water");
            let top_of = |g: &Game, x: i32, z: i32| -> Option<i32> {
                let mut y = world::MAX_Y - 1;
                while y > world::MIN_Y {
                    let b = g.world.get(x, y, z);
                    if b == water { return None; }
                    if def(b).solid { return Some(y); }
                    y -= 1;
                }
                None
            };
            let mut best = (8, 8, top_of(self, 8, 8));
            'search: for r in 0..40i32 {
                for dz in -r..=r { for dx in -r..=r {
                    if dx.abs().max(dz.abs()) != r { continue; }
                    let (x, z) = (8 + dx, 8 + dz);
                    if let Some(y) = top_of(self, x, z) { if y >= 64 { best = (x, z, Some(y)); break 'search; } }
                } }
            }
            let y = best.2.unwrap_or(80);
            self.spawn = DVec3::new(best.0 as f64 + 0.5, y as f64 + 1.0, best.1 as f64 + 0.5);
            self.player.pos = self.spawn;
            self.player.prev_pos = self.spawn;
            self.spawned = true;
        }
        self.tick_acc += dt.min(0.25);
        let mut steps = 0;
        while self.tick_acc >= 0.05 && steps < 5 {
            self.tick_acc -= 0.05;
            self.tick20();
            steps += 1;
        }
        self.partial = (self.tick_acc / 0.05) as f32;
        self.update_target();
    }

    fn tick20(&mut self) {
        self.time_ticks += 1.0;
        if self.item_name_timer > 0.0 { self.item_name_timer -= 0.05; }
        let playing = self.screen == Screen::Playing;
        let keys = if playing { self.keys } else { Keys::default() };
        let was_ground = self.player.on_ground;
        let creative = self.mode == Mode::Creative;
        self.player.tick(&self.world, keys, creative);
        // Fall damage on landing (survival).
        if self.player.on_ground && !was_ground || self.player.on_ground {
            let fd = self.player.fall_distance;
            if fd > 3.0 && self.mode == Mode::Survival && !self.player.flying { self.hurt((fd - 3.0).ceil() as f32); }
            self.player.fall_distance = 0.0;
        }
        if self.player.pos.y < -80.0 {
            if self.mode == Mode::Survival { self.hurt(4.0 * 0.05); } else { self.player.pos.y = self.spawn.y + 20.0; self.player.vel.y = 0.0; }
        }
        if self.mode == Mode::Survival && self.health < 20.0 && self.hunger >= 18.0 { self.health = (self.health + 0.0125).min(20.0); }
        if self.show_debug {
            self.debug_timer -= 0.05;
            if self.debug_timer <= 0.0 {
                self.debug_timer = 0.5;
                let p = self.player.pos;
                self.debug_light = self.world.light_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            }
        }
        self.update_target();
        if self.break_cooldown > 0 { self.break_cooldown -= 1; }
        if self.place_cooldown > 0 { self.place_cooldown -= 1; }
        // Left click on a mob attacks it (once per click).
        if self.left_clicked {
            self.left_clicked = false;
            if playing {
                if let Some((id, _)) = self.target_mob { self.attack_mob(id); } else if self.target.is_none() { self.swing_ticks = 0; self.attack_ticker = 0; }
            }
        }
        // Mining (ServerPlayerGameMode / MultiPlayerGameMode.continueDestroyBlock).
        if playing && self.left_down {
            if self.swing_ticks < 0 { self.swing_ticks = 0; }
            if let Some(h) = &self.target {
                let pos = h.block;
                let name = def(self.world.get(pos[0], pos[1], pos[2])).name;
                match self.mode {
                    Mode::Creative => {
                        // Vanilla: swords cannot break blocks in creative mode.
                        let sword = self.held().and_then(|s| crate::items::item_def(s.id)).map_or(false, |d| d.kind == crate::items::Kind::Sword);
                        if !sword && self.break_cooldown == 0 { self.break_block_at(pos); self.break_cooldown = 5; }
                    }
                    Mode::Survival => {
                        let held = self.slots[self.selected].map(|s| s.id);
                        let (per_tick, _) = crate::items::destroy_progress(held, name);
                        if per_tick > 0.0 {
                            let same = self.mining.as_ref().is_some_and(|m| m.pos == pos);
                            if !same { self.mining = Some(Mining { pos, progress: 0.0 }); }
                            let m = self.mining.as_mut().unwrap();
                            m.progress += per_tick;
                            let nrm = self.target.as_ref().map(|h| h.normal).unwrap_or([0, 1, 0]);
                            let bid = self.world.get(pos[0], pos[1], pos[2]);
                            self.crack_particles(pos, nrm, bid);
                            let m = self.mining.as_mut().unwrap();
                            if m.progress >= 1.0 { self.mining = None; self.break_block_at(pos); self.break_cooldown = 5; }
                        } else { self.mining = None; }
                    }
                }
            } else { self.mining = None; }
        } else { self.mining = None; }
        if playing && self.right_down && self.place_cooldown == 0 && self.target.is_some() {
            if self.use_on_block() { self.place_cooldown = 10; } else if self.held().map_or(false, |s| crate::items::is_block(s.id)) {
                self.place_block();
                self.place_cooldown = 4;
            }
        }
        self.gameplay_tick(playing);
        // ItemInHandRenderer.tick + LocalPlayer view-bob lag.
        self.prev_x_bob = self.x_bob; self.prev_y_bob = self.y_bob;
        self.x_bob += (self.player.pitch - self.x_bob) * 0.5;
        self.y_bob += (self.player.yaw - self.y_bob) * 0.5;
        self.hand_prev_height = self.hand_height;
        {
            let sel = self.slots[self.selected].map(|s| s.id).unwrap_or(0);
            let (_, speed) = self.slots[self.selected].map(|s| crate::items::weapon_stats(s.id)).unwrap_or((0.0, 4.0));
            let cooldown = (20.0 / speed).max(1.0);
            let f = ((self.attack_ticker as f32 + 1.0) / cooldown).clamp(0.0, 1.0);
            let target = if self.hand_item == sel { f * f * f } else { 0.0 };
            self.hand_height += (target - self.hand_height).clamp(-0.4, 0.4);
            if self.hand_height < 0.1 { self.hand_item = sel; }
        }
        { let mut r = std::mem::replace(&mut self.rng, worldgen::random::Xoro::new(1)); self.particles.tick(&self.world, &mut r); self.rng = r; }
        // Arm swing: 6 ticks (LivingEntity.getCurrentSwingDuration).
        self.prev_swing = self.swing_progress();
        if self.swing_ticks >= 0 { self.swing_ticks += 1; if self.swing_ticks >= 6 { self.swing_ticks = if self.left_down && playing { 0 } else { -1 }; } }
    }

    pub fn swing_progress(&self) -> f32 { if self.swing_ticks < 0 { 0.0 } else { self.swing_ticks as f32 / 6.0 } }
    pub fn swing_lerp(&self, a: f32) -> f32 { let c = self.swing_progress(); let p = self.prev_swing; if self.swing_ticks < 0 && p > 0.0 { 0.0 } else { p + (c - p) * a } }

    pub fn hurt(&mut self, amount: f32) {
        self.health -= amount;
        if self.health <= 0.0 {
            self.say("§cYou died!");
            self.health = 20.0;
            self.player.pos = self.spawn;
            self.player.vel = DVec3::ZERO;
        }
    }

    // ------------------------------------------------------------ rendering state

    pub fn camera(&self, _alpha: f32) -> Camera {
        let a = self.partial as f64;
        let mut pos = self.player.lerp_pos(a);
        pos.y += self.player.lerp_eye(a);
        let mut fov = self.settings.fov;
        // Sprinting widens the FOV; flying too (vanilla speed-based fov modifier).
        let target = if self.player.sprinting { 1.15 } else if self.player.flying { 1.05 } else { 1.0 };
        fov *= target;
        let mut yaw = self.player.yaw;
        let mut pitch = self.player.pitch;
        if self.settings.bobbing {
            // GameRenderer.bobView
            let pf = self.partial;
            let wd = self.player.prev_walk_dist + (self.player.walk_dist - self.player.prev_walk_dist) * pf;
            let bob = self.player.prev_bob + (self.player.bob - self.player.prev_bob) * pf;
            let t = wd * std::f32::consts::PI;
            let right = (-(yaw.to_radians().cos()) as f64, -(yaw.to_radians().sin()) as f64);
            let sx = (t.sin() * bob * 0.5) as f64;
            pos.x += right.0 * sx;
            pos.z += right.1 * sx;
            pos.y += (-(t.cos().abs() * bob)) as f64;
            pitch += (t - 0.2).cos().abs() * bob * 5.0;
        }
        // GameRenderer.bobHurt: tilt away from the source of damage.
        let mut roll = 0.0;
        if self.player_hurt > 0 {
            let f = ((self.player_hurt as f32 - self.partial) / 10.0).clamp(0.0, 1.0);
            let k = (f.powi(4) * std::f32::consts::PI).sin() * 14.0;
            roll = k * self.hurt_dir.sin();
            pitch -= k * 0.5 * self.hurt_dir.cos();
        }
        Camera { pos, yaw, pitch, fov_deg: fov, roll }
    }

    pub fn sky(&self) -> (SkyState, f32) {
        let day_frac = ((self.time_ticks % 24000.0) / 24000.0 - 0.25).rem_euclid(1.0);
        let d1 = 0.5 - (day_frac * std::f64::consts::PI).cos() / 2.0;
        let celestial = ((day_frac * 2.0 + d1) / 3.0) as f32;
        let phi = celestial * std::f32::consts::TAU;
        let f = (phi.cos() * 2.0 + 0.5).clamp(0.0, 1.0);
        let zenith = [0.47 * f, 0.65 * f, 1.0 * f];
        let horizon = [0.7529 * (f * 0.94 + 0.06), 0.8471 * (f * 0.94 + 0.06), 1.0 * (f * 0.91 + 0.09)];
        let cc = f * 0.9 + 0.1;
        let sky = SkyState {
            sun_dir: [-phi.sin(), phi.cos(), 0.0],
            night: 1.0 - f,
            zenith, horizon,
            cloud_color: if self.settings.clouds { [cc, cc, cc] } else { [0.0; 3] },
            cloud_scroll: self.clock * 1.2,
            cloud_height: 192.33,
            time_angle: phi,
        };
        (sky, 0.2 + 0.8 * f)
    }
}
