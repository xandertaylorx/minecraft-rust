#!/bin/sh
# Downloads Mojang's official 1.21.11 client jar (textures, models, lang, shaders, worldgen data)
# into assets/client.jar. The game reads vanilla resources from it; they are never committed.
set -eu
cd "$(dirname "$0")/.."
mkdir -p assets
MANIFEST=$(curl -fsSL https://piston-meta.mojang.com/mc/game/version_manifest_v2.json)
VERSION_URL=$(printf '%s' "$MANIFEST" | python3 -c 'import sys,json; m=json.load(sys.stdin); print(next(v["url"] for v in m["versions"] if v["id"]=="1.21.11"))')
INFO=$(curl -fsSL "$VERSION_URL")
URL=$(printf '%s' "$INFO" | python3 -c 'import sys,json; print(json.load(sys.stdin)["downloads"]["client"]["url"])')
SHA=$(printf '%s' "$INFO" | python3 -c 'import sys,json; print(json.load(sys.stdin)["downloads"]["client"]["sha1"])')
curl -fSL "$URL" -o assets/client.jar
echo "$SHA  assets/client.jar" | shasum -c -
echo "ok: assets/client.jar"
