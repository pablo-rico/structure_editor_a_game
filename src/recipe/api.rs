//! Lo que una receta puede llamar: el registro en el motor y su referencia (`cantero api`). Cada función registrada
//! tiene su línea en [`API`] (lo vigila un test), para que la ayuda no se quede atrás.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Context, Result, bail, ensure};
use rhai::{Dynamic, Engine, EvalAltResult, INT};

use super::{Canvas, hex, parse_hex, shift, to_json};
use crate::bridge::proto::Rgb;
use crate::edit::ops::{self, BoxShape, Clip};

type R<T> = std::result::Result<T, Box<EvalAltResult>>;

/// Referencia: `(sección, firma, qué hace)`. Coordenadas en sub-cubos (vóxeles en un modelo); las cajas son
/// semiabiertas: de la esquina menor a la mayor **sin incluirla**, como las marcas de las reglas de `cantero render`.
pub const API: &[(&str, &str, &str)] = &[
    ("Documento", "new_model(nombre, sx, sy, sz)", "Modelo nuevo y vacío. `nombre` es su sitio bajo Assets/_Game/Art sin `_model.json` (\"Objects/Voxel/farol\"). Receta generativa: rehace el fichero entero."),
    ("Documento", "new_structure(nombre, celdas_x, celdas_z, niveles)", "Estructura nueva (8 sub-cubos por celda y por nivel) con el forjado de piedra en el nivel 0. Va a ArtSource/Structures/<nombre>.agvox. Receta generativa."),
    ("Documento", "open(ruta)", "Abre un modelo o una estructura para retocarlo (receta de parche). Ruta absoluta o relativa a la carpeta del juego."),
    ("Documento", "open(ruta, huella)", "Como open(ruta), pero solo si el fichero tiene esa huella (la de `cantero info`): un parche no se aplica dos veces."),
    ("Documento", "clear()", "Borra todos los sub-cubos (conserva tamaño, paleta y demás datos): para rehacer por completo un fichero abierto."),
    ("Documento", "resize(sx, sy, sz)", "Modelos: cambia el tamaño creciendo o encogiendo por +x, +y, +z sin mover lo que hay."),
    ("Documento", "size()", "Tamaño de la rejilla: [sx, sy, sz]."),
    ("Documento", "solid_box()", "Caja de lo sólido [x0, y0, z0, x1, y1, z1], o () si está vacío."),
    ("Documento", "get(x, y, z)", "Qué hay en un sub-cubo: el color \"rrggbb\", el nombre del material, o \"\" si es aire."),
    ("Documento", "count()", "Cuántos sub-cubos sólidos hay."),
    ("Documento", "count(nombre)", "Cuántos sub-cubos hay de ese color o material."),
    ("Pincel", "color(nombre)", "Modelos: pinta con el color \"rrggbb\" (se añade a la paleta) o con un material definido con tones()."),
    ("Pincel", "tones(nombre, tapa, borde, lateral, sombra)", "Modelos: define un material sombreado de cuatro tonos y lo elige. Al acabar la receta, sus caras de arriba salen en `tapa` (`borde` en las aristas), los laterales en `lateral` y lo que queda bajo un saliente o por debajo, en `sombra`."),
    ("Pincel", "shaded(color)", "Modelos: como tones(), sacando los cuatro tonos de un solo color (el lateral). El material se llama como el color."),
    ("Pincel", "material(nombre)", "Estructuras: pinta con un material del juego por su nombre (\"Stone\", \"Plank\"…; `cantero catalog materiales`)."),
    ("Pincel", "air()", "Pinta con aire: desde aquí las formas borran."),
    ("Pincel", "detail(si)", "Estructuras: lo que se pinte a partir de aquí es detalle (muebles, escaleras): no cierra el paso como un muro."),
    ("Pincel", "mirror(en_x, en_z)", "Simetría: cada forma se repite reflejada respecto al centro de la rejilla en x y/o en z."),
    ("Formas", "voxel(x, y, z)", "Un sub-cubo."),
    ("Formas", "box(x0, y0, z0, x1, y1, z1)", "Caja maciza."),
    ("Formas", "hollow(x0, y0, z0, x1, y1, z1)", "Caja hueca: solo la cáscara de un sub-cubo de grosor."),
    ("Formas", "walls(x0, y0, z0, x1, y1, z1)", "Las cuatro paredes de la caja, sin suelo ni techo."),
    ("Formas", "line(x0, y0, z0, x1, y1, z1)", "Línea de sub-cubos entre dos, ambos incluidos."),
    ("Formas", "cylinder(cx, cz, r, y0, y1)", "Cilindro vertical de radio r con el eje en (cx, cz), de y0 a y1. Centro entero = diámetro par; acabado en .5 = diámetro impar."),
    ("Formas", "tube(cx, cz, r, r_dentro, y0, y1)", "Cilindro vertical hueco."),
    ("Formas", "cylinder_x(cy, cz, r, x0, x1)", "Cilindro tumbado a lo largo de x."),
    ("Formas", "cylinder_z(cx, cy, r, z0, z1)", "Cilindro tumbado a lo largo de z."),
    ("Formas", "cone(cx, cz, r0, r1, y0, y1)", "Tronco de cono vertical: radio r0 abajo y r1 arriba (0 para acabar en punta)."),
    ("Formas", "revolve(cx, cz, y0, radios)", "Sólido de revolución: una capa por cada radio de la lista, desde y0 hacia arriba (copas, jarrones, cúpulas)."),
    ("Formas", "sphere(cx, cy, cz, r)", "Esfera."),
    ("Formas", "ellipsoid(cx, cy, cz, rx, ry, rz)", "Elipsoide."),
    ("Formas", "roof(x0, y, z0, x1, z1, cumbrera)", "Tejado macizo sobre el rectángulo, desde la altura y: a dos aguas con la cumbrera a lo largo de \"x\" o \"z\", o a cuatro con \"\"."),
    ("Formas", "roof(x0, y, z0, x1, z1, cumbrera, paso)", "Como roof(), subiendo `paso` capas por cada sub-cubo que se mete (más empinado)."),
    ("Formas", "fill(x, y, z)", "Rellena con el pincel la región unida por caras del mismo material que ese sub-cubo (también el aire encerrado)."),
    ("Formas", "replace(de, a)", "Cambia un color o material por otro en todo el documento. Devuelve cuántos."),
    ("Formas", "replace(de, a, x0, y0, z0, x1, y1, z1)", "Como replace(), solo dentro de la caja."),
    ("Formas", "speckle(x0, y0, z0, x1, y1, z1, porcentaje, semilla)", "Repinta con el pincel, al azar, ese porcentaje de lo sólido de la caja (vetas, desgaste, hojas distintas). La misma semilla da siempre lo mismo."),
    ("Formas", "speckle(x0, y0, z0, x1, y1, z1, porcentaje, semilla, solo)", "Como speckle(), repintando solo lo que sea de ese color o material."),
    ("Formas", "noise(semilla, x, y, z)", "Número de 0 a 99, fijo para esa semilla y ese sub-cubo: para decidir en el script."),
    ("Sprite", "sprite_radii(ruta, eje, suelo, y0, y1)", "Modelos: perfil de un sprite del juego (PNG; ruta relativa a la carpeta del juego) como lista de radios para revolve(), una capa por cada y de y0 a y1. `eje` es la columna del eje del dibujo (16 = entre los píxeles 15 y 16) y `suelo` la fila del sprite que corresponde a y = 0. Sin el contorno que el sprite trae dibujado."),
    ("Sprite", "sprite_wrap(ruta, eje, suelo, x0, y0, z0, x1, y1, z1)", "Modelos: al acabar la receta, forra las caras de la caja con el dibujo del sprite tal como lo proyecta la cámara del juego (un vóxel ≈ un píxel), por cuartos: desde cada una de las cuatro diagonales, el centro del modelo es el centro del sprite, píxel a píxel. Lo que cae fuera del dibujo toma el píxel de dentro más cercano de su fila. Compruébalo con `cantero vista`."),
    ("Copiar", "copy(x0, y0, z0, x1, y1, z1)", "Copia la caja (con su sombreado). Devuelve el recorte."),
    ("Copiar", "paste(recorte, x, y, z)", "Pega el recorte con su esquina menor en (x, y, z). El aire del recorte no borra."),
    ("Copiar", "rotated(recorte)", "El recorte girado un cuarto de vuelta, como gira el juego: (x, z) pasa a (fondo − 1 − z, x)."),
    ("Copiar", "flipped_x(recorte)", "El recorte reflejado en x."),
    ("Copiar", "flipped_z(recorte)", "El recorte reflejado en z."),
    ("Modelo", "origin(x, y, z)", "Punto del modelo que cae en la esquina de la casilla (modelos más anchos que su casilla)."),
    ("Estructura", "style(nombre)", "Estilo de la estructura (\"Northern\"…; `cantero catalog estilos`)."),
    ("Estructura", "ground_level(n)", "Nivel del suelo de fuera: los cimientos ocupan los niveles de debajo."),
    ("Estructura", "bands(lista)", "Plantas, de abajo arriba: [[sub-cubo donde empieza la franja, nivel de su suelo], …]."),
    ("Estructura", "entrance(x)", "Celda (en x) de fuera pegada a la puerta, en la fachada del frente (z = 0)."),
    ("Estructura", "courtyard(si)", "Tiene patio interior."),
    ("Estructura", "floor_y(y)", "Altura donde se pisa en la planta sobre la que trabajan las herramientas (por defecto, el suelo de la calle)."),
    ("Estructura", "look(barrio, clase)", "Materiales por papel (muro, tejado…) de ese barrio y clase para las herramientas (`cantero catalog estilos`)."),
    ("Estructura", "role(papel, material)", "Cambia el material de un papel suelto para las herramientas."),
    ("Estructura", "tool(nombre, donde)", "Herramienta de construcción del juego con sus parámetros por defecto, sobre la planta de floor_y(). `donde` según el `target` de la herramienta en `cantero catalog herramientas`: rect = #{rect: [cx0, cz0, cx1, cz1]} en celdas incluidas; line = lo mismo, de una celda de ancho; face = #{cell: [cx, cz], dir: d}, la cara del muro en ese borde de la celda; cell = #{cell: [cx, cz], dir: d}, hacia donde sube o mira. dir: 0 +X, 1 −X, 2 +Z, 3 −Z."),
    ("Estructura", "tool(nombre, donde, parametros)", "Como tool(), con parámetros por su `key`: #{storeys: 2, walls: 1}. Los de sí o no van como 1 o 0; los de lista, con el `value` de la opción."),
    ("Estructura", "stamp(mueble, x, y, z, mira)", "Mueble del juego apoyado en (x, y, z), con y = el suelo donde se pisa, mirando a `mira` (0 +X, 1 −X, 2 +Z, 3 −Z). Ver `cantero catalog muebles`."),
    ("Estructura", "stamp(mueble, x, y, z, mira, ancho)", "Como stamp(), para los muebles de ancho variable."),
];

/// La referencia en texto, por secciones.
pub fn reference() -> String {
    let mut out = String::from(
        "Recetas de cantero: un script Rhai (https://rhai.rs) con estas funciones.\n\
         Coordenadas en sub-cubos (vóxeles en un modelo): x a la derecha, y arriba, z hacia el fondo; el frente es z = 0.\n\
         Las cajas van de la esquina menor a la mayor sin incluirla, como las marcas de las reglas de `cantero render`.\n\
         Los bucles del script colocan formas, no sub-cubos. print() sale en el resumen.\n",
    );
    let mut section = "";
    for (s, signature, help) in API {
        if *s != section {
            section = s;
            out.push_str(&format!("\n{section}\n"));
        }
        out.push_str(&format!("  {signature}\n      {help}\n"));
    }
    out
}

fn rt(e: anyhow::Error) -> Box<EvalAltResult> {
    format!("{e:#}").into()
}

fn int(d: &Dynamic) -> Result<i32> {
    if let Ok(i) = d.as_int() {
        return i32::try_from(i).ok().context("número demasiado grande");
    }
    match d.as_float() {
        Ok(f) if f.fract() == 0.0 => Ok(f as i32),
        Ok(f) => bail!("aquí va un número entero, no {f}"),
        Err(_) => bail!("aquí va un número entero, no un valor de tipo {}", d.type_name()),
    }
}

fn num(d: &Dynamic) -> Result<f32> {
    if let Ok(i) = d.as_int() {
        return Ok(i as f32);
    }
    d.as_float().map(|f| f as f32).map_err(|_| anyhow::anyhow!("aquí va un número, no un valor de tipo {}", d.type_name()))
}

fn text(d: &Dynamic) -> Result<String> {
    ensure!(d.is_string(), "aquí va un texto entre comillas, no un valor de tipo {}", d.type_name());
    Ok(d.clone().into_string().unwrap_or_default())
}

fn flag(d: &Dynamic) -> Result<bool> {
    d.as_bool().map_err(|_| anyhow::anyhow!("aquí va true o false, no un valor de tipo {}", d.type_name()))
}

fn ints<const N: usize>(a: [&Dynamic; N]) -> Result<[i32; N]> {
    let mut out = [0; N];
    for (o, d) in out.iter_mut().zip(a) {
        *o = int(d)?;
    }
    Ok(out)
}

fn rgb(d: &Dynamic) -> Result<Rgb> {
    let t = text(d)?;
    parse_hex(&t).with_context(|| format!("«{t}» no es un color rrggbb"))
}

fn clip(d: &Dynamic) -> Result<Clip> {
    d.clone().try_cast::<Clip>().context("aquí va un recorte de copy()")
}

fn list(items: impl IntoIterator<Item = i32>) -> Dynamic {
    Dynamic::from_array(items.into_iter().map(|n| Dynamic::from(n as INT)).collect())
}

/// Registra `nombre(args…)`: los argumentos llegan como `Dynamic` (así un 4 y un 4.5 valen donde va un número) y el
/// cuerpo devuelve `Result<Dynamic>`.
macro_rules! def {
    ($engine:expr, $shared:expr, $name:literal, |$c:ident $(, $a:ident)*| $body:block) => {{
        let shared = $shared.clone();
        $engine.register_fn($name, move |$($a: Dynamic),*| -> R<Dynamic> {
            let mut guard = shared.borrow_mut();
            let $c: &mut Canvas = &mut guard;
            let out: Result<Dynamic> = (|| $body)();
            out.map_err(rt)
        });
    }};
}

const DONE: Result<Dynamic> = Ok(Dynamic::UNIT);

pub(crate) fn register(e: &mut Engine, s: &Rc<RefCell<Canvas>>) {
    e.register_type_with_name::<Clip>("Recorte");

    // Documento
    def!(e, s, "new_model", |c, name, sx, sy, sz| {
        c.new_model(&text(&name)?, ints([&sx, &sy, &sz])?)?;
        DONE
    });
    def!(e, s, "new_structure", |c, name, cx, cz, levels| {
        c.new_structure(&text(&name)?, ints([&cx, &cz])?, int(&levels)?)?;
        DONE
    });
    def!(e, s, "open", |c, path| {
        c.open(&text(&path)?, None)?;
        DONE
    });
    def!(e, s, "open", |c, path, base| {
        c.open(&text(&path)?, Some(&text(&base)?))?;
        DONE
    });
    def!(e, s, "clear", |c| {
        let doc = c.doc()?;
        let [sx, sy, sz] = doc.s.grid.size;
        for v in ops::cuboid([0, 0, 0], [sx - 1, sy - 1, sz - 1]) {
            if doc.s.grid.get(v[0], v[1], v[2]) != 0 {
                doc.paint(v[0], v[1], v[2], 0, false);
            }
        }
        DONE
    });
    def!(e, s, "resize", |c, sx, sy, sz| {
        c.resize(ints([&sx, &sy, &sz])?)?;
        DONE
    });
    def!(e, s, "size", |c| { Ok(list(c.doc()?.s.grid.size)) });
    def!(e, s, "solid_box", |c| {
        Ok(match c.doc()?.s.grid.solid_box() {
            Some((lo, hi)) => list(lo.into_iter().chain(hi)),
            None => Dynamic::UNIT,
        })
    });
    def!(e, s, "get", |c, x, y, z| {
        let [x, y, z] = ints([&x, &y, &z])?;
        let b = c.doc()?.s.grid.get(x, y, z);
        Ok(c.name_of(b)?.into())
    });
    def!(e, s, "count", |c| { Ok(Dynamic::from(c.doc()?.s.grid.counts()[1..].iter().sum::<u32>() as INT)) });
    def!(e, s, "count", |c, name| {
        let b = c.byte_of(&text(&name)?)?;
        Ok(Dynamic::from(c.doc()?.s.grid.counts()[b as usize] as INT))
    });

    // Pincel
    def!(e, s, "color", |c, name| {
        c.color(&text(&name)?)?;
        DONE
    });
    def!(e, s, "tones", |c, name, top, rim, side, under| {
        c.define_tones(&text(&name)?, rgb(&top)?, rgb(&rim)?, rgb(&side)?, rgb(&under)?)?;
        DONE
    });
    def!(e, s, "shaded", |c, color| {
        let side = rgb(&color)?;
        c.define_tones(&hex(side), shift(side, 0.16), shift(side, 0.34), side, shift(side, -0.36))?;
        DONE
    });
    def!(e, s, "material", |c, name| {
        c.material(&text(&name)?)?;
        DONE
    });
    def!(e, s, "air", |c| {
        c.doc()?;
        c.brush = 0;
        DONE
    });
    def!(e, s, "detail", |c, on| {
        c.structure_only("detail()")?;
        c.detail = flag(&on)?;
        DONE
    });
    def!(e, s, "mirror", |c, x, z| {
        c.mirror = (flag(&x)?, flag(&z)?);
        DONE
    });

    // Formas
    def!(e, s, "voxel", |c, x, y, z| {
        c.put(ints([&x, &y, &z])?)?;
        DONE
    });
    for (name, shape) in [("box", BoxShape::Solid), ("hollow", BoxShape::Hollow), ("walls", BoxShape::Walls)] {
        let shared = s.clone();
        e.register_fn(name, move |x0: Dynamic, y0: Dynamic, z0: Dynamic, x1: Dynamic, y1: Dynamic, z1: Dynamic| -> R<Dynamic> {
            let b = ints([&x0, &y0, &z0, &x1, &y1, &z1]).map_err(rt)?;
            shared.borrow_mut().shaped(b, shape).map_err(rt)?;
            Ok(Dynamic::UNIT)
        });
    }
    def!(e, s, "line", |c, x0, y0, z0, x1, y1, z1| {
        let b = ints([&x0, &y0, &z0, &x1, &y1, &z1])?;
        c.put_all(ops::line([b[0], b[1], b[2]], [b[3], b[4], b[5]]))?;
        DONE
    });
    def!(e, s, "cylinder", |c, cx, cz, r, y0, y1| {
        let [y0, y1] = span(&y0, &y1)?;
        c.put_all(ops::cylinder(1, [num(&cx)?, num(&cz)?], radius(&r)?, 0.0, y0, y1))?;
        DONE
    });
    def!(e, s, "tube", |c, cx, cz, r, r_in, y0, y1| {
        let [y0, y1] = span(&y0, &y1)?;
        c.put_all(ops::cylinder(1, [num(&cx)?, num(&cz)?], radius(&r)?, num(&r_in)?, y0, y1))?;
        DONE
    });
    def!(e, s, "cylinder_x", |c, cy, cz, r, x0, x1| {
        let [x0, x1] = span(&x0, &x1)?;
        c.put_all(ops::cylinder(0, [num(&cy)?, num(&cz)?], radius(&r)?, 0.0, x0, x1))?;
        DONE
    });
    def!(e, s, "cylinder_z", |c, cx, cy, r, z0, z1| {
        let [z0, z1] = span(&z0, &z1)?;
        c.put_all(ops::cylinder(2, [num(&cx)?, num(&cy)?], radius(&r)?, 0.0, z0, z1))?;
        DONE
    });
    def!(e, s, "cone", |c, cx, cz, r0, r1, y0, y1| {
        let [y0, y1] = span(&y0, &y1)?;
        let (r0, r1, n) = (num(&r0)?, num(&r1)?, y1 - y0 + 1);
        let profile: Vec<f32> = (0..n).map(|i| if n == 1 { r0 } else { r0 + (r1 - r0) * i as f32 / (n - 1) as f32 }).collect();
        c.put_all(ops::revolve([num(&cx)?, num(&cz)?], y0, &profile))?;
        DONE
    });
    def!(e, s, "revolve", |c, cx, cz, y0, radii| {
        let radii = radii.clone().into_array().map_err(|_| anyhow::anyhow!("los radios van en una lista: [3, 3.5, 2]"))?;
        let profile = radii.iter().map(num).collect::<Result<Vec<f32>>>()?;
        c.put_all(ops::revolve([num(&cx)?, num(&cz)?], int(&y0)?, &profile))?;
        DONE
    });
    def!(e, s, "sphere", |c, cx, cy, cz, r| {
        let r = radius(&r)?;
        c.put_all(ops::ellipsoid([num(&cx)?, num(&cy)?, num(&cz)?], [r; 3]))?;
        DONE
    });
    def!(e, s, "ellipsoid", |c, cx, cy, cz, rx, ry, rz| {
        c.put_all(ops::ellipsoid([num(&cx)?, num(&cy)?, num(&cz)?], [radius(&rx)?, radius(&ry)?, radius(&rz)?]))?;
        DONE
    });
    def!(e, s, "roof", |c, x0, y, z0, x1, z1, ridge| {
        c.put_all(roof(ints([&x0, &y, &z0, &x1, &z1])?, &text(&ridge)?, 1)?)?;
        DONE
    });
    def!(e, s, "roof", |c, x0, y, z0, x1, z1, ridge, step| {
        c.put_all(roof(ints([&x0, &y, &z0, &x1, &z1])?, &text(&ridge)?, int(&step)?)?)?;
        DONE
    });
    def!(e, s, "fill", |c, x, y, z| {
        c.fill(ints([&x, &y, &z])?)?;
        DONE
    });
    def!(e, s, "replace", |c, from, to| { Ok(Dynamic::from(c.replace(&text(&from)?, &text(&to)?, None)?)) });
    def!(e, s, "replace", |c, from, to, x0, y0, z0, x1, y1, z1| {
        let b = ints([&x0, &y0, &z0, &x1, &y1, &z1])?;
        Ok(Dynamic::from(c.replace(&text(&from)?, &text(&to)?, Some(b))?))
    });
    def!(e, s, "speckle", |c, x0, y0, z0, x1, y1, z1, percent, seed| {
        let b = ints([&x0, &y0, &z0, &x1, &y1, &z1])?;
        Ok(Dynamic::from(c.speckle(b, int(&percent)?, int(&seed)?, None)?))
    });
    def!(e, s, "speckle", |c, x0, y0, z0, x1, y1, z1, percent, seed, only| {
        let b = ints([&x0, &y0, &z0, &x1, &y1, &z1])?;
        Ok(Dynamic::from(c.speckle(b, int(&percent)?, int(&seed)?, Some(&text(&only)?))?))
    });
    def!(e, s, "noise", |_c, seed, x, y, z| {
        let [seed, x, y, z] = ints([&seed, &x, &y, &z])?;
        Ok(Dynamic::from((ops::noise(seed as u32, [x, y, z]) % 100) as INT))
    });

    // Copiar
    def!(e, s, "copy", |c, x0, y0, z0, x1, y1, z1| { Ok(Dynamic::from(c.copy(ints([&x0, &y0, &z0, &x1, &y1, &z1])?)?)) });
    def!(e, s, "paste", |c, piece, x, y, z| {
        c.paste(&clip(&piece)?, ints([&x, &y, &z])?)?;
        DONE
    });
    def!(e, s, "rotated", |_c, piece| { Ok(Dynamic::from(clip(&piece)?.rotate_quarter())) });
    def!(e, s, "flipped_x", |_c, piece| { Ok(Dynamic::from(clip(&piece)?.mirror_x())) });
    def!(e, s, "flipped_z", |_c, piece| { Ok(Dynamic::from(clip(&piece)?.mirror_z())) });

    // Sprite
    def!(e, s, "sprite_radii", |c, path, axis, ground, y0, y1| {
        let radii = c.sprite_radii(&text(&path)?, num(&axis)?, int(&ground)?, int(&y0)?, int(&y1)?)?;
        Ok(Dynamic::from_array(radii.into_iter().map(|r| Dynamic::from(r as rhai::FLOAT)).collect()))
    });
    def!(e, s, "sprite_wrap", |c, path, axis, ground, x0, y0, z0, x1, y1, z1| {
        c.sprite_wrap(&text(&path)?, num(&axis)?, int(&ground)?, ints([&x0, &y0, &z0, &x1, &y1, &z1])?)?;
        DONE
    });

    // Modelo
    def!(e, s, "origin", |c, x, y, z| {
        c.model()?.origin = Some(ints([&x, &y, &z])?);
        DONE
    });

    // Estructura
    def!(e, s, "style", |c, name| {
        c.structure_only("style()")?;
        let name = text(&name)?;
        ensure!(c.game.catalog()?.styles.iter().any(|st| st.name == name), "el juego no tiene el estilo «{name}»");
        c.doc()?.edit_extras("Estilo", |x| x.meta.style = name);
        DONE
    });
    def!(e, s, "ground_level", |c, n| {
        c.structure_only("ground_level()")?;
        let n = int(&n)?;
        c.doc()?.edit_extras("Suelo", |x| x.ground_level = n);
        DONE
    });
    def!(e, s, "bands", |c, bands| {
        c.structure_only("bands()")?;
        let rows = bands.clone().into_array().map_err(|_| anyhow::anyhow!("las plantas van en una lista: [[8, 1], [24, 3]]"))?;
        let mut pairs = Vec::new();
        for row in rows {
            let pair = row.into_array().map_err(|_| anyhow::anyhow!("cada planta es [sub-cubo donde empieza, nivel del suelo]"))?;
            ensure!(pair.len() == 2, "cada planta es [sub-cubo donde empieza, nivel del suelo]");
            pairs.push([int(&pair[0])?, int(&pair[1])?]);
        }
        c.doc()?.edit_extras("Plantas", |x| {
            x.band_starts = pairs.iter().map(|p| p[0]).collect();
            x.band_floors = pairs.iter().map(|p| p[1]).collect();
        });
        DONE
    });
    def!(e, s, "entrance", |c, x| {
        c.structure_only("entrance()")?;
        let x = int(&x)?;
        c.doc()?.edit_extras("Entrada", |e| e.entrance = [x, -1]);
        DONE
    });
    def!(e, s, "courtyard", |c, on| {
        c.structure_only("courtyard()")?;
        let on = flag(&on)?;
        c.doc()?.edit_extras("Patio", |x| x.courtyard = on);
        DONE
    });
    def!(e, s, "floor_y", |c, y| {
        c.structure_only("floor_y()")?;
        c.floor_y = Some(int(&y)?);
        DONE
    });
    def!(e, s, "look", |c, district, tier| {
        c.look(&text(&district)?, &text(&tier)?)?;
        DONE
    });
    def!(e, s, "role", |c, role, material| {
        c.structure_only("role()")?;
        let material = text(&material)?;
        ensure!(c.game.catalog()?.materials.contains(&material), "el juego no tiene el material «{material}»");
        c.roles.insert(text(&role)?, material.into());
        DONE
    });
    def!(e, s, "tool", |c, name, target| {
        c.tool(&text(&name)?, to_json(&target)?, serde_json::json!({}))?;
        DONE
    });
    def!(e, s, "tool", |c, name, target, params| {
        c.tool(&text(&name)?, to_json(&target)?, to_json(&params)?)?;
        DONE
    });
    def!(e, s, "stamp", |c, piece, x, y, z, facing| {
        c.stamp(&text(&piece)?, ints([&x, &y, &z])?, int(&facing)?, None, 0)?;
        DONE
    });
    def!(e, s, "stamp", |c, piece, x, y, z, facing, width| {
        c.stamp(&text(&piece)?, ints([&x, &y, &z])?, int(&facing)?, Some(int(&width)?), 0)?;
        DONE
    });
}

fn radius(d: &Dynamic) -> Result<f32> {
    let r = num(d)?;
    ensure!(r > 0.0 && r <= 256.0, "el radio va de 0 a 256, no {r}");
    Ok(r)
}

/// Tramo semiabierto `a..b` como extremos incluidos.
fn span(a: &Dynamic, b: &Dynamic) -> Result<[i32; 2]> {
    let [a, b] = ints([a, b])?;
    ensure!(a < b, "tramo vacío: va del menor al mayor, sin incluir el mayor ({a}, {b})");
    Ok([a, b - 1])
}

fn roof(b: [i32; 5], ridge: &str, step: i32) -> Result<Vec<ops::V>> {
    let [x0, y, z0, x1, z1] = b;
    ensure!(x0 < x1 && z0 < z1, "rectángulo vacío: va de la esquina menor a la mayor, sin incluirla");
    ensure!((1..=16).contains(&step), "el paso va de 1 a 16");
    let ridge = match ridge {
        "x" => Some(0),
        "z" => Some(2),
        "" => None,
        other => bail!("la cumbrera va a lo largo de \"x\" o de \"z\", o \"\" para cuatro aguas, no «{other}»"),
    };
    Ok(ops::roof([x0, y, z0], [x1 - 1, y, z1 - 1], ridge, step))
}
