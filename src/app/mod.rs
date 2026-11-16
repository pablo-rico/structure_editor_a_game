//! Ventana principal: estado, disposición acoplable por modo (egui_dock), barra superior, barra de estado, primer
//! arranque, búsqueda y atajos globales (docs/diseno-gui.md §5 y §8). Los visores van en `view`, el modo Editar en
//! `edit`.

mod edit;
mod view;

pub(crate) use view::floor_name;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, TextureHandle, Ui, WidgetText};
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};

use crate::bridge::Paths;
use crate::bridge::proto::{BuildRequest, Catalog, CheckAt, ErrorInfo};
use crate::editor::Editor;
use crate::host::{Host, Purpose, Reply, Status};
use crate::mesh::Paint;
use crate::model::{SUB, Structure};
use crate::params::Params;
use crate::render::{self, ViewState, Viewport};
use crate::theme;
use crate::ui::title_font::TitleFont;
use crate::ui::widgets::{self, ToastKind, Toasts, dim};
use crate::ui::{catalog, console, inspector};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Catalog,
    View,
    Sheet,
    Compare,
    Options,
    Inspector,
    Console,
    Palette,
    Properties,
    Validation,
    History,
    Library,
}

impl Tab {
    fn title(self) -> String {
        let (glyph, text) = match self {
            Tab::Catalog => (icon::BUILDINGS, "Catálogo"),
            Tab::View => (icon::CUBE, "Vista"),
            Tab::Sheet => (icon::GRID_FOUR, "Muestrario"),
            Tab::Compare => (icon::COLUMNS, "Comparar"),
            Tab::Options => (icon::SLIDERS_HORIZONTAL, "Opciones"),
            Tab::Inspector => (icon::CROSSHAIR, "Bajo el cursor"),
            Tab::Console => (icon::TERMINAL_WINDOW, "Consola"),
            Tab::Palette => (icon::PALETTE, "Paleta"),
            Tab::Properties => (icon::SLIDERS_HORIZONTAL, "Propiedades"),
            Tab::Validation => (icon::CHECK_CIRCLE, "Validación"),
            Tab::History => (icon::CLOCK_COUNTER_CLOCKWISE, "Historial"),
            Tab::Library => (icon::BOOKS, "Biblioteca"),
        };
        format!("{glyph}  {text}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Explore,
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input {
    Mouse,
    Trackpad,
}

/// Lo que se recuerda entre sesiones.
#[derive(Default, Serialize, Deserialize)]
struct Settings {
    agame_root: Option<PathBuf>,
    input: Option<Input>,
    hints_done: bool,
    kind: String,
    params: HashMap<String, Params>,
    /// Ficheros abiertos o guardados hace poco (Biblioteca › Recientes).
    #[serde(default)]
    recent: Vec<PathBuf>,
    /// Asociación color → material de cada `.vox` importado (por nombre de fichero y de material).
    #[serde(default)]
    vox_maps: HashMap<String, std::collections::BTreeMap<u8, Option<String>>>,
}

/// Una estructura pedida al generador y lo que respondió (se conserva la última buena mientras llega otra).
#[derive(Default)]
struct Slot {
    req: Option<BuildRequest>,
    s: Option<Arc<Structure>>,
    error: Option<ErrorInfo>,
    loading: bool,
}

impl Slot {
    fn ask(&mut self, host: &mut Host, purpose: Purpose, req: BuildRequest) {
        self.loading = true;
        self.req = Some(req.clone());
        host.build(purpose, req);
    }

    fn take(&mut self, req: BuildRequest, result: Result<Arc<Structure>, ErrorInfo>) {
        self.loading = false;
        self.req = Some(req);
        match result {
            Ok(s) => {
                self.s = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

/// Qué varía de una fila a otra del muestrario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SheetRows {
    Nothing,
    Style,
    Damage,
    Design,
}

struct Sheet {
    n: usize,
    rows: SheetRows,
    base_seed: i32,
    cells: Vec<Slot>,
    images: Vec<Option<TextureHandle>>,
    /// Firma de lo pedido para no repetir peticiones.
    asked: String,
}

impl Default for Sheet {
    fn default() -> Self {
        Self { n: 3, rows: SheetRows::Nothing, base_seed: 0, cells: Vec::new(), images: Vec::new(), asked: String::new() }
    }
}

/// Visores 3D (necesitan la GPU de eframe).
struct Viewports {
    rs: egui_wgpu::RenderState,
    main: Viewport,
    edit: Viewport,
    compare: [Viewport; 2],
}

pub struct App {
    docks: [DockState<Tab>; 2],
    st: State,
    capture: Option<Capture>,
}

/// Captura automática para revisar la interfaz sin permiso de grabación de pantalla:
/// `EDITOR_CAPTURA=ruta.png` (y opcionalmente `EDITOR_TIPO=Shop`, `EDITOR_MODO=editar`) guarda la ventana cuando ya se
/// ve la estructura y cierra la app.
struct Capture {
    path: PathBuf,
    kind: Option<String>,
    edit: bool,
    /// Ajustes de Editar para la captura: `EDITOR_BIBLIOTECA` (estructuras|juego|piezas), `EDITOR_HERRAMIENTA`
    /// (id de construcción, p. ej. `room`) y `EDITOR_VOX` (ruta de un .vox para el diálogo de importar).
    setup: Option<(Option<String>, Option<String>, Option<PathBuf>)>,
    asked: bool,
    ready_at: Option<std::time::Instant>,
}

struct State {
    mode: Mode,
    settings: Settings,
    host: Host,
    catalog: Option<Arc<Catalog>>,
    params: Params,
    filter: String,
    main: Slot,
    /// Tipo encuadrado por última vez (al cambiar de tipo se vuelve a encuadrar).
    framed: String,
    view: ViewState,
    compare: [Slot; 2],
    compare_views: [ViewState; 2],
    linked: bool,
    sheet: Sheet,
    thumbs: HashMap<String, TextureHandle>,
    thumbs_generation: u64,
    viewports: Option<Viewports>,
    hover: Option<[i32; 3]>,
    pinned: bool,
    toasts: Toasts,
    focus_mode: bool,
    show_help: bool,
    command: Option<String>,
    /// Pestaña a abrir (o enfocar) en el modo activo: menú Ver, chip de recarga.
    open_tab: Option<Tab>,
    reset_layout: bool,
    /// Títulos con `font_title` del juego (marca y bienvenida); `None` dentro si no se pudo leer.
    titles: Option<Option<[TextureHandle; 2]>>,
    editor: Editor,
    edit_view: ViewState,
    /// Plano bloqueado de edición (altura en sub-cubos) y si está activo.
    plane: (i32, bool),
    /// Lo que hay que preguntar antes de seguir (descartar cambios sin guardar).
    confirm: Option<edit::Confirm>,
    /// Puntero del modo Editar ya resuelto a sub-cubos (del fotograma anterior) y dónde caería un sub-cubo nuevo.
    edit_pointer: crate::edit::tools::Pointer,
    edit_place: Option<[i32; 3]>,
    /// Tema subido al visor de Editar (remallar solo si cambia).
    edit_theme: Option<usize>,
    /// Rect del visor dibujado en este fotograma (los toasts se anclan a él).
    view_rect: Option<egui::Rect>,
    /// Editar de la fase 3: construcción, piezas, biblioteca, Aspecto.
    ed: edit::EditUi,
}

fn default_dock() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::View, Tab::Sheet, Tab::Compare]);
    let s = d.main_surface_mut();
    let [center, _] = s.split_left(NodeIndex::root(), 0.18, vec![Tab::Catalog]);
    let [_, right] = s.split_right(center, 0.75, vec![Tab::Options]);
    s.split_below(right, 0.64, vec![Tab::Inspector]);
    d
}

fn default_edit_dock() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::View]);
    let s = d.main_surface_mut();
    let [center, _] = s.split_left(NodeIndex::root(), 0.2, vec![Tab::Palette, Tab::Library]);
    let [center, _] = s.split_right(center, 0.74, vec![Tab::Properties]);
    s.split_below(center, 0.8, vec![Tab::Validation, Tab::History, Tab::Console]);
    d
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        // Cmd± y Cmd+0 son del visor (zoom entero), no de la escala de la interfaz.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, "settings")).unwrap_or_default();
        let docks = [
            cc.storage.and_then(|s| eframe::get_value(s, "dock")).unwrap_or_else(default_dock),
            // `_v3`: la disposición de Editar cambió en la fase 3 (Biblioteca); la vieja no se reutiliza.
            cc.storage.and_then(|s| eframe::get_value(s, "dock_edit_v3")).unwrap_or_else(default_edit_dock),
        ];
        let root = settings.agame_root.clone().unwrap_or_else(Paths::default_agame_root);
        render::set_agame_root(&root);
        let host = Host::new(cc.egui_ctx.clone(), root);
        let viewports = cc.wgpu_render_state.clone().map(|rs| Viewports {
            main: Viewport::new(&rs),
            edit: Viewport::new(&rs),
            compare: [Viewport::new(&rs), Viewport::new(&rs)],
            rs,
        });
        let edit_view = ViewState {
            layers: render::Layers { interactables: true, ..Default::default() },
            ..Default::default()
        };
        let st = State {
            mode: Mode::Explore,
            settings,
            host,
            catalog: None,
            params: Params::placeholder(),
            filter: String::new(),
            main: Slot::default(),
            framed: String::new(),
            view: ViewState::default(),
            compare: Default::default(),
            compare_views: Default::default(),
            linked: true,
            sheet: Sheet::default(),
            thumbs: HashMap::new(),
            thumbs_generation: 0,
            viewports,
            hover: None,
            pinned: false,
            toasts: Toasts::default(),
            focus_mode: false,
            show_help: false,
            command: None,
            open_tab: None,
            reset_layout: false,
            titles: None,
            editor: Editor::default(),
            edit_view,
            plane: (8, false),
            confirm: None,
            edit_pointer: Default::default(),
            edit_place: None,
            edit_theme: None,
            view_rect: None,
            ed: edit::EditUi::default(),
        };
        let capture = std::env::var_os("EDITOR_CAPTURA").map(|p| Capture {
            path: p.into(),
            kind: std::env::var("EDITOR_TIPO").ok(),
            edit: std::env::var("EDITOR_MODO").is_ok_and(|m| m == "editar"),
            setup: Some((std::env::var("EDITOR_BIBLIOTECA").ok(), std::env::var("EDITOR_HERRAMIENTA").ok(), std::env::var_os("EDITOR_VOX").map(PathBuf::from))),
            asked: false,
            ready_at: None,
        });
        Self { docks, st, capture }
    }

    fn tick_capture(&mut self, ctx: &egui::Context) {
        let Some(c) = self.capture.as_mut() else { return };
        if let Some(kind) = c.kind.take_if(|_| self.st.catalog.is_some()) {
            self.st.select_kind(&kind);
        }
        let shown = self.st.main.s.is_some() && !self.st.main.loading && c.kind.is_none();
        if shown && c.edit && self.st.mode == Mode::Explore {
            self.st.fork_main();
        }
        if shown
            && c.edit
            && self.st.mode == Mode::Edit
            && !self.st.ed.tools_cat.is_empty()
            && let Some((lib, tool, vox)) = c.setup.take()
        {
            self.st.capture_setup(lib, tool, vox);
        }
        if shown && c.ready_at.is_none() {
            c.ready_at = Some(std::time::Instant::now());
        }
        // Un respiro para que lleguen miniaturas y se asiente la disposición.
        if !c.asked && c.ready_at.is_some_and(|t| t.elapsed().as_secs_f32() > 3.0) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            c.asked = true;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let _ = save_png(&img, &c.path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn save_png(img: &egui::ColorImage, path: &std::path::Path) -> anyhow::Result<()> {
    let [w, h] = img.size;
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or_else(|| anyhow::anyhow!("imagen vacía"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    buf.save(path)?;
    Ok(())
}

impl State {
    fn rebuild_main(&mut self) {
        if self.catalog.is_some() {
            let mut req = self.params.request();
            req.procedural = self.ed.explore_original;
            self.main.ask(&mut self.host, Purpose::Main, req);
        }
    }

    /// Cambia de tipo recordando los parámetros de cada uno.
    fn select_kind(&mut self, name: &str) {
        let Some(cat) = self.catalog.clone() else { return };
        let Some(kind) = cat.kind(name) else { return };
        self.settings.params.insert(self.params.kind.clone(), self.params.clone());
        let mut p = self.settings.params.get(name).cloned().unwrap_or_else(|| Params::defaults(kind, &cat));
        p.fit(kind, &cat);
        self.params = p;
        self.settings.kind = name.into();
        self.hover = None;
        self.pinned = false;
        self.view.cut_level = None;
        self.ed.explore_original = false;
        self.rebuild_main();
    }

    fn on_catalog(&mut self, cat: Arc<Catalog>) {
        self.catalog = Some(cat.clone());
        let wanted = if cat.kind(&self.settings.kind).is_some() { self.settings.kind.clone() } else { "Home".into() };
        let name =
            if cat.kind(&wanted).is_some() { wanted } else { cat.kinds.first().map(|k| k.name.clone()).unwrap_or_default() };
        if let Some(kind) = cat.kind(&name) {
            let mut p = self.settings.params.get(&name).cloned().unwrap_or_else(|| Params::defaults(kind, &cat));
            p.fit(kind, &cat);
            self.params = p;
            self.settings.kind = name;
        }
        self.rebuild_main();
        self.request_thumbs();
        self.sheet.asked.clear();
    }

    /// Miniatura de cada tipo: primer estilo, valores por defecto, semilla 0 (§6).
    fn request_thumbs(&mut self) {
        let Some(cat) = self.catalog.clone() else { return };
        self.thumbs_generation = self.host.generation;
        for k in &cat.kinds {
            let p = Params::defaults(k, &cat);
            self.host.build(Purpose::Thumb(k.name.clone()), p.request());
        }
    }

    fn paint(&self, s: &Structure) -> Option<Paint> {
        Some(Paint::new(self.catalog.as_ref()?, s.theme, None))
    }

    /// Imagen fuera de pantalla de una estructura (miniaturas y muestrario), sin capas.
    fn picture(&self, ctx: &egui::Context, name: &str, s: &Structure, size: [u32; 2]) -> Option<TextureHandle> {
        let vp = self.viewports.as_ref()?;
        let paint = self.paint(s)?;
        let view = ViewState { layers: render::Layers::none(), ..Default::default() };
        let img = render::render_image(&vp.rs, s, &paint, &view, size);
        Some(ctx.load_texture(name, img, egui::TextureOptions::LINEAR))
    }

    fn handle(&mut self, ctx: &egui::Context, replies: Vec<Reply>) {
        for r in replies {
            match r {
                Reply::Catalog(cat) => self.on_catalog(cat),
                Reply::Reloaded { quiet } => {
                    if !quiet {
                        self.toasts.push(ToastKind::Info, "El juego ha cambiado: ya ves la versión nueva.", None);
                    }
                }
                Reply::Built(purpose, req, result) => self.on_built(ctx, purpose, req, result),
                Reply::Validated(revision, result) => match result {
                    Ok(s) => self.editor.on_validated(revision, s),
                    Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido validar: {}", e.message), None),
                },
                Reply::Tools(t) => self.ed.tools_cat = t,
                Reply::Furniture(c) => self.ed.furniture = Some(c),
                Reply::Look(roles) => self.on_look(roles),
                Reply::Tool(seq, result) => self.on_tool(seq, result),
                Reply::Stamp(key, result) => self.on_stamp(ctx, key, result),
                Reply::Exported(result) => {
                    self.editor.exporting = false;
                    match result {
                        Ok(h) if h["exported"].as_bool() == Some(true) => {
                            let n = h["written"].as_array().map_or(0, Vec::len);
                            self.toasts.push(ToastKind::Ok, format!("Exportado: la estructura ya está en el juego ({n} ficheros de código)."), None);
                            // El catálogo («Hechas a mano») sale del código del juego: hay que recompilar el generador.
                            self.host.recompile();
                        }
                        Ok(h) => {
                            // Alguna fuente de ArtSource/Structures tiene errores: no se ha escrito nada.
                            let first = h["sources"].as_array().into_iter().flatten().find_map(|src| {
                                let check = src["checks"].as_array()?.iter().find(|c| c["severity"] == "error")?;
                                Some(format!("{}: {}", src["name"].as_str().unwrap_or("?"), check["message"].as_str().unwrap_or("")))
                            });
                            let why = first.unwrap_or_else(|| "hay errores en las estructuras a mano".into());
                            self.toasts.push(ToastKind::Error, format!("No se ha exportado nada. {why}"), None);
                        }
                        Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha exportado: {}", e.message), None),
                    }
                }
            }
        }
        // Un generador nuevo deja viejo lo que se ve: se vuelve a pedir.
        if self.catalog.is_some() && self.thumbs_generation != self.host.generation && self.host.status == Status::Ready {
            self.sheet.asked.clear();
            self.request_thumbs();
            for i in 0..2 {
                if let Some(req) = self.compare[i].req.clone() {
                    self.compare[i].ask(&mut self.host, Purpose::Compare(i), req);
                }
            }
        }
    }

    fn on_built(&mut self, ctx: &egui::Context, purpose: Purpose, req: BuildRequest, result: Result<Arc<Structure>, ErrorInfo>) {
        match purpose {
            Purpose::Main => {
                if let Err(e) = &result {
                    self.toasts.push(ToastKind::Error, format!("{} (semilla {})", e.message, req.seed), None);
                }
                self.main.take(req, result);
                if let (Some(s), Some(paint)) = (self.main.s.clone(), self.main.s.as_deref().and_then(|s| self.paint(s))) {
                    if let Some(vp) = self.viewports.as_mut() {
                        vp.main.set_structure(s.clone(), &paint);
                    }
                    if self.framed != self.params.kind {
                        self.framed = self.params.kind.clone();
                        self.view.frame(&s);
                    }
                }
            }
            Purpose::Compare(i) => {
                self.compare[i].take(req, result);
                if let (Some(s), Some(vp)) = (self.compare[i].s.clone(), self.viewports.as_ref()) {
                    let _ = vp;
                    if let Some(paint) = self.paint(&s) {
                        let vp = self.viewports.as_mut().unwrap();
                        vp.compare[i].set_structure(s.clone(), &paint);
                        self.compare_views[i].frame(&s);
                    }
                }
            }
            Purpose::Sheet(i) => {
                if i < self.sheet.cells.len() {
                    self.sheet.cells[i].take(req, result);
                    let s = self.sheet.cells[i].s.clone();
                    let img = s.and_then(|s| self.picture(ctx, &format!("hoja{i}"), &s, [360, 300]));
                    self.sheet.images[i] = img;
                }
            }
            Purpose::Library => {
                if let Ok(s) = result {
                    self.ed.library.game_thumb = self.picture(ctx, "biblioteca-juego", &s, [480, 360]);
                    self.ed.library.game_s = Some(s);
                }
            }
            Purpose::Thumb(kind) => {
                if let Ok(s) = result
                    && let Some(t) = self.picture(ctx, &format!("mini-{kind}"), &s, [208, 176])
                {
                    self.thumbs.insert(kind, t);
                }
            }
        }
    }

    /// «Duplicar para editar» (§5): lleva lo que se ve, sin girar, intacto y con estado 0, al modo Editar.
    fn fork_main(&mut self) {
        let Some(cat) = self.catalog.clone() else { return };
        if self.editor.doc.as_ref().is_some_and(|d| d.unsaved) {
            self.confirm = Some(edit::Confirm::Fork);
            return;
        }
        let mut p = self.params.clone();
        p.facing = cat.facings.iter().find(|f| f.turns == 0).map_or(3, |f| f.value);
        p.damage = cat.damages.first().map_or_else(|| "Intact".into(), |d| d.name.clone());
        p.dynamic = 0;
        if p != self.params {
            // Lo que se ve está girado o dañado: se pide la versión limpia y se duplica al llegar.
            self.params = p;
            self.rebuild_main();
            self.toasts.push(ToastKind::Info, "Preparando la versión sin girar ni dañar…", None);
            return;
        }
        let Some(s) = self.main.s.clone() else { return };
        let label = cat.kind(&self.params.kind).map_or(self.params.kind.clone(), |k| k.label.clone());
        self.editor.fork(&s, format!("{label} {}", self.params.seed), self.params.style.clone());
        self.after_open();
    }

    /// «Editar este diseño del juego» desde Explorar: lo prepara en Biblioteca › Del juego (sin girar, intacto, sin
    /// medianeras y siempre el original), donde se confirma con un clic.
    fn edit_game_from_explore(&mut self) {
        let Some(cat) = self.catalog.clone() else { return };
        let mut p = self.params.clone();
        p.facing = cat.facings.iter().find(|f| f.turns == 0).map_or(3, |f| f.value);
        p.damage = cat.damages.first().map_or_else(|| "Intact".into(), |d| d.name.clone());
        p.dynamic = 0;
        self.ed.library.game = Some(p);
        self.ed.library.game_party = 0;
        self.ed.library.game_s = None;
        self.ed.library.game_thumb = None;
        self.ed.library.section = crate::ui::library::Section::Game;
        self.mode = Mode::Edit;
        self.open_tab = Some(Tab::Library);
        self.toasts.push(ToastKind::Info, "Revisa el diseño en la Biblioteca y pulsa «Editar este diseño del juego».", None);
    }

    fn sheet_params(&self, i: usize) -> Option<Params> {
        let cat = self.catalog.as_ref()?;
        let kind = cat.kind(&self.params.kind)?;
        let (row, col) = (i / self.sheet.n, i % self.sheet.n);
        let mut p = self.params.clone();
        p.seed = self.sheet.base_seed.wrapping_add(col as i32);
        match self.sheet.rows {
            SheetRows::Nothing => p.seed = self.sheet.base_seed.wrapping_add(i as i32),
            SheetRows::Style if !kind.styles.is_empty() => p.style = kind.styles[row % kind.styles.len()].clone(),
            SheetRows::Damage if !cat.damages.is_empty() => p.damage = cat.damages[row % cat.damages.len()].name.clone(),
            SheetRows::Design => {
                if let Some(v) = p.fields.get_mut("design") {
                    *v = row as i64;
                }
            }
            _ => {}
        }
        p.fit(kind, cat);
        Some(p)
    }

    fn refresh_sheet(&mut self) {
        let sig = format!("{:?}{:?}{}{}", self.params, self.sheet.rows, self.sheet.n, self.sheet.base_seed);
        if sig == self.sheet.asked || self.catalog.is_none() {
            return;
        }
        self.sheet.asked = sig;
        let n = self.sheet.n * self.sheet.n;
        self.sheet.cells = (0..n).map(|_| Slot::default()).collect();
        self.sheet.images = vec![None; n];
        for i in 0..n {
            if let Some(p) = self.sheet_params(i) {
                self.sheet.cells[i].ask(&mut self.host, Purpose::Sheet(i), p.request());
            }
        }
    }

    fn send_to_compare(&mut self, p: &Params) {
        if self.compare[0].req.is_none() {
            let main = self.params.request();
            self.compare[0].ask(&mut self.host, Purpose::Compare(0), main);
        }
        self.compare[1].ask(&mut self.host, Purpose::Compare(1), p.request());
    }

    /// «Localizar»: caja que parpadea en el visor activo.
    fn locate(&mut self, at: &CheckAt) {
        let b = match *at {
            CheckAt::Voxel { voxel: v } => [v[0], v[1], v[2], v[0] + 1, v[1] + 1, v[2] + 1],
            CheckAt::Cell { cell: c, level } => {
                [c[0] * SUB, level * SUB, c[1] * SUB, c[0] * SUB + SUB, level * SUB + SUB, c[1] * SUB + SUB]
            }
        };
        match self.mode {
            Mode::Explore => self.view.highlight = Some(b),
            Mode::Edit => self.edit_view.highlight = Some(b),
        }
    }

    /// Títulos con la fuente grabada del juego (se cargan una vez).
    fn titles(&mut self, ctx: &egui::Context) -> Option<[TextureHandle; 2]> {
        self.titles
            .get_or_insert_with(|| {
                let f = TitleFont::load(&self.host.paths.agame_root)?;
                let tex = |name: &str, text: &str, scale| {
                    let img = f.render(text, theme::TITLE, theme::TEXT_SHADOW, scale);
                    ctx.load_texture(name, img, egui::TextureOptions::NEAREST)
                };
                Some([tex("marca", "ESTRUCTURAS", 2), tex("bienvenida", "Bienvenido al taller", 2)])
            })
            .clone()
    }

    /// Imagen de píxel nítido: tamaño en puntos = píxeles / escala de pantalla.
    fn pixel_image(ui: &mut Ui, t: &TextureHandle) {
        let size = t.size_vec2() / ui.ctx().pixels_per_point();
        ui.add(egui::Image::new((t.id(), size)));
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        let brand = self.titles(ui.ctx());
        ui.horizontal_centered(|ui| {
            match &brand {
                Some([marca, _]) => Self::pixel_image(ui, marca),
                None => {
                    ui.label(RichText::new("ESTRUCTURAS").font(theme::semibold(18.0)).color(theme::TITLE));
                }
            }
            ui.add_space(8.0);
            ui.scope(|ui| {
                // Menús de la barra como texto (sin fondo de botón) hasta que se pasa el ratón.
                let w = &mut ui.visuals_mut().widgets.inactive;
                w.weak_bg_fill = egui::Color32::TRANSPARENT;
                w.bg_stroke = egui::Stroke::NONE;
                ui.menu_button("Archivo", |ui| self.file_menu(ui));
                ui.menu_button("Ver", |ui| {
                    let tabs: &[Tab] = match self.mode {
                        Mode::Explore => &[Tab::Catalog, Tab::Sheet, Tab::Compare, Tab::Options, Tab::Inspector, Tab::Console],
                        Mode::Edit => &[Tab::Palette, Tab::Library, Tab::Properties, Tab::Validation, Tab::History, Tab::Console],
                    };
                    for t in tabs {
                        if ui.button(t.title()).clicked() {
                            self.open_tab = Some(*t);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Modo foco").shortcut_text(ui.ctx().format_shortcut(&FOCUS))).clicked() {
                        self.focus_mode = !self.focus_mode;
                        ui.close();
                    }
                    if ui.button(format!("{} Recompilar el generador", icon::ARROWS_CLOCKWISE)).clicked() {
                        self.host.recompile();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Restablecer disposición").clicked() {
                        self.reset_layout = true;
                        ui.close();
                    }
                });
                ui.menu_button("Ayuda", |ui| {
                    if ui.add(egui::Button::new("Atajos de teclado").shortcut_text("F1")).clicked() {
                        self.show_help = true;
                        ui.close();
                    }
                    ui.separator();
                    ui.label(dim(format!("Estructuras · a_game {}", env!("CARGO_PKG_VERSION"))));
                });
            });
            ui.separator();
            let mut mode = self.mode;
            widgets::segmented(
                ui,
                &mut mode,
                &[
                    (Mode::Explore, format!("{}  Explorar", icon::BINOCULARS)),
                    (Mode::Edit, format!("{}  Editar", icon::HAMMER)),
                ],
            );
            if mode != self.mode {
                self.mode = mode;
                if mode == Mode::Edit && self.editor.doc.is_none() {
                    self.editor_start();
                }
            }
            ui.separator();
            match self.mode {
                Mode::Explore => {
                    let search = ui.add(
                        egui::Button::new(format!("{}  Buscar…", icon::MAGNIFYING_GLASS))
                            .shortcut_text(ui.ctx().format_shortcut(&SEARCH))
                            .min_size(egui::vec2(220.0, 0.0)),
                    );
                    if search.clicked() {
                        self.command = Some(String::new());
                    }
                }
                Mode::Edit => self.edit_title(ui),
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // De derecha a izquierda: la acción principal en la esquina, luego el resto y el chip.
                match self.mode {
                    Mode::Explore => {
                        let edit = ui
                            .add_enabled_ui(self.main.s.is_some(), |ui| widgets::primary(ui, &format!("{}  EDITAR {}", icon::PENCIL_SIMPLE, icon::CARET_DOWN)))
                            .inner
                            .on_hover_text("Llevar lo que ves al modo Editar");
                        let city = self.catalog.as_ref().and_then(|c| c.kind(&self.params.kind)).is_some_and(|k| k.group == "city");
                        // ¿Ya hay una sustitución de lo que se ve? Entonces la opción la abre.
                        let key = crate::agvox::Replaces {
                            kind: self.params.kind.clone(),
                            style: self.params.style.clone(),
                            design: crate::ui::library::design_field(&self.params.fields).map_or(0, |(_, v)| v),
                            size: [self.params.width, self.params.depth],
                            party: 0,
                            ..Default::default()
                        };
                        let existing = self.ed.library.override_of(&key).map(|e| e.path.clone());
                        egui::Popup::menu(&edit).show(|ui| {
                            if ui.button(format!("{}  Copiar como estructura nueva", icon::COPY)).clicked() {
                                self.fork_main();
                            }
                            ui.label(dim("Una estructura tuya, aparte. El juego no cambia."));
                            ui.separator();
                            let text = if existing.is_some() { "Abrir el diseño editado" } else { "Editar este diseño del juego" };
                            if ui
                                .add_enabled(city, egui::Button::new(format!("{}  {text}", icon::PENCIL_SIMPLE_LINE)))
                                .on_disabled_hover_text("Solo los edificios de ciudad se pueden sustituir.")
                                .clicked()
                            {
                                match existing.clone() {
                                    Some(path) => self.ask_open_path(path),
                                    None => self.edit_game_from_explore(),
                                }
                            }
                            ui.label(dim("Tu versión sustituye a este diseño en todas las ciudades del juego."));
                        });
                        if ui.button(format!("{}  Captura", icon::CAMERA)).on_hover_text("Guardar una imagen (Cmd+Shift+I)").clicked() {
                            self.capture_view();
                        }
                    }
                    Mode::Edit => self.edit_actions(ui),
                }
                ui.add_space(8.0);
                let (glyph, text, color) = match &self.host.status {
                    Status::Ready => (icon::CHECK_CIRCLE, "Juego al día".to_string(), theme::GREEN),
                    Status::Compiling | Status::Preparing => (icon::ARROWS_CLOCKWISE, "Recompilando…".into(), theme::AMBER),
                    Status::Broken(n) => (icon::X_CIRCLE, format!("No compila ({n})"), theme::ERROR),
                    Status::Off(_) => (icon::PLUGS, "Sin generador".into(), theme::TEXT_DIM),
                };
                let tip = match &self.host.status {
                    Status::Off(why) => why.clone(),
                    _ => "Estado del generador del juego. Pulsa para ver la consola.".into(),
                };
                if widgets::chip(ui, glyph, &text, color).on_hover_text(tip).clicked() {
                    self.open_tab = Some(Tab::Console);
                }
            });
        });
    }

    fn file_menu(&mut self, ui: &mut Ui) {
        if ui.button(format!("{} Nueva estructura", icon::FILE_PLUS)).clicked() {
            self.mode = Mode::Edit;
            self.ask_new();
            ui.close();
        }
        if ui.add(egui::Button::new(format!("{} Abrir…", icon::FOLDER_OPEN)).shortcut_text(ui.ctx().format_shortcut(&OPEN))).clicked() {
            self.ask_open();
            ui.close();
        }
        let has_doc = self.editor.doc.is_some();
        if ui.add_enabled(has_doc, egui::Button::new(format!("{} Guardar", icon::FLOPPY_DISK)).shortcut_text(ui.ctx().format_shortcut(&SAVE))).clicked() {
            self.save(false);
            ui.close();
        }
        if ui.add_enabled(has_doc, egui::Button::new(format!("{} Guardar como…", icon::FLOPPY_DISK))).clicked() {
            self.save(true);
            ui.close();
        }
        ui.separator();
        if ui.button(format!("{} Elegir la carpeta del juego…", icon::FOLDER_OPEN)).clicked() {
            self.pick_root();
            ui.close();
        }
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            let mono = |t: String| RichText::new(t).font(theme::mono(12.0)).color(theme::TEXT_DIM);
            match self.hover {
                Some(v) => {
                    ui.label(mono(format!(
                        "Celda {},{} · Nivel {} · Sub {},{},{}",
                        v[0].div_euclid(SUB),
                        v[2].div_euclid(SUB),
                        v[1].div_euclid(SUB),
                        v[0].rem_euclid(SUB),
                        v[1].rem_euclid(SUB),
                        v[2].rem_euclid(SUB)
                    )));
                }
                None => {
                    ui.label(mono("—".into()));
                }
            }
            ui.separator();
            let hint = match self.mode {
                Mode::Explore => format!(
                    "{} {}: semilla · D: semilla al azar · Q/R: girar · C: ver por plantas · Cmd+K: buscar · F1: atajos",
                    icon::ARROW_LEFT,
                    icon::ARROW_RIGHT
                ),
                Mode::Edit => match self.editor.tools.tool {
                    crate::edit::tools::Tool::Build => self
                        .ed
                        .tools_cat
                        .iter()
                        .find(|t| t.id == self.editor.tools.build)
                        .map_or(crate::edit::tools::Tool::Build.hint(), |t| edit::build_hint(&t.target))
                        .to_string(),
                    t => t.hint().to_string(),
                },
            };
            if self.mode == Mode::Edit && self.editor.tools.tool.uses_mode() {
                let m = self.editor.tools.mode;
                let c = if m == crate::edit::tools::Mode::Erase { theme::ERROR } else { theme::TEXT };
                ui.label(RichText::new(m.status()).color(c).small());
                ui.separator();
            }
            ui.label(dim(hint));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let view = if self.mode == Mode::Edit { &self.edit_view } else { &self.view };
                ui.label(mono(format!("Zoom {}×", view.scale)));
                ui.separator();
                let s = match self.mode {
                    Mode::Explore => self.main.s.as_deref(),
                    Mode::Edit => self.editor.doc.as_ref().map(|d| &d.s),
                };
                if let Some(s) = s {
                    let [sx, sy, sz] = s.grid.size;
                    if s.model_cell.is_some() {
                        ui.label(mono(format!("{sx}×{sz}×{sy} vóxeles")));
                    } else {
                        ui.label(mono(format!("{}×{} celdas · {} niveles", sx / SUB, sz / SUB, sy / SUB)));
                    }
                }
            });
        });
    }

    fn pick_root(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Carpeta del juego (a_game)").pick_folder() {
            match Paths::check_agame(&dir) {
                Ok(()) => {
                    self.settings.agame_root = Some(dir.clone());
                    render::set_agame_root(&dir);
                    self.catalog = None;
                    self.titles = None;
                    self.host.set_root(dir);
                }
                Err(why) => self.toasts.push(ToastKind::Error, why, None),
            }
        }
    }

    /// Primer arranque (§8): carpeta del juego y ratón o trackpad.
    fn first_run(&mut self, ctx: &egui::Context) {
        let root_bad = Paths::check_agame(&self.host.paths.agame_root).err();
        if root_bad.is_none() && self.settings.input.is_some() {
            return;
        }
        let titles = self.titles(ctx);
        egui::Modal::new(egui::Id::new("primer-arranque")).show(ctx, |ui| {
            ui.set_width(440.0);
            match &titles {
                Some([_, bienvenida]) => Self::pixel_image(ui, bienvenida),
                None => {
                    ui.label(RichText::new("Bienvenido al taller").font(theme::semibold(18.0)).color(theme::TITLE));
                }
            }
            ui.add_space(6.0);
            if let Some(why) = root_bad {
                ui.label("Para enseñarte las estructuras necesito la carpeta del juego (a_game).");
                ui.label(RichText::new(why).color(theme::WARN));
                ui.add_space(6.0);
                if widgets::primary(ui, &format!("{} Elegir carpeta…", icon::FOLDER_OPEN)).clicked() {
                    self.pick_root();
                }
                return;
            }
            ui.label("¿Con qué vas a trabajar? Ajusta cómo se gira y se mueve la cámara.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (input, glyph, title, desc) in [
                    (Input::Mouse, icon::MOUSE, "Ratón", "Botón derecho: girar · Rueda: zoom · Central: mover"),
                    (Input::Trackpad, icon::HAND_SWIPE_RIGHT, "Trackpad", "Dos dedos: mover · Pellizcar: zoom · Alt+dos dedos: girar"),
                ] {
                    let r = ui.add_sized([210.0, 96.0], egui::Button::new(RichText::new(format!("{glyph}\n{title}\n")).size(16.0)));
                    if r.on_hover_text(desc).clicked() {
                        self.settings.input = Some(input);
                    }
                }
            });
        });
    }

    fn hints(&mut self, ctx: &egui::Context) {
        if self.settings.hints_done || self.settings.input.is_none() || self.main.s.is_none() || self.mode != Mode::Explore {
            return;
        }
        egui::Window::new(RichText::new("Primeros pasos").color(theme::TITLE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(format!("{}  Elige una estructura en el Catálogo.", icon::BUILDINGS));
                ui.label(format!("{}  Ajusta su estilo, diseño y semilla en Opciones.", icon::SLIDERS_HORIZONTAL));
                ui.label(format!("{}  Gira la cámara con Q y R; corta por plantas con C.", icon::ARROW_ARC_RIGHT));
                if widgets::primary(ui, "Entendido").clicked() {
                    self.settings.hints_done = true;
                }
            });
    }

    fn command_palette(&mut self, ctx: &egui::Context) {
        let Some(query) = self.command.as_mut() else { return };
        let mut pick = None;
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        egui::Modal::new(egui::Id::new("buscar")).show(ctx, |ui| {
            ui.set_width(420.0);
            let r = ui.add(egui::TextEdit::singleline(query).hint_text("Busca una estructura…").desired_width(f32::INFINITY));
            r.request_focus();
            let q = query.to_lowercase();
            if let Some(cat) = &self.catalog {
                let hits: Vec<_> = cat.kinds.iter().filter(|k| k.label.to_lowercase().contains(&q)).take(12).collect();
                for k in &hits {
                    if ui.selectable_label(false, format!("{}  {}", crate::labels::group_icon(&k.group), k.label)).clicked() {
                        pick = Some(k.name.clone());
                    }
                }
                if ui.input(|i| i.key_pressed(Key::Enter))
                    && let Some(k) = hits.first()
                {
                    pick = Some(k.name.clone());
                }
            }
        });
        if let Some(k) = pick {
            self.mode = Mode::Explore;
            self.select_kind(&k);
            close = true;
        }
        if close {
            self.command = None;
        }
    }

    fn help(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        let arrows = format!("{} {}", icon::ARROW_LEFT, icon::ARROW_RIGHT);
        let updown = format!("Shift+{} {}", icon::ARROW_UP, icon::ARROW_DOWN);
        let plane = format!("{} {}", icon::ARROW_UP, icon::ARROW_DOWN);
        egui::Window::new(RichText::new("Atajos de teclado").color(theme::TITLE)).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.columns(2, |cols| {
                let table = |ui: &mut Ui, id: &str, rows: &[(&str, &str)]| {
                    egui::Grid::new(id).striped(true).show(ui, |ui| {
                        for (k, what) in rows {
                            ui.label(RichText::new(*k).font(theme::mono(13.0)));
                            ui.label(*what);
                            ui.end_row();
                        }
                    });
                };
                cols[0].label(RichText::new("Ver").font(theme::semibold(13.0)));
                table(&mut cols[0], "ver", &[
                    (arrows.as_str(), "Semilla anterior / siguiente"),
                    ("D", "Semilla al azar"),
                    ("Q / R", "Girar la cámara 90°"),
                    ("F", "Encuadrar"),
                    ("P", "Iso o vista libre"),
                    ("+ / −", "Zoom"),
                    ("C", "Cortar por planta"),
                    (updown.as_str(), "Planta arriba / abajo"),
                    ("1–8", "Capas"),
                    ("Cmd+K", "Buscar"),
                    ("Cmd+.", "Modo foco"),
                    ("Cmd+Shift+I", "Captura"),
                ]);
                cols[1].label(RichText::new("Editar").font(theme::semibold(13.0)));
                table(&mut cols[1], "editar", &[
                    ("B E U L G", "Lápiz, borrar, caja, línea, relleno"),
                    ("X I M V", "Extruir, cuentagotas, selección, mover"),
                    ("Alt+clic", "Cuentagotas"),
                    (", / .", "Pincel más pequeño / grande"),
                    (plane.as_str(), "Plano bloqueado ±1 sub-cubo"),
                    ("Cmd C X V D", "Copiar, cortar, pegar, duplicar"),
                    ("Shift+X / Z", "Espejo de la selección"),
                    ("Cmd+R", "Girar la selección"),
                    ("Cmd+Z / Shift+Cmd+Z", "Deshacer / rehacer"),
                    ("Cmd+S / Cmd+O", "Guardar / abrir"),
                    ("Cmd+E", "Exportar al juego"),
                ]);
            });
        });
        self.show_help = open;
    }

    /// Atajos de vista comunes (C, 1–8, Shift+↑↓) sobre el visor del modo activo.
    fn view_keys(&mut self, ctx: &egui::Context) {
        let (view, s) = match self.mode {
            Mode::Explore => (&mut self.view, self.main.s.as_deref()),
            Mode::Edit => (&mut self.edit_view, self.editor.doc.as_ref().map(|d| &d.s)),
        };
        let Some(s) = s else { return };
        let (cut, up, down, layers) = ctx.input(|i| {
            let plain = i.modifiers.is_none();
            let layers: Vec<usize> = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8]
                .iter()
                .enumerate()
                .filter(|(_, k)| plain && i.key_pressed(**k))
                .map(|(n, _)| n)
                .collect();
            let shift = i.modifiers.shift && !i.modifiers.command;
            (plain && i.key_pressed(Key::C), shift && i.key_pressed(Key::ArrowUp), shift && i.key_pressed(Key::ArrowDown), layers)
        });
        for n in layers {
            let l = view::layer_mut(&mut view.layers, n);
            *l = !*l;
        }
        let floors = &s.band_floors;
        if cut {
            view.cut_level = match view.cut_level {
                Some(_) => None,
                None => floors.first().copied(),
            };
        }
        if (up || down) && !floors.is_empty() {
            let i = view.cut_level.and_then(|c| floors.iter().position(|&f| f == c));
            let next = match (i, up) {
                (None, false) => Some(floors.len() - 1),
                (None, true) => None,
                (Some(i), true) => (i + 1 < floors.len()).then(|| i + 1),
                (Some(i), false) => Some(i.saturating_sub(1)),
            };
            view.cut_level = next.map(|i| floors[i]);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SEARCH)) {
            self.command = Some(String::new());
        }
        if ctx.input_mut(|i| i.consume_shortcut(&FOCUS)) {
            self.focus_mode = !self.focus_mode;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&CAPTURE)) {
            self.capture_view();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SAVE)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&OPEN)) {
            self.ask_open();
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_help = !self.show_help;
        }
        if ctx.egui_wants_keyboard_input() || self.command.is_some() {
            return;
        }
        self.view_keys(ctx);
        match self.mode {
            Mode::Explore => {
                let (left, right, dice) = ctx.input(|i| {
                    let plain = i.modifiers.is_none();
                    (plain && i.key_pressed(Key::ArrowLeft), plain && i.key_pressed(Key::ArrowRight), plain && i.key_pressed(Key::D))
                });
                if left || right || dice {
                    self.params.seed =
                        if dice { widgets::random_seed() } else { self.params.seed.wrapping_add(if right { 1 } else { -1 }) };
                    self.rebuild_main();
                }
            }
            Mode::Edit => self.edit_shortcuts(ctx),
        }
    }
}

const SEARCH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
const FOCUS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Period);
const CAPTURE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::I);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

impl Params {
    /// Marcador hasta que llega el catálogo.
    fn placeholder() -> Self {
        Params {
            kind: String::new(),
            style: String::new(),
            fields: Default::default(),
            seed: 0,
            width: 0,
            depth: 0,
            facing: 3,
            foundation: 1,
            damage: "Intact".into(),
            dynamic: 0,
        }
    }
}

struct Tabs<'a> {
    st: &'a mut State,
}

impl TabViewer for Tabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> WidgetText {
        let mut t = tab.title();
        if *tab == Tab::Validation
            && let Some(doc) = &self.st.editor.doc
        {
            let errors = self.st.editor.errors();
            let warnings = doc.s.checks.iter().filter(|c| c.severity == crate::bridge::proto::Severity::Warning).count();
            if errors > 0 {
                t = format!("{t}  {} {errors}", icon::X_CIRCLE);
            }
            if warnings > 0 {
                t = format!("{t}  {} {warnings}", icon::WARNING);
            }
        }
        t.into()
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        *tab != Tab::View
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::View | Tab::Compare | Tab::Catalog | Tab::Sheet | Tab::Palette => [false, false],
            _ => [false, true],
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Tab) {
        let st = &mut *self.st;
        match tab {
            Tab::Catalog => match st.catalog.clone() {
                Some(cat) => {
                    let selected = st.params.kind.clone();
                    if let Some(k) = catalog::ui(ui, &cat, &selected, &mut st.filter, &st.thumbs) {
                        st.select_kind(&k);
                    }
                }
                None => loading(ui, &st.host.status),
            },
            Tab::Options => {
                let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
                let Some(kind) = cat.kind(&st.params.kind).cloned() else { return };
                if crate::ui::options::ui(ui, &cat, &kind, &mut st.params, st.main.s.as_deref()) {
                    st.rebuild_main();
                }
            }
            Tab::Inspector => {
                let (Some(cat), Some(s)) = (st.catalog.clone(), st.main.s.clone()) else {
                    return;
                };
                inspector::under_cursor(ui, &cat, &s, st.hover, &mut st.pinned);
                ui.separator();
                if let Some(at) = inspector::checks(ui, &s) {
                    st.locate(&at);
                }
            }
            Tab::Console => console::ui(ui, &mut st.host),
            Tab::View => match st.mode {
                Mode::Explore => view::explore(ui, st),
                Mode::Edit => edit::view(ui, st),
            },
            Tab::Sheet => view::sheet(ui, st),
            Tab::Compare => view::compare(ui, st),
            Tab::Palette => edit::palette(ui, st),
            Tab::Properties => edit::properties(ui, st),
            Tab::Validation => edit::validation(ui, st),
            Tab::History => edit::history(ui, st),
            Tab::Library => edit::library(ui, st),
        }
    }
}

fn loading(ui: &mut Ui, status: &Status) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| match status {
        Status::Off(why) => {
            ui.label(RichText::new(icon::PLUGS).size(28.0).color(theme::STONE3));
            ui.label(RichText::new(why).color(theme::WARN));
        }
        _ => {
            ui.label(RichText::new("Preparando el generador del juego…").color(theme::TEXT));
            ui.add(egui::ProgressBar::new(0.0).animate(true).desired_width(220.0).fill(theme::AMBER));
        }
    });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let replies = self.st.host.tick();
        self.st.handle(&ctx, replies);
        let materials = self.st.catalog.as_ref().map(|c| c.materials.clone()).unwrap_or_default();
        self.st.editor.tick(&mut self.st.host, &materials);
        self.st.shortcuts(&ctx);
        let m = self.st.mode as usize;
        if self.st.reset_layout {
            self.docks[m] = if m == 0 { default_dock() } else { default_edit_dock() };
            self.st.reset_layout = false;
        }
        if let Some(tab) = self.st.open_tab.take() {
            let dock = &mut self.docks[m];
            match dock.find_tab(&tab) {
                Some(path) => {
                    let _ = dock.set_active_tab(path);
                }
                // La consola, debajo del visor; el resto, en el panel con el foco.
                None => match (tab, dock.find_tab(&Tab::View)) {
                    (Tab::Console, Some(path)) => {
                        dock.main_surface_mut().split_below(path.node, 0.78, vec![Tab::Console]);
                    }
                    _ => dock.push_to_focused_leaf(tab),
                },
            }
        }

        egui::Panel::top("barra")
            .exact_size(40.0)
            .frame(egui::Frame::new().fill(theme::STONE0).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| self.st.top_bar(ui));
        egui::Panel::bottom("estado")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(theme::STONE0).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| self.st.status_bar(ui));
        let m = self.st.mode as usize;
        let area = egui::CentralPanel::no_frame()
            .show(ui, |ui| {
                if self.st.focus_mode {
                    match self.st.mode {
                        Mode::Explore => view::explore(ui, &mut self.st),
                        Mode::Edit => edit::view(ui, &mut self.st),
                    }
                } else {
                    DockArea::new(&mut self.docks[m])
                        .id(egui::Id::new(("dock", m)))
                        .style(dock_style(ui.style()))
                        .show_leaf_collapse_buttons(false)
                        .show_leaf_close_all_buttons(false)
                        .show_inside(ui, &mut Tabs { st: &mut self.st });
                }
                ui.max_rect()
            })
            .inner;
        self.st.toasts.show(&ctx, self.st.view_rect.take().unwrap_or(area));
        self.st.first_run(&ctx);
        self.st.hints(&ctx);
        self.st.command_palette(&ctx);
        self.st.help(&ctx);
        self.st.confirm_dialog(&ctx);
        self.st.edit_dialogs(&ctx);
        self.tick_capture(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.st.settings.params.insert(self.st.params.kind.clone(), self.st.params.clone());
        eframe::set_value(storage, "settings", &self.st.settings);
        eframe::set_value(storage, "dock", &self.docks[0]);
        eframe::set_value(storage, "dock_edit_v3", &self.docks[1]);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(theme::PANEL).to_array()
    }
}

/// Estilo de egui_dock con los tokens (§2, «egui_dock»).
fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(style);
    s.tab_bar.bg_fill = theme::STONE0;
    s.tab_bar.height = 28.0;
    s.tab_bar.hline_color = theme::STONE0;
    s.separator.width = 2.0;
    s.separator.color_idle = theme::STONE0;
    s.separator.color_hovered = theme::STONE3;
    s.separator.color_dragged = theme::GOLD;
    s.overlay.selection_color = theme::alpha(theme::GOLD, 0.2);
    let t = &mut s.tab;
    for (st, text, fill) in [
        (&mut t.active, theme::TEXT, theme::PANEL),
        (&mut t.focused, theme::TITLE, theme::PANEL),
        (&mut t.active_with_kb_focus, theme::TEXT, theme::PANEL),
        (&mut t.focused_with_kb_focus, theme::TITLE, theme::PANEL),
        (&mut t.inactive, theme::TEXT_DIM, theme::STONE0),
        (&mut t.inactive_with_kb_focus, theme::TEXT_DIM, theme::STONE0),
        (&mut t.hovered, theme::TEXT, theme::STONE1),
    ] {
        st.text_color = text;
        st.bg_fill = fill;
        st.outline_color = theme::STONE0;
        st.corner_radius = egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 };
    }
    t.focused.outline_color = theme::GOLD;
    t.hline_below_active_tab_name = false;
    t.tab_body.bg_fill = theme::PANEL;
    t.tab_body.stroke = egui::Stroke::NONE;
    t.tab_body.inner_margin = egui::Margin::same(8);
    s.main_surface_border_stroke = egui::Stroke::NONE;
    s
}
