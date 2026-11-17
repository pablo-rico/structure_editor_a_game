# Contrato editor ↔ juego

Fuente única del acuerdo entre el editor (Rust), el puente (`bridge/`, C#) y a_game. Si cambias algo de aquí, cambia
los dos lados en el mismo commit y sube `protocol` en `hello`.

**Regla de oro: el editor no copia datos del juego.** Paleta, nombres de materiales y elementos, rangos de tamaño,
empaquetado de `Variant`, navegación y comprobaciones salen del C# por el puente. En Rust solo vive el mallado, el render,
la UI y la tabla material → etiqueta → familia (por *nombre*; un material desconocido cae en «Otros»).

## Unidades

- Sub-cubo: 8 por lado de celda (`WorldConstants.SubPerCell`) y 8 por nivel (`SubPerLevel`).
- Rejilla: `size = [sx, sy, sz]` en sub-cubos; índice `(y * sz + z) * sx + x` (el de `VoxelGrid.Raw`). Byte = valor de
  `StructureMaterial` (0 = `Air`). Fuera de límites todo es aire.
- Espacio de vista (como `StructureMeshBuilder`): vértice `(x / 8, y / 8 · 0.81649658, z / 8)`; celda = 1 u, nivel =
  0.81649658 u (`WorldConstants.LevelHeight3D`).
- Planos sin girar: puerta en −Z, entrada = celda de fuera `(doorX, −1)`. `facing` = índice de dirección de
  `StructureInstance.Facing` (3 = −Z, sin giro); giro con `StructureRotation` (un cuarto: `(x, z) → (sizeZ − 1 − z, x)`).

## Proceso del puente

- Compilar: `dotnet build bridge/AGame.StructureBridge.csproj -c Release -p:AGameRoot=<abs> -p:UnityManaged=<abs>`
  (`UnityManaged` = carpeta con `UnityEngine.CoreModule.dll`:
  `/Applications/Unity/Hub/Editor/<m_EditorVersion de ProjectSettings/ProjectVersion.txt>/Unity.app/Contents/Resources/Scripting/Managed/UnityEngine`).
  Las DLL `Unity.Mathematics` y `Unity.Collections` salen de `<AGameRoot>/Library/ScriptAssemblies/`. Por defecto
  `AGameRoot = bridge/../../a_game` y `UnityVersion` = `m_EditorVersion` de `<AGameRoot>/ProjectSettings/ProjectVersion.txt`
  (`-p:UnityVersion=` la fuerza; de ella sale la ruta por defecto de `UnityManaged`).
- Con un a_game sin `Structures/Authored/AuthoredDef.cs` el puente compila sin estructuras a mano (sin la constante
  `AGAME_AUTHORED`): `export` responde `ok: false` y no hay grupo `authored`; `validate` funciona igual.
- Un solo ensamblado con los fuentes de los asmdef `Shared`, `Lang`, `Core`, `World` y `Generation` (el cierre de las
  referencias de Generation; si un asmdef nuevo entra en ese cierre, añade su carpeta al `.csproj`), más
  `Rendering/Terrain3D/StructurePalette.cs` y `bridge/src`. C# 9, sin `UNITY_*`. Solo se usa código gestionado de
  `UnityEngine.CoreModule` (`Vector2Int`, `Color32`, `Mathf`…); nada llama al motor nativo. Compila los fuentes **del
  árbol de trabajo** de a_game: si a_game no compila, el puente tampoco.
- Servir: `dotnet bridge/bin/Release/net8.0/AGame.StructureBridge.dll serve`. Un proceso vivo por editor; si muere, el
  editor lo relanza. Si se cierra stdin, sale con 0.
- CLI: `selftest` (todas las combinaciones del catálogo; código de salida 0/1; `--hashes` imprime `<muestra> <fnv>` del
  FNV-1a de 32 bits de `Grid.Raw` de las muestras de paridad y, en la última línea, el fragmento para
  `unity command eval -- --code '<fragmento>' --timeout 60000`, que devuelve los mismos `<muestra>=<fnv>` desde Unity) y
  `dump <carpeta>` (escribe las fixtures, ver abajo).
- stderr del puente → consola del editor (texto libre). stdout es **solo** tramas (el puente desvía `Console.Out` y el
  `Log` del juego a stderr).

## Tramas

- **Petición** (stdin): una línea JSON UTF-8 `{"id": <u64>, "op": "<op>", ...}` + `\n`. Si lleva `"payloadLen": n`,
  justo después del `\n` van `n` bytes crudos.
- **Respuesta** (stdout): `u32 LE` longitud de la cabecera · cabecera JSON UTF-8 · `u32 LE` longitud de la carga · carga.
- Toda cabecera lleva `id` y `ok`. Si `ok = false`: `"error": {"message": "<español, para artistas>", "detail":
  "<excepción completa>"}` y carga vacía.
- Las respuestas salen en el orden de las peticiones (el puente atiende de una en una).

## `hello`

`{"id":1,"op":"hello"}` → `{"id":1,"ok":true,"protocol":2,"agameRoot":"…","unityVersion":"6000.3.28f1"}`. Los dos
valores son los de la compilación (ruta absoluta de a_game y su versión de Unity).

## `catalog`

Cabecera (sin carga):

```jsonc
{
  "id": 2, "ok": true,
  "materials": ["Air", "Stone", "…"],              // índice = valor del enum
  "palette": [[r, g, b], "…"],                      // StructurePalette.Color(mat, sin tema, sin equipo), sRGB 0–255, mismo índice
  "elements": [{"name": "Aer", "label": "Aire", "palette": [[r, g, b], "…"]}],  // paleta completa con ese tema
  "teamMaterials": ["SailCloth", "TeamCloth"],      // los que se tiñen con el color del reino
  "styles": [{"name": "Northern", "label": "Norteño"}, "…"],
  "facings": [{"value": 3, "label": "Sur (−Z)", "turns": 0}, "…"],   // ordenados por turns
  "damages": [{"name": "Intact", "label": "Intacto"}, "…"],
  "groups": [{"key": "city", "label": "Ciudad"}, "…"],   // orden de presentación
  "kinds": [{
    "id": 4, "name": "Home", "label": "Casa", "group": "city", "legacy": false,
    "description": "Casa de ciudad: …",               // una línea en lenguaje llano
    "styles": ["Northern", "Mediterranean", "Arabic"],  // vacío = no depende del estilo
    "size": {"min": [w, d], "max": [w, d]},             // null = lo decide la semilla (antiguos, barcos)
    "fields": [ /* campos de variante, ver abajo */ ],
    "partStates": true                                  // tiene partes móviles que se pueden previsualizar
  }]
}
```

- `size` es la unión de las huellas de todos los diseños (sin girar). Algunos tipos tienen huella por diseño (la posada
  es mayor que la tienda, cada pieza de puerto tiene la suya, el puente de piedra es más ancho…): el puente recorta
  `width`/`depth` al rango del diseño pedido y lo dice con un aviso `size-adjusted` (las cifras reales salen en `size`
  de la respuesta). En la bocamina (`MineEntrance`) el fondo decide el largo de la galería.
- `partStates` = el tipo tiene partes con sus campos por defecto y la huella mínima (se construye una vez al pedir el
  catálogo).
- Grupos: `city`, `fort`, `bridge`, `harbour` (con el `Dock` viejo como `legacy`), `zone`, `school`, `wild`, `boat`
  (solo el casco, sin mástiles ni velas), `legacy` (`House`, `Tower`, `Castle`, `Ruin`, `Tent`) y `authored` (las
  hechas a mano exportadas: tipo `Authored`, campo `choice` `id`). Solo salen los grupos con algún tipo. Fuera:
  `PlayerBase` (la pinta la base del jugador, no una semilla).

**Campos de variante** (el puente los empaqueta en `Variant` con las funciones `Pack` del juego; Rust nunca ve bits):

| `type` | Extra | Control (docs/diseno-gui.md) |
|---|---|---|
| `index` | `min`, `max`, `maxByStyle?` `{style: max}` | «‹ Diseño n de N ›» |
| `choice` | `options: [{value, label}]` | segmentado (≤ 4) o desplegable |
| `mask` | `bits: [{bit, label}]` | casillas (puertos de torre: en cruz) |
| `bool` | — | conmutador |
| `element` | — (índices de `elements`) | rejilla 7×2 |
| `range` | `min`, `max` | deslizador entero |

Todos llevan `key`, `label` y `default`. Los parámetros comunes no son campos: `style`, `seed`, `width`, `depth`,
`facing`, `foundation`, `damage`, `dynamic`, `party`. Un campo que falta toma su `default`; un valor fuera de rango se
recorta (en `index`, al máximo del estilo). `mask` de puertos de torre: bit `i` = `dirs[i]` sin girar.

## `build`

```jsonc
{"id": 3, "op": "build", "kind": "Home", "style": "Northern", "fields": {"design": 2, "district": 3},
 "seed": 1234, "width": 6, "depth": 5, "facing": 3, "foundation": 1, "damage": "Intact", "dynamic": 0, "party": 0}
```

- `width`/`depth`: huella **sin girar** (W×D de la plantilla, como `size`); el puente la gira con `facing`.
- `party`: lados medianeros **sin girar** (bit `i` = `dirs[i]` de la plantilla); el puente los gira.
- `foundation` ≥ 1 (los sitios salvajes suben a la suya: la pirámide lleva sus sótanos en los cimientos; aviso
  `foundation-adjusted`). `style` que no admite el tipo → el primero de los suyos (aviso `style-adjusted`).
- Si `size` del tipo es null (antiguas y barcos) se ignoran `width`, `depth`, `facing`, `damage`, `dynamic` y `party`:
  se construyen sin girar con la semilla (`StructureBlueprints.Build(kind, seed, g)`, `BoatBlueprints.Build`).

Respuesta:

```jsonc
{
  "id": 3, "ok": true, "ms": 12.5,
  "size": [sx, sy, sz], "groundLevel": 1,
  "bandStarts": [8, 24], "bandFloors": [1, 3],
  "entrance": [x, z], "courtyard": false,
  "theme": null,                                   // índice de elemento si los Theme* deben teñirse (escuelas)
  "parts": [{"kind": "Door", "pattern": "Solid", "material": "Plank", "box": [x0, y0, z0, x1, y1, z1],
             "swing": 1, "detail": true, "startsOpen": false, "locked": false, "cell": [cx, cz]}],
  "interactables": [{"kind": "Bed", "box": [x0, y0, z0, x1, y1, z1], "arg": 0, "facing": 0}],
  "footprint": null,                               // o base64 de cellsX·cellsZ bytes (1 = cubierta) si hay huella con huecos
  "nav": {"levels": L, "cellsX": cx, "cellsZ": cz, "dirs": ["+X", "-X", "+Z", "-Z"]},  // StructureNav.Directions
  "checks": [{"severity": "error", "code": "door-headroom", "message": "…", "at": {"cell": [x, z], "level": 1}}],
  "payload": {"grid": n1, "detail": n2, "navFloors": n3, "navSlabs": n4, "navEdges": n5}
}
```

- **Todas las cajas** (`parts[].box`, `interactables[].box`) van en el marco de la rejilla devuelta (ya giradas),
  semiabiertas `[x0, x1)`. `parts[].cell` (`StructureBuild.PartCells`) e `interactables[].facing` (índice de `dirs`)
  también van girados; `parts[].swing` es el de la plantilla (sin girar: ±1 en el eje del grosor de la hoja). El orden de
  `parts` es el de sus 2 bits en `dynamic` (`StructureParts`). Toda puerta sale también en `interactables` (`Door`,
  `arg` = índice de la parte).
- `dirs` = `StructureNav.Directions`: **`+X, -X, +Z, -Z`** (la opuesta de `i` es `i ^ 1`; `facing` usa los mismos índices).
- `footprint`: base64 de `cellsX·cellsZ` bytes, índice `z·cellsX + x` (marco girado), 1 = la estructura cubre la celda;
  solo si `StructureFootprints.IsMasked` (lienzo diagonal, torre de muralla, puente a 45°).
- `at` puede ser `{"cell": [x, z], "level": n}`, `{"voxel": [x, y, z]}` o `null`. `severity`: `error | warning | info`.
- `checks` (las reglas de `CityBlueprintTests.BlueprintIsSound`; `code` estable, `message` para artistas):
  - `too-tall` (más de 60 niveles), `too-many-parts` (más de `StructureParts.MaxParts`), `bands-order` (franjas o
    suelos que no crecen), `entrance-inside` (la entrada cae dentro de la huella): todos los tipos con huella.
  - Solo en tipos cerrados (los que no son `IsOpen` en ciudad, fortificación, puentes y puerto; zonas salvo la cerca;
    escuelas; casa larga orca y choza goblin; casa y torre antiguas; las piezas de muralla en obra no):
    `door-headroom` (sub-cubo sólido en las 2 columnas centrales de la celda de la puerta, del borde a celda y media
    hacia dentro, en los 12 de alto libre; solo sin girar, si no sale `door-headroom-skipped` de `info`), `no-entry` (no
    se entra desde la entrada) y `floor-unreachable` (por la navegación no se llega a una franja; se miran todas menos
    la del tejado; la puerta de muralla solo la planta baja, la torre todas, los salvajes solo la planta baja).
  - Con daño (`damage` ≠ `Intact`) no se comprueban la puerta ni las plantas (`info` `damaged-skipped`).
  - `info`: `size-adjusted` (no sale en tipos de una sola huella), `style-adjusted`, `foundation-adjusted`.
- `ms`: lo que tardó el plano en el puente (sin la codificación). Orientativo, en caliente: 2–15 ms una casa o una
  torre, 15–150 ms escuelas, catedral o palacio imperial; la primera petición paga el JIT (~0,3 s).
- **Carga**, concatenada en el orden de `payload` (longitudes en bytes):
  - `grid`: `sx·sy·sz` bytes.
  - `detail`: 0 o `sx·sy·sz` bytes (1 = detalle).
  - `navFloors`, `navSlabs`: `cellsX·cellsZ` `u64 LE`, índice `z·cellsX + x`; bit N = el nivel N tiene suelo (o forjado).
    `levels` ≤ 64.
  - `navEdges`: `levels·cellsX·cellsZ` bytes, índice `(level·cellsZ + z)·cellsX + x`; bits 0–3 = paso llano a ese nivel
    bloqueado en la dirección `dirs[i]`; bits 4–7 = subir de ese nivel al siguiente (o bajar del siguiente a este)
    bloqueado en `dirs[i]` (`StructureNav.IsEdgeBlocked`). Las aristas se miran desde dentro de la celda; quien cruza
    mira las dos celdas.

## `validate`

Misma forma que una respuesta de `build` pero al revés: la cabecera lleva `size`, `groundLevel`, `bandStarts`,
`bandFloors`, `entrance`, `courtyard`, `parts`, `interactables`, `hasDetail`, `dynamic` (opcional, 0) y `payloadLen`; la
carga, la rejilla (`sx·sy·sz` bytes, valores de `StructureMaterial`) y, si `hasDetail`, el detalle detrás (otros tantos
0/1). Todo **sin girar** (puerta en −Z, entrada `[x, -1]`). Respuesta = la de `build` con `grid = 0` y `detail = 0` en
`payload` (no se devuelve la rejilla; sí la navegación).

- El puente monta la `StructureBuild` del juego (`new StructureBuild(Authored, …)`): las partes se pintan en su estado
  (`dynamic`) como en el juego (`StructureParts.Apply`), y la navegación sale de esa rejilla.
- Mismos `checks` que `build` en un tipo cerrado (las hechas a mano se entran y se recorren), y además, como `error`:
  `unknown-material` (bytes que no son un `StructureMaterial`; `at` = el primero), `bad-part` / `bad-interactable`
  (tipo, dibujo o material que el juego no tiene, o caja vacía; `at` = su primer vóxel; esa parte no se usa),
  `bad-entrance` (la entrada no es `[x, -1]` con `x` en la fachada) y `too-many-parts` (más de
  `StructureParts.MaxParts`).
- `ok: false` si la rejilla no mide celdas enteras en x y z, si la carga no casa con el tamaño o si las franjas no casan
  (`bandStarts` y `bandFloors` de distinto largo).

## `export`

`{"id": n, "op": "export", "sourceDir": "<a_game>/ArtSource/Structures"}` (sin carga). El editor asigna antes los ids
que falten (máximo + 1) y guarda los `.agvox`. El puente:

1. Lee **todos** los `*.agvox` de `sourceDir` (orden de nombre de fichero) y los pasa a `AuthoredDef` (nombres de
   enum → valores; uno que el juego no tiene es un error que dice cuál).
2. Los decodifica con `AuthoredGrid.Decode` del juego y los construye con `StructureBlueprints.Build(in s, def)` en las
   cuatro orientaciones (cimientos = `groundLevel`), con los checks de `validate`; los de las orientaciones giradas
   salen solo si traen un `code` nuevo, con «Con la puerta hacia …» y `at = null`.
3. Rechaza ids que faltan (`missing-id`) o repetidos (`duplicate-id`) y nombres de fichero que dan la misma clase
   (`duplicate-name`).
4. Si **ninguna** fuente tiene errores, escribe en `<a_game>/Assets/_Game/Scripts/Generation/WorldGen1/Structures/Authored/Generated/`
   un `Authored<Pascal>.g.cs` por fuente (`Pascal` del nombre de fichero: `casa-de-prueba` → `CasaDePrueba`; tildes y ñ
   sin ellas) y `AuthoredCatalog.g.cs`; solo reescribe los que cambian, crea el `.meta` que falte (MonoImporter) y borra
   los `Authored*.g.cs` huérfanos con su `.meta`. Si alguna falla, no escribe nada.

Respuesta (sin carga):

```jsonc
{"id": n, "ok": true, "exported": true, "outDir": "…/Authored/Generated",
 "written": ["AuthoredCasaDePrueba.g.cs", "AuthoredCatalog.g.cs"], "unchanged": 1, "removed": ["AuthoredVieja.g.cs"],
 "sources": [{"file": "ArtSource/Structures/casa-de-prueba.agvox", "id": 1, "name": "Casa de prueba",
              "checks": [/* como los de build */]}]}
```

`ok: false` solo si falta la carpeta o el a_game no tiene estructuras a mano. Lo exportado entra en el catálogo del
puente (grupo `authored`, «Hechas a mano»: el tipo `Authored` con un campo `choice` `id` = sus nombres) cuando se vuelve
a compilar el puente: tras exportar, el editor recompila y relanza el puente.

## Edición fácil y diseños del juego (fase 3)

Todo esto se añade sin romper lo anterior: campos opcionales y operaciones nuevas. `hello` sube a `protocol: 2`.
`catalog` lleva además `"districts"` y `"tiers"` (`[{"name": "Residential", "label": "Residencial"}, …]`, nombres de
`CityDistrict` y `DistrictTier` con las etiquetas del campo de variante de `Home`): son los nombres de `look` y de
`replaces.district`/`tier`.

Con un a_game sin las sustituciones (`AuthoredDef` sin `Replaces`) el puente compila sin ellas (constante
`AGAME_OVERRIDES`, como `AGAME_AUTHORED`): `build` siempre da el plano del juego y `override: null`, y un `.agvox` con
`replaces` no se exporta (`bad-replaces`). `look`, `tools`, `tool`, `furniture` y `stamp` funcionan con cualquier a_game.

### `build`: original y sustituciones

- Petición: `"procedural": true` construye el plano del juego **saltándose las sustituciones** (para ver o cargar el
  original). Por defecto `false`: se ve lo que usa el juego. `party` (bits sin girar) también cuenta para encontrar la
  sustitución.
- Respuesta: `"override": {"file": "ArtSource/Structures/ayto.agvox", "name": "Ayuntamiento norteño"}` si lo construido
  es una sustitución hecha a mano; `null` si es el plano procedural.
- Solo se sustituyen edificios de ciudad que pasan por `CityBlueprints.Construct` hasta `DoorX`: **no** el kit de
  murallas y castillo (`FortKit.IsFort`), el parque ni las piezas de puerto. El palacio imperial, siempre norteño.

### `look`

`{"op": "look", "style": "Northern", "district": "Residential", "tier": "Modest"}` → `{"roles": {…}}` con los papeles
`wall`, `timber`, `plinth`, `quoin`, `roof`, `ridge`, `verge`, `frame`, `shutter`, `floor`, `leaf`, `trim`, `chimney` y
`dome` (materiales por nombre de enum; `verge` puede ser `Air`: sin remate). Salen de los campos del estilo de las casas
tras `DistrictLook.Apply(st, barrio, clase, 0)`; donde el estilo no tiene el papel, el material con el que lo pinta su
plano:

| Papel | Norteño (`HouseStyle`) | Mediterráneo (`MedStyle`) | Árabe (`PatioStyle`) |
|---|---|---|---|
| `wall` | `Infill` | `Wall` | `Wall` |
| `timber` | `Timber` | `Leaf` | `Leaf` |
| `plinth` | `Ground` | `Plinth` | `Plinth` |
| `quoin`, `frame` | `Quoin`, `Timber` | `Trim` (o `Wall`) | `Trim` |
| `roof`, `ridge`, `verge` | `RoofMat`, `Ridge`, `Verge` | `RoofMat`, `RoofMat`, `Air` | `Trim`, `Trim`, `Air` (azoteas) |
| `shutter` | `Shutter` | `Shutter` | `Leaf` (celosía) |
| `floor`, `leaf` | `Plank`, `Plank` | `Plank`, `Leaf` | `Plank`, `Leaf` |
| `trim`, `chimney`, `dome` | `Quoin`, `ChimneyMat`, `Lead` | `DoorTrim`, `Wall`, `Terracotta` | `DoorTrim`, `Wall`, `DomeMat` |

### `tools`

`{"op": "tools"}` → `{"tools": [{"id": "room", "label": "Habitación", "icon": "HOUSE_LINE", "target": "rect",
"params": [ /* campos con el esquema de los campos de variante */ ]}, …]}`. `icon` es el nombre de la constante de
`egui_phosphor::regular`. `target`:

| `target` | Gesto | Qué manda el editor |
|---|---|---|
| `rect` | arrastrar un rectángulo de celdas | `{"rect": [cx0, cz0, cx1, cz1]}` (celdas incluidas, en cualquier orden) |
| `line` | arrastrar en línea recta | `{"rect": [...]}` de una celda de ancho |
| `face` | clic en la cara de un muro | `{"cell": [cx, cz], "dir": i}` (`dirs` del contrato; `y` se puede mandar y no se usa) |
| `cell` | clic en una celda (o arrastrar para la dirección) | `{"cell": [cx, cz], "dir": i}` |

| Herramienta | `target` | Pincel del juego | Parámetros |
|---|---|---|---|
| `room` | `rect` | `FramedStorey`/`StoneStorey`/muro liso por planta en las 4 caras, suelo `Box` en F − 1, techo | `storeys` 1–4, `walls` (entramado, piedra, liso), `floor`, `ceiling` |
| `wall` | `line` | lo mismo en una cara, en el borde sur/oeste o norte/este de las celdas | `storeys`, `walls`, `side` |
| `floor` | `rect` | `Box` en F − 1 | `stone` (`plinth` en vez de `floor`), `edge` (hasta el borde) |
| `roof` | `rect` | `Roof` con alero en el borde, `Gable` en los hastiales y cierre de muro hasta el faldón (como `CityHouse`) | `shape` (`RoofShape`), `pitch` 1–4, `gables`, `framed` |
| `window` | `face` | `Window`, `ArchWindow`, `Slit`, `RoseWindow` | `kind`, `width` 2–6, `shutters`, `flowers`, `arch`, `stained` |
| `door` | `face` | `Door` (arco rebajado) o `ArchedDoor`, hoja con `AddDoorLeaf` | `arch` (`ArchKind`), `leaf` |
| `stairs` | `cell` | `CultureBrush.Stairs` (abre el hueco), un tramo por planta: el siguiente sale del rellano, una celda a un lado, de vuelta | `floors` 1–4, `turn`, `rail` |
| `chimney` | `cell` | `Chimney` de 4×3 desde F hasta pasar lo más alto de encima | `above` 2–12 |
| `crenels` | `rect` | `FortArt.CrenelRing` o `Crenels` en un lado (materiales de `FortStyle` del estilo) | `ring`, `side` |
| `dome` | `rect` | `CultureBrush.Dome` inscrita en el rectángulo, con `Finial` | `rise` (100 media naranja, 130 peraltada, 70 rebajada), `ribs`, `finial` |
| `spire` | `rect` | `SacredBrush.Spire` y `Cross` | `height` 8–96, `octagonal`, `edges`, `cross` |

### `tool`

Cabecera = la de `validate` (el documento entero: `size`, `groundLevel`, `bandStarts`, `bandFloors`, `entrance`,
`courtyard`, `parts`, `interactables`, `hasDetail`, `replaces?`) más:

```jsonc
{"op": "tool", "name": "roof", "params": {"shape": 2, "pitch": 2}, "roles": {"roof": "RoofTile", "…": "…"},
 "target": {"rect": [0, 0, 3, 2]}, "floorY": 8, "style": "Northern", "payloadLen": n}
```

Carga = rejilla (+ detalle). Respuesta = la de `build` **con la rejilla nueva completa** (`grid` y `detail` en
`payload`) y las `parts`/`interactables` nuevas completas, más navegación y `checks` (como `validate`). El editor la
enseña como vista previa y, al confirmar, guarda la diferencia como un paso de deshacer. Si no se puede, `ok: false` con
`message` en español para artistas («Pon la ventana sobre un muro…», «Ya hay 16 partes móviles…», «Medianera: aquí no
puede haber huecos…», «Arrastra en línea recta…», «La escalera no cabe…», «El rectángulo se sale de la estructura»).

- `floorY` = altura (sub-cubos) donde se pisa en la planta de trabajo: la losa va en `floorY − 1` y la planta
  siguiente en `floorY + 16`; el tejado y las almenas, la cúpula y la aguja van **sobre** esa planta (`floorY + 16` es lo
  alto de sus muros). Por defecto, `groundLevel · 8`.
- `roles` cambia papeles sueltos; los que faltan salen de `look(style, Residential, Modest)`. `style` (opcional; por
  defecto el de `replaces` o `Northern`) da además los materiales de las almenas.
- `params` como `fields` en `build`: lo que falta toma su `default`, lo que se sale se recorta.
- Las herramientas pueden **crecer la rejilla hacia arriba** (en niveles enteros, hasta 63): `size` de la respuesta
  manda. Las plantas nuevas (`room`/`wall` de varias plantas) añaden su franja (`bandStarts` = suelo − 1,
  `bandFloors` = suelo / 8) si quedan por encima de la última.
- `window` y `door` buscan el muro desde el borde `dir` de la celda hacia dentro, a la altura de la planta: la primera
  capa sólida es su cara. Con `replaces`, no admiten huecos en un lado de `party` en el borde de la huella.
- `door` a pie de calle en la fachada sur (`dir` 3, fila 0, `floorY` = suelo de la planta baja) pasa a ser la
  **entrada** (`entrance` de la respuesta). Con la hoja, la parte nueva cuenta para `MaxParts`.
- Las partes se pintan en su estado (`dynamic`, 0) como en el juego: una hoja nueva sale abierta.

### `furniture` y `stamp`

- `{"op": "furniture"}` → `{"categories": [{"key": "bedroom", "label": "Dormitorio"}, …], "pieces": [{"id": "Bed",
  "label": "Cama", "category": "bedroom", "size": [u, v, y], "width": {"min": 4, "max": 6, "default": 5} | null,
  "interacts": true}, …]}`. Categorías (claves fijas): `bedroom` (Dormitorio), `kitchen` (Cocina y taller), `shop`
  (Tienda y banco), `church` (Iglesia), `jail` (Cárcel), `fields` (Campo y oficios), `school` (Escuela de magia),
  `decor` (Decoración). 117 piezas de `VoxelFurniture`, `SacredFurniture`, `ZoneFurniture` y
  `SchoolBlueprints.Furniture`. `size` = la pieza recortada mirando al norte (`u` = x, `v` = z).
- `{"op": "stamp", "piece": "Bed", "facing": 3, "width": 5, "seed": 0}` (**`piece`**, no `id`: `id` es el de la
  trama) → cabecera con `size`, `interactables`, `parts` (vacías), `anchor` `[x, y, z]` y `payload` con `grid` y `detail`
  de la pieza recortada (sin navegación: `navFloors` y demás a 0). `facing` = `dirs` del contrato hacia donde mira el
  mueble (la sala). `width` 0 o fuera de rango = el `default`. `seed` para las piezas con azar (colores, género).
- La pieza se pinta como **detalle** (como los muebles del juego) con `Spot.At(origen, facing)` en una rejilla aparte y se
  recorta por lo que no es aire. `anchor` = ese origen dentro de la pieza: la esquina del suelo en la que se apoya, con
  `y` = el suelo donde se pisa (la capa de abajo, `y − 1`, es el forjado). La pieza se extiende desde él según `facing`:
  norte (2) hacia +X y +Z, sur (3) hacia −X y −Z, este (0) hacia −Z y +X, oeste (1) hacia +Z y −X; las centradas en `u`
  (trono, altar, escudo…) también hacia el otro lado. Puede caer fuera de la pieza: `anchor.y` negativo en las que
  cuelgan del techo (araña, farol colgado: del forjado de una planta de 16) y mayor que 0 en las que van **en** la capa
  del suelo (círculo y estrella de runas: sustituyen esa capa).

### `validate` con `replaces`

Si la cabecera trae `replaces` (ver `.agvox`), además, como `error`: `override-size` (la rejilla no mide `replaces.size`,
o ese tamaño no está en el `SizeRange` del diseño), `override-door` (`entrance[0] != DoorX(kind, style, variant, size)`),
`override-party` (vidrio o hueco en una medianera de `party`, como `CityBlueprintTests`; `at` = el primero),
`override-counter` (Tienda de tipo `Store`/`Inn` o Banco sin mostrador: `VendorSpots.TryFind`), `override-courtyard`
(patio donde el plano del juego no lo tiene) y `bad-replaces` (tipo que no es de ciudad, nombres que el juego no tiene).
El `Variant` de las comprobaciones: el `design` con rango, barrio y clase (los `null`, corriente/residencial/modesta).

### `export` con sustituciones

Las fuentes con `replaces` se exportan como sustituciones (el juego las usa en `CityBlueprints.Construct` cuando
encajan: mismo tipo, estilo, diseño, tamaño y medianeras, y rango/barrio/clase si no son `null`; gana la que fija más de
los tres y, a igualdad, la de id menor; los templos árabes usan las mediterráneas). Además de las comprobaciones de
`validate`: `duplicate-override` si dos fuentes sustituyen exactamente lo mismo y `override-style` si el `style` de la
fuente no es el de `replaces`. Cada `.g.cs` lleva `source:` y, si sustituye, `replaces:`, `design:`, `party:` y los
`rank:`/`district:`/`tier:` que fije (por nombre de enum).

- **Clave de diseño** (`replaces.design`): en `Home` el campo `design` (`CityHouseVariant.Design`), en `Shop` el campo
  `shop` (`ShopStyle`), en el resto el `Variant` en bruto, que en el catálogo es: el campo `design` (Ayuntamiento,
  Cárcel, Puesto, templos), `prop` (mobiliario urbano), `oldGate`/`gateTower` (0/1: Pasaje, Puente de ciudad) y 0 en los
  que no tienen campos. `replaces.rank` es el entero del rango (0–2; solo `Home`); `district`/`tier`, solo `Home` y `Shop`.

## `shutdown`

`{"id":n,"op":"shutdown"}` → responde `ok` y sale.

## Fixtures

`dump <carpeta>` escribe, para cada muestra, `<nombre>.json` (la cabecera de `build`, con la petición en `"request"`) y
`<nombre>.bin` (la carga). Muestras fijas (también son las de paridad con Unity): `home_northern`, `shop_arabic`,
`walltower_ports`, `schooltower`, `harbour`, más el `catalog` en `catalog.json`. Van en `fixtures/` del editor y los tests
de Rust las leen sin dotnet (`tests/fixtures.rs`). En las fixtures `ms` = 0, para que un volcado nuevo solo cambie si
cambia el juego.

| Muestra | Petición | FNV-1a de `Grid.Raw` (puente = Unity, 2026-11-16) |
|---|---|---|
| `home_northern` | `Home` Norteño 5×5, diseño 2, altura corriente, residencial, modesta, semilla 1234 | `76da07d6` |
| `shop_arabic` | `Shop` Árabe 6×7, tienda, mercado, acomodada, semilla 4321 | `a00c1e14` |
| `walltower_ports` | `WallTower` Norteño, subida 1, salidas +X −X +Z, semilla 77 | `03d9d7ce` |
| `schooltower` | `SchoolTower` de Fuego (Ignis), semilla 99 | `d2fbf342` |
| `harbour` | `Harbour` caseta del capitán de puerto (mesa, silla, cofre, puerta), semilla 5 | `0f47ecd7` |

Todas sin girar, `foundation` 1, intactas. Si un cambio del juego mueve un hash, vuelve a volcar las fixtures.

## Formato `.agvox`

Fuente de una estructura hecha a mano; vive en `a_game/ArtSource/Structures/<slug>.agvox`. JSON:

```jsonc
{
  "format": "agvox", "version": 1,
  "id": 7,                              // estable; se asigna al primer export (máximo + 1); nunca se reutiliza
  "name": "Torre del puerto", "style": "Northern",
  "size": [sx, sy, sz], "groundLevel": 1,
  "bands": [[8, 1], [24, 3]],           // [bandStart, bandFloor]
  "entranceX": 3, "courtyard": false,
  "materials": ["Air", "Stone", "Plank"],   // tabla local: índice local → nombre del enum
  "voxels": "<base64>",                 // RLE: tramos (u8 índice local, u16 LE largo 1–65535) en el orden de Raw
  "detail": null,                       // o base64 del mismo RLE sobre 0/1
  "parts": [{"kind": "Door", "pattern": "Solid", "material": "Plank", "box": [..6], "swing": 1,
             "detail": true, "startsOpen": false, "locked": false}],
  "interactables": [{"kind": "Bed", "box": [..6], "arg": 0, "facing": 0}],
  // Opcionales (fase 3):
  "replaces": {"kind": "Home", "style": "Northern", "design": 2, "size": [5, 5], "party": 0,
               "rank": null, "district": null, "tier": null},   // sustituye a ese diseño del juego (null = cualquiera)
  "look": {"district": "Residential", "tier": "Modest", "roles": {"wall": "Plaster"}},  // Aspecto de las herramientas
  "recipe": {"file": "ArtSource/Structures/torre.rhai", "fingerprint": "55697b2a4208b3d9"}  // la generó una receta
}
```

Sin girar (puerta en −Z). Guardar y volver a abrir da los mismos bytes.

`recipe` lo escribe `cantero` cuando una receta generativa rehace la estructura (`docs/recetas.md`): la receta y la
huella de lo que dejó (`Doc::fingerprint`). El editor lo conserva al guardar; si la huella ya no casa con el contenido,
la estructura se ha retocado a mano y la receta no la pisa. El juego y el puente lo ignoran (`export` lee solo los
campos que conoce).

## Exportación al juego

- Lo hace el puente con `export` (arriba). Cabecera `// <auto-generated>` con la ruta de la fuente: no se editan a mano.
- Materiales y partes por nombre (`M.Stone`, `PartKind.Door`): un enum renombrado rompe la compilación en vez de pintar
  otra cosa.
- En el juego (`Generation/WorldGen1/Structures/Authored/`, docs/ciudades.md de a_game, «Estructuras a mano»):
  `StructureKind.Authored` (al final del enum), `Variant` = id; en las APIs por semilla (`Footprint`, `Entrance`,
  `WorldEdit.PlaceStructure`) la semilla es el id. Con más cimientos que `groundLevel` se repite la capa de abajo (con
  menos, se recorta); franjas, partes e interactuables suben con ella. Las interactuables `Door` de la fuente se ignoran:
  las pone `AddPart`. La huella es la de la def y no hay azar.
- Forma de cada `.g.cs` (una clase estática por fuente; el catálogo solo las enumera, así no depende del orden de
  inicialización entre ficheros):

```csharp
public static class AuthoredCasaDePrueba
{
    public static readonly AuthoredDef Def = new AuthoredDef(
        id: 1, name: "Casa de prueba", style: ArchStyle.Northern,
        sizeX: 40, sizeY: 120, sizeZ: 40,        // sub-cubos; x y z múltiplos de 8
        groundLevel: 1,
        bandStarts: new int[] { 0, 23, 39, 55 }, bandFloors: new int[] { 1, 3, 5, 7 },
        entranceX: 2, courtyard: false,
        materials: new StructureMaterial[] { M.Air, M.Cobble, /* … tabla local del .agvox */ },
        voxels: string.Concat(new[] { "AcArAnsA…", /* el base64 del .agvox en trozos de 100 */ }),
        detail: string.Concat(new[] { "…" }),    // o null si no hay detalle
        parts: new VoxelPart[] { new VoxelPart(PartKind.Door, PartPattern.Solid, M.Plank, 18, 8, 2, 23, 19, 3,
            swing: 1, detail: false, startsOpen: true, locked: false) },
        interactables: new VoxelInteractable[] { new VoxelInteractable(InteractKind.Bed, 29, 24, 3, 34, 29, 13,
            arg: 0, facing: 2) });
}
//     source: "ArtSource/Structures/casa-de-prueba.agvox",
//     replaces: StructureKind.TownHall, design: 0, party: 0, rank: 1, district: CityDistrict.Noble, tier: DistrictTier.Rich
//     (estos solo si sustituye; rank/district/tier, los que fije)
// AuthoredCatalog.g.cs: public static readonly IReadOnlyList<AuthoredDef> All = new AuthoredDef[] { AuthoredCasaDePrueba.Def, … };
```

- Límite de la fase 2b: toda estructura a mano tiene que entrarse por su puerta y llegar a sus plantas (los checks de
  un tipo cerrado son errores, en `export` y en `AuthoredBlueprintTests`). Las piezas abiertas (fuentes, puestos)
  necesitarán un campo `open` en el `.agvox`.

## Modelos (`*_model.json`)

Los assets 3D del juego que no son estructuras (árboles, objetos, ruinas, muebles, intro, libro de política) son
`VoxelModel` (`a_game/Assets/_Game/Scripts/Rendering/Terrain3D/VoxelModel.cs`, la referencia). **El puente no
interviene**: `VoxelModel.cs` no corre fuera de Unity, así que el editor lee y escribe el fichero del juego
directamente (`src/vmodel.rs`), y ese fichero es la única fuente (no hay `.agvox` de modelos).

Campos: `cell` y `level` (vóxeles por casilla y por nivel, > 0), `size` `[x, y, z]`, `palette` (colores `rrggbb`),
`faces` (cinco enteros por cara expuesta: `x, y, z`, dirección `0 +x, 1 −x, 2 +y, 3 −y, 4 +z, 5 −z` y color) y
`origin` opcional (punto del modelo que cae en la esquina de la huella; puede ser negativo). El resto de claves (`_`,
`pivot`, `hinge`, `pivotFlame`, `pageMin`, `pageMax`, `bookTilt`, `pivotBook`) no son del editor y se conservan tal cual.

Al leer (mismas reglas que `VoxelModel.Parse`, más las del editor):

- Cada cara dentro de `size`, con dirección 0–5 y color dentro de la paleta; sin caras repetidas. Límites del editor:
  256 vóxeles por lado y 255 colores.
- El sólido se reconstruye por filas: en cada `(y, z)`, una cara `−x` abre y una `+x` cierra. Las caras expuestas de
  ese sólido tienen que ser exactamente las del fichero; las de abajo en `y = 0` nunca se escriben. Si no cuadra, el
  modelo no se abre.
- El color es por cara. El de un vóxel es el más frecuente entre sus caras (en empate, el de la cara de arriba si
  empata; si no, el de índice menor); las caras que difieren van aparte (`Structure.face_colors`). Los vóxeles
  enterrados heredan el color del vecino por el que se llega a ellos desde la superficie.

Al escribir:

- La cabecera original no se reserializa: solo cambian los valores de `size`, `palette` y `origin`, y `_` gana delante
  la marca `A mano (editor de estructuras). `. **Hoy ningún script de arte de a_game mira esa marca**: relanzar uno
  pisa el modelo (pendiente en a_game: que se los salten).
- Un modelo que rehace una receta generativa de `cantero` lleva en `_`, tras la marca, `Receta: <ruta>; huella
  <16 hex>.`: la huella de lo que la receta dejó (`docs/recetas.md`).
- `palette`: solo los colores que lleva alguna cara, en su orden; los nuevos al final.
- `faces`: una por línea (`    x, y, z, d, c`); las que ya estaban, en su orden original (no es uniforme entre
  scripts); las nuevas al final por `(y, z, x, dirección)`. Se conserva el fin de línea del fichero.
- Un modelo abierto y guardado sin tocar queda idéntico byte a byte salvo la marca (`tests/models_live.rs` lo
  comprueba con todos los del juego).
