# CLAUDE.md

Editor de las estructuras de sub-cubos de **a_game** (`../a_game`, Unity 6 / C#), en Rust, para artistas sin código.
Dos modos: **Explorar** (los planos procedurales del juego, con su aspecto) y **Editar** (estructuras únicas a mano que
se exportan al juego como `StructureKind.Authored`). Uso y requisitos: `README.md`.

## Reglas

- **Idioma:** comentarios, interfaz, logs, docs y commits en español; identificadores en inglés.
- **No dupliques el juego.** Paleta, nombres de materiales y elementos, tamaños, campos y empaquetado de `Variant`,
  navegación y comprobaciones salen del C# del juego a través del puente (`bridge/`). En Rust solo viven el mallado, el
  render, la UI y la tabla material → etiqueta → familia (`src/labels.rs`, **por nombre** de enum; uno nuevo cae en
  «Otros»). Si te falta un dato del juego, añádelo al puente, no a Rust.
- **Contrato:** `docs/contrato.md` es la fuente única (protocolo, carga binaria, `.agvox`, `validate`, `export`). Si lo
  cambias, cambia los dos lados en el mismo commit y sube `protocol` en `hello`.
- **Diseño:** `docs/diseno-gui.md` («Taller de cantero»). Ningún color suelto: todo sale de `src/theme.rs`. El dorado
  solo marca lo activo; nada de jerga ni nombres de enum en pantalla; tooltips en todo lo que es un icono.
- **Sin mocks ni capas «por si acaso»:** el código va contra el puente real o contra `fixtures/`.
- Commits solo cuando el usuario lo pida. En a_game, en una rama y con sus reglas (`../a_game/CLAUDE.md`).

## Comandos

- Toolchain: `export PATH="$HOME/.cargo/bin:$PATH"` (rustup 1.99; el rustc de Homebrew es viejo para egui 0.36).
- `cargo clippy --all-targets -- -D warnings` y `cargo test` (tests en línea en `#[cfg(test)] mod tests`; los de
  `tests/` leen `fixtures/` sin dotnet).
- Con el generador real (ignorados por defecto): `cargo test --test bridge_live -- --ignored`;
  `AGAME_ROOT=../a_game-autoria cargo test --test export_live -- --ignored` (exporta, recompila, construye y limpia;
  necesita un a_game con estructuras a mano).
- Puente: `dotnet build bridge/AGame.StructureBridge.csproj -c Release [-p:AGameRoot=<abs>]` (~3–10 s), luego
  `dotnet bridge/bin/Release/net8.0/AGame.StructureBridge.dll <modo>`:
  - `selftest`: ~9000 construcciones (cada estilo × campos × tamaños × orientaciones); `--hashes` imprime el FNV-1a de
    las 5 muestras de paridad y el fragmento para `unity command eval` con Unity abierto.
  - `dump fixtures`: rehace `fixtures/` (deterministas, `ms` = 0). Hazlo si cambias el catálogo o la carga.
  - `serve`: lo que lanza el editor.
- Visor suelto: `cargo run --example visor` (ventana), `-- --shots` (PNG de las fixtures en `target-render/shots/`),
  `-- --egui` (un fotograma real de `Viewport::ui`).
- **Ver la app sin permiso de grabación de pantalla:** `EDITOR_CAPTURA=/ruta.png [EDITOR_TIPO=Shop]
  [EDITOR_MODO=editar] ./target/release/structure_editor_a_game` guarda la ventana cuando ya se ve la estructura y
  sale. Lee el PNG para revisar la interfaz.
- Varios agentes a la vez: cada uno sus ficheros, su `CARGO_TARGET_DIR=target-<nombre>` (ignorado por git) y
  `rustfmt --edition 2024` solo sobre los suyos.

## Mapa

- `src/model.rs`: `VoxelGrid` (índice `(y·sz + z)·sx + x`, el de `VoxelGrid.Raw`), `Nav`, `Structure`.
- `src/bridge/`: `mod.rs` compila el puente con dotnet (busca dotnet fuera del PATH: una app del Finder no lo hereda),
  lanza el proceso y lee tramas en un hilo; `proto.rs` = tipos del contrato, `decode_build`, carga de fixtures.
- `src/host.rs`: vida del generador. Arranca la DLL que haya y recompila detrás; vigila `Assets/_Game/Scripts` con
  `notify` (espera 800 ms) y recompila; colas (lo urgente sale ya, miniaturas y muestrario de 2 en 2); `validate` y
  `export`; estado del chip.
- `src/mesh/`, `src/render/`, `src/pick.rs`: mallado greedy (port de `StructureMeshBuilder`), visor wgpu (escena toon
  con `VoxelTone`, AO de contacto, sombra dura, LUT del juego, contorno, cutaway, capas y gizmos; upscale entero
  pixel-perfect) y picking por rayo DDA en CPU. `render_image` = imágenes fuera de pantalla (miniaturas, muestrario,
  capturas).
- `src/params.rs`: parámetros de Explorar y estado de partes móviles. `src/labels.rs`: nombres en español.
- `src/agvox.rs`: formato `.agvox`. `src/edit/`: `Doc` (historial de diferencias dispersas por trazo, `version`),
  `ops` (formas, relleno, extruir, portapapeles con el giro de `RotateQuarter`), `tools` (máquina de estados de las
  herramientas). `src/editor.rs`: documento abierto, guardar/abrir, validación en vivo (300 ms), exportar, recuperación.
- `src/platform.rs`: carpeta de datos y personal, mostrar en el explorador, procesos sin consola en Windows.
- `src/app/`: `mod.rs` (estado, docks por modo, barra superior y de estado, primer arranque, Cmd+K, atajos), `view.rs`
  (Vista con controles flotantes, Muestrario, Comparar, captura), `edit.rs` (modo Editar). `src/ui/`: paneles y
  controles (`widgets.rs` tiene segmentado, paso a paso, conmutador, chip, toasts).
- `bridge/`: AGame.StructureBridge (net8.0, C# 9, **un solo ensamblado**) con los fuentes de a_game
  `Assets/_Game/Scripts/{Shared,Lang,Core,World,Generation}` + `Rendering/Terrain3D/StructurePalette.cs` y
  `bridge/src/{Program,Catalog,Builder,Checks,Authored}.cs`. Referencia `UnityEngine.CoreModule.dll` de la instalación
  de Unity y `Unity.{Mathematics,Collections}.dll` de `a_game/Library/ScriptAssemblies`.
- `docs/`: `contrato.md`, `diseno-gui.md`. `assets/fonts/`: Atkinson (dentro del binario con `include_bytes!`).

## Cosas que ya nos han mordido

- **Versiones fijadas:** eframe/egui **0.36** porque `egui_dock` 0.21 y `egui-phosphor` 0.14 van con egui 0.36. El wgpu
  es el de `eframe::wgpu` y `eframe::egui_wgpu`: no lo añadas aparte. Nombres de iconos Phosphor: compruébalos en
  `egui-phosphor/src/variants/codepoints.rs` (no existe `ROPE`, por ejemplo). Las flechas `← →` no están en Atkinson:
  usa los iconos `ARROW_*`.
- **Tema:** `theme::apply` fija `Theme::Dark` y usa `all_styles_mut`; con `global_style_mut` y macOS en modo claro,
  ventanas, popups y textos salen en tema claro.
- **Teclado:** `zoom_with_keyboard = false` (Cmd± y Cmd+0 son del visor). Cmd+C/X/V llegan como
  `Event::Copy/Cut/Paste`, no como teclas. El visor solo lee Q/R/F/P/+/−/Cmd+0 con el puntero encima; C, 1–8 y
  Shift+↑/↓ los lee la app. `NativeOptions` sin multisampling ni depth (el blit del visor es de una muestra).
- **Puente:** su stdout es **solo** tramas (el `Log` del juego se desvía a stderr). Compila el **árbol de trabajo** de
  a_game: si a_game no compila, el chip pasa a «No compila» y se sigue viendo la última DLL buena. El primer
  `catalog` tarda ~1 s; un `build` de 4 ms (casa) a ~100 ms (catedral).
- **Estructuras a mano:** el puente define `AGAME_AUTHORED` solo si existe `Authored/AuthoredDef.cs` en el a_game
  elegido (`shortcut:` en el `.csproj`, quitarlo cuando la rama `editor-estructuras-autoria` entre). Sin él, `export`
  responde `ok: false`. Tras exportar hay que recompilar el puente para que salgan en «Hechas a mano»
  (`State::handle` ya lo hace). `export` con `exported: false` = alguna fuente tiene errores y no se escribió nada.
  Las interactuables `Door` las pone el juego desde las partes: no las guardes en el `.agvox`.
- **Contrato, detalles fáciles de olvidar:** `dirs` = `+X, -X, +Z, -Z` (opuesta = `i ^ 1`; también para el `facing`
  de los interactuables); las cajas de partes e interactuables llegan ya giradas al marco de la rejilla; `width`/`depth`
  y `party` se mandan sin girar; `cut_level` del visor es el nivel del suelo de la planta (`band_floors[i]`).
- **Paridad:** si tocas el mallado o el shader, `VoxelTone` usa aritmética u32 *wrapping* exacta a la del HLSL; los
  hashes de las 5 muestras (`selftest --hashes`) deben seguir iguales a los de Unity.
- **Fixtures:** ~9 MB (`schooltower.bin` es 7,6 MB). No las rehagas sin motivo.

## Publicar una versión

- La versión es `version` de `Cargo.toml` (sale en el título y en *Ayuda* con `CARGO_PKG_VERSION`). Subirla y hacer
  push a `main`/`master` lanza `.github/workflows/publicar.yml`: si no existe el tag `v<versión>`, pasa `cargo test
  --locked` y publica tag y release con `.exe` (Windows), `.dmg` y `.app.zip` (macOS universal, firma ad hoc, sin
  notarizar) y `.sh` autoextraíble (Linux, compilado en ubuntu-22.04 por la glibc). Necesita `Cargo.lock` y
  `fixtures/` en el repo.
- Empaquetado: `packaging/macos/bundle.sh <versión> <binario> <salida>` y `packaging/linux/make_sh.sh <versión>
  <binario> <salida.sh>` (se pueden probar en local); notas de la release en `packaging/notas-release.md`.
- Una versión publicada no tiene el repo: `Paths::new` usa `bridge/` del repo si existe (`CARGO_MANIFEST_DIR`) y si no
  escribe las fuentes que lleva dentro (`BRIDGE_SOURCES`, `include_str!`) en `platform::data_dir()/bridge-<versión>` y
  compila allí. **Un `.cs` nuevo en `bridge/src` hay que añadirlo a `BRIDGE_SOURCES`** (lo vigila un test). Lo que
  cambia por sistema (carpetas, dotnet, Unity, consola de Windows) va en `src/platform.rs` y `bridge/mod.rs`.
- Antes de tocar algo de Windows o Linux: `rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-gnu` y
  `CARGO_TARGET_DIR=target-cross cargo clippy --release --target <target> -- -D warnings`.

## Tocar el lado del juego (a_game)

- Sus reglas mandan (`../a_game/CLAUDE.md`, `../a_game/docs/ciudades.md` «Estructuras a mano»): enums que solo crecen
  por el final, cada `.cs` con su `.meta`, texto visible del juego por claves de idioma (`Lang`), regla nueva en el
  servidor.
- Las piezas del juego de esta herramienta: `StructureKind.Authored`, `Generation/WorldGen1/Structures/Authored/`
  (`AuthoredDef`, `AuthoredGrid`, `AuthoredBlueprints`, `Generated/*.g.cs`, que **no se editan a mano**),
  `AuthoredBlueprintTests`, menú *Tools › Mundo › Estructuras › Estructura a mano*.
- Tests sin Unity: `python -I Tools/mp/harness/harness.py setup|sync|test <carpeta> AuthoredBlueprintTests` desde
  a_game; con Unity, `/unity-test`.
- Si hay otra sesión trabajando en a_game, haz los cambios en un worktree propio
  (`git -C ../a_game worktree add ../a_game-<tema> -b <rama>`, con un enlace a su `Library`) y no toques su árbol.
