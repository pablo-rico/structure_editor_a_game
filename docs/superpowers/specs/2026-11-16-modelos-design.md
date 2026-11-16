# Diseño: editar los modelos de vóxeles del juego («Modelos»)

Fecha: 16-11-2026. Estado: pendiente de aprobación.

## Para qué

Los artistas ya ven y editan edificios (estructuras de sub-cubos) sin código. Quieren hacer lo mismo con **el resto de
assets 3D de a_game**: árboles, rocas, cactus, cofres, ruinas, losas, el pilar de llegada y los modelos de la intro y
del libro de política. Hoy solo se pueden cambiar tocando scripts de Python. Hay que poder **retocar los que existen** y
**crear nuevos**, con las mismas herramientas, vista y deshacer que los edificios.

## Lo que hay en el juego (medido sobre los 280 ficheros)

- Todos son `*_model.json` (`VoxelModel`, `a_game/Assets/_Game/Scripts/Rendering/Terrain3D/VoxelModel.cs`), bajo
  `Assets/_Game/Art/Objects/{Voxel,Trees3D,Decor,Storage,Furniture,Lights}` y `Art/Intro/Resources/Intro/Models`.
- Campos: `cell` y `level` (vóxeles por casilla y por nivel: 16/16 en 279, 8/8 en `pbk_table`), `size` [x, y, z],
  `palette` (colores `rrggbb`, hasta 31), `faces` (5 enteros por cara expuesta: x, y, z, dirección `+x −x +y −y +z −z`,
  color) y `origin` opcional (puede ser negativo). Mayor tamaño: 80×72×57.
- **Solo guardan caras, pero el volumen se reconstruye exacto**: están todas las caras expuestas salvo las de abajo en
  y = 0 (nunca se escriben). Rellenar cada fila entre su cara `−x` y su cara `+x` da el sólido; comprobado en los 280
  (rellenar por x o por z da lo mismo). 15 ficheros tienen bolsas de aire cerradas dentro, con sus caras: se conservan.
- **El color va por cara, no por vóxel**: en 108 ficheros (intro, tropas, libro, ruinas, losas, muebles, pilar, troncos
  de `trees.py`) hay vóxeles con caras de distinto color (sombreado dibujado: tapa más clara, bajo voladizo más oscuro).
  En los otros 172 (objetos y árboles de vóxeles) cada vóxel tiene un solo color.
- Claves extra que el juego ignora al leer pero usan otros (`_`, `pivot`, `origin`, `pivotFlame`, `pageMin`, `pageMax`,
  `hinge`, `bookTilt`, `pivotBook`): hay que conservarlas tal cual.
- 88 ficheros tienen `size` mayor que lo ocupado. Paletas sin repetidos ni colores sin usar, en minúsculas.
- Los escriben 13 scripts de arte (`ArtSource/Pixel3D/*.py`, `ArtSource/Intro/*.py`, `ArtSource/PoliticsBook/models.py`,
  `ArtSource/Nature/**`), **cada uno con su propio escritor**; todos con el mismo formato de texto (cabecera, `palette`
  en una línea, una cara por línea, UTF-8; LF en git y CRLF en el árbol de trabajo de Windows). El orden de las caras
  **no** es uniforme: 152 ficheros van por (y, z, x) y dirección; los otros 128, en el orden de su script.
- Unity reimporta un JSON cambiado él solo. Cada JSON tiene su `.meta` (el juego lo referencia por GUID).
- Un modelo **nuevo** no sale en el juego solo por existir: hace falta un sprite/objeto con ese nombre y volver a lanzar
  un menú de Unity (o código, si es un objeto nuevo o un modelo de la intro).
- `VoxelModel.cs` no puede ejecutarse fuera de Unity (`JsonUtility`, `ColorUtility`, `Mesh`).

## Objetivos

1. Abrir cualquiera de los 280 modelos desde la Biblioteca, verlo con el aspecto del editor y editarlo con las
   herramientas básicas de siempre.
2. Guardarlo en su sitio en a_game de forma que **un modelo abierto y guardado sin tocar quede igual** (mismas caras,
   mismos colores, mismas claves) y que lo editado no estropee el sombreado dibujado del resto.
3. Crear modelos nuevos (vacíos o desde MagicaVoxel).
4. Que volver a lanzar un script de arte no borre en silencio lo editado a mano.

## Fuera de alcance

- Cambiar nada del juego en tiempo de ejecución, ni pasar modelos a sub-cubos.
- Enganchar un modelo nuevo a un objeto del juego (sprite, prefab, menú de Unity, código): el editor crea el fichero y
  dice qué falta; lo hace programación.
- Dibujar ni editar las copas de hojas de los 12 troncos de `trees.py`/`trees_tiered.py` (ver «Copas»).
- Modelos en el modo Explorar, Muestrario y Comparar.
- Los dibujos 2D con el mismo nombre que un modelo (el sprite del objeto, `chest.png`…): el editor no los toca ni avisa
  si dejan de parecerse al modelo editado.
- Herramientas de Construir, muebles del juego, sellos, Aspecto, puerta, plantas, partes, interactuables y validación
  de reglas de juego: no tienen sentido en un modelo.
- Paridad píxel a píxel con Unity (la de las estructuras, con hashes). Aquí el aspecto es «el del editor, sin ruido».

## Lo que fallaría aunque funcione

- Que guardar un modelo sin tocar cambie su fichero más allá del texto equivalente (diffs enormes, sombreado perdido).
- Que se pierdan claves extra (`pivot`, `hinge`…): rompería la intro sin que nadie lo vea en el editor.
- Que el modo Modelos enseñe opciones de edificio que no hacen nada, o jerga (`cell`, `faces`) en pantalla.
- Que tocar el modo Modelos cambie algo de las estructuras (mallado, hashes de paridad, `.agvox`).

## Caminos considerados

- **A. Modo «Modelos» en el editor sobre el formato actual (elegido por el usuario).** El juego no cambia y el aspecto
  se conserva; se reutiliza rejilla, mallado, visor, herramientas e historial.
- **B. Pasarlo todo a sub-cubos.** Pierde la mitad del detalle (16 → 8 por celda), obliga a mapear colores a materiales
  y a rehacer en el juego cómo se pintan 280 objetos. Descartado por caro y arriesgado.
- **C. Conversor a MagicaVoxel sin tocar el editor.** Barato, pero no se editan como los edificios ni con este visor.
  Descartado; de él solo se queda importar/exportar `.vox` como extra.

## Decisiones

Las marcadas *(mía)* las ha tomado quien escribe el diseño, no el usuario; se pueden cambiar.

### 1. El `_model.json` es la única fuente *(mía)*

No hay fichero editable aparte (nada de `.agvox` para modelos): el editor **lee y escribe el `_model.json` directamente**
en su carpeta de a_game. Es posible porque el volumen se reconstruye exacto de las caras, y evita tener dos copias que
se desincronicen. **Guardar (Ctrl+S) ya es «llevarlo al juego»**; no hay botón de exportar para modelos. La copia de
recuperación cada 2 minutos sigue, en la carpeta de datos del editor.

### 2. Color por vóxel + caras con color propio *(mía)*

La rejilla sigue siendo un `u8` por vóxel (0 = aire, `i + 1` = color `i` de la paleta del modelo; máximo 255 colores).
Para no perder el sombreado dibujado:

- Al abrir, el color de un vóxel es el más frecuente entre sus caras (empate: el de la cara de arriba si existe; si
  no, el de índice menor). Cada cara cuyo color difiera del de su vóxel se guarda en una tabla dispersa
  `(vóxel, dirección) → color` («caras con color propio»).
- Los vóxeles enterrados (sin ninguna cara) no tienen color en el fichero: lo heredan del vecino por el que se llega a
  ellos rellenando desde la superficie hacia dentro (en el orden de direcciones del formato, para que sea determinista).
  Solo importa si luego se excava; nunca se escribe mientras sigan enterrados.
- Al escribir, cada cara expuesta lleva su color propio si lo tiene, y si no el de su vóxel.
- **Añadir, borrar o pintar un vóxel borra los colores propios de ese vóxel.** Las caras que quedan al aire por borrar
  un vecino salen con el color de su vóxel. Los vóxeles sin tocar conservan su sombreado.
- El modo **Pintar** gana un conmutador **«Solo la cara»** (visible solo en modelos): pinta la cara señalada, no el
  vóxel, y así se puede dibujar sombreado nuevo. El cuentagotas coge el color de la cara señalada.
- La tabla entra en el historial (deshacer/rehacer la restaura) y en mover/copiar/espejo/giro de la selección va con
  sus vóxeles (con la dirección de cara girada o reflejada).
- El mallado usa el color propio de la cara cuando existe.

### 3. Tamaño en pantalla: el vóxel del modelo se trata como un sub-cubo *(mía)*

`SUB = 8` está en ~40 sitios, incluido el shader. No se parametriza: un modelo se enseña con cada vóxel del tamaño de un
sub-cubo (queda el doble de grande que en el juego, indistinguible con el zoom; las proporciones ancho/alto coinciden:
`1/16 : 0,8165/16` = `1/8 : 0,8165/8`). Solo leen el `cell` del modelo: las líneas de celda de la capa «rejilla», el
pincel «una celda» y la lectura «celda/nivel» de «Bajo el cursor». En código, con un comentario `shortcut:` que diga
cuándo cambiarlo (si algún día se ven modelos junto a estructuras a escala real).

### 4. Aspecto *(mía)*

Mismo visor (luz toon, AO, sombra, contorno, LUT del juego si existe). En modelos **se apaga el ruido por sub-cubo
(`voxel_tone`)** con un valor en los uniformes, porque el juego pinta los modelos sin ruido. El camino de las estructuras
no cambia y sus hashes de paridad deben seguir iguales.

### 5. Lectura y escritura del formato en Rust *(mía)*

La regla «no dupliques el juego» manda llevarlo al puente, pero `VoxelModel.cs` no corre fuera de Unity. Se porta a Rust
en un módulo nuevo (hermano de `agvox.rs`/`vox.rs`): leer, validar con las mismas reglas que `VoxelModel.Parse`,
reconstruir vóxeles, escribir. `docs/contrato.md` gana una sección «Modelos» que describe el formato y remite a
`VoxelModel.cs` como referencia. **El puente y el protocolo no se tocan** (no sube `protocol`).

Al escribir:

- Mismo formato de texto que los scripts (la cabecera original se conserva tal cual, `palette` en una línea, una cara
  por línea, UTF-8, sin caras de abajo en y = 0). Se conserva el fin de línea del fichero (LF en los nuevos) y el
  **orden original de las caras** que sigan existiendo; las caras nuevas van al final por (y, z, x) y dirección. Así un
  retoque pequeño da un diff pequeño.
- Todas las claves que no sean `size`, `palette` y `faces` se conservan con su valor y su orden (`cell`, `level`,
  `origin`, `pivot`, `hinge`…), salvo `_` (ver 7) y `origin` si se edita en Propiedades.
- Paleta: los colores originales que sigan en uso, en su orden; los nuevos al final; los que ya no se usan, fuera.
- Escritura atómica (fichero temporal y renombrar). El `.meta` existente no se toca. En un modelo nuevo se escribe un
  `.meta` de `TextScriptImporter` con un GUID nuevo, igual que los que crea Unity.
- Si la reconstrucción no cuadra al abrir (caras que no encierran un sólido, cara repetida, índice fuera de rango), el
  modelo **no se abre** y se explica por qué; nunca se abre «a medias» algo que luego se guardaría mal.

### 6. Dónde vive en la interfaz *(mía)*

- **Biblioteca › «Modelos»** (cuarta sección, en modo Editar): los modelos del juego agrupados con nombres en español
  (Árboles, Naturaleza y objetos, Ruinas y caminos, Muebles y cofres, Pilar de llegada, Intro, Libro de política) según
  su carpeta y el script de origen (`_`), con miniatura, buscador y **«Nuevo modelo…»** (nombre, grupo/carpeta y tamaño).
  Las miniaturas usan el mismo sistema que las de estructuras (2 por fotograma).
- Con un modelo abierto:
  - **Paleta**: los colores del modelo como muestras, «Añadir color» con el selector de color de egui, y cambiar un
    color existente (recolorea todo lo que lo usa). Sin familias ni materiales.
  - **Herramientas**: las Básicas completas (lápiz, caja, línea, relleno, extruir, cuentagotas, selección, mover;
    añadir/pintar/borrar; pinceles, espejo, plano). Construir y Colocar no aparecen.
  - **Planta de trabajo**: no aparece; el corte se hace con el «Plano» (K, ↑/↓) y el corte por altura del visor.
  - **Propiedades**: nombre (el del fichero; fijo en los existentes), grupo, tamaño y el **punto de apoyo** con su
    gizmo en el visor. Nada de puerta, plantas, partes ni interactuables.
    - **Tamaño**: solo crece o encoge por los lados +x, +y, +z, así **las coordenadas de lo que ya hay nunca cambian**
      (hay posiciones en vóxeles repetidas fuera del JSON: `PoliticsBookScene.cs`, los `_canopy.json`,
      `intro_art.json`). No deja encoger por debajo de lo ocupado. Para crecer hacia −x/−z el artista mueve el contenido
      con la selección, a sabiendas.
    - **Punto de apoyo** = solo `origin`; se enseña si el modelo lo tiene o es nuevo. `pivot`, `hinge` y las demás
      claves extra no se editan ni se desplazan: se conservan tal cual.
  - **Validación**: solo avisos propios, sin llamadas al puente: modelo vacío; más de 255 colores; modelo nuevo aún sin
    enganchar al juego (con el texto de qué pedir a programación); y, en un modelo existente cuyo **tamaño** haya
    cambiado, «avisa a programación: hay posiciones y pruebas del juego que dependen del tamaño de este modelo» (los
    tests de a_game exigen, por ejemplo, que cofre, saco, losas y pilar midan una celda de lado). Los modelos de la
    intro se centran solos por lo que ocupan: añadir vóxeles a un lado los desplaza; es del juego y no se avisa.
  - **Barra superior**: «EXPORTAR AL JUEGO» no aparece; Guardar dice dónde ha escrito.
- El color magenta `ff00ff` de la intro (el juego lo cambia por el color de la nación) se rotula «Color de la nación» en
  la Paleta.
- Todo texto nuevo en español y sin jerga; colores de interfaz desde `theme.rs`; tooltips en los iconos nuevos.

### 7. Que los scripts no borren lo editado *(mía; toca a_game)*

- Al guardar, el editor deja en `_` la marca `"A mano (editor de estructuras). "` delante del texto original.
- En a_game, un módulo nuevo `ArtSource/model_guard.py` con una función que, dado el destino, dice si se puede escribir
  (no existe, o su `_` no empieza por la marca) y, si no, lo avisa por pantalla y se lo salta. Los 13 escritores la
  llaman antes de escribir (y `shrine_model.js` hace la misma comprobación). Una opción `--pisar-editados` permite
  regenerar a propósito.
- El guardián solo protege el `_model.json`. Lo que un script deriva de su propia versión del modelo (la entrada de
  `intro_art.json`, las imágenes de muestra, las copas) se sigue escribiendo. Antes de dar la fase por hecha hay que
  comprobar si algo del juego lee de `intro_art.json` datos de un modelo (tamaño, `pivot`); si lo hace, parar y
  consultarlo con el usuario en vez de improvisar.
- Va en **una rama propia de a_game, en un worktree aparte**, con sus reglas. Es una fase independiente y la última: el
  editor funciona sin ella (el riesgo, mientras tanto, es que alguien relance un script; se vería en el diff de git).
- En esta máquina Python está como `python3` (el alias `python` no funciona).

### 8. Copas de los árboles mixtos *(mía; cambia lo dicho en la conversación)*

Los 12 troncos `*_trunk_model.json` tienen al lado `_canopy.json` y `_clumps.png`. El editor **no dibuja las hojas**:
enseña con los gizmos que ya existen el centro de la copa y la posición de cada racimo, como guía para editar el tronco,
y no modifica esos ficheros. (En la conversación se dijo «se verían de referencia»; dibujarlas de verdad exige un tipo
de render nuevo y se deja fuera.) Los 31 árboles enteros de vóxeles se editan completos.

### 9. MagicaVoxel *(mía)*

Con modelos, importar un `.vox` no pregunta materiales: los colores pasan tal cual a la paleta (máx. 256 por lado y 255
colores). Exportar a `.vox` escribe la paleta del modelo. Mismos ejes que hoy. Los colores propios de cara no caben en
un `.vox`: al exportar se avisa de que el sombreado por cara no viaja.

## Cómo encaja en el código

Guía, no mandato; el detalle es de quien lo construya.

- `edit::Doc` gana un tipo de documento (estructura o modelo). El modelo lleva: paleta (`Vec<Rgb>`), caras con color
  propio, las claves conservadas (texto/JSON en orden), `cell`, y la ruta. Sigue usando `Structure`/`VoxelGrid` para la
  rejilla, así las herramientas de `edit::ops` no cambian. Nada de traits ni jerarquías: una rama donde hoy se asume
  estructura.
- `mesh::Paint` se construye desde el documento cuando es un modelo (hoy siempre desde el catálogo, en 4 sitios).
- Puntos que tienen que ramificar: `Editor` (nuevo, abrir, guardar, validar, recuperación), la tira de herramientas, la
  Paleta, Propiedades, Validación, Biblioteca, la barra superior, capa de rejilla/pincel de celda, `vox`.
- La carpeta del juego es `st.host.paths.agame_root`; el editor de modelos debe funcionar aunque el generador no esté
  («Sin generador»), porque no lo usa.

## Comprobación

- **Tests sin a_game** (`cargo test`): 5 modelos pequeños copiados a `fixtures/models/` que cubran los casos: un color
  por vóxel con `origin` negativo, sombreado por cara, filas con huecos, claves extra (`pivot`, `hinge`) y `cell` 8
  (las bolsas de aire cerradas las cubre el test de los 280). Para cada uno:
  abrir → escribir da las mismas caras (como conjunto y en el mismo orden), misma paleta y mismas claves; y un test de
  edición (borrar un vóxel no cambia el color de las caras de los demás; deshacer restaura los colores propios).
- **Test con a_game real** (ignorado por defecto, como `bridge_live`): los 280 ficheros, abrir → escribir → **mismo
  texto byte a byte salvo la clave `_`**. Si algún script usa un formato de cabecera distinto que lo impida, se relaja
  a «mismo JSON» solo para esos y se anota cuáles.
- Los hashes de paridad de las estructuras (`selftest --hashes`) no cambian; `cargo clippy --all-targets -- -D warnings`.
- Revisión visual con `EDITOR_CAPTURA` de un árbol, el cofre y un modelo de la intro abiertos en el editor.
- En Unity (lo hace el usuario): retocar un modelo existente, guardar, entrar en Play y verlo cambiado.

## Fases

1. Formato: leer, reconstruir, escribir, tests de ida y vuelta (fixtures y los 280). Sección «Modelos» del contrato.
2. Documento de tipo modelo: abrir y guardar desde la Biblioteca › Modelos, visor sin ruido, Paleta de colores,
   herramientas básicas, deshacer, recuperación.
3. Caras con color propio en edición: «Solo la cara», selección/espejo/giro, cuentagotas.
4. Propiedades (tamaño, punto de apoyo), avisos, «Nuevo modelo…» con su `.meta`, guías de copa.
5. MagicaVoxel para modelos.
6. a_game: `model_guard.py` y los 13 escritores, en su rama.
7. Docs: `README.md` (sección «Modelos»), `CLAUDE.md` (mapa y «cosas que ya nos han mordido»), `docs/diseno-gui.md`.

Cada fase deja el editor funcionando; se puede parar tras la 2 y ya se editan los 172 modelos de un color por vóxel sin
pérdida, y los otros 108 conservando el sombreado de lo no tocado.

## Riesgos

- El LUT del juego recolorea los colores libres en el visor; se da por bueno porque el juego también lo aplica a toda
  la escena, pero no está comprobado que los modelos se vean igual que en Unity.
- No se ha comprobado que la ida y vuelta byte a byte sea posible con las 13 cabeceras distintas (por eso el plan B del
  test).
- En esta máquina no hay Rust instalado: hay que instalar rustup antes de construir nada.
