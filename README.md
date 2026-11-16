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

Para hacer estructuras **únicas** a mano.

- Empieza con *Archivo › Nueva estructura*, abriendo un `.agvox`, o con **«Duplicar para editar»** desde Explorar (lleva
  lo que ves, sin girar e intacto, para retocarlo).
- **Herramientas**: lápiz **B**, borrar **E**, caja **U**, línea **L**, relleno **G**, extruir cara **X**, cuentagotas
  **I** (o Alt+clic), selección **M**, mover **V**. Pincel de 1, 2, 4 sub-cubos o una celda (**, .**), espejo en X y Z,
  «Solo plano» para pintar a una altura (**↑/↓** la mueve) y «Detalle» para muebles y escaleras (el corte por planta no
  los quita).
- Con una selección: **Cmd+C / X / V / D** copiar, cortar, pegar y duplicar, **Shift+X / Z** espejo, **Cmd+R** girar.
- **Paleta**: los materiales del juego por familias, con recientes y búsqueda. Los de color variable (tema de escuela,
  color del reino) llevan una esquina partida; «Ver con» elige el tema con que se ven.
- **Propiedades**: nombre, estilo, tamaño, la celda de la **puerta** (siempre en la fachada sur), las **plantas** (a qué
  altura empieza cada una), las **partes móviles** (puertas con su bisagra, muros rompibles, rastrillos…; máximo 16) y
  los **interactuables** (camas, sillas, cofres…). Las partes y los interactuables se crean desde una selección.
- **Validación**: se comprueba sola mientras pintas, con las mismas reglas que el juego. **Deshacer** sin límite
  (Cmd+Z), **Historial**, guardar con **Cmd+S**, y una copia de recuperación cada 2 minutos.

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
| Exportar da error | Hay errores en la validación de alguna estructura de `ArtSource/Structures`, o el a_game elegido no tiene la rama de estructuras a mano. |

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
