---
name: cantero
description: Crear o retocar estructuras de sub-cubos (.agvox) y modelos de vóxeles (*_model.json) de a_game sin ventana, con el CLI `cantero` y recetas en script. Úsala cuando pidan hacer, cambiar, revisar o exportar un edificio, un árbol, un objeto del mapa o cualquier modelo de vóxeles del juego.
---

# Cantero: estructuras y modelos de a_game desde la línea de comandos

`cantero` es el editor sin ventana. Tú no pintas sub-cubos: escribes una **receta** (un script corto con formas) y
miras el resultado en una **lámina** PNG. El ciclo entero tarda 1–3 s, así que itera mucho y en pasos pequeños.

## Preparar

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --bin cantero        # una vez; después: ./target/release/cantero (cantero.exe en Windows)
```

- La carpeta del juego es `AGAME_ROOT`, o `../a_game`, o `--juego <carpeta>`.
- Los **modelos** no necesitan nada más. Las **estructuras** arrancan el generador del juego (dotnet + Unity
  instalados); si el juego ha cambiado, añade `--recompilar` una vez.
- `cantero --ayuda` lista los comandos; `cantero api` (o `referencia.md`, aquí al lado) lista las funciones de las
  recetas. Léela antes de escribir la primera receta: no inventes funciones.

## El ciclo de trabajo

1. **Mira referencias antes de inventar.** Busca en el juego dos o tres cosas parecidas y míralas:
   `cantero info <fichero>` (tamaño, colores, huella) y `cantero render <fichero> --out <png>`. Los modelos están en
   `a_game/Assets/_Game/Art/**/*_model.json`; las estructuras a mano, en la carpeta de fuentes del juego
   (`Structures/*.agvox`; puede no haber ninguna todavía). Copia de ellas la escala, el número de colores y los propios
   colores: lo nuevo tiene que parecer del mismo juego.
2. **Escribe la receta por volúmenes**, de lo grande a lo pequeño: primero la silueta (cajas, cilindros, tejados),
   luego el detalle. Guárdala donde toca (abajo) desde el principio.
3. **Ejecuta en borrador y mira**: `cantero run <receta> --salida <carpeta> --render --out <png>` y lee el PNG. El
   resumen dice cuántos sub-cubos hay, qué ha cambiado respecto a la vez anterior y los avisos.
4. **Corrige mirando coordenadas.** Los alzados y la planta llevan reglas en sub-cubos: lee en ellas dónde está lo
   que no te gusta y cambia esos números en la receta. La vista `lado` mira desde +X (z crece a la derecha); `planta`,
   desde arriba con el frente abajo. Si dudas de un detalle, acércate:
   `--vista frente --caja x0,y0,z0,x1,y1,z1`, o léelo sub-cubo a sub-cubo con `cantero capas <fichero> --caja …`.
5. **Repite 3–4 hasta que pase la lista de calidad.** No des nada por bueno sin haber leído la lámina de la última
   versión.
6. **Escribe en el juego**: la misma orden sin `--salida`. Dilo antes de hacerlo si no te lo han pedido ya.

Mientras tanto, si alguien tiene el fichero abierto en el editor, lo ve cambiar solo.

## Recetas

Un script [Rhai](https://rhai.rs): `let`, `for i in 0..8 { }`, `if`, funciones `fn`, listas `[1, 2]`, mapas
`#{clave: 1}`, comentarios `//`. Los bucles colocan **formas**, nunca sub-cubos uno a uno.

- **Coordenadas**: x a la derecha, y arriba, z hacia el fondo; el frente es z = 0. Las cajas van de la esquina menor
  a la mayor **sin incluirla**: `box(4, 0, 4, 12, 8, 12)` ocupa x = 4…11. Coinciden con las marcas de las reglas.
- **Generativa** (`new_model`, `new_structure`): rehace el fichero entero cada vez. Es la normal para algo nuevo. Deja
  apuntada su huella; si luego alguien retoca el fichero a mano, se niega a pisarlo (`--forzar` lo rehace igualmente:
  pregunta antes, se pierde trabajo de otro).
- **De parche** (`open`): retoca algo que ya existe sin rehacerlo. Úsala para los modelos del juego que no nacieron
  de una receta y para lo retocado a mano. Con `open(ruta, huella)` el parche no se aplica dos veces.
- Para rehacer por completo un modelo que ya existe **conservando su sitio y sus datos** (`pivot`, `origin`…):
  `open(ruta); clear(); …`.
- **Dónde se guardan**: en la carpeta de fuentes de arte del juego, que es `a_game/Assets/_Game/ArtSource~/` si
  existe y si no `a_game/ArtSource/`: las de modelos en `Recetas/<nombre>.rhai`; las de estructuras en
  `Structures/<nombre>.rhai`, junto a su `.agvox` (crea la carpeta si falta). Mientras trabajas en borrador, la receta
  puede vivir en la carpeta del borrador; llévala a su sitio al escribir en el juego.

Ejemplo completo y comentado: `fixtures/recetas/pozo.rhai`.

## Modelos (árboles, objetos, ruinas, muebles, intro)

- **Juzga el aspecto con `cantero vista <modelo> --out <png>`, no con la lámina.** `render` dibuja cada vóxel al
  doble y con luz cara a cara: sirve para medir. `vista` enseña el modelo como el juego: un vóxel ≈ un píxel, sin
  luz por cara (los árboles se iluminan como un bulto liso: solo se ve el color pintado), con el contorno oscuro y
  desde los cuatro lados, porque la cámara orbita. Lo que no se lee ahí no se leerá en el juego.
- **Si el objeto tiene sprite dibujado a mano** (los árboles: `Art/Objects/Trees/<bioma>/*.png`), no imites su
  detalle con formas: revuelve su perfil y fórralo con el dibujo (`sprite_radii` + `revolve` + `sprite_wrap`; ejemplo
  en `ArtSource~/Recetas/tree_meadow_basic_1.rhai` del juego). El eje es la columna central del sprite y el suelo, la
  fila que deja la punta del dibujo a la altura del modelo. El relieve hecho con matas y líneas de un vóxel no
  sobrevive al tamaño real.
- **Escala**: 16 vóxeles = una casilla del mapa (a lo ancho y a lo alto). En el juego un vóxel mide poco más de un
  píxel: un detalle de un vóxel suelto no se ve y ensucia. Trabaja con masas de 2 vóxeles o más. Tamaños de
  referencia: un objeto pequeño, 16×12×16; un árbol, 26×45×26; lo más grande del juego, 80×72×57. Tope: 256 por lado.
- **Apoyo**: el modelo se posa en y = 0 y su casilla empieza en x = 0, z = 0. Si es más ancho que su casilla, pon
  `origin(x, y, z)`.
- **Color**: pocos (lo normal son 8–15; el máximo del juego, 31). Usa `shaded("rrggbb")` para casi todo: da a cada
  material su tapa clara, su borde y su sombra bajo los salientes, que es lo que da volumen a esta escala.
  `tones(...)` si quieres elegir los cuatro tonos; `color(...)` para lo plano (agua, luz, tela).
- **Variación**: `speckle` con un tono vecino al 10–25 % rompe las superficies grandes (piedra, hojas, tierra). Más
  es ruido.
- `ff00ff` es el color clave de la intro: el juego lo cambia por el color de la nación. No lo uses para otra cosa.
- Al releer un modelo sombreado, `capas` e `info` enseñan cada vóxel con el color más frecuente de sus caras, no el
  que pusiste: es lo que guarda el fichero (el color va por cara).

## Estructuras (edificios)

- **Escala**: 8 sub-cubos = una celda = un nivel. Una planta mide 16 de alto. `new_structure` deja los cimientos de
  piedra en y = 0…7: la calle y el suelo de la planta baja están en y = 8.
- **Construye con las herramientas del juego**, no a mano: `tool("room", …)`, `roof`, `door`, `window`, `stairs`,
  `chimney`… salen con el aspecto y las reglas del juego (`cantero catalog herramientas`). Lo hecho a mano con `box`
  y `material` queda para lo que ninguna herramienta hace.
- **Niveles**: pide 1 de cimientos + 2 por planta + lo que suba el remate (un tejado, 3–5; unas almenas, 1–2). Las
  herramientas hacen crecer la rejilla hacia arriba si les falta; `size` en el resumen dice cómo ha quedado.
- `floor_y(y)` dice sobre qué planta trabaja la herramienta: 8 la baja, 24 la primera, 40 la segunda. El tejado va
  **sobre** esa planta: para techar una casa de dos plantas, `floor_y(24)`. Con almenas, cúpula y aguja comprueba en el
  alzado dónde han caído: si salen incrustadas en el muro de la última planta, súbelas una (`floor_y(40)` en ese caso).
- Cómo se dice **dónde** va cada herramienta (`rect`, `line`, `face`, `cell`) y cómo se pasan sus parámetros está en
  `tool` de la referencia; qué parámetros tiene cada una, en `cantero catalog herramientas`.
- **Escalera** (`stairs`): `cell` es donde arranca y `dir` hacia donde sube; necesita **cuatro celdas libres en
  fila** en esa dirección. En menos de 4 celdas de lado no cabe: agranda la planta antes que hacerla a mano (una
  escalera a mano no la puedes comprobar).
- `look(barrio, clase)` da los materiales de muro, tejado, etc.; `role(papel, material)` cambia uno. Los nombres de
  los papeles salen en `cantero catalog estilos` (`roles`).
- La puerta de la calle va en el frente: `tool("door", #{cell: [cx, 0], dir: 3})` con `floor_y(8)`; pasa a ser la
  entrada sola.
- Muebles: `stamp("Bed", x, y, z, mira)` con y = el suelo donde se pisa (`cantero catalog muebles`).
- **Mira dentro**: `cantero render <fichero> --planta 0 --capas rejilla,entrada,partes,nav` corta por esa planta.
  Hazlo con cada planta. Leyenda: azul claro = suelo por el que se camina; rojo anaranjado en un borde = paso cerrado;
  naranja = partes móviles (hojas de puerta, con su arco de giro); flecha amarilla = entrada; línea amarilla
  discontinua = huella.
- **Los avisos mandan, pero no bastan**: el resumen trae `checks` del propio juego (sin entrada, puerta sin altura…).
  Un `error` impide exportar: arréglalo, no lo ignores. Que no haya avisos no prueba que se pueda subir a cada planta
  o salir a la azotea: compruébalo tú en los cortes con la capa `nav`.

## Lista de calidad (antes de darlo por terminado)

- [ ] He leído la lámina de la **última** versión con `--vista todas` (y cada planta cortada, si es un edificio) y,
      si es un modelo, su `cantero vista`.
- [ ] La silueta se reconoce en la vista `iso` sin leer el nombre.
- [ ] Nada flota sin querer, nada se sale de la rejilla (`outOfBounds` no aparece en el resumen) y todo apoya en el
      suelo.
- [ ] En los alzados, dos piezas del mismo color pegadas se leen como una mancha: lo que debe distinguirse tiene otro
      tono, o lo he comprobado con `capas`.
- [ ] No quedan sub-cubos sueltos ni agujeros de un vóxel que no sean a propósito.
- [ ] Lo que debe ser simétrico lo es (usa `mirror`); lo natural (árboles, rocas) no lo es del todo.
- [ ] Escala y colores casan con las referencias del juego que miré al empezar.
- [ ] `checks` sin errores; los avisos, entendidos y contados a quien lo pidió.
- [ ] Volver a ejecutar la receta da `"unchanged": true`, y al escribir en el juego la receta queda en su sitio.

## Trampas

- **Un modelo nuevo no sale en el juego solo por existir**: programación tiene que enlazarlo por su nombre (aviso
  `model-new`). Cambiar el **tamaño** de uno que ya existe también hay que avisarlo (`model-size`).
- **Los scripts de arte de a_game (`ArtSource/**/*.py`) no miran si un modelo se ha retocado**: si alguien los
  relanza, pisan lo hecho con recetas o a mano. Dilo al entregar un modelo que nació de uno de esos scripts.
- **Exportar estructuras escribe C# en a_game** (`cantero export`). Hazlo solo cuando te lo pidan, y siguiendo las
  reglas de a_game (su `CLAUDE.md`: en una rama; si otra sesión trabaja allí, en un worktree propio). Los modelos no
  se exportan: su fichero ya es el del juego.
- **No edites a mano** los `.agvox`, los `_model.json` ni los `Generated/*.g.cs`: siempre con receta o con el editor.
- Si `run` dice que el fichero «se ha retocado», alguien lo cambió después de tu receta: mira qué (`cantero info`,
  `render`) y sigue con una receta de parche; no fuerces sin preguntar.
- Los radios y centros admiten decimales (`cylinder(12.5, 12.5, 3.5, …)`); las coordenadas de sub-cubo, no.
