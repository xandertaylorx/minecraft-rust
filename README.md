# Minecraft in Rust

> Unofficial fan project. Not affiliated with or endorsed by Mojang or Microsoft. "Minecraft" is a trademark of Mojang AB.
> No Mojang assets are included in this repository: `scripts/fetch-assets.sh` downloads the official client jar from Mojang's servers
> on your machine, and the game reads textures, sounds-less resources and worldgen data from it at runtime. You need a legitimate copy of the game.

A from-scratch Rust rewrite of Minecraft Java Edition 1.21.11, using Mojang's own textures,
colormaps and HUD sprites so it looks the same. World generation is ported from the Java universe code and
verified against output of the real Java classes.

## Run

```sh
./scripts/fetch-assets.sh          # once: downloads the official client jar into assets/ (not committed)
cargo run -p game --release
```

Controls (vanilla): WASD move, mouse look, Space jump (double-tap in creative to toggle flight), Shift sneak/descend,
Ctrl or double-tap W sprint, 1-9 / wheel hotbar, left click break (hold), right click place, middle click pick block,
E inventory, T chat, / command, Esc pause, F1 hide HUD, F2 screenshot, F3 debug screen, F11 fullscreen.

Commands: /help /gamemode /time /tp /give /setblock /fill /clear /seed /kill /spawn /say.

Headless screenshot (for checking rendering without a window):

```sh
cargo run -p game --release -- --shot out.png --pos 8.5 100 8.5 --look 30 10 --radius 8 [--time 1000] [--size 1280 720] \
  [--screen title|pause|options|creative|inventory|chat] [--f3] [--chat] [--mode survival] [--mine] [--swing]
```

## Layout

- `crates/universe` - the Java universe/space code (catalogue, planet terrain, flight physics), tests compare with real Java output
- `crates/worldgen` - vanilla overworld generation: Xoroshiro/noise, density functions loaded from Mojang's JSON, cell-interpolated terrain,
  aquifers, ore veins, climate/biomes, BiomeManager zoom, surface rules. Verified against the real Java classes.
- `crates/nbt` - NBT and Anvil region files
- `crates/game` - wgpu renderer, chunk streaming, meshing with smooth lighting/AO, physics, HUD

See `ROADMAP.md` for what is done and what is not.
