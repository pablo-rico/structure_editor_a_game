# Estructuras · a_game

Taller para **ver, comparar y hacer a mano** las estructuras de sub-cubos de **a_game**: casas, tiendas, murallas,
castillos, puentes, puertos, escuelas de magia, campamentos, barcos… Con el mismo aspecto que en el juego y sin abrir
Unity.

En a_game las estructuras no son ficheros: son *planos* en C# que se construyen a partir de una semilla. El editor no
los copia ni los imita: lanza un **generador** (`bridge/`) que compila esos mismos planos del juego con `dotnet` y le
pide cada estructura. Lo que ves aquí es exactamente lo que construye el juego (comprobado byte a byte contra Unity), y
si alguien cambia un plano en a_game, el editor recompila solo y enseña la versión nueva.

## Descargas

Cada versión publicada está en **Releases** del repositorio:

| Sistema | Fichero |
|---|---|
| Windows (x64) | `Estructuras-<versión>-windows-x64.exe` (no está firmado: SmartScreen puede pedir «Ejecutar de todas formas») |
| macOS (Apple Silicon e Intel) | `Estructuras-<versión>-macos.dmg` o `…-macos.app.zip` (no está notarizado: la primera vez, clic derecho › Abrir) |
| Linux (x86_64) | `Estructuras-<versión>-linux-x64.sh`: `sh` lo descomprime y lo arranca; con `--instalar` lo añade al menú |

La versión publicada lleva dentro las fuentes del generador: la primera vez las escribe en la carpeta de datos del
editor (`~/Library/Application Support/structure_editor_a_game` en macOS, `%APPDATA%\structure_editor_a_game` en
Windows, `~/.local/share/structure_editor_a_game` en Linux) y las compila allí con `dotnet`. La versión del editor sale
en el título de la ventana y en *Ayuda*.

## Requisitos

- El **SDK de .NET 8** (`dotnet`) y, para compilar desde el código, Rust instalado con rustup (`~/.cargo/bin`).
- **Unity** en la versión del proyecto (la de `a_game/ProjectSettings/ProjectVersion.txt`) instalado desde Unity Hub en
  su sitio de siempre (o su carpeta `Managed/UnityEngine` en `UNITY_MANAGED`), y a_game **abierto en Unity al menos una
  vez** (el generador usa sus `Library/ScriptAssemblies`).
- La carpeta de a_game junto a este repo (`../a_game`) o la que elijas al arrancar.

## Arrancar

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release
```

El primer arranque compila el generador (unos segundos), pregunta si usas **ratón o trackpad** y abre la Casa. Si no
encuentra el juego, te pide la carpeta (también en *Archivo › Elegir la carpeta del juego…*, o con `AGAME_ROOT`).
**F1** enseña todos los atajos.

## Explorar

Para ver lo que generan los planos del juego.

- **Catálogo** (izquierda): todos los tipos por grupos (ciudad, fortificación, puentes, puerto, zonas, escuelas,
  salvajes, barcos, antiguas y, si las hay, hechas a mano). Filtra por nombre o busca con **Cmd+K**.
- **Opciones** (derecha): estilo (norteño, mediterráneo, árabe), diseño, barrio, clase, tamaño, orientación,
  cimientos, daño y **semilla** (← → la cambian, **D** pone una al azar). En «Estado (vista previa)» abres o cierras
  puertas, bajas rastrillos o rompes muros (no se guarda en el diseño).
- **Vista**: cámara isométrica como la del juego. **Q / R** giran 90°, **F** encuadra, **P** cambia a vista libre,
  rueda o pellizco hace zoom. **C** corta por plantas y **Shift+↑/↓** cambia de planta (o el ascensor de la derecha).
  Las teclas **1–8** encienden capas: rejilla, navegación (por dónde se anda; la valla roja es paso cortado), partes
  móviles, interactuables, entrada, huella, detalle y franjas.
- **Bajo el cursor**: material, celda, nivel y si se anda por ahí. Clic en el visor lo fija. Debajo, las
  **comprobaciones** del plano (se entra por la puerta, se llega a todas las plantas…) con «Localizar».
- **Muestrario**: rejilla de semillas, con filas que varían el estilo, el daño o el diseño. Clic abre una en la Vista;
  **Cmd+clic** la manda a **Comparar** (dos visores con la cámara enlazada).
- **Captura** (Cmd+Shift+I) guarda una imagen en `~/Pictures/Estructuras/`.

## Editar

Para hacer estructuras **únicas** a mano o retocar las del juego.

- **Biblioteca** (pestaña junto a la Paleta):
  - *Estructuras*: tus `.agvox` con miniatura, los recientes, «Nueva» e «Importar de MagicaVoxel…».
  - *Del juego*: cualquier diseño del juego (tipo, estilo, diseño, tamaño, medianeras y semilla) para **copiarlo como
    estructura nueva** o para **editar ese diseño del juego** (ver abajo).
  - *Piezas*: los muebles del juego por categorías (cama, mesa, mostrador, altar…) y tus sellos.
- **Planta de trabajo** (arriba del visor, `Todo | Baja | 1 | 2 | +`): corta la vista en esa planta y construyes sobre su
  suelo. «+» añade una planta encima.
- **Herramientas** (tira de la izquierda):
  - *Básicas*: Lápiz **B**, Caja **U** (llena, hueca o solo paredes), Línea **L**, Relleno **G** (hasta esta planta o
    toda), Extruir **X**, Cuentagotas **I** (o Alt+clic), Selección **M**, Mover **V**. Con los modos **Añadir (A)**,
    **Pintar (N)** —cambia el material sin añadir ni quitar— y **Borrar (E)**. Pincel de 1, 2, 4 sub-cubos o una celda
    (**, .**), espejo en X y Z, «Plano» (**K** lo pone en la cara señalada; **↑/↓** lo mueve) y «Detalle» (muebles y
    escaleras).
  - *Construir*, con los pinceles reales del juego: Habitación **H**, Muro **W**, Suelo **S**, Tejado **T**, Ventana,
    Puerta, Escalera, Chimenea, Almenas, Cúpula y Aguja. Arrastra (o haz clic en un muro): ves el resultado antes de
    soltar y se confirma al soltar, en un solo paso de deshacer. Los materiales salen del **Aspecto** (estilo, barrio y
    clase) y puedes cambiar cada papel (muro, tejado, marcos…) con la Paleta.
  - *Colocar*: el mueble o sello elegido en la Biblioteca va en la mano; clic donde va y **O** para girarlo.
- Con una selección: **flechas** para moverla (Shift: una celda; Alt+↑↓: altura), **Cmd+C / X / V / D**, **Shift+X / Z**
  espejo, **Cmd+R** girar, **Supr** vaciar y «Guardar como sello…» en Propiedades.
- **Propiedades**: nombre, estilo, tamaño, la celda de la **puerta** (siempre en la fachada sur), las **plantas**, las
  **partes móviles** (máximo 16) y los **interactuables**.
- **Validación** en vivo con las reglas del juego, **deshacer** sin límite (Cmd+Z), **Historial**, **Cmd+S** y una copia
  de recuperación cada 2 minutos. **Esc** cancela lo que estés haciendo.

## Editar los diseños del juego

En Explorar, **EDITAR ▾ › Editar este diseño del juego** (o Biblioteca › Del juego) carga un diseño de ciudad —por
ejemplo el Ayuntamiento norteño, diseño 1, de 10×10— para retocarlo. Al exportarlo, **el juego usa tu versión en todas
las ciudades donde ese diseño salga** con ese tamaño y medianeras; en Propiedades puedes acotarlo a un barrio, una clase
o una altura. Mientras lo editas no se pueden cambiar el estilo, el tamaño ni la celda de la puerta (el juego los fija),
las medianeras deben quedar sin huecos y tiendas y bancos necesitan mostrador. En Explorar verás «Diseño editado a mano»
con un botón para **ver el original**. «Convertir en estructura propia» deja de sustituirlo.

## Modelos: árboles, objetos, intro…

Lo que en el juego es 3D pero no es un edificio (árboles, rocas, cactus, cofres, ruinas, losas, el pilar de llegada y
los modelos de la intro y del libro de política) son **modelos de vóxeles**: más finos que los sub-cubos (16 por
casilla) y con colores libres en vez de materiales. Se editan igual que una estructura, con estas diferencias:

- **Biblioteca › Modelos** los enseña por grupos, con buscador. Clic abre uno. «Nuevo modelo» empieza uno vacío e
  «Importar de MagicaVoxel…» trae un `.vox` con sus colores tal cual.
- **Paleta**: los colores del modelo. «Añadir color» pone uno nuevo; el selector de arriba cambia el color elegido en
  todo el modelo. El magenta de los modelos de la intro es el «color de la nación» (el juego lo cambia por el del reino).
- **Herramientas**: las Básicas. No hay Construir, muebles, plantas, puerta ni partes: son cosas de edificios.
- **Sombreado dibujado**: muchos modelos (intro, ruinas, cofres, troncos) llevan luces y sombras pintadas cara a cara.
  Lo que no tocas las conserva. Pintar o borrar un vóxel lo deja liso; con **Pintar › «Solo la cara»** pintas la cara
  que señalas y no el vóxel entero. Mover, copiar, girar o reflejar una selección se lleva su sombreado.
- **Guardar es llevarlo al juego** (Cmd+S o «GUARDAR EN EL JUEGO»): se escribe su fichero dentro de a_game y Unity lo
  recarga solo. No hay «exportar». Si el fichero cambió en disco mientras lo tenías abierto, el editor pregunta antes
  de pisarlo.
- **Propiedades**: el tamaño crece o encoge por la derecha, por detrás y por arriba (lo que hay nunca se mueve de
  sitio) y el **punto de apoyo** es el punto del modelo que cae en la esquina de su casilla.
- **Troncos de árboles con copa de hojas**: las hojas no son vóxeles y no se editan aquí; los cubos fantasma marcan
  dónde va cada racimo.
- Un modelo retocado queda marcado en su nota. **Ojo:** los scripts de arte del juego todavía no miran esa marca; si
  alguien relanza el script del que salió el modelo, lo pisa.

Un **modelo nuevo no sale en el juego solo por existir**: programación tiene que enlazarlo a un objeto con su mismo
nombre (la pestaña Validación lo recuerda). Lo mismo si cambias el **tamaño** de un modelo que ya existía: avisa, porque
el juego tiene posiciones y pruebas que dependen de él. Los dibujos 2D con el mismo nombre que un modelo no se tocan.

## MagicaVoxel

*Importar de MagicaVoxel…* abre un `.vox` (hasta 256 por lado): eliges qué lado da a la calle y el material del juego
para cada color (te propone el más parecido) y se abre como estructura nueva. *Exportar a MagicaVoxel…* (clic derecho en
una estructura de la Biblioteca) guarda un `.vox` que vuelve exacto al importarlo.

## Llevar una estructura al juego

1. Con la validación sin errores, pulsa **EXPORTAR AL JUEGO** (Cmd+E). El editor le da un número fijo, guarda la fuente
   en `a_game/ArtSource/Structures/<nombre>.agvox` y genera su código en
   `a_game/Assets/_Game/Scripts/Generation/WorldGen1/Structures/Authored/Generated/`.
2. El generador se recompila y la estructura aparece en Explorar, en el grupo **«Hechas a mano»**.
3. En Unity, en Play: *Tools › Mundo › Estructuras › Estructura a mano › Siguiente* la pone delante del jugador.
4. Commitea juntos el `.agvox`, los `.g.cs` y sus `.meta`, y pasa `AuthoredBlueprintTests`.

Hoy el juego exige que toda estructura a mano **se pueda entrar por su puerta y llegar a todas sus plantas**; las
piezas abiertas (fuentes, puestos) todavía no se pueden exportar.

> Las estructuras a mano están en la rama **`editor-estructuras-autoria`** de a_game. Hasta que se fusione, para
> exportar elige como carpeta del juego un checkout de esa rama (por ejemplo el worktree `../a_game-autoria`). Contra
> un a_game sin ella, Explorar y la validación funcionan, pero exportar responde con un error.

## Problemas frecuentes

| Lo que ves | Qué pasa |
|---|---|
| Chip «Sin generador» | Falta dotnet, la versión de Unity del proyecto o la carpeta `Library` de a_game. El motivo sale al pasar el ratón por el chip. |
| Chip «No compila (n)» | El código del juego tiene errores ahora mismo; sigues viendo la última versión buena. La consola (clic en el chip) tiene el detalle para programación. |
| «Esta combinación ha fallado al generarse» | Un plano del juego falla con esos parámetros. El error completo está en la consola; en el Muestrario, clic derecho en la celda lo copia. |
| Un modelo de la Biblioteca dice «No se puede abrir» | Su fichero no encierra un sólido o está a medio escribir. El motivo sale al pasar el ratón; el editor no lo abre para no guardarlo mal. |
| Exportar da error | Hay errores en la validación de alguna estructura de `ArtSource/Structures`, o el a_game elegido no tiene la rama de estructuras a mano. |

## Sin ventana: `cantero`

El mismo editor desde la línea de comandos, pensado para que Claude Code (u otro agente) haga y retoque estructuras y
modelos: escribe una **receta** (un script corto con cajas, cilindros, tejados y las herramientas de construcción del
juego), la ejecuta y mira el resultado en una **lámina** PNG con reglas de coordenadas.

```sh
cargo build --release --bin cantero
cantero run fixtures/recetas/pozo.rhai --salida borrador --render --out pozo.png   # receta → modelo → lámina
cantero info borrador/pozo_model.json          # tamaño, colores, avisos, huella
cantero render <fichero> --planta 0 --capas rejilla,entrada,partes   # un edificio cortado por la planta baja
cantero api                                    # funciones de las recetas
```

- Con el editor abierto sobre el mismo fichero, lo que `cantero` cambia **se recarga solo**; si tienes cambios sin
  guardar, pregunta.
- Una receta no pisa lo que has retocado a mano después: se niega y lo dice.
- Para Claude Code hay una skill en `.claude/skills/cantero/`; cómo está hecho, en `docs/recetas.md`.

## Publicar una versión

Sube `version` en `Cargo.toml` (por ejemplo `0.1.0` → `0.2.0`) y haz push a la rama principal. El workflow
`.github/workflows/publicar.yml` ve que aún no existe el tag `v0.2.0`, pasa los tests, compila en Windows, macOS
(binario universal) y Linux, y crea el tag y la release con los tres instaladores. Si la versión no cambia, no hace
nada. También se puede lanzar a mano desde *Actions › Publicar versión*.

## Desarrollo

Las reglas para trabajar en el código están en `CLAUDE.md`. El contrato editor ↔ juego (protocolo del generador,
formato `.agvox`, exportación) está en `docs/contrato.md`, y el diseño de la interfaz en `docs/diseno-gui.md`.

```sh
cargo clippy --all-targets -- -D warnings
cargo test                                            # sin dotnet: usa fixtures/
cargo test --test bridge_live -- --ignored            # con el generador real
dotnet bridge/bin/Release/net8.0/AGame.StructureBridge.dll selftest   # todas las combinaciones del juego
```

## Licencias

Fuentes Atkinson Hyperlegible Next y Mono bajo SIL OFL 1.1 (`assets/fonts/OFL.txt`); iconos Phosphor (MIT, vía
`egui-phosphor`).
