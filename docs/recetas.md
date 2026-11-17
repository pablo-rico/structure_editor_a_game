# `cantero` y las recetas

`cantero` es el editor sin ventana: el mismo núcleo (documento, formas, formatos, visor, generador del juego) detrás
de una línea de comandos, para quien no puede usar el ratón: un agente como Claude Code, un script, la integración
continua. Se trabaja con **recetas**: scripts cortos que construyen o retocan un modelo o una estructura con formas, y
con **láminas**: imágenes PNG del resultado con reglas de coordenadas.

Cómo usarlo bien (ciclo de trabajo, escalas, lista de calidad) está en `.claude/skills/cantero/SKILL.md`; aquí va
cómo está hecho y qué garantiza.

## Órdenes

`cargo run --bin cantero -- <orden>` (o `target/release/cantero`). Cada orden imprime un JSON corto (una clave por
línea) por la salida estándar; un fallo va por la de errores, en llano, y el proceso sale con 1.

| Orden | Qué hace |
|---|---|
| `info <fichero>` | Tamaño, caja de lo sólido, recuento por color o material, plantas, avisos y huella. |
| `render <fichero>` | Lámina de vistas en un PNG (`--out`; si no, en la carpeta temporal). |
| `vista <modelo>` | El modelo como se ve en el juego: un vóxel ≈ un píxel, luz lisa, contorno, desde los cuatro lados. |
| `capas <fichero> --caja …` | Rebanadas en texto de una región acotada, una letra por color o material. |
| `run <receta>` | Ejecuta una receta, guarda y resume: qué ha cambiado, avisos, lo que el script imprimió. |
| `export` | Lleva al juego las estructuras de `ArtSource/Structures` (da número a las que no tienen). |
| `catalog [sección]` | Materiales, herramientas de construcción, muebles y estilos del juego. |
| `api` | Referencia de las funciones de las recetas. |

Un fichero es un modelo del juego (`*_model.json`) o una estructura a mano (`.agvox`). Los modelos no arrancan el
generador; las estructuras sí (catálogo, `validate`, herramientas, muebles, `export`), con el cliente síncrono de
`src/bridge/client.rs`. Opciones comunes: `--juego <carpeta de a_game>` y `--recompilar`.

### La lámina

`render` dibuja con el visor del editor (`render::Offscreen`: se malla una vez y se sacan varias vistas), así que lo
que se ve es el aspecto del juego. Vistas (`--vista`, por defecto `iso,iso-atras,frente,planta`):

| Vista | Qué enseña |
|---|---|
| `iso` | La isométrica del juego desde el frente (caras −X y −Z). |
| `iso-atras` | Lo mismo desde detrás. |
| `frente` | Alzado: X a la derecha, Y arriba. |
| `lado` | Alzado desde +X: Z a la derecha, Y arriba. |
| `planta` | Desde arriba: X a la derecha, Z arriba (el frente queda abajo). |

Los alzados y la planta llevan **reglas en sub-cubos** en los márgenes y guías tenues sobre la imagen: son las mismas
coordenadas que usan las recetas. Las isométricas marcan el origen y el extremo de cada eje. `--caja` encuadra una
región, `--marca` la resalta en rojo, `--planta n` corta por esa planta como el visor, `--capas` enciende capas
(`rejilla`, `nav`, `partes`, `interactuables`, `entrada`, `huella`, `detalle`, `plantas`) y `--tam` fija el tamaño.
A diferencia del visor, la lámina llena el recuadro aunque el zoom no sea entero.

### La vista de juego

`vista` (`src/cli/gameview.rs`) dibuja en CPU, sin tarjeta gráfica, con la proyección de la cámara del juego en
píxeles de arte: `x_pantalla = x − z`, `y_pantalla = y + (x + z) / 2`. No aplica luz por cara (el juego ilumina los
árboles de vóxeles como un volumen liso), así que lo que se ve es el color pintado, y añade el contorno casi negro de
fuera. La lámina de `render` enseña cada vóxel de un modelo al doble de tamaño: para medir vale; para saber cómo
quedará, no.

## Recetas

Un script [Rhai](https://rhai.rs) (motor embebido, Rust puro) con las funciones de `cantero api`
(`src/recipe/api.rs`; cada función registrada tiene su línea de ayuda, lo vigila un test). Las formas son nativas
(`src/edit/ops.rs`): el script solo decide dónde van. Un tope de operaciones corta los bucles desbocados.

- **Coordenadas** en sub-cubos (vóxeles en un modelo): x a la derecha, y arriba, z hacia el fondo; el frente es
  z = 0. Las cajas son semiabiertas: de la esquina menor a la mayor sin incluirla.
- **Una receta, un documento.** Empieza con `new_model`, `new_structure` u `open`.
- **Modelos**: `color` pinta plano; `tones` y `shaded` definen materiales de cuatro tonos (tapa, borde de la tapa,
  lateral, sombra) que se aplican cara a cara al acabar, con la regla de `ArtSource/Intro/intro_models.py` del juego
  (`vmodel::shade`): arriba la tapa (el borde donde falta un vecino al mismo nivel), por debajo y bajo un saliente la
  sombra, y los demás laterales en el color del vóxel.
- **Modelos con sprite** (`src/recipe/sprite.rs`): `sprite_radii` da el perfil de un sprite del juego para
  `revolve`, y `sprite_wrap` forra las caras con su dibujo tal como lo proyecta la cámara, por cuartos (uno por cada
  diagonal desde la que se mira de frente), sin el contorno que el sprite trae por fuera. Se aplica al acabar, encima
  del sombreado.
- **Estructuras**: `material` pinta con un material del juego **por nombre**; `tool` aplica una herramienta de
  construcción del generador al documento entero (el `tool` del contrato) y `stamp` coloca un mueble; `look` y `role`
  eligen los materiales por papel. Al acabar se valida con el juego y los avisos salen en el resumen.

### Dos clases de receta y la huella

La **huella** de un documento (`Doc::fingerprint`, 16 cifras hexadecimales; la da `cantero info`) resume lo que su
fichero guarda: en un modelo, el tamaño y el color final de cada cara expuesta (no depende del orden de la paleta ni
del de las caras); en una estructura, la rejilla, el detalle, las plantas, la entrada, las partes y los
interactuables.

- **Generativa** (`new_model`, `new_structure`): rehace el fichero entero. Al guardar apunta de qué receta salió y
  la huella de lo que escribió: en la nota `_` del modelo (`Receta: <ruta>; huella <hex>.`) o en el campo `recipe`
  del `.agvox` (`docs/contrato.md`). La próxima vez, si el fichero ya no tiene esa huella (alguien lo retocó en el
  editor, o no viene de una receta), **se niega a pisarlo** salvo `--forzar`. De una estructura que ya existe conserva
  el `id`, el estilo y a qué diseño sustituye.
- **De parche** (`open`): retoca lo que hay. Con `open(ruta, huella)` solo se aplica sobre esa base: repetirla falla
  con «ya está aplicado». No apunta huella, así que una generativa posterior verá el fichero como retocado.

Con esto recetas y retoques a mano conviven sin estado fuera de los ficheros: lo que se hace a mano después de una
receta nunca se pierde sin avisar.

### Borrador y juego

`run --salida <carpeta>` escribe en esa carpeta en vez de en el juego (y `open` sigue trabajando sobre lo que ya
haya en ella). Sin `--salida`, un modelo va a `Assets/_Game/Art/<nombre>_model.json` (con su `.meta` de Unity si es
nuevo) y una estructura a `ArtSource/Structures/<nombre>.agvox`. Todo se escribe sin dejarlo a medias (temporal y
renombrar); si el resultado es idéntico a lo que hay, no se toca el fichero (`"unchanged": true`).

Las recetas se guardan en la carpeta de fuentes de arte del juego (`editor::art_source`: `Assets/_Game/ArtSource~/`
si existe, que Unity no importa por acabar en `~`; si no, `ArtSource/` en la raíz): las de modelos en `Recetas/`, las
de estructuras en `Structures/`, junto a su `.agvox`. Las rutas `ArtSource/…` de este documento son relativas a esa
carpeta.

### El resumen de `run`

`written`, `unchanged`, `kind`, `size`, `solidBox`, `solids`, `fingerprint`, `checks` (los avisos del juego o del
modelo), `log` (lo que el script imprimió) y `change`: `{"new": true}`, `{"resized": […]}` o cuántos sub-cubos se han
añadido, quitado y repintado y la caja que los contiene. En un modelo, «repintado» compara el color de las caras, no
el byte del vóxel (la paleta se compacta al guardar). `outOfBounds` avisa de lo que caía fuera de la rejilla.

## Con el editor abierto

El editor mira cada medio segundo si el fichero abierto ha cambiado en disco. Sin cambios sin guardar, lo recarga
solo (sin mover la cámara) y lo dice; con cambios, pregunta si recargar o seguir. Así se puede tener la ventana
abierta mientras `cantero` trabaja, y retocar a mano entre receta y receta.

## Pruebas

- `cargo test`: recetas (`src/recipe/tests.rs`), órdenes (`src/cli/mod.rs`, `tests/cli.rs` lanza el binario contra
  `fixtures/recetas/pozo.rhai`), formas y sombreado.
- `cargo test --test cli -- --ignored`: la lámina (necesita tarjeta gráfica).
- `cargo test --test bridge_live -- --ignored`: el cliente síncrono contra el generador real.
