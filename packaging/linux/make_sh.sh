#!/usr/bin/env bash
# Crea un .sh autoextraíble para Linux: al ejecutarlo descomprime el editor en
# ~/.local/share/estructuras_a_game/<versión> (solo la primera vez) y lo arranca; con --instalar añade además el
# acceso directo al menú de aplicaciones.
# Uso: make_sh.sh <versión> <binario> <salida.sh>
set -euo pipefail
version="$1"
bin="$2"
out="$3"

tmp="$(mktemp -d)"
cp "$bin" "$tmp/structure_editor_a_game"
chmod +x "$tmp/structure_editor_a_game"
tar -C "$tmp" -czf "$tmp/payload.tgz" structure_editor_a_game

cat > "$out" <<'HEADER'
#!/bin/sh
# Estructuras · a_game __VERSION__ para Linux (x86_64).
#   sh Estructuras-__VERSION__-linux-x64.sh             descomprime (la primera vez) y arranca el editor
#   sh Estructuras-__VERSION__-linux-x64.sh --instalar  además lo añade al menú de aplicaciones
# Necesita el SDK de .NET 8, Unity en la versión de a_game y la carpeta de a_game (ver README del repo).
set -e
dest="${XDG_DATA_HOME:-$HOME/.local/share}/estructuras_a_game/__VERSION__"
if [ ! -x "$dest/structure_editor_a_game" ]; then
  mkdir -p "$dest"
  line=$(awk '/^__PAYLOAD__$/ { print NR + 1; exit }' "$0")
  tail -n +"$line" "$0" | tar -xz -C "$dest"
fi
if [ "${1:-}" = "--instalar" ]; then
  apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  mkdir -p "$apps"
  cat > "$apps/estructuras-a-game.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Estructuras · a_game
Comment=Editor de estructuras de a_game
Exec=$dest/structure_editor_a_game
Terminal=false
Categories=Graphics;Development;
EOF
  echo "Instalado en $dest y en el menú de aplicaciones."
  exit 0
fi
exec "$dest/structure_editor_a_game" "$@"
__PAYLOAD__
HEADER
sed -i.bak "s/__VERSION__/$version/g" "$out"
rm -f "$out.bak"
cat "$tmp/payload.tgz" >> "$out"
chmod +x "$out"
rm -rf "$tmp"
