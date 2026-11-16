# Diseño de la interfaz: «Taller de cantero»

Especificación del agente de arte. Contrastes medidos con la fórmula WCAG. Fuentes del juego en que se basa:
`a_game/ArtSource/UI/ui_palette.py`, `a_game/docs/arte.md`, `a_game/Assets/_Game/Art/UI/`,
`a_game/Assets/_Game/Scripts/Rendering/Terrain3D/StructurePalette.cs`.

## 1. Concepto y principios

- **Taller de cantero**: el marco de la herramienta es la piedra tallada del juego (rampa STONE, luz arriba-izquierda,
  sombras duras sin desenfoque, contorno en el tono oscuro de cada color). La herramienta no se dibuja en pixel art.
- **Marco vectorial, contenido en pixel art.** Paneles, iconos y texto son nítidos a cualquier escala; en píxeles solo
  va lo que viene del juego: visor, miniaturas y muestras de material.
- **El dorado marca lo activo.** GOLD/AMBER solo para selección, herramienta, pestaña activa y acción principal.
- **Cada capa del visor tiene una forma propia** (relleno, barra, caja, flecha, rayado, chincheta); el color refuerza,
  nunca es la única pista.
- **Nada de jerga.** Todo nombre visible está en español; nombres de enum, bits y «Variant» nunca llegan a la pantalla.

## 2. Tokens de tema (egui `Visuals` / `Style`)

**Solo tema oscuro**: la UI del juego es oscura, el visor ocupa el 70 % y un entorno oscuro deja ver las tres bandas del
toon sin falsear el contraste (como Blender, MagicaVoxel y Aseprite).

| Campo | Valor |
|---|---|
| `panel_fill`, `window_fill` | `2a3040` (INNER) |
| `window_stroke` | 1 px `4f5a70` |
| `extreme_bg_color` (campos de texto, pozos) | `161a24` (DEEP) |
| `code_bg_color`, huecos y tarjetas | `242a38` (HOLE) |
| `faint_bg_color` (filas alternas) | `2f3646` |
| Barra superior y barra de pestañas | `1c2130` (STONE 0) |
| `noninteractive` | fondo `2a3040`, borde 1 px `343c50`, texto `f2ead8` |
| `inactive` | fondo `343c50`, borde 1 px `1c2130`, texto `f2ead8` |
| `hovered` | fondo `4f5a70`, borde 1 px `717e96`, texto `f2ead8`, `expansion` 0 |
| `active` (pulsado, hundido) | fondo `242a38`, borde 1 px `f0c030` |
| `open` | fondo `343c50`, borde 1 px `717e96` |
| `selection` | fondo `6a4410`, trazo 1,5 px `f0c030` |
| `hyperlink_color` | `6ad4f0` |
| `warn_fg_color` | `f8c860` |
| `error_fg_color` | **`f48c6d`** |
| Texto atenuado | `9aa4b8` |
| Títulos de diálogo y pestaña activa | `ffd77a` |
| Cursor de texto | 2 px `f0c030` |
| Botón principal | fondo `e8a23a`, hover `f0c030`, texto `1c2130` |
| Botón destructivo | fondo `a02830`, texto `f2ead8` |
| Fondo del visor | `303338` (neutro); suelo en damero `33373f`/`373b44` |

Contrastes que obligan a dos reglas:
- El rojo del juego `e8564a` da 3,67:1 sobre el panel: no sirve como texto. Error = `f48c6d` (5,53 sobre panel, 4,62
  sobre hover).
- `9aa4b8` da 4,39 sobre `343c50`: **el texto atenuado solo va sobre fondos iguales o más oscuros que `2f3646`.**
- Medidos: crema/panel 10,99; título/panel 9,56; `1c2130` sobre ámbar 7,38.

**Forma:** radios widgets 2, menús 3, ventanas 4. `window_shadow` [3,3] desenfoque 0, `0d1018` al 70 %;
`popup_shadow` [2,2]. `item_spacing` 6×5, `button_padding` 8×4, `interact_size.y` 24, `indent` 14, márgenes de ventana
10 y de menú 6, barra de desplazamiento sólida de 8 px (asa `4f5a70`, hover `717e96`), `tooltip_width` 280.

**`bevel(rect, raised)`** (canto tallado): en relieve arriba/izquierda `717e96`, abajo/derecha `1c2130`; hundido
arriba/izquierda `161a24`, abajo/derecha `4f5a70`. Solo en barra superior, botones de herramienta, muestras de material,
tarjetas y celdas del muestrario.

**`egui_dock`:** barra de pestañas `1c2130`, 28 pt. Activa: fondo `2a3040` (se funde con el panel), texto `ffd77a`, y
con foco una línea de 2 px `f0c030` arriba. Inactiva: texto `9aa4b8`. Separadores 2 px `1c2130` (hover `717e96`). Zona
de soltar: `f0c030` al 20 %.

| Acento | Uso |
|---|---|
| GOLD / AMBER | Lo activo y la acción principal |
| CYAN | Información, enlaces y suelo transitable |
| GREEN `7ad860` | Correcto, «Juego al día» |
| RED | Error, bloqueo, destructivo |
| ORANGE | Partes móviles |
| WOOD | Solo el borde de las chinchetas de interactuables |

## 3. Tipografía

| Rol | Fuente | Tamaño (pt) |
|---|---|---|
| Cuerpo, botones | Atkinson Hyperlegible Next Regular (SIL OFL 1.1) | 14 |
| Ayudas, chips, barra de estado (`Small`) | Ídem | 12 (mínimo de la app) |
| Título de sección | Atkinson Next SemiBold, `f2ead8` | 13 |
| Título de diálogo (`Heading`) | SemiBold, `ffd77a` | 18 |
| Semilla, coordenadas, números, consola | Atkinson Hyperlegible Mono (OFL) | 13 |

- Atkinson distingue 0/O e 1/l/I y cubre el español. Los números van en mono (egui no tiene cifras tabulares).
- **Fuente bitmap del juego:** solo `font_title` (tiene ¡¿ÁÉÍÑÓÚÜ), como textura con su JSON a escala entera, en
  `ffd77a` con sombra de 1 px `141822`: la marca «Estructuras» (×2) y los títulos de estados vacíos y primer arranque
  (×3). Se lee de a_game; si no está, SemiBold.
- `font_5x7`, `font_4x6` y `font_3x5` no se usan.

## 4. Iconografía

**Phosphor** (`egui-phosphor`, MIT): Regular, y Fill para conmutadores encendidos. Comprobar nombres contra el crate;
si falta uno, el más parecido.

| Grupo | Iconos |
|---|---|
| Modos | Explorar `BINOCULARS`, Editar `HAMMER` |
| Global | `MAGNIFYING_GLASS`, `GEAR_SIX`, deshacer `ARROW_COUNTER_CLOCKWISE`, rehacer `ARROW_CLOCKWISE`, `FLOPPY_DISK`, `FOLDER_OPEN`, Exportar `EXPORT`, Captura `CAMERA`, Duplicar para editar `COPY` |
| Recarga | `CHECK_CIRCLE`, `ARROWS_CLOCKWISE` (girando), `X_CIRCLE`, `PLUGS` |
| Cámara | orbitar `ARROW_ARC_LEFT`/`ARROW_ARC_RIGHT`, encuadrar `CORNERS_OUT`, Iso `CUBE`, Vista libre `PERSPECTIVE`, enlazar `LINK` |
| Capas | Rejilla `GRID_FOUR`, Navegación `FOOTPRINTS`, Partes `DOOR_OPEN`, Interactuables `HAND_TAP`, Entrada `SIGN_IN`, Huella `BOUNDING_BOX`, Detalle `ARMCHAIR`, Franjas `STACK`, fantasma `GHOST` |
| Herramientas | Lápiz `PENCIL_SIMPLE`, Borrar `ERASER`, Caja `CUBE`, Línea `LINE_SEGMENT`, Relleno `PAINT_BUCKET`, Cuentagotas `EYEDROPPER`, Selección `SELECTION`, Mover `ARROWS_OUT_CARDINAL`, Pegar `CLIPBOARD_TEXT`, Espejo `FLIP_HORIZONTAL`, bloquear plano `LOCK_SIMPLE` |
| Validación | `X_CIRCLE`, `WARNING`, `INFO`, Localizar `CROSSHAIR` |
| Categorías | Ciudad `BUILDINGS`, Fortificación `CASTLE_TURRET`, Puentes `BRIDGE`, Puerto `ANCHOR`, Zonas `AXE`, Escuelas `MAGIC_WAND`, Salvajes `SKULL`, Barcos `SAILBOAT`, Antiguas `ARCHIVE` |
| Interactuables | Cama `BED`, Silla `CHAIR`, Cofre `PACKAGE`, Mostrador `STOREFRONT`, Timón `STEERING_WHEEL`, Ancla `ANCHOR`, Escotilla `TRAY`, Pasarela/amarre `ROPE`/`LINK`; Cañón propio |

**Iconos propios** (máximo 7; SVG sobre la rejilla de 256 de Phosphor, trazo 16, extremos redondos): Extruir cara,
Girar selección, Rastrillo, Levadizo, Hojas de puerta, Muro rompible, Cañón. En `assets/icons/`.

## 5. Distribución por modo

**Explorar (fase 1).** El botón Editar no aparece hasta que exista: nada de interfaz que no haga nada.

```
+-----------------------------------------------------------------------------------------+
| ESTRUCTURAS  Archivo Editar Ver Ayuda | [Explorar] | [Buscar ...  Cmd+K] | (o) Juego al día | [Captura] [DUPLICAR PARA EDITAR] |
+--------------+--------------------------------------------------+-----------------------+
| CATÁLOGO     | [Vista] [Muestrario] [+ Comparar]                | OPCIONES · Casa       |
| [filtrar]    | +capas: # pies puerta mano entrada huella ...+   | Estilo [Norteño|Medit.|Árabe]
| v Ciudad 22  |                              +-brújula: Frente-+ | Diseño   < 2 de 6 >   |
| +--++--++--+ |                              | Q   Iso/Libre  R| | Barrio   [Noble   v]  |
| |  ||  ||  | |                              +-----------------+ | Ancho    ---o---  6   |
| +--++--++--+ |             (estructura)              +-------+   | Fondo    --o----  5   |
| Casa Tienda  |                                       |Tejado |   | Orientación [S][O][N][E]
| > Fortific.  |                                       |Pl. 1 <|   | Cimientos  [-] 1 [+]  |
| > Puentes    |                                       |Pl. 0  |   | Daño [Intacto|Dañado|Ruinas|Arrasado]
| > Puerto ... |  +Leyenda-------+                     |Cimient|   | Semilla [48213] dado < > |
| > Antiguas   |  | relleno Suelo|              3x     | Todo  |   | > Estado (vista previa)  |
|              |  +--------------+                     +-------+   | BAJO EL CURSOR [fijar]   |
+--------------+--------------------------------------------------+-----------------------+
| Celda 4,2 · Nivel 3 · Sub 5,1,7 | Arrastre der.: girar · Rueda: zoom · D: semilla al azar | 3x |
+-----------------------------------------------------------------------------------------+
```

**Editar (fase 2)**

```
+-----------------------------------------------------------------------------------------+
| ESTRUCTURAS  Archivo... | [Explorar|Editar] | Torre del puerto (*) | deshacer rehacer | (o) | [Guardar] [EXPORTAR AL JUEGO] |
+--------------+--+-----------------------------------------------+-----------------------+
| PALETA       |B | Pincel [1|2|4|8]  Espejo [X][Z]  [Solo plano] | PROPIEDADES           |
| Recientes    |E | [Pintar como detalle]                         | > Estructura: nombre, |
| v Piedra     |U |                                  +-------+    |   estilo, huella, cimientos |
|  ■ ■ ■ ■ ■   |L |      (rejilla del plano bloqueado)| Pl. 2 |    |   plantas, entrada    |
| > Madera     |G |                                  | Pl. 1<|    | > Selección 6x3x2     |
| > Tejados... |X |                                  +-------+    | > Parte: Puerta,      |
+--------------+I |                                               |   bisagra izq., cerrada |
| BIBLIOTECA   |M |                                               |                       |
| [Partes|Interact.|Sellos]                                       |                       |
+--------------+--+-----------------------------------------------+-----------------------+
| [Validación  x1 !2] [Historial] [Consola]                                                |
| x  La puerta no se alcanza desde fuera                                       [Localizar] |
| !  Techo bajo en planta 1: 10 sub-cubos (mínimo 12)                          [Localizar] |
+-----------------------------------------------------------------------------------------+
```

**Disposición por defecto:** laterales de 260 y 300 pt; panel inferior de 150 pt, plegado en Explorar y abierto con
Validación en Editar. La pestaña Vista no se puede cerrar; la barra de herramientas va fija dentro del visor. Todo lo
demás se cierra y se vuelve a abrir desde *Ver* (con *Restablecer disposición*); la disposición se guarda por modo.
`Cmd+.` = modo foco (esconde los paneles).

**Controles flotantes del visor** (máximo 4 grupos, fondo `2a3040` al 92 %): arriba-izquierda las capas; arriba-derecha
la brújula (marca el lado de la puerta como «Frente») con orbitar e Iso/Libre; a la derecha el ascensor de plantas (corte
y leyenda de franjas a la vez); abajo la leyenda y el zoom.

## 6. Componentes clave

**Tarjeta del catálogo:** 112×128 pt; miniatura de 104×88 en un hueco hundido `242a38`, nombre (14 pt) y una línea
atenuada («6 diseños · 3 estilos»). Hover: borde `717e96`; seleccionada: 2 px `f0c030`; foco de teclado: anillo cian
`a8ecff`. Miniatura con el renderer real (Norteño, diseño 0, semilla 0), en caché por tipo y versión del generador;
mientras carga, el icono de la categoría atenuado, sin animación. Tooltip: descripción llana y rango de tamaño.
Categorías plegables con recuento; «Antiguas» empieza plegada.

**Controles de variante** (de la descripción de campos del puente, `docs/contrato.md`):

| Tipo de campo | Control |
|---|---|
| Opción de hasta 4 valores (estilo, daño, orientación) | Botones segmentados con icono y texto |
| Índice (diseño, rango, clase) | Paso a paso «< Diseño 2 de 6 >» |
| Lista con nombre (barrio, pieza del puerto) | Desplegable |
| Elemento mágico (14) | Rejilla 7×2 de muestras con el color del tema; nombre en tooltip y bajo la elegida |
| Rango (ancho, fondo, cimientos) | Deslizador entero con `DragValue` mono y el rango visible («4–9») |
| Máscara de puertos de torre | Cuatro casillas en cruz alrededor de un icono de torre, cada una con su lado escrito |
| Semilla | Campo mono, dado, < >, copiar |

El estado móvil va en «Estado (vista previa)» con el aviso «no se guarda en el diseño»: puertas abiertas/cerradas; muros
rompibles Intacto/Agrietado/Roto/Derribado; hojas de la puerta Abiertas/Cerradas/Astilladas/Reventadas; rastrillo
subido/bajado.

**Muestrario de semillas:** rejilla 3×3, 4×4 o 6×6 con la misma cámara, semilla en mono en una plaquita. Columnas =
semillas; filas opcionalmente varían un campo (nada, Estilo, Daño, Diseño). Clic: lleva la semilla a la Vista;
`Cmd+clic`: a Comparar; «Otra tanda» con D. Celdas progresivas; si una falla, rayado rojo con ✕ y «Copiar informe».

**Comparar:** dos visores con «Cámara enlazada» activada por defecto, cabecera de una línea («A: Casa · Norteño · 1234»).

**Paleta de materiales** (sin `Hidden`): muestras de 28 pt como mini cubo isométrico con las tres bandas del toon;
seleccionada con marco dorado de 2 px; rejilla o lista; fila de Recientes (8); búsqueda por nombre.

| Familia | Materiales |
|---|---|
| Piedra | Piedra, Mampostería, Sillería, Granito, Arenisca, Piedra gastada, Mármol, Mármol rojo, Mármol oscuro |
| Muros y revoques | Enlucido, Cal, Estuco ocre, Adobe, Ladrillo, Terracota, Fresco |
| Madera | Tablón, Tronco, Entramado, Cedro, Contraventana, Andamio, Casco, Cubierta, Brea |
| Tejados | Teja, Teja parda, Teja vidriada, Teja verde, Pizarra, Bálago, Tablilla, Tablilla alquitranada, Plomo |
| Vidrio y azulejo | Vidrio, Vidriera azul, Vidriera roja, Vidriera dorada, Cristal, Azulejo, Zellige |
| Metal | Hierro, Oro, Bronce, Cobre (pátina) |
| Telas y cabos | Tela roja, azul, verde, amarilla y morada; Lana, Lona, Vela, Cabo, Paja |
| Naturaleza | Césped, Grava, Musgo, Hojas, Hiedra, Flores, Agua |
| Fuego y daño | Brasa, Hollín, Madera quemada, Escombro |
| Color variable | Piedra, Moldura, Tejado, Tela y Brillo del tema; Tela del reino |

Las de color variable (y la Vela) llevan una esquina partida en diagonal; la cabecera de la familia incluye
«Ver con: [Fuego v]».

**Lista de validación:** icono de gravedad, mensaje llano y botón Localizar (clic en la fila = Localizar: encuadra y
parpadea en rojo 3 veces, 600 ms). Errores primero; chip de resumen en la pestaña. Con errores, *Exportar al juego*
queda desactivado y su tooltip dice por qué («Corrige 1 error para exportar»). Los avisos no bloquean.

**Toasts:** abajo a la derecha del visor, máximo 3, 360 pt, panel de piedra con barra izquierda de color e icono; 4 s,
el hover los detiene, los de error esperan a que se cierren. Ej.: «Captura guardada · Mostrar en Finder».

**Estados:** vacío (título en `font_title` y una sola acción); primera carga «Preparando el generador del juego…» con
barra ámbar indeterminada; error de compilación: se sigue viendo la última versión buena con una franja «El juego no
compila. Ves la última versión buena. [Ver detalles]»; fallo de una combinación: «Esta combinación ha fallado al
generarse», la semilla y «Copiar informe».

**Chip de recarga** (barra superior, 24 pt, fondo `242a38`, siempre icono + texto): `CHECK_CIRCLE` verde «Juego al
día»; icono girando ámbar «Recompilando…»; `X_CIRCLE` `f48c6d` «No compila (3)»; `PLUGS` atenuado «Sin generador». Al
pulsarlo se abre la Consola: arriba un resumen para artistas («El código del juego tiene errores; avisa a
programación» y «Copiar errores»), el detalle técnico plegado. **La consola no se abre sola.**

## 7. Capas del visor

- **Geometría de capas** (rellenos, barras, cajas, rayados): en el mismo búfer de baja resolución, después del
  sombreado y antes del escalado; líneas de 1 píxel de arte **sin luz toon ni el contorno del shader**, con su propio
  contorno de 1 px en el tono oscuro de su color (nunca negro ni blanco puros).
- **Etiquetas, chinchetas y tiradores:** en espacio de pantalla con egui, nítidos en HiDPI.
- **Lo tapado** (partes, entrada, selección): segunda pasada a rayas al 25 %.

| Capa (tecla) | Forma | Color / opacidad |
|---|---|---|
| Rejilla (1) | Líneas por celda, más marcadas cada 4; sub-cubo solo en el plano de edición desde 4× | `0d1018` al 35 % (55 % marcadas); sub-cubo 12 % |
| Navegación (2) | Relleno en la cara superior del suelo transitable; chevrones en escaleras; **barra baja tipo valla** en aristas bloqueadas | Cian `6ad4f0` 30 % con borde `2a82a8` (otras plantas 10 %); barras `f07a5a` con contorno `5a1420` |
| Partes (3) | Caja de alambre. Puerta: arco de giro a rayas en el suelo y punto en la bisagra. Rastrillo: ↕ del recorrido. Levadizo: arco vertical. Rompible: «2/3» | Naranja `f07a2a`, contorno `6a2410` |
| Interactuables (4) | Chincheta en pantalla: placa de 20 pt `f2ead8` con glifo `1c2130` y borde `3e2414`, palito a un punto dorado; solapadas se agrupan «+3» | — |
| Entrada (5) | Doble chevron en el suelo hacia dentro y etiqueta «Entrada»; siempre visible | `f8c860`, contorno `5a3010` |
| Huella (6) | Borde a rayas sobre la cota de cimientos; celdas de la máscara rellenas; lo no ocupado, rayado tenue | `ffe070`; relleno 15 % |
| Detalle (7) | Rayado a 45° sobre las caras de detalle | `f2ead8` 35 % |
| Franjas (8) | Líneas a rayas en los cambios de franja | Crema 40 % |

Por defecto: Explorar = Rejilla, Entrada y Partes; Editar = además Interactuables. La leyenda lista solo las capas
encendidas, cada una con su forma en miniatura.

**Gizmos:** corte (lo de encima se descarta, como el cutaway; rectángulo a rayas `f0c030` en la cota; plantas superiores
opcionalmente en fantasma desaturadas al 12 %). Puntero en Explorar: contorno de 1 píxel de arte crema alrededor del
sub-cubo (como `HoverOutline`). Pincel en Editar: cubo fantasma del material al 55 % con contorno crema; al borrar,
contorno rojo con rayado; medidas junto al cursor en mono («6×3×2»). Selección: «hormigas» `f0c030`/`1c2130` a 4 fps
(fijas con «Reducir movimiento»), tiradores de 8 pt en las 6 caras, tinte dorado del 20 %. Plano bloqueado: lámina
crema al 8 % con rejilla de sub-cubo.

## 8. Interacción

`Cmd` = `Ctrl` fuera de macOS. Ningún atajo usa `[ ] \ = ;` (en teclado ISO español piden Alt).

| Acción | Tecla |
|---|---|
| Lápiz / Borrar / Caja / Línea / Relleno | B / E / U / L / G |
| Extruir cara / Cuentagotas (o Alt) / Selección / Mover | X / I / M / V |
| Tamaño de pincel −/+ (1, 2, 4, 8 = celda) | `,` / `.` |
| Copiar / Cortar / Pegar / Duplicar | Cmd C / X / V / D |
| Espejo de la selección X / Z | Shift+X / Shift+Z |
| Girar la selección 90° | Cmd+R (Shift+Cmd+R al revés) |
| Girar la cámara 90° | Q / R |
| Encuadrar / Iso o Vista libre | F / P |
| Zoom entero / ajustar | `+` `-` / Cmd+0 |
| Planta arriba/abajo / plano ±1 sub-cubo (Editar) | Shift+↑↓ / ↑↓ |
| Corte por planta / capas | C / 1–8 |
| Semilla anterior/siguiente / al azar (Explorar) | ← → / D |
| Deshacer / Rehacer | Cmd+Z / Shift+Cmd+Z |
| Guardar / Abrir / Exportar / Captura | Cmd+S / Cmd+O / Cmd+E / Cmd+Shift+I |
| Buscar / Ayuda / Modo foco | Cmd+K / F1 / Cmd+. |
| Confirmar / cancelar | Intro / Esc |

**Ratón (MagicaVoxel):** izquierdo = herramienta (en Explorar fija «Bajo el cursor»); Shift+izquierdo = línea desde el
último punto o bloqueo a un eje; Alt+izquierdo = cuentagotas; arrastre derecho = orbitar (imán a 90° al soltar en Iso),
clic derecho sin arrastre (< 4 px) = menú contextual; central o Espacio+izquierdo = desplazar; rueda = zoom entero;
Shift+rueda = elevación 15–45°. **Trackpad:** dos dedos desplazan, pellizcar = zoom, Alt+dos dedos = orbitar.

**Primer arranque:** detectar la carpeta del juego (o pedirla con un mensaje claro); «¿Ratón o trackpad?» con dos
tarjetas; entrar en Explorar con Casa cargada (nunca un visor vacío) y 3 indicaciones descartables (elige, ajusta, gira
con Q/R). Siempre: tooltips en todo control con icono (400 ms: nombre, atajo, una línea); pistas de la herramienta en la
barra de estado; F1 = chuleta. Editar guarda una copia de recuperación cada 2 minutos y el título lleva un punto si hay
cambios sin guardar.

## 9. Qué NO hacer

- Marcos 9-slice de pixel art (se rompen a 1,25×/1,5× y pelean con egui). Del juego: paleta, luz y sombras duras.
- Dorado decorativo.
- Fuentes bitmap en el texto normal; texto atenuado sobre hover; `e8564a` como texto.
- Teñir el fondo del visor con el azul de la piedra.
- Luz toon o contorno del shader en las capas.
- Abrir la consola sola, bloquear el visor mientras recompila o dejar cerrar la pestaña Vista.
- Mostrar nombres de enum.
- Atajos pensados con teclado americano; resolver choques de atajos con modos (Q/R siempre giran la cámara).

## 10. Edición fácil y diseños del juego (fase 3)

Ampliación del agente de arte. Donde choque con lo anterior, manda esta sección (la tira de herramientas pasa a ser vertical; Borrar pasa de herramienta a modo; en Editar el selector de planta sustituye al ascensor).

**Disposición de Editar:**
- **tira de herramientas vertical y fija** a la izquierda de la Vista, en dos columnas de 30×28, con tres grupos: BÁSICAS (Lápiz, Caja, Línea, Relleno, Extruir, Cuentagotas, Selección, Mover), CONSTRUIR (Habitación, Muro, Suelo, Tejado, Ventana, Puerta, Escalera, Chimenea, Almenas, Cúpula, Aguja) y COLOCAR (Mueble, Sello);
- **barra contextual** arriba del visor: `Planta [Todo|Baja|1|2|+]` y luego solo los parámetros de la herramienta activa, que pasan a otra línea si no caben;
- **Biblioteca**: pestaña junto a la Paleta; solo se pone delante sola al entrar sin estructura o al elegir Mueble o Sello;
- **Aspecto**: sección de Propiedades, primera y abierta con una herramienta de construcción, y un chip resumen en la barra.

**Barra contextual por grupo:**
- Básicas: Modo `[Añadir|Pintar|Borrar]`, Pincel, Espejo, Plano (`ARROWS_IN_LINE_VERTICAL`; `LOCK_SIMPLE` queda para los candados) y Detalle.
- Caja añade `[Llena|Hueca|Solo paredes]` y Relleno `Hasta [Esta planta|Toda]`.
- Construir: el chip de Aspecto y sus parámetros.
- Colocar: nombre y tamaño, girar `[↺][↻]` y Detalle.

**Herramientas de construcción:**

| Herramienta | Icono | Gesto | Parámetros (valor por defecto) | Si no se puede |
|---|---|---|---|---|
| Habitación (H) | `HOUSE_LINE` | rectángulo | Plantas 1–6 (1), Suelos ☑ | «Se sale de la estructura» |
| Muro (W) | `WALL` | línea | Plantas (1) | — |
| Suelo (S) | `CHECKERBOARD` | rectángulo | — | — |
| Tejado (T) | `TRIANGLE` | clic en una habitación o rectángulo | Forma: Dos aguas (caballete paralelo a la fachada), Dos aguas (hastial a la fachada), Cuatro aguas, Media cuatro aguas, Aguja, Un agua, Mansarda; Inclinación `[Suave\|Media\|Empinada]` | «No hay nada que cubrir» |
| Ventana | `SQUARES_FOUR` | clic en un muro | Normal, Arco, Saetera o Rosetón; Contraventanas ☑; Jardinera ☐ | «Pon la ventana sobre un muro» / «Medianera: aquí no puede haber huecos» |
| Puerta | `DOOR` | clic en un muro, a ras de suelo | Hoja, Bisagra, Empieza abierta, Con llave | «La puerta de este diseño está fija en la celda N» / «Ya hay 16 partes» |
| Escalera | `STAIRS` | clic y arrastrar hacia donde sube | Hasta la planta N; Abrir hueco ☑ | «No hay planta encima: añade una con +» |
| Chimenea | `FIRE_SIMPLE` | clic | — | — |
| Almenas | `CASTLE_TURRET` | clic en lo alto de un muro | — | «Haz clic en lo alto de un muro» |
| Cúpula | `MOSQUE` | cuadrado de 1 a 6 celdas | — | — |
| Aguja | `CHURCH` | clic o hasta 3×3 | Altura Baja, Media o Alta | — |
| Mueble / Sello | `COUCH` / `STAMP` | clic; la pieza sigue en la mano | Detalle ☑ | «No cabe: choca con un muro» |

Detalles de la tabla:
- Habitación y Muro crean las plantas que falten en el mismo paso.
- La vista previa sale **opaca, con los materiales reales**, contorno dorado y la etiqueta «Habitación 4×3 celdas · 2 plantas — Suelta para construir · Esc cancela».
- Mientras llega la siguiente vista previa se mantiene la última.

**Atajos nuevos:**

| Tecla | Acción |
|---|---|
| A / N / E | Modo Añadir / Pintar / Borrar. Con Selección, Mover o Cuentagotas, pasan a Lápiz |
| H / W / S / T | Habitación / Muro / Suelo / Tejado |
| K | Plano en la cara señalada; otra vez, lo suelta |
| O / Shift+O | Girar lo que hay en la mano (mueble, sello, escalera, caballete) |
| Flechas con algo seleccionado | Mover la selección 1 sub-cubo según la cámara; Shift = celda; Alt+↑↓ = altura |
| Supr | Vaciar la selección |
| Esc | Cancela el arrastre, luego suelta la pieza, luego quita la selección |

**Biblioteca:**
- buscador arriba y `[Estructuras | Del juego | Piezas]`;
- tarjetas como las del Catálogo; la abierta lleva borde dorado;
- placa cian `SWAP` y «Sustituye · diseño N»;
- en «Del juego», dos botones a todo el ancho, cada uno con su línea de explicación;
- estados vacíos con una sola acción.

**Diseño del juego:**
- **Aviso modal:** «Editar un diseño del juego. Vas a retocar *Ayuntamiento norteño, diseño 1 (10×10, medianera a la izquierda)*. Al exportarlo, el juego usará tu versión en todas las ciudades donde salga… No podrás cambiar el tamaño ni el sitio de la puerta; las medianeras deben quedar sin huecos; necesita mostrador.»
- **Candados con su explicación:**
  - Tamaño: «Este diseño ocupa 10×10 celdas en el juego. Para otro tamaño, usa *Copiar como estructura nueva*.»
  - Puerta: «El juego pone la calle delante de esta celda. Puedes cambiar la hoja, no su sitio.»
- Si falta el mostrador, error de Validación con el botón «Colocar mostrador».
- **En Explorar**, aviso con barra cian: «✎ Diseño editado a mano: es el que usa el juego. [Ver el original] [Abrir en Editar]».

**Importar `.vox`:** ventana modal de 680 pt con:
- vista previa;
- tamaño en sub-cubos, celdas y niveles;
- Frente;
- modelos;
- una fila por color (muestra, sub-cubos, → material, ⚠ si ninguno se parece);
- avisos de puerta y plantas;
- botón «IMPORTAR COMO ESTRUCTURA NUEVA», desactivado si algún eje pasa de 256.

**Qué NO hacer:**
- tercera fila de botones o selector de grupo;
- ascensor y selector de planta a la vez;
- vista previa de construcción translúcida o con botón «Confirmar»;
- jerga («X/Z», «Variant», «party»);
- dorado en las insignias;
- bloquear el visor mientras llega la vista previa;
- volver de Borrar a Añadir sin avisar.

## 11. Modelos (árboles, objetos, intro)

Un modelo de vóxeles del juego se edita en el mismo modo Editar, quitando lo que es de edificios:

- **Biblioteca › Modelos**: cuarta sección. Tarjetas como las de estructuras, agrupadas («Árboles», «Naturaleza y
  objetos», «Ruinas y caminos», «Muebles y cofres», «Pilar de llegada», «Intro», «Libro de política») con el número de
  cada grupo; debajo del nombre, sus medidas y «retocado» si ya se guardó desde el editor. Arriba, «Nuevo modelo» e
  «Importar de MagicaVoxel…». Una tarjeta que no se puede abrir lo dice y explica por qué en el tooltip.
- **Paleta**: muestras con los colores del modelo (el cubo isométrico de siempre), el elegido con cerco dorado; arriba,
  el selector de color del elegido con su `#rrggbb` en mono; al final, «Añadir color».
- **Tira de herramientas**: solo BÁSICAS. La barra contextual no tiene «Planta» ni «Detalle»; en Pintar con el lápiz
  aparece el conmutador «Solo la cara».
- **Barra superior**: un único botón primario, «GUARDAR EN EL JUEGO» (no hay exportar).
- **Propiedades**: nombre (el del fichero, no editable), de qué script salió, tamaño en vóxeles con «Aplicar» y punto de
  apoyo. **Validación**: solo avisos (modelo vacío, tamaño cambiado, modelo nuevo sin enlazar); sin ellos, una línea
  verde.
- **Visor**: sin el moteado por sub-cubo (el juego pinta los modelos lisos). Las guías de la copa de un tronco y el
  punto de apoyo se dibujan como cubos fantasma (el apoyo, en rojo). La barra de estado da las medidas en vóxeles.
- Nada de `cell`, `faces`, `origin` ni `palette` en pantalla: «casilla», «caras», «punto de apoyo», «colores».
