Recetas de cantero: un script Rhai (https://rhai.rs) con estas funciones.
Coordenadas en sub-cubos (vóxeles en un modelo): x a la derecha, y arriba, z hacia el fondo; el frente es z = 0.
Las cajas van de la esquina menor a la mayor sin incluirla, como las marcas de las reglas de `cantero render`.
Los bucles del script colocan formas, no sub-cubos. print() sale en el resumen.

Documento
  new_model(nombre, sx, sy, sz)
      Modelo nuevo y vacío. `nombre` es su sitio bajo Assets/_Game/Art sin `_model.json` ("Objects/Voxel/farol"). Receta generativa: rehace el fichero entero.
  new_structure(nombre, celdas_x, celdas_z, niveles)
      Estructura nueva (8 sub-cubos por celda y por nivel) con el forjado de piedra en el nivel 0. Va a ArtSource/Structures/<nombre>.agvox. Receta generativa.
  open(ruta)
      Abre un modelo o una estructura para retocarlo (receta de parche). Ruta absoluta o relativa a la carpeta del juego.
  open(ruta, huella)
      Como open(ruta), pero solo si el fichero tiene esa huella (la de `cantero info`): un parche no se aplica dos veces.
  clear()
      Borra todos los sub-cubos (conserva tamaño, paleta y demás datos): para rehacer por completo un fichero abierto.
  resize(sx, sy, sz)
      Modelos: cambia el tamaño creciendo o encogiendo por +x, +y, +z sin mover lo que hay.
  size()
      Tamaño de la rejilla: [sx, sy, sz].
  solid_box()
      Caja de lo sólido [x0, y0, z0, x1, y1, z1], o () si está vacío.
  get(x, y, z)
      Qué hay en un sub-cubo: el color "rrggbb", el nombre del material, o "" si es aire.
  count()
      Cuántos sub-cubos sólidos hay.
  count(nombre)
      Cuántos sub-cubos hay de ese color o material.

Pincel
  color(nombre)
      Modelos: pinta con el color "rrggbb" (se añade a la paleta) o con un material definido con tones().
  tones(nombre, tapa, borde, lateral, sombra)
      Modelos: define un material sombreado de cuatro tonos y lo elige. Al acabar la receta, sus caras de arriba salen en `tapa` (`borde` en las aristas), los laterales en `lateral` y lo que queda bajo un saliente o por debajo, en `sombra`.
  shaded(color)
      Modelos: como tones(), sacando los cuatro tonos de un solo color (el lateral). El material se llama como el color.
  material(nombre)
      Estructuras: pinta con un material del juego por su nombre ("Stone", "Plank"…; `cantero catalog materiales`).
  air()
      Pinta con aire: desde aquí las formas borran.
  detail(si)
      Estructuras: lo que se pinte a partir de aquí es detalle (muebles, escaleras): no cierra el paso como un muro.
  mirror(en_x, en_z)
      Simetría: cada forma se repite reflejada respecto al centro de la rejilla en x y/o en z.

Formas
  voxel(x, y, z)
      Un sub-cubo.
  box(x0, y0, z0, x1, y1, z1)
      Caja maciza.
  hollow(x0, y0, z0, x1, y1, z1)
      Caja hueca: solo la cáscara de un sub-cubo de grosor.
  walls(x0, y0, z0, x1, y1, z1)
      Las cuatro paredes de la caja, sin suelo ni techo.
  line(x0, y0, z0, x1, y1, z1)
      Línea de sub-cubos entre dos, ambos incluidos.
  cylinder(cx, cz, r, y0, y1)
      Cilindro vertical de radio r con el eje en (cx, cz), de y0 a y1. Centro entero = diámetro par; acabado en .5 = diámetro impar.
  tube(cx, cz, r, r_dentro, y0, y1)
      Cilindro vertical hueco.
  cylinder_x(cy, cz, r, x0, x1)
      Cilindro tumbado a lo largo de x.
  cylinder_z(cx, cy, r, z0, z1)
      Cilindro tumbado a lo largo de z.
  cone(cx, cz, r0, r1, y0, y1)
      Tronco de cono vertical: radio r0 abajo y r1 arriba (0 para acabar en punta).
  revolve(cx, cz, y0, radios)
      Sólido de revolución: una capa por cada radio de la lista, desde y0 hacia arriba (copas, jarrones, cúpulas).
  sphere(cx, cy, cz, r)
      Esfera.
  ellipsoid(cx, cy, cz, rx, ry, rz)
      Elipsoide.
  roof(x0, y, z0, x1, z1, cumbrera)
      Tejado macizo sobre el rectángulo, desde la altura y: a dos aguas con la cumbrera a lo largo de "x" o "z", o a cuatro con "".
  roof(x0, y, z0, x1, z1, cumbrera, paso)
      Como roof(), subiendo `paso` capas por cada sub-cubo que se mete (más empinado).
  fill(x, y, z)
      Rellena con el pincel la región unida por caras del mismo material que ese sub-cubo (también el aire encerrado).
  replace(de, a)
      Cambia un color o material por otro en todo el documento. Devuelve cuántos.
  replace(de, a, x0, y0, z0, x1, y1, z1)
      Como replace(), solo dentro de la caja.
  speckle(x0, y0, z0, x1, y1, z1, porcentaje, semilla)
      Repinta con el pincel, al azar, ese porcentaje de lo sólido de la caja (vetas, desgaste, hojas distintas). La misma semilla da siempre lo mismo.
  speckle(x0, y0, z0, x1, y1, z1, porcentaje, semilla, solo)
      Como speckle(), repintando solo lo que sea de ese color o material.
  noise(semilla, x, y, z)
      Número de 0 a 99, fijo para esa semilla y ese sub-cubo: para decidir en el script.

Sprite
  sprite_radii(ruta, eje, suelo, y0, y1)
      Modelos: perfil de un sprite del juego (PNG; ruta relativa a la carpeta del juego) como lista de radios para revolve(), una capa por cada y de y0 a y1. `eje` es la columna del eje del dibujo (16 = entre los píxeles 15 y 16) y `suelo` la fila del sprite que corresponde a y = 0. Sin el contorno que el sprite trae dibujado.
  sprite_wrap(ruta, eje, suelo, x0, y0, z0, x1, y1, z1)
      Modelos: al acabar la receta, forra las caras de la caja con el dibujo del sprite tal como lo proyecta la cámara del juego (un vóxel ≈ un píxel), por cuartos: desde cada una de las cuatro diagonales, el centro del modelo es el centro del sprite, píxel a píxel. Lo que cae fuera del dibujo toma el píxel de dentro más cercano de su fila. Compruébalo con `cantero vista`.

Copiar
  copy(x0, y0, z0, x1, y1, z1)
      Copia la caja (con su sombreado). Devuelve el recorte.
  paste(recorte, x, y, z)
      Pega el recorte con su esquina menor en (x, y, z). El aire del recorte no borra.
  rotated(recorte)
      El recorte girado un cuarto de vuelta, como gira el juego: (x, z) pasa a (fondo − 1 − z, x).
  flipped_x(recorte)
      El recorte reflejado en x.
  flipped_z(recorte)
      El recorte reflejado en z.

Modelo
  origin(x, y, z)
      Punto del modelo que cae en la esquina de la casilla (modelos más anchos que su casilla).

Estructura
  style(nombre)
      Estilo de la estructura ("Northern"…; `cantero catalog estilos`).
  ground_level(n)
      Nivel del suelo de fuera: los cimientos ocupan los niveles de debajo.
  bands(lista)
      Plantas, de abajo arriba: [[sub-cubo donde empieza la franja, nivel de su suelo], …].
  entrance(x)
      Celda (en x) de fuera pegada a la puerta, en la fachada del frente (z = 0).
  courtyard(si)
      Tiene patio interior.
  floor_y(y)
      Altura donde se pisa en la planta sobre la que trabajan las herramientas (por defecto, el suelo de la calle).
  look(barrio, clase)
      Materiales por papel (muro, tejado…) de ese barrio y clase para las herramientas (`cantero catalog estilos`).
  role(papel, material)
      Cambia el material de un papel suelto para las herramientas.
  tool(nombre, donde)
      Herramienta de construcción del juego con sus parámetros por defecto, sobre la planta de floor_y(). `donde` según el `target` de la herramienta en `cantero catalog herramientas`: rect = #{rect: [cx0, cz0, cx1, cz1]} en celdas incluidas; line = lo mismo, de una celda de ancho; face = #{cell: [cx, cz], dir: d}, la cara del muro en ese borde de la celda; cell = #{cell: [cx, cz], dir: d}, hacia donde sube o mira. dir: 0 +X, 1 −X, 2 +Z, 3 −Z.
  tool(nombre, donde, parametros)
      Como tool(), con parámetros por su `key`: #{storeys: 2, walls: 1}. Los de sí o no van como 1 o 0; los de lista, con el `value` de la opción.
  stamp(mueble, x, y, z, mira)
      Mueble del juego apoyado en (x, y, z), con y = el suelo donde se pisa, mirando a `mira` (0 +X, 1 −X, 2 +Z, 3 −Z). Ver `cantero catalog muebles`.
  stamp(mueble, x, y, z, mira, ancho)
      Como stamp(), para los muebles de ancho variable.

