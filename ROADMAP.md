# Rust rewrite roadmap

Goal: replace the Java game entirely with Rust, same look and feel, then improve it.
Vanilla textures/sounds/lang are reused from Mojang's pinned asset download (same as the Java build), so visuals match by construction; everything else is reimplemented.

Rules: every ported subsystem is checked against golden output produced by the real Java code before the Java it replaces is deleted. Java stays in the repo until the final milestone.

| # | Milestone | Done when |
|---|-----------|-----------|
| 1 | `universe` catalogue (seeded systems) | DONE: matches Java golden output |
| 2 | Planet terrain/environment functions, atmosphere, clouds, layout, flight physics, pulse nav, stream planner, positions | DONE: all match real Java output (terrain 43k blocks; 400-step flight bit-exact). Ecology/fauna/survival ticks need entities, so they live in milestones 7 and 9 |
| 3 | Vanilla worldgen: noise, biomes, density functions, carvers, features | Same chunks as Java for a fixed seed |
| 4 | Chunk storage + Anvil/NBT region read/write | NBT + region read/write DONE (reads Java-written NBT; round-trips regions). Still to do: chunk section/palette decoding, LZ4 and external `.mcc` chunks, `level.dat` schema, real-save test |
| 5 | Renderer (wgpu): block models, resource packs, lighting, sky, clouds, universe shaders | Screenshot parity with Java client |
| 6 | Client: input, GUI/menus, fonts, sound | Main menu → world |
| 7 | Server: ticking, blocks, redstone, fluids, entities, AI, recipes, loot, commands | Gameplay smokes pass |
| 8 | Protocol 1.21.11 (LAN, multiplayer) | Joins Java-era servers/clients |
| 9 | Custom content: Tether, Tetherlings, bows, Mjolnir, spaceflight | All smoke tests ported |
| 10 | Delete `src/main/java`, Gradle build, Java toolchain | `cargo run` is the game |

Honest scale: steps 3, 5 and 7 are each person-years of work in vanilla Minecraft. Expect this to run across many sessions.

## Status of the playable client (crates/game + crates/worldgen)

Verified against the real Java classes (golden files + parity tests): noise/random primitives, every noise-router density
function (final density, caves, veins, climate), cell-interpolated terrain columns including aquifer water/lava and ore-vein
blocks. Biome selection uses a parameter table dumped from the Java builder (tests/GoldBiomes data) plus a Rust port of
BiomeManager; surface rules are loaded from Mojang's noise_settings JSON.

Working in the client: vanilla overworld terrain (biome-tinted grass/foliage/water with 5x5 blending), wgpu renderer with
vanilla lightmap curve + smooth lighting/AO, sky (gradient, sunrise glow, sun, moon, stars), clouds, day/night cycle,
vanilla bitmap font, title/pause/options screens, creative + survival inventories, hotbar/hearts/hunger HUD, F3 debug screen,
chat + commands, survival mining with crack stages and held-item swing, creative flight, sprint/sneak, fall damage,
view bobbing, headless screenshot mode.

Not yet: trees/flowers/grass/features, carver caves and canyons, structures (villages etc.), mobs/entities, sounds, saving
and loading worlds, non-cube blocks (stairs, slabs, plants, torches), fluid flow, redstone, crafting, tools and item
durability, weather, multiplayer, the Tether dimension, spaceflight in the client, porting OverworldBiomeBuilder to Rust
(the table is currently extracted from Java), exact vanilla snow-line temperature noise.

## Gameplay layer (done in crates/game)

Items and tools (vanilla tiers, speeds, durability, attack damage/speed), survival mining with correct tool rules and drops as
item entities, melee combat (cooldown scaling, crits, knockback, invulnerability frames), bows (charge -> power -> arrow speed),
arrows with gravity/drag/sticking, flint and steel + spreading fire blocks, burning (mobs and player), lava, explosions
(ray-cast block destruction + damage), dropped items and pickup, food, hunger, mob loot.

Mobs: zombie, skeleton, creeper, pig, cow, sheep, chicken with vanilla stats, box models + walk/hurt/death animations,
a goal selector ported from the Java goal system (float, stroll, look, panic, melee, bow attack, swell, nearest-target,
hurt-by-target), A* navigation, move/look control, sunburn, natural spawning (passive herds per chunk, hostiles at night/caves).

## Next

1. Terrain features (trees, flowers, grass, carver caves, structures), non-cube blocks.
2. More mobs (spider, enderman, villagers...), breeding/taming, mob drops XP, armor and enchanting.
3. Crafting, furnaces, containers; redstone; fluids that flow.
4. Sound, particles, saving/loading worlds, multiplayer.
