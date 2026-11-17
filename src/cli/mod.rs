//! `cantero`: el editor sin ventana (`docs/recetas.md`). Cada subcomando escribe un JSON corto por la salida estándar;
//! un fallo va por la de errores, en lenguaje llano, y el proceso sale con 1. Los modelos no arrancan el generador;
//! las estructuras hablan con el puente real.

pub mod gameview;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::agvox;
use crate::bridge::Paths;
use crate::bridge::client::Client;
use crate::bridge::proto::Catalog;
use crate::edit::Doc;
use crate::labels;
use crate::mesh::Paint;
use crate::render::sheet::{self, ViewKind};
use crate::render::{self, Layers};
use crate::vmodel;

const HELP: &str = "\
cantero: estructuras y modelos de a_game sin ventana.

  cantero info <fichero>
      Tamaño, caja de lo sólido, recuento por color o material, plantas, avisos y huella.
  cantero render <fichero> [--out <png>] [--vista iso,iso-atras,frente,lado,planta|todas] [--tam <ancho>[x<alto>]]
                 [--caja x0,y0,z0,x1,y1,z1] [--marca x0,y0,z0,x1,y1,z1] [--planta <n>] [--capas <lista>]
      Lámina de vistas en un PNG, con reglas en sub-cubos en alzados y planta. --caja encuadra una región,
      --marca la resalta en rojo, --planta corta por esa planta (0 = la baja).
      Capas: rejilla, nav, partes, interactuables, entrada, huella, detalle, plantas.
  cantero vista <modelo> [--out <png>] [--giro 0..3|todos] [--zoom <n>] [--sin-contorno]
      Un modelo como se ve en el juego: a su tamaño real (un vóxel ≈ un píxel), con la luz lisa de los árboles y
      el contorno oscuro, desde los cuatro lados. Para juzgar el aspecto; `render` es para medir.
  cantero capas <fichero> --caja x0,y0,z0,x1,y1,z1 [--eje y|x|z]
      Rebanadas en texto de una región (una letra por color o material), para ver sub-cubo a sub-cubo.
  cantero run <receta.rhai> [--salida <carpeta>] [--forzar] [--render [opciones de render]]
      Ejecuta una receta, guarda lo que deja y resume el cambio y los avisos. --salida escribe un borrador en esa
      carpeta en vez de en el juego; --forzar rehace un fichero aunque se haya retocado a mano; --render dibuja
      además la lámina del resultado.
  cantero export
      Lleva al juego todas las estructuras de ArtSource/Structures: da número a las que no lo tienen y regenera el
      C# del juego. Si alguna tiene errores no se escribe nada. Los modelos no se exportan: su fichero ya es el del juego.
  cantero api
      Referencia de las funciones de las recetas.
  cantero catalog [materiales|herramientas|muebles|estilos]
      Lo que ofrece el juego: materiales con su color, herramientas de construcción y sus parámetros, muebles,
      estilos, barrios y clases.

Un fichero es un modelo del juego (`*_model.json`) o una estructura a mano (`.agvox`).
Opciones comunes: --juego <carpeta de a_game> (por defecto AGAME_ROOT o ../a_game), --recompilar (el generador).";

/// Argumentos: posicionales y `--clave valor` (o `--clave` a secas para las que no llevan valor).
pub struct Args {
    pub pos: Vec<String>,
    opts: HashMap<String, String>,
}

/// Opciones sin valor.
const FLAGS: [&str; 4] = ["recompilar", "forzar", "ayuda", "render"];

impl Args {
    pub fn parse(args: &[String]) -> Result<Self> {
        let (mut pos, mut opts) = (Vec::new(), HashMap::new());
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.strip_prefix("--") {
                Some(key) if FLAGS.contains(&key) => {
                    opts.insert(key.to_string(), String::new());
                }
                Some(key) => {
                    let value = it.next().with_context(|| format!("a --{key} le falta el valor"))?;
                    opts.insert(key.to_string(), value.clone());
                }
                None => pos.push(a.clone()),
            }
        }
        Ok(Self { pos, opts })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.opts.get(key).map(String::as_str)
    }

    pub fn has(&self, key: &str) -> bool {
        self.opts.contains_key(key)
    }

    /// Lista de enteros separados por comas, de largo `N`.
    pub fn ints<const N: usize>(&self, key: &str) -> Result<Option<[i32; N]>> {
        let Some(text) = self.get(key) else { return Ok(None) };
        let nums: Vec<i32> = text
            .split(',')
            .map(|n| n.trim().parse().with_context(|| format!("--{key}: «{n}» no es un número entero")))
            .collect::<Result<_>>()?;
        let out: [i32; N] = nums.try_into().map_err(|_| anyhow::anyhow!("--{key} lleva {N} números separados por comas"))?;
        Ok(Some(out))
    }

    fn file(&self) -> Result<PathBuf> {
        self.pos.get(1).map(PathBuf::from).context("falta el fichero")
    }
}

/// El juego: carpeta de a_game y, solo si hace falta, el generador y su catálogo.
pub struct Game {
    pub root: PathBuf,
    rebuild: bool,
    client: Option<Client>,
    catalog: Option<Catalog>,
}

impl Game {
    pub fn new(root: PathBuf, rebuild: bool) -> Self {
        Self { root, rebuild, client: None, catalog: None }
    }

    fn from_args(args: &Args) -> Self {
        let root = args.get("juego").map_or_else(Paths::default_agame_root, PathBuf::from);
        Self::new(root, args.has("recompilar"))
    }

    /// El generador, lanzado la primera vez que se pide.
    pub fn client(&mut self) -> Result<&mut Client> {
        if self.client.is_none() {
            self.client = Some(Client::start(Paths::new(self.root.clone()), self.rebuild)?);
        }
        Ok(self.client.as_mut().expect("recién lanzado"))
    }

    pub fn catalog(&mut self) -> Result<&Catalog> {
        if self.catalog.is_none() {
            self.catalog = Some(self.client()?.catalog()?);
        }
        Ok(self.catalog.as_ref().expect("recién pedido"))
    }

    /// Suelta el generador (tras exportar hay que recompilarlo para que vea lo nuevo).
    pub fn restart(&mut self) {
        self.client = None;
        self.catalog = None;
        self.rebuild = true;
    }
}

pub fn is_model_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "json")
}

/// Abre un modelo (`.json`) o una estructura (`.agvox`).
pub fn open_doc(path: &Path, game: &mut Game) -> Result<Doc> {
    let text = std::fs::read_to_string(path).with_context(|| format!("no puedo leer {}", path.display()))?;
    if is_model_path(path) {
        let (model, s) = vmodel::load(&text).with_context(|| format!("{}", path.display()))?;
        return Ok(Doc::new_model(model, s, Some(path.to_path_buf())));
    }
    ensure!(path.extension().is_some_and(|e| e == "agvox"), "no sé abrir {}: espero un _model.json o un .agvox", path.display());
    let (meta, s) = agvox::load(&text, &game.catalog()?.materials).with_context(|| format!("{}", path.display()))?;
    Ok(Doc::new(meta, s, Some(path.to_path_buf())))
}

/// Con qué se pinta un documento.
pub fn paint_of(doc: &Doc, game: &mut Game) -> Result<Paint> {
    Ok(match &doc.model {
        Some(m) => m.paint(),
        None => Paint::new(game.catalog()?, doc.s.theme, None),
    })
}

fn hex(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Resumen de un documento: lo que `info` imprime y lo que `run` adjunta.
pub fn describe(doc: &Doc, game: &mut Game) -> Result<Value> {
    let g = &doc.s.grid;
    let counts = g.counts();
    let solids: u32 = counts[1..].iter().sum();
    let mut out = json!({
        "kind": if doc.is_model() { "model" } else { "structure" },
        "name": doc.meta.name,
        "size": g.size,
        "solidBox": g.solid_box().map(|(lo, hi)| [lo, hi]),
        "solids": solids,
        "fingerprint": doc.fingerprint(),
    });
    if let Some(p) = &doc.path {
        out["file"] = json!(p);
    }
    let used = counts.iter().enumerate().skip(1).filter(|(_, n)| **n > 0);
    match &doc.model {
        Some(m) => {
            out["cell"] = json!(m.cell);
            out["origin"] = json!(m.origin);
            out["note"] = json!(m.note());
            out["editedByHand"] = json!(m.edited());
            out["shadedFaces"] = json!(doc.s.face_colors.len());
            out["colors"] = used.map(|(b, n)| json!({ "color": m.palette.get(b - 1).copied().map(hex), "voxels": n })).collect();
            out["checks"] = json!(vmodel::checks(&doc.s, None, false));
        }
        None => {
            let names = game.catalog()?.materials.clone();
            out["materials"] = used
                .map(|(b, n)| {
                    let name = names.get(b).cloned().unwrap_or_default();
                    json!({ "material": name, "label": labels::material_label(&name), "subcubes": n })
                })
                .collect();
            out["style"] = json!(doc.meta.style);
            out["id"] = json!(doc.meta.id);
            out["groundLevel"] = json!(doc.s.ground_level);
            out["bands"] = doc.s.band_starts.iter().zip(&doc.s.band_floors).map(|(a, b)| json!([a, b])).collect();
            out["entranceX"] = json!(doc.s.entrance[0]);
            out["parts"] = json!(doc.s.parts.len());
            out["interactables"] = json!(doc.s.interactables.len());
            let checked = game.client()?.validate(&doc.s, doc.meta.replaces.as_ref())?;
            out["checks"] = json!(checked.checks);
        }
    }
    Ok(out)
}

fn info(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let doc = open_doc(&args.file()?, &mut game)?;
    describe(&doc, &mut game)
}

fn layers(list: &str) -> Result<Layers> {
    let mut l = Layers::none();
    for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match name {
            "rejilla" => l.grid = true,
            "nav" => l.nav = true,
            "partes" => l.parts = true,
            "interactuables" => l.interactables = true,
            "entrada" => l.entrance = true,
            "huella" => l.footprint = true,
            "detalle" => l.detail = true,
            "plantas" => l.bands = true,
            _ => bail!("no conozco la capa «{name}»"),
        }
    }
    Ok(l)
}

/// Opciones de la lámina a partir de los argumentos (también las usa `run --render`).
pub fn sheet_options(args: &Args, doc: &Doc) -> Result<sheet::Options> {
    let mut o = sheet::Options::default();
    if let Some(list) = args.get("vista") {
        o.views = if list == "todas" {
            ViewKind::ALL.to_vec()
        } else {
            list.split(',')
                .map(|n| ViewKind::parse(n.trim()).with_context(|| format!("no conozco la vista «{n}»")))
                .collect::<Result<_>>()?
        };
        ensure!(!o.views.is_empty(), "--vista necesita al menos una vista");
    }
    if let Some(t) = args.get("tam") {
        let (w, h) = t.split_once('x').unwrap_or((t, ""));
        let w: u32 = w.parse().context("--tam: el ancho no es un número")?;
        let h: u32 = if h.is_empty() { w * 3 / 4 } else { h.parse().context("--tam: el alto no es un número")? };
        ensure!((200..=4096).contains(&w) && (200..=4096).contains(&h), "--tam: entre 200 y 4096 píxeles");
        o.size = [w, h];
    }
    o.region = args.ints("caja")?;
    o.mark = args.ints("marca")?;
    for (key, b) in [("caja", o.region), ("marca", o.mark)] {
        if let Some(b) = b {
            ensure!((0..3).all(|k| b[k] < b[k + 3]), "--{key}: la caja va de la esquina menor a la mayor (x0,y0,z0,x1,y1,z1)");
        }
    }
    if let Some(n) = args.get("planta") {
        let n: usize = n.parse().context("--planta: no es un número")?;
        let floor = doc.s.band_floors.get(n).with_context(|| format!("no hay planta {n}: hay {}", doc.s.band_floors.len()))?;
        o.cut_level = Some(*floor);
    }
    if let Some(list) = args.get("capas") {
        o.layers = layers(list)?;
    }
    Ok(o)
}

/// Dibuja la lámina de `doc` y la guarda en `out` (o en la carpeta temporal). Devuelve dónde.
pub fn render_doc(doc: &Doc, game: &mut Game, o: &sheet::Options, out: Option<&str>) -> Result<PathBuf> {
    let paint = paint_of(doc, game)?;
    // La navegación y la huella no van en el fichero: las da el generador.
    let mut s = std::borrow::Cow::Borrowed(&doc.s);
    if !doc.is_model() && (o.layers.nav || o.layers.footprint) {
        let checked = game.client()?.validate(&doc.s, doc.meta.replaces.as_ref())?;
        let s = s.to_mut();
        (s.nav, s.footprint) = (checked.nav, checked.footprint);
    }
    render::set_agame_root(&game.root);
    let rs = render::headless_render_state().context("no hay tarjeta gráfica con la que dibujar")?;
    let img = sheet::render(&rs, &s, &paint, o);
    let path = match out {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join("cantero").join(format!("{}.png", crate::editor::slug(&doc.meta.name))),
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    img.save(&path).with_context(|| format!("no puedo escribir {}", path.display()))?;
    Ok(path)
}

fn render_cmd(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let doc = open_doc(&args.file()?, &mut game)?;
    let o = sheet_options(args, &doc)?;
    let path = render_doc(&doc, &mut game, &o, args.get("out"))?;
    Ok(json!({
        "image": path,
        "views": o.views.iter().map(|v| v.name()).collect::<Vec<_>>(),
        "size": doc.s.grid.size,
    }))
}

/// `cantero vista`: el modelo a su tamaño real en el juego y con su luz lisa (`gameview`), sin tarjeta gráfica.
fn game_view_cmd(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let doc = open_doc(&args.file()?, &mut game)?;
    ensure!(doc.is_model(), "la vista de juego es para modelos; para una estructura usa `render`");
    let turns: Vec<u8> = match args.get("giro") {
        None | Some("todos") => vec![0, 1, 2, 3],
        Some(n) => vec![n.parse::<u8>().ok().filter(|n| *n < 4).context("--giro va de 0 a 3, o `todos`")?],
    };
    let zoom = match args.get("zoom") {
        Some(z) => z.parse::<u32>().ok().filter(|z| (1..=32).contains(z)).context("--zoom va de 1 a 32")?,
        None => 8,
    };
    let img = gameview::sheet(&doc.s, &paint_of(&doc, &mut game)?, &turns, zoom, !args.has("sin-contorno"));
    let path = match args.get("out") {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join("cantero").join(format!("{}_juego.png", crate::editor::slug(&doc.meta.name))),
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    img.save(&path).with_context(|| format!("no puedo escribir {}", path.display()))?;
    Ok(json!({ "image": path, "turns": turns, "zoom": zoom, "pixels": [img.width() / zoom, img.height() / zoom] }))
}

/// Letras de las rebanadas, por orden de aparición; lo que no cabe sale como `#`.
const SYMBOLS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// Tope de una rebanada (ancho y alto) y de cuántas se imprimen: más no se lee.
const SLICE_SIDE: i32 = 96;
const SLICE_COUNT: i32 = 16;

/// Rebanadas en texto de la caja `b` de `doc`, cortando por el eje `axis` (0 = x, 1 = y, 2 = z).
pub fn slices(doc: &Doc, game: &mut Game, b: [i32; 6], axis: usize) -> Result<String> {
    let g = &doc.s.grid;
    ensure!((0..3).all(|k| b[k] < b[k + 3]), "--caja: va de la esquina menor a la mayor (x0,y0,z0,x1,y1,z1)");
    ensure!((0..3).all(|k| b[k] >= 0 && b[k + 3] <= g.size[k]), "--caja se sale de la rejilla, que mide {:?}", g.size);
    // Columnas y filas de cada rebanada: en planta, x y z; en los alzados, la horizontal que queda e y.
    let (cols, rows) = match axis {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };
    let len = |k: usize| b[k + 3] - b[k];
    ensure!(
        len(axis) <= SLICE_COUNT && len(cols) <= SLICE_SIDE && len(rows) <= SLICE_SIDE,
        "la caja es demasiado grande para leerla en texto: como mucho {SLICE_COUNT} rebanadas de {SLICE_SIDE}×{SLICE_SIDE}. Acótala o usa `cantero render --caja`"
    );
    let mut seen: Vec<u8> = Vec::new();
    let mut body = String::new();
    let names = ["x", "y", "z"];
    for a in b[axis]..b[axis + 3] {
        body.push_str(&format!("\n{} = {a}   ({} →, {} ↑)\n", names[axis], names[cols], names[rows]));
        for r in (b[rows]..b[rows + 3]).rev() {
            body.push_str(&format!("{r:>4} "));
            for c in b[cols]..b[cols + 3] {
                let mut v = [0; 3];
                (v[axis], v[cols], v[rows]) = (a, c, r);
                let m = g.get(v[0], v[1], v[2]);
                body.push(if m == 0 {
                    '.'
                } else {
                    let i = seen.iter().position(|&s| s == m).unwrap_or_else(|| {
                        seen.push(m);
                        seen.len() - 1
                    });
                    SYMBOLS.get(i).map_or('#', |&s| s as char)
                });
            }
            body.push('\n');
        }
        // Regla de las columnas: decenas y unidades.
        for digit in [10, 1] {
            body.push_str("     ");
            body.extend((b[cols]..b[cols + 3]).map(|c| if digit == 10 && c % 10 != 0 && c != b[cols] { ' ' } else { char::from(b'0' + (c / digit % 10) as u8) }));
            body.push('\n');
        }
    }
    let counts = g.counts();
    let mut out = format!("{} · caja {b:?} · «.» = aire\n", doc.meta.name);
    for (i, &m) in seen.iter().enumerate() {
        let name = match &doc.model {
            Some(model) => model.palette.get(m as usize - 1).copied().map(hex).unwrap_or_default(),
            None => {
                let name = game.catalog()?.materials.get(m as usize).cloned().unwrap_or_default();
                format!("{name} ({})", labels::material_label(&name))
            }
        };
        out.push_str(&format!("  {} = {name} · {} en todo el documento\n", SYMBOLS.get(i).map_or('#', |&s| s as char), counts[m as usize]));
    }
    Ok(out + body.as_str())
}

fn slices_cmd(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let doc = open_doc(&args.file()?, &mut game)?;
    let b = args.ints("caja")?.context("capas necesita --caja x0,y0,z0,x1,y1,z1: una región acotada")?;
    let axis = match args.get("eje").unwrap_or("y") {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        other => bail!("--eje es x, y o z, no «{other}»"),
    };
    Ok(Value::String(slices(&doc, &mut game, b, axis)?))
}

fn run_cmd(args: &Args) -> Result<Value> {
    let recipe = args.file().context("falta la receta")?;
    let opts = crate::recipe::Options { out_dir: args.get("salida").map(PathBuf::from), force: args.has("forzar") };
    let mut out = crate::recipe::run(&recipe, Game::from_args(args), opts)?;
    if args.has("render") {
        let o = sheet_options(args, &out.doc)?;
        out.summary["image"] = json!(render_doc(&out.doc, &mut out.game, &o, args.get("out"))?);
    }
    Ok(out.summary)
}

/// Da número a las estructuras de `dir` que aún no lo tienen (el juego las identifica por él). Devuelve cuáles.
pub fn assign_ids(dir: &Path, materials: &[String]) -> Result<Vec<(String, u32)>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("no hay estructuras que exportar: falta {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "agvox"))
        .collect();
    files.sort();
    let mut given = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        // Una fuente rota la explica el generador al exportar, con su nombre.
        let Ok((mut meta, s)) = agvox::load(&text, materials) else { continue };
        if meta.id.is_none() {
            let id = crate::editor::next_id(dir, materials);
            meta.id = Some(id);
            crate::editor::write_atomic(&file, &agvox::save(&meta, &s, materials)?)?;
            given.push((file.file_name().unwrap_or_default().to_string_lossy().into_owned(), id));
        }
    }
    Ok(given)
}

fn export_cmd(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let dir = crate::editor::source_dir(&game.root);
    let materials = game.catalog()?.materials.clone();
    let given = assign_ids(&dir, &materials)?;
    let h = game.client()?.ask("export", json!({ "sourceDir": dir }), &[])?.header;
    let exported = h["exported"].as_bool() == Some(true);
    let mut out = json!({
        "exported": exported,
        "outDir": h["outDir"],
        "written": h["written"],
        "unchanged": h["unchanged"],
        "removed": h["removed"],
        "sources": h["sources"],
        "newIds": given.iter().map(|(file, id)| json!({ "file": file, "id": id })).collect::<Vec<_>>(),
    });
    if exported {
        // El generador se compila con el juego: hay que rehacerlo para que vea las estructuras nuevas.
        let paths = game.client()?.paths.clone();
        game.restart();
        let built = crate::bridge::compile(&paths);
        out["generator"] = json!(if built.ok { "recompilado: ya ve las estructuras exportadas" } else { "no compila con lo exportado: revisa el juego" });
    } else {
        out["hint"] = json!("alguna estructura tiene errores (mira sources[].checks): no se ha escrito nada");
    }
    Ok(out)
}

fn catalog_cmd(args: &Args) -> Result<Value> {
    let mut game = Game::from_args(args);
    let section = args.pos.get(1).map(String::as_str);
    Ok(match section {
        Some("materiales") => {
            let cat = game.catalog()?;
            let list: Vec<Value> = cat
                .materials
                .iter()
                .zip(&cat.palette)
                .skip(1)
                .map(|(name, c)| json!({ "material": name, "label": labels::material_label(name), "color": hex(*c) }))
                .collect();
            json!({ "materials": list })
        }
        Some("herramientas") => json!({ "tools": game.client()?.ask("tools", json!({}), &[])?.header["tools"] }),
        Some("muebles") => {
            let h = game.client()?.ask("furniture", json!({}), &[])?.header;
            json!({ "categories": h["categories"], "pieces": h["pieces"] })
        }
        Some("estilos") => {
            let cat = game.catalog()?;
            let names = |list: &[crate::bridge::proto::Named]| list.iter().map(|n| json!({ "name": n.name, "label": n.label })).collect::<Vec<_>>();
            let mut out = json!({ "styles": names(&cat.styles), "districts": names(&cat.districts), "tiers": names(&cat.tiers) });
            // Los papeles de material de las herramientas, con lo que vale cada uno en el primer aspecto.
            if let (Some(st), Some(d), Some(t)) = (cat.styles.first(), cat.districts.first(), cat.tiers.first()) {
                let (st, d, t) = (st.name.clone(), d.name.clone(), t.name.clone());
                out["roles"] = json!({ "of": format!("look(\"{d}\", \"{t}\") con el estilo {st}"), "materials": game.client()?.look(&st, &d, &t)? });
            }
            out
        }
        Some(other) => bail!("no conozco «{other}»: materiales, herramientas, muebles o estilos"),
        None => {
            let cat = game.catalog()?;
            json!({
                "materials": cat.materials.len() - 1,
                "styles": cat.styles.iter().map(|s| &s.name).collect::<Vec<_>>(),
                "hint": "cantero catalog materiales | herramientas | muebles | estilos",
            })
        }
    })
}

fn run(args: &[String]) -> Result<Value> {
    let args = Args::parse(args)?;
    match args.pos.first().map(String::as_str) {
        _ if args.has("ayuda") => Ok(Value::String(HELP.into())),
        Some("info") => info(&args),
        Some("render") => render_cmd(&args),
        Some("capas") => slices_cmd(&args),
        Some("vista") => game_view_cmd(&args),
        Some("run") => run_cmd(&args),
        Some("export") => export_cmd(&args),
        Some("api") => Ok(Value::String(crate::recipe::reference())),
        Some("catalog") => catalog_cmd(&args),
        Some(other) => bail!("no conozco «{other}». {}", "Prueba: cantero --ayuda"),
        None => Ok(Value::String(HELP.into())),
    }
}

/// JSON con una clave por línea y lo de dentro en una sola: corto de leer para quien lo pide.
fn compact(v: &Value) -> String {
    let Value::Object(map) = v else { return v.to_string() };
    // Una lista larga de objetos (materiales, muebles, avisos), uno por línea.
    let value = |v: &Value| match v.as_array().filter(|a| a.len() > 3 && a.iter().all(Value::is_object)) {
        Some(items) => format!("[\n{}\n  ]", items.iter().map(|i| format!("    {i}")).collect::<Vec<_>>().join(",\n")),
        None => v.to_string(),
    };
    let lines: Vec<String> = map.iter().map(|(k, v)| format!("  {}: {}", Value::String(k.clone()), value(v))).collect();
    format!("{{\n{}\n}}", lines.join(",\n"))
}

/// Punto de entrada del binario: imprime el resultado y devuelve el código de salida.
pub fn main(args: Vec<String>) -> i32 {
    match run(&args) {
        Ok(Value::String(text)) => {
            println!("{text}");
            0
        }
        Ok(v) => {
            println!("{}", compact(&v));
            0
        }
        Err(e) => {
            eprintln!("cantero: {e:#}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::parse(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn argumentos() {
        let a = args(&["render", "x.json", "--caja", "0,1,2,3,4,5", "--recompilar", "--tam", "800"]);
        assert_eq!(a.pos, ["render", "x.json"]);
        assert_eq!(a.ints::<6>("caja").unwrap(), Some([0, 1, 2, 3, 4, 5]));
        assert!(a.has("recompilar") && !a.has("forzar"));
        assert!(a.ints::<3>("caja").is_err());
        assert!(Args::parse(&["--tam".to_string()]).is_err());
    }

    #[test]
    fn exportar_da_numero_a_las_que_no_tienen() {
        let dir = std::env::temp_dir().join(format!("cantero-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mats: Vec<String> = vec!["Air".into(), "Stone".into()];
        let s = crate::model::Structure { grid: crate::model::VoxelGrid::new([8, 8, 8]), ..Default::default() };
        let write = |name: &str, id: Option<u32>| {
            let meta = agvox::Meta { id, name: name.into(), ..Default::default() };
            std::fs::write(dir.join(format!("{name}.agvox")), agvox::save(&meta, &s, &mats).unwrap()).unwrap();
        };
        write("a", Some(4));
        write("b", None);
        write("c", None);
        std::fs::write(dir.join("rota.agvox"), "no es json").unwrap();
        assert_eq!(assign_ids(&dir, &mats).unwrap(), [("b.agvox".to_string(), 5), ("c.agvox".to_string(), 6)]);
        // Los números se quedan en el fichero, sin tocar lo demás; la segunda vez no hay nada que dar.
        let (meta, again) = agvox::load(&std::fs::read_to_string(dir.join("b.agvox")).unwrap(), &mats).unwrap();
        assert_eq!((meta.id, meta.name.as_str(), again.grid == s.grid), (Some(5), "b", true));
        assert!(assign_ids(&dir, &mats).unwrap().is_empty());
        assert!(assign_ids(&dir.join("no-existe"), &mats).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn info_de_un_modelo_sin_generador() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/models/chest_0_model.json");
        let mut game = Game::new(PathBuf::from("no-existe"), false);
        let doc = open_doc(&path, &mut game).unwrap();
        let v = describe(&doc, &mut game).unwrap();
        assert_eq!(v["kind"], "model");
        assert_eq!(v["name"], "chest_0");
        assert_eq!(v["fingerprint"].as_str().unwrap().len(), 16);
        let total: u64 = v["colors"].as_array().unwrap().iter().map(|c| c["voxels"].as_u64().unwrap()).sum();
        assert_eq!(total, v["solids"].as_u64().unwrap());
    }

    #[test]
    fn opciones_de_la_lamina() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/models/chest_0_model.json");
        let doc = open_doc(&path, &mut Game::new(PathBuf::new(), false)).unwrap();
        let o = sheet_options(&args(&["render", "x", "--vista", "frente,planta", "--tam", "800", "--capas", "rejilla"]), &doc).unwrap();
        assert_eq!(o.views, [ViewKind::Front, ViewKind::Top]);
        assert_eq!(o.size, [800, 600]);
        assert!(o.layers.grid && !o.layers.nav);
        assert!(sheet_options(&args(&["render", "x", "--vista", "arriba"]), &doc).is_err());
        assert!(sheet_options(&args(&["render", "x", "--caja", "4,0,0,2,3,3"]), &doc).is_err());
        assert!(sheet_options(&args(&["render", "x", "--planta", "0"]), &doc).is_err(), "un modelo no tiene plantas");
    }
}
