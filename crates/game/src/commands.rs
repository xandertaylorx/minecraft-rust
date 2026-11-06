//! Chat commands with Brigadier-style suggestions, usage hints and vanilla feedback messages.
use glam::DVec3;

use crate::blocks::{self, display_name};
use crate::entity::Kind;
use crate::game::{Game, Mode};
use crate::items;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Arg {
    Lit(&'static str),
    /// Coordinate component: axis 0..2 of a triple.
    Coord(usize),
    Block,
    Item,
    Mob,
    Int,
    Choice(&'static [&'static str]),
    Text,
}

pub struct Spec {
    pub name: &'static str,
    pub args: &'static [Arg],
    pub doc: &'static str,
}

const MODES: &[&str] = &["survival", "creative"];
const FILL_MODES: &[&str] = &["replace", "keep", "hollow", "outline", "destroy"];
const SET_MODES: &[&str] = &["replace", "keep", "destroy"];
const DIFFS: &[&str] = &["peaceful", "easy", "normal", "hard"];
const TIMES: &[&str] = &["set", "add", "query"];
const TIME_VALUES: &[&str] = &["day", "noon", "night", "midnight"];
const QUERIES: &[&str] = &["daytime", "gametime", "day"];
const WEATHER: &[&str] = &["clear", "rain", "thunder"];
const RULES: &[&str] = &["doDaylightCycle", "doMobSpawning", "mobGriefing", "keepInventory"];
const BOOLS: &[&str] = &["true", "false"];
const LOCATE: &[&str] = &["biome"];

use Arg::*;
pub static SPECS: &[Spec] = &[
    Spec { name: "help", args: &[Text], doc: "/help [command]" },
    Spec { name: "gamemode", args: &[Choice(MODES)], doc: "/gamemode <survival|creative>" },
    Spec { name: "time", args: &[Choice(TIMES), Choice(TIME_VALUES)], doc: "/time <set|add|query> ..." },
    Spec { name: "tp", args: &[Coord(0), Coord(1), Coord(2)], doc: "/tp <x> <y> <z>" },
    Spec { name: "teleport", args: &[Coord(0), Coord(1), Coord(2)], doc: "/teleport <x> <y> <z>" },
    Spec { name: "give", args: &[Item, Int], doc: "/give <item> [count]" },
    Spec { name: "summon", args: &[Mob, Coord(0), Coord(1), Coord(2)], doc: "/summon <mob> [x y z]" },
    Spec { name: "setblock", args: &[Coord(0), Coord(1), Coord(2), Block, Choice(SET_MODES)], doc: "/setblock <x> <y> <z> <block> [destroy|keep|replace]" },
    Spec { name: "fill", args: &[Coord(0), Coord(1), Coord(2), Coord(0), Coord(1), Coord(2), Block, Choice(FILL_MODES), Block], doc: "/fill <x1> <y1> <z1> <x2> <y2> <z2> <block> [replace|keep|hollow|outline|destroy]" },
    Spec { name: "clone", args: &[Coord(0), Coord(1), Coord(2), Coord(0), Coord(1), Coord(2), Coord(0), Coord(1), Coord(2)], doc: "/clone <x1 y1 z1> <x2 y2 z2> <dx dy dz>" },
    Spec { name: "kill", args: &[Choice(&["@e", "@s"])], doc: "/kill [@e|@s]" },
    Spec { name: "clear", args: &[Item], doc: "/clear [item]" },
    Spec { name: "difficulty", args: &[Choice(DIFFS)], doc: "/difficulty [peaceful|easy|normal|hard]" },
    Spec { name: "weather", args: &[Choice(WEATHER)], doc: "/weather <clear|rain|thunder>" },
    Spec { name: "gamerule", args: &[Choice(RULES), Choice(BOOLS)], doc: "/gamerule <rule> [true|false]" },
    Spec { name: "locate", args: &[Choice(LOCATE), Text], doc: "/locate biome <biome>" },
    Spec { name: "seed", args: &[], doc: "/seed" },
    Spec { name: "spawnpoint", args: &[Coord(0), Coord(1), Coord(2)], doc: "/spawnpoint [x y z]" },
    Spec { name: "setworldspawn", args: &[Coord(0), Coord(1), Coord(2)], doc: "/setworldspawn [x y z]" },
    Spec { name: "say", args: &[Text], doc: "/say <message>" },
    Spec { name: "me", args: &[Text], doc: "/me <action>" },
];

fn mob_names() -> Vec<&'static str> { vec!["zombie", "skeleton", "creeper", "pig", "cow", "sheep", "chicken"] }
fn kind_of(n: &str) -> Option<Kind> {
    Some(match n.trim_start_matches("minecraft:") {
        "zombie" => Kind::Zombie, "skeleton" => Kind::Skeleton, "creeper" => Kind::Creeper, "pig" => Kind::Pig,
        "cow" => Kind::Cow, "sheep" => Kind::Sheep, "chicken" => Kind::Chicken, _ => return None,
    })
}

pub struct Suggestions {
    pub items: Vec<String>,
    /// Byte offset in the input where the token being completed starts.
    pub from: usize,
    pub usage: String,
}

fn tokens(input: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in input.char_indices() {
        if c == ' ' { if let Some(s) = start.take() { out.push((s, &input[s..i])); } } else if start.is_none() { start = Some(i); }
    }
    if let Some(s) = start { out.push((s, &input[s..])); }
    out
}

fn ranked(cands: impl Iterator<Item = String>, q: &str) -> Vec<String> {
    let q = q.trim_start_matches("minecraft:").to_lowercase();
    let mut pre: Vec<String> = Vec::new();
    let mut mid: Vec<String> = Vec::new();
    for c in cands {
        let l = c.to_lowercase();
        if l.starts_with(&q) { pre.push(c); } else if l.contains(&q) { mid.push(c); }
    }
    pre.sort();
    mid.sort();
    pre.extend(mid);
    pre.truncate(20);
    pre
}

fn coord_suggestions(game: &Game, axis: usize, group_start: bool) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(t) = &game.target {
        // Vanilla suggests the targeted block's coordinate for the component being typed.
        v.push(t.block[axis].to_string());
    }
    let p = game.player.pos;
    v.push([p.x.floor(), p.y.floor(), p.z.floor()][axis].to_string());
    v.push("~".to_string());
    let _ = group_start;
    v
}

/// Suggestions for the text typed so far (without the leading `/`).
pub fn suggest(game: &Game, input: &str) -> Option<Suggestions> {
    let body = input.strip_prefix('/')?;
    let toks = tokens(body);
    let ends_space = body.ends_with(' ');
    let off = 1;
    if toks.is_empty() || (toks.len() == 1 && !ends_space) {
        let q = toks.first().map(|t| t.1).unwrap_or("");
        let items = ranked(SPECS.iter().map(|s| s.name.to_string()), q);
        let usage = SPECS.iter().find(|s| s.name == q).map(|s| s.doc.to_string()).unwrap_or_default();
        return Some(Suggestions { items, from: off, usage });
    }
    let spec = SPECS.iter().find(|s| s.name == toks[0].1)?;
    let idx = if ends_space { toks.len() - 1 } else { toks.len() - 2 }; // argument index being typed
    let (q, from) = if ends_space { ("", body.len() + off) } else { (toks.last().unwrap().1, toks.last().unwrap().0 + off) };
    let arg = spec.args.get(idx).copied();
    let items: Vec<String> = match arg {
        Some(Coord(axis)) => coord_suggestions(game, axis, true),
        Some(Block) => ranked(blocks::BLOCKS.iter().skip(1).map(|b| b.name.to_string()), q),
        Some(Item) => { let mut all: Vec<String> = blocks::BLOCKS.iter().skip(1).map(|b| b.name.to_string()).collect(); all.extend(items::ITEMS.iter().map(|i| i.name.to_string())); ranked(all.into_iter(), q) }
        Some(Mob) => ranked(mob_names().into_iter().map(String::from), q),
        Some(Choice(c)) => ranked(c.iter().map(|s| s.to_string()), q),
        Some(Lit(l)) => ranked(std::iter::once(l.to_string()), q),
        _ => Vec::new(),
    };
    // Usage: the remaining arguments as <placeholders>.
    let names = |a: &Arg| match a { Coord(0) => "<x>", Coord(1) => "<y>", Coord(_) => "<z>", Block => "<block>", Item => "<item>", Mob => "<mob>", Int => "<amount>", Choice(_) => "<option>", Text => "<text>", Lit(s) => s };
    let rest_from = if ends_space { idx } else { idx + 1 };
    let usage = spec.args.iter().skip(rest_from).map(names).collect::<Vec<_>>().join(" ");
    Some(Suggestions { items, from, usage })
}

// ------------------------------------------------------------------ execution

fn rel(tok: &str, base: f64, center: bool) -> Option<f64> {
    if let Some(r) = tok.strip_prefix('~') { return Some(base + if r.is_empty() { 0.0 } else { r.parse::<f64>().ok()? }); }
    let v: f64 = tok.parse().ok()?;
    Some(if center && !tok.contains('.') { v + 0.5 } else { v })
}

fn pos3(g: &Game, t: &[&str], center: bool) -> Option<DVec3> {
    let p = g.player.pos;
    Some(DVec3::new(rel(t.first()?, p.x, center)?, rel(t.get(1)?, p.y, false)?, rel(t.get(2)?, p.z, center)?))
}

fn block3(g: &Game, t: &[&str]) -> Option<[i32; 3]> {
    let p = g.player.pos;
    let c = |s: &str, b: f64| -> Option<i32> { if let Some(r) = s.strip_prefix('~') { Some(b.floor() as i32 + if r.is_empty() { 0 } else { r.parse::<f64>().ok()?.floor() as i32 }) } else { Some(s.parse::<f64>().ok()?.floor() as i32) } };
    Some([c(t.first()?, p.x)?, c(t.get(1)?, p.y)?, c(t.get(2)?, p.z)?])
}

fn block_id(n: &str) -> Option<u16> {
    let n = n.trim_start_matches("minecraft:");
    let n = n.split('[').next().unwrap_or(n);
    blocks::BLOCKS.iter().position(|b| b.name == n).map(|i| i as u16)
}

impl Game {
    fn err(&mut self, msg: &str, line: &str, at: usize) {
        self.say("§cUnknown or incomplete command, see below for error");
        let ctx: String = line.chars().take(at.max(1)).collect();
        self.say(&format!("§7{ctx}§c§o<--[HERE]"));
        if !msg.is_empty() { self.say(&format!("§c{msg}")); }
    }

    pub fn run_command(&mut self, line: &str) {
        let body = line.trim_start_matches('/').trim();
        let toks: Vec<&str> = body.split_whitespace().collect();
        let Some(&cmd) = toks.first() else { return };
        let args = &toks[1..];
        let Some(spec) = SPECS.iter().find(|s| s.name == cmd) else {
            self.err("", line, line.trim_start_matches('/').find(' ').map(|i| i + 1).unwrap_or(line.len()));
            return;
        };
        let usage_err = |g: &mut Game| { g.say(&format!("§cUsage: {}", spec.doc)); };
        match cmd {
            "help" => {
                if let Some(c) = args.first().and_then(|a| SPECS.iter().find(|s| s.name == *a)) { self.say(&format!("§e{}", c.doc)); }
                else { let names: Vec<&str> = SPECS.iter().map(|s| s.name).collect(); self.say(&format!("§eAvailable commands: /{}", names.join(", /"))); }
            }
            "gamemode" => match args.first().copied() {
                Some("creative" | "c") => { self.set_mode(Mode::Creative); self.say("Set own game mode to Creative Mode"); }
                Some("survival" | "s") => { self.set_mode(Mode::Survival); self.say("Set own game mode to Survival Mode"); }
                _ => usage_err(self),
            },
            "time" => {
                let day = (self.time_ticks / 24000.0).floor() * 24000.0;
                match (args.first().copied(), args.get(1).copied()) {
                    (Some("set"), Some(v)) => {
                        let t = match v { "day" => Some(1000.0), "noon" => Some(6000.0), "night" => Some(13000.0), "midnight" => Some(18000.0), n => n.parse::<f64>().ok() };
                        match t { Some(t) => { self.time_ticks = day + t; self.say(&format!("Set the time to {}", t as i64)); } None => usage_err(self) }
                    }
                    (Some("add"), Some(v)) => match v.parse::<f64>() { Ok(n) => { self.time_ticks += n; self.say(&format!("Set the time to {}", (self.time_ticks % 24000.0) as i64)); } Err(_) => usage_err(self) },
                    (Some("query"), Some(w)) => {
                        let t = self.time_ticks;
                        match w { "daytime" => self.say(&format!("The time is {}", (t % 24000.0) as i64)), "gametime" => self.say(&format!("The time is {}", t as i64)), "day" => self.say(&format!("The time is {}", (t / 24000.0) as i64)), _ => usage_err(self) }
                    }
                    _ => usage_err(self),
                }
            }
            "tp" | "teleport" => match pos3(self, args, true) {
                Some(p) => { self.player.pos = p; self.player.prev_pos = p; self.player.vel = DVec3::ZERO; self.say(&format!("Teleported Player to {:.2}, {:.2}, {:.2}", p.x, p.y, p.z)); }
                None => { let at = line.find(args.first().copied().unwrap_or("")).unwrap_or(line.len()); self.err("Expected coordinates", line, at); }
            },
            "give" => {
                let name = args.first().copied().unwrap_or("");
                let count: i32 = args.get(1).and_then(|c| c.parse().ok()).unwrap_or(1);
                match items::id_by_name(name) {
                    Some(id) => {
                        let mut left = count.clamp(1, 2304);
                        let max = items::max_stack(id) as i32;
                        while left > 0 { let n = left.min(max); if !self.add_to_inventory(id, n as u8) { break; } left -= n; }
                        self.say(&format!("Gave {} [{}] to Player", count - left, display_name(items::name_of(id))));
                    }
                    None => { let at = line.find(name).unwrap_or(1); self.err(&format!("Unknown item '{name}'"), line, at + name.len()); }
                }
            }
            "summon" => match args.first().and_then(|n| kind_of(n)) {
                Some(k) => {
                    let d = self.player.look_dir();
                    let p = if args.len() >= 4 { pos3(self, &args[1..], true) } else { Some(self.player.pos + DVec3::new(d.x as f64 * 3.0, 0.5, d.z as f64 * 3.0)) };
                    match p { Some(p) => { self.spawn_mob(k, p); self.say(&format!("Summoned new {}", crate::entity::stats(k).name)); } None => usage_err(self) }
                }
                None => { let n = args.first().copied().unwrap_or(""); let at = line.find(n).map(|i| i + n.len()).unwrap_or(line.len()); self.err(&format!("Unknown entity '{n}'"), line, at); }
            },
            "setblock" => {
                let (Some(p), Some(id)) = (block3(self, args), args.get(3).and_then(|b| block_id(b))) else { return usage_err(self) };
                let mode = args.get(4).copied().unwrap_or("replace");
                if !self.world.loaded(p[0], p[2]) { return self.say("§cThat position is not loaded"); }
                let cur = self.world.get(p[0], p[1], p[2]);
                if mode == "keep" && cur != 0 { return self.say("§cCould not set the block"); }
                if self.world.set(p[0], p[1], p[2], id) { self.mark_dirty_pub(p[0], p[2]); self.say("Changed the block"); } else { self.say("§cCould not set the block"); }
            }
            "fill" => {
                let (Some(a), Some(b), Some(id)) = (block3(self, args), args.get(3..).and_then(|r| block3(self, r)), args.get(6).and_then(|b| block_id(b))) else { return usage_err(self) };
                let mode = args.get(7).copied().unwrap_or("replace");
                let filter = if mode == "replace" { args.get(8).and_then(|f| block_id(f)) } else { None };
                let (lo, hi) = ([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]);
                let vol = (hi[0] - lo[0] + 1) as i64 * (hi[1] - lo[1] + 1) as i64 * (hi[2] - lo[2] + 1) as i64;
                if vol > 32768 { return self.say(&format!("§cToo many blocks in the specified area (maximum 32768, specified {vol})")); }
                if !self.world.loaded(lo[0], lo[2]) || !self.world.loaded(hi[0], hi[2]) { return self.say("§cThat position is not loaded"); }
                let mut n = 0;
                for x in lo[0]..=hi[0] { for y in lo[1]..=hi[1] { for z in lo[2]..=hi[2] {
                    let edge = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                    let cur = self.world.get(x, y, z);
                    let place = match mode {
                        "keep" => cur == 0,
                        "hollow" => true,
                        "outline" => edge,
                        _ => filter.map_or(true, |f| f == cur),
                    };
                    let target = if mode == "hollow" && !edge { 0 } else { id };
                    if place && cur != target && self.world.set(x, y, z, target) { n += 1; self.mark_dirty_pub(x, z); }
                } } }
                if n == 0 { self.say("§cCould not set the block"); } else { self.say(&format!("Successfully filled {n} block(s)")); }
            }
            "clone" => {
                let (Some(a), Some(b), Some(d)) = (block3(self, args), args.get(3..).and_then(|r| block3(self, r)), args.get(6..).and_then(|r| block3(self, r))) else { return usage_err(self) };
                let (lo, hi) = ([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]);
                let vol = (hi[0] - lo[0] + 1) as i64 * (hi[1] - lo[1] + 1) as i64 * (hi[2] - lo[2] + 1) as i64;
                if vol > 32768 { return self.say(&format!("§cToo many blocks in the specified area (maximum 32768, specified {vol})")); }
                let mut copy = Vec::new();
                for x in lo[0]..=hi[0] { for y in lo[1]..=hi[1] { for z in lo[2]..=hi[2] { copy.push(([x - lo[0], y - lo[1], z - lo[2]], self.world.get(x, y, z))); } } }
                let mut n = 0;
                for (o, id) in copy { let (x, y, z) = (d[0] + o[0], d[1] + o[1], d[2] + o[2]); if self.world.set(x, y, z, id) { n += 1; self.mark_dirty_pub(x, z); } }
                self.say(&format!("Successfully cloned {n} blocks"));
            }
            "kill" => {
                if args.first() == Some(&"@e") {
                    let n = self.ents.mobs.len();
                    self.ents.mobs.clear();
                    self.say(if n == 1 { "Killed 1 entity".to_string() } else { format!("Killed {n} entities") }.as_str());
                } else { self.say("Ouch. That looked like it hurt."); self.hurt_player_force(); }
            }
            "clear" => {
                let filter = args.first().and_then(|n| items::id_by_name(n));
                let mut n = 0;
                for s in self.slots.iter_mut() { if let Some(st) = s { if filter.map_or(true, |f| f == st.id) { n += st.count as i32; *s = None; } } }
                if n == 0 { self.say("§cNo items were found on player Player"); } else { self.say(&format!("Removed {n} item(s) from player Player")); }
            }
            "difficulty" => match args.first().and_then(|d| DIFFS.iter().position(|x| x == d)) {
                Some(i) => { self.difficulty = i as u8; if i == 0 { self.ents.mobs.retain(|m| !m.st().hostile); } self.say(&format!("The difficulty has been set to {}", display_name(DIFFS[i]))); }
                None => { let d = DIFFS[self.difficulty as usize]; self.say(&format!("The difficulty is {}", display_name(d))); }
            },
            "weather" => match args.first() { Some(w) if WEATHER.contains(w) => self.say(&format!("Changed the weather to {w}")), _ => usage_err(self) },
            "gamerule" => match args.first() {
                Some(r) if RULES.contains(r) => match args.get(1) {
                    Some(v) => { self.rules.insert(r.to_string(), v.to_string()); self.say(&format!("Gamerule {r} is now set to: {v}")); }
                    None => { let v = self.rules.get(*r).cloned().unwrap_or(if *r == "keepInventory" { "false" } else { "true" }.to_string()); self.say(&format!("Gamerule {r} is currently set to: {v}")); }
                },
                _ => usage_err(self),
            },
            "locate" => {
                let want = args.get(1).copied().unwrap_or("").trim_start_matches("minecraft:");
                let Some(target) = self.wgen.biomes.id(want) else { return self.say(&format!("§cUnknown biome '{want}'")) };
                let (px, pz) = (self.player.pos.x as i32, self.player.pos.z as i32);
                let mut found = None;
                'o: for ring in 0..120i32 {
                    let r = ring * 64;
                    for i in -ring..=ring { for (dx, dz) in [(i * 64, -r), (i * 64, r), (-r, i * 64), (r, i * 64)] {
                        if self.wgen.biome_at(px + dx, 64, pz + dz) == target { found = Some((px + dx, pz + dz)); break 'o; }
                    } }
                }
                match found {
                    Some((x, z)) => { let d = (((x - px) as f64).powi(2) + ((z - pz) as f64).powi(2)).sqrt() as i32; self.say(&format!("The nearest {want} is at [{x}, ~, {z}] ({d} blocks away)")); }
                    None => self.say(&format!("§cCould not find a {want} within reasonable distance")),
                }
            }
            "seed" => self.say(&format!("Seed: [§a{}§r]", crate::SEED)),
            "spawnpoint" | "setworldspawn" => {
                let p = if args.len() >= 3 { block3(self, args) } else { Some([self.player.pos.x.floor() as i32, self.player.pos.y.floor() as i32, self.player.pos.z.floor() as i32]) };
                match p { Some(p) => { self.spawn = DVec3::new(p[0] as f64 + 0.5, p[1] as f64, p[2] as f64 + 0.5); self.say(&format!("Set the world spawn point to {}, {}, {}", p[0], p[1], p[2])); } None => usage_err(self) }
            }
            "say" => { let m = args.join(" "); self.say(&format!("[Player] {m}")); }
            "me" => { let m = args.join(" "); self.say(&format!("* Player {m}")); }
            _ => usage_err(self),
        }
    }

    fn hurt_player_force(&mut self) {
        let m = self.mode;
        self.mode = Mode::Survival;
        self.player_invuln = 0;
        self.hurt_player(1000.0, None, 0.0, "");
        self.mode = m;
    }
}
