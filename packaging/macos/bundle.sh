#!/usr/bin/env bash
# Empaqueta el binario de macOS en Estructuras.app y de ahí saca Estructuras-<versión>-macos.dmg y
# Estructuras-<versión>-macos.app.zip.
# Uso: bundle.sh <versión> <binario> <carpeta de salida>
set -euo pipefail
version="$1"
bin="$2"
out="$3"

app="$out/Estructuras.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/structure_editor_a_game"
chmod +x "$app/Contents/MacOS/structure_editor_a_game"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Estructuras</string>
  <key>CFBundleDisplayName</key><string>Estructuras · a_game</string>
  <key>CFBundleIdentifier</key><string>dev.agame.structure-editor</string>
  <key>CFBundleExecutable</key><string>structure_editor_a_game</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Firma ad hoc: sin firma, macOS no arranca un binario arm64 (y lipo invalida la del enlazador).
codesign --force --deep --sign - "$app"

staging="$(mktemp -d)"
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Aplicaciones"
hdiutil create -quiet -volname "Estructuras $version" -srcfolder "$staging" -ov -format UDZO \
  "$out/Estructuras-$version-macos.dmg"
rm -rf "$staging"
(cd "$out" && ditto -c -k --keepParent Estructuras.app "Estructuras-$version-macos.app.zip")
