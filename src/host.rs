//! Vida del generador (el puente): compilarlo, lanzarlo, vigilar el C# del juego para recargar en caliente y
//! enrutar las respuestas a quien las pidió. La UI llama a `tick` cada fotograma y nunca espera.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use notify::{RecursiveMode, Watcher};
use serde_json::{Value, json};

use crate::bridge::proto::{self, BuildHeader, BuildRequest, Catalog, ErrorInfo};
use crate::bridge::{self, CompileError, CompileOutcome, Event, Paths, Process};
use crate::model::Structure;

/// Para quién es una construcción.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    Main,
    Compare(usize),
    Sheet(usize),
    Thumb(String),
    /// Vista previa de «Del juego» en la Biblioteca.
    Library,
}

impl Purpose {
    fn urgent(&self) -> bool {
        matches!(self, Purpose::Main | Purpose::Compare(_) | Purpose::Library)
    }
}

/// Estado visible del generador (chip de recarga, §6).
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Sin generador: carpeta mal, falta dotnet o Unity, o el proceso murió.
    Off(String),
    /// Compilando por primera vez (no hay nada que enseñar todavía).
    Preparing,
    /// Recompilando; se sigue viendo la última versión buena.
    Compiling,
    Ready,
    /// El C# del juego no compila; se sigue viendo la última versión buena.
    Broken(usize),
}

/// Lo que la UI recibe de `tick`.
#[allow(clippy::large_enum_variant)] // vive un fotograma; no merece la pena encajarlo
pub enum Reply {
    Catalog(std::sync::Arc<Catalog>),
    Built(Purpose, BuildRequest, Result<std::sync::Arc<Structure>, ErrorInfo>),
    /// Hay un generador nuevo: lo construido antes puede estar viejo. `quiet` = la recompilación del arranque (no
    /// merece aviso).
    Reloaded { quiet: bool },
    /// Navegación y comprobaciones de la estructura que se edita (`validate`); `u64` = revisión del documento.
    Validated(u64, Result<Structure, ErrorInfo>),
    /// Resultado de `export` (cabecera tal cual: ficheros escritos y comprobaciones).
    Exported(Result<Value, ErrorInfo>),
    /// Catálogos de la fase 3 (generadores viejos no los tienen: entonces no llegan).
    Tools(Vec<proto::ToolInfo>),
    Furniture(proto::FurnitureCatalog),
    /// Papeles de material de un Aspecto (`look`).
    Look(std::collections::BTreeMap<String, String>),
    /// Resultado de una herramienta de construcción; `u64` = número de la vista previa.
    Tool(u64, Result<std::sync::Arc<Structure>, ErrorInfo>),
    /// Pieza de mueble (`stamp`); la clave es la de quien la pidió.
    Stamp(String, Result<proto::Stamp, ErrorInfo>),
}

#[derive(Clone)]
enum Pending {
    Hello,
    Catalog,
    Build(Purpose, BuildRequest),
    Validate(u64, Box<crate::model::VoxelGrid>),
    Export,
    Tools,
    Furniture,
    Look,
    Tool(u64),
    Stamp(String),
}

pub struct LogLine {
    pub at: Instant,
    pub text: String,
    pub error: bool,
}

const DEBOUNCE: Duration = Duration::from_millis(800);
/// Peticiones de poca prioridad (miniaturas, muestrario) en vuelo a la vez, para no tapar a la vista principal.
const BACKGROUND_IN_FLIGHT: usize = 2;

pub struct Host {
    ctx: egui::Context,
    pub paths: Paths,
    pub status: Status,
    process: Option<Process>,
    pending: HashMap<u64, Pending>,
    /// Último id pedido por cada propósito urgente: las respuestas viejas se descartan.
    latest: HashMap<Purpose, u64>,
    queue: VecDeque<(Purpose, BuildRequest)>,
    compile_rx: Option<Receiver<CompileOutcome>>,
    pub last_compile: Option<CompileOutcome>,
    _watcher: Option<notify::RecommendedWatcher>,
    watch_rx: Option<Receiver<notify::Result<notify::Event>>>,
    changed_at: Option<Instant>,
    pub log: Vec<LogLine>,
    /// Sube con cada generador nuevo (caché de miniaturas).
    pub generation: u64,
    /// Id de la última petición `tool` (las respuestas viejas se descartan).
    latest_tool: u64,
    /// La compilación en curso es la del arranque.
    startup: bool,
}

impl Host {
    pub fn new(ctx: egui::Context, agame_root: PathBuf) -> Self {
        let mut host = Self {
            ctx,
            paths: Paths::new(agame_root),
            status: Status::Off(String::new()),
            process: None,
            pending: HashMap::new(),
            latest: HashMap::new(),
            queue: VecDeque::new(),
            compile_rx: None,
            last_compile: None,
            _watcher: None,
            watch_rx: None,
            changed_at: None,
            log: Vec::new(),
            generation: 0,
            latest_tool: 0,
            startup: true,
        };
        host.start();
        host
    }

    /// Cambia la carpeta del juego y vuelve a empezar.
    pub fn set_root(&mut self, root: PathBuf) {
        self.process = None;
        self.pending.clear();
        self.paths = Paths::new(root);
        self.start();
    }

    /// Arranque: si ya hay una DLL compilada se sirve enseguida y se recompila detrás (por si el juego cambió).
    pub fn start(&mut self) {
        if let Err(why) = Paths::check_agame(&self.paths.agame_root) {
            self.status = Status::Off(why);
            return;
        }
        if bridge::dotnet().is_none() {
            self.status = Status::Off("No encuentro dotnet: instala el SDK de .NET 8.".into());
            return;
        }
        self.watch();
        if self.paths.dll().is_file() {
            self.spawn();
            self.status = Status::Compiling;
        } else {
            self.status = Status::Preparing;
        }
        self.compile();
    }

    fn note(&mut self, text: impl Into<String>, error: bool) {
        self.log.push(LogLine { at: Instant::now(), text: text.into(), error });
        if self.log.len() > 2000 {
            self.log.drain(..500);
        }
    }

    fn compile(&mut self) {
        let (tx, rx) = channel();
        let paths = self.paths.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(bridge::compile(&paths));
            ctx.request_repaint();
        });
        self.compile_rx = Some(rx);
        self.changed_at = None;
        if self.status == Status::Ready || matches!(self.status, Status::Broken(_)) {
            self.status = Status::Compiling;
        }
        self.note("Recompilando el generador…", false);
    }

    fn spawn(&mut self) {
        self.process = None;
        self.pending.clear();
        self.latest.clear();
        let ctx = self.ctx.clone();
        match Process::spawn(&self.paths, move || ctx.request_repaint()) {
            Ok(mut p) => {
                let ops = [
                    ("hello", Pending::Hello),
                    ("catalog", Pending::Catalog),
                    ("tools", Pending::Tools),
                    ("furniture", Pending::Furniture),
                ];
                for (op, pending) in ops {
                    if let Ok(id) = p.request(op, json!({}), &[]) {
                        self.pending.insert(id, pending);
                    }
                }
                self.process = Some(p);
                self.generation += 1;
            }
            Err(e) => {
                let why = format!("{e:#}");
                self.note(why.clone(), true);
                self.status = Status::Off(why);
            }
        }
    }

    /// Vigila el C# que compila el puente: un cambio recompila (con espera para agrupar guardados seguidos).
    fn watch(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        let watcher = notify::recommended_watcher(move |e| {
            let _ = tx.send(e);
            ctx.request_repaint_after(DEBOUNCE);
        });
        let scripts = self.paths.agame_root.join("Assets/_Game/Scripts");
        match watcher.and_then(|mut w| w.watch(&scripts, RecursiveMode::Recursive).map(|_| w)) {
            Ok(w) => {
                self._watcher = Some(w);
                self.watch_rx = Some(rx);
            }
            Err(_) => self.note("No puedo vigilar los cambios del juego; recompila a mano (Ver › Recompilar).", true),
        }
    }

    /// Ficheros del juego que entran en el puente.
    fn relevant(path: &Path) -> bool {
        let p = path.to_string_lossy();
        p.ends_with(".cs")
            && (["/Scripts/Shared/", "/Scripts/Lang/", "/Scripts/Core/", "/Scripts/World/", "/Scripts/Generation/", "/Scripts/Nations/"]
                .iter()
                .any(|d| p.contains(d))
                || p.ends_with("StructurePalette.cs"))
    }

    pub fn recompile(&mut self) {
        if self.compile_rx.is_none() && Paths::check_agame(&self.paths.agame_root).is_ok() {
            self.compile();
        }
    }

    /// Pide una construcción. Las urgentes salen ya; las de fondo, en cola.
    pub fn build(&mut self, purpose: Purpose, req: BuildRequest) {
        if purpose.urgent() {
            self.send_build(purpose, req);
        } else {
            self.queue.retain(|(p, _)| *p != purpose);
            self.queue.push_back((purpose, req));
        }
    }

    pub fn cancel_background(&mut self) {
        self.queue.clear();
    }

    fn send_build(&mut self, purpose: Purpose, req: BuildRequest) {
        let Some(p) = self.process.as_mut() else { return };
        match p.request("build", serde_json::to_value(&req).unwrap_or(Value::Null), &[]) {
            Ok(id) => {
                if purpose.urgent() {
                    self.latest.insert(purpose.clone(), id);
                }
                self.pending.insert(id, Pending::Build(purpose, req));
            }
            Err(e) => self.note(format!("No he podido pedir la construcción: {e}"), true),
        }
    }

    /// Pide navegación y comprobaciones de una estructura hecha a mano (`validate`, docs/contrato.md).
    /// `revision` identifica la versión del documento: las respuestas viejas se descartan.
    pub fn validate(&mut self, revision: u64, s: &Structure, replaces: Option<&crate::agvox::Replaces>) {
        let Some(p) = self.process.as_mut() else { return };
        let (header, payload) = crate::bridge::proto::doc_request(s, replaces);
        match p.request("validate", header, &payload) {
            Ok(id) => {
                self.pending.insert(id, Pending::Validate(revision, Box::new(s.grid.clone())));
            }
            Err(e) => self.note(format!("No he podido pedir la validación: {e}"), true),
        }
    }

    /// Aplica una herramienta de construcción al documento (`tool`). `extra` lleva `name`, `params`, `roles`,
    /// `target` y `floorY`. La respuesta llega como `Reply::Tool(seq, …)`; las viejas se descartan.
    pub fn tool(&mut self, seq: u64, s: &Structure, replaces: Option<&crate::agvox::Replaces>, extra: Value) {
        let Some(p) = self.process.as_mut() else { return };
        let (mut header, payload) = crate::bridge::proto::doc_request(s, replaces);
        if let (Some(h), Value::Object(e)) = (header.as_object_mut(), extra) {
            h.extend(e);
        }
        match p.request("tool", header, &payload) {
            Ok(id) => {
                self.latest_tool = id;
                self.pending.insert(id, Pending::Tool(seq));
            }
            Err(e) => self.note(format!("No he podido pedir la herramienta: {e}"), true),
        }
    }

    /// Papeles de material de un Aspecto.
    pub fn look(&mut self, style: &str, district: &str, tier: &str) {
        let Some(p) = self.process.as_mut() else { return };
        if let Ok(id) = p.request("look", json!({ "style": style, "district": district, "tier": tier }), &[]) {
            self.pending.insert(id, Pending::Look);
        }
    }

    /// Pieza de mueble girada hacia `facing`.
    pub fn stamp(&mut self, key: String, piece: &str, facing: i32, width: Option<i32>, seed: i32) {
        let Some(p) = self.process.as_mut() else { return };
        // `id` es el de la trama: el mueble va en `piece` (docs/contrato.md).
        let body = json!({ "piece": piece, "facing": facing, "width": width, "seed": seed });
        if let Ok(id) = p.request("stamp", body, &[]) {
            self.pending.insert(id, Pending::Stamp(key));
        }
    }

    /// Regenera el C# del juego a partir de todos los `.agvox` de `source_dir` (`export`).
    pub fn export(&mut self, source_dir: &Path) -> bool {
        let Some(p) = self.process.as_mut() else { return false };
        match p.request("export", json!({ "sourceDir": source_dir }), &[]) {
            Ok(id) => {
                self.pending.insert(id, Pending::Export);
                true
            }
            Err(e) => {
                self.note(format!("No he podido pedir la exportación: {e}"), true);
                false
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.values().any(|p| matches!(p, Pending::Build(..)))
    }

    /// Procesa lo que haya llegado. Llamar una vez por fotograma.
    pub fn tick(&mut self) -> Vec<Reply> {
        let mut out = Vec::new();
        self.tick_watch();
        self.tick_compile(&mut out);
        self.tick_process(&mut out);
        let background = self.pending.values().filter(|p| matches!(p, Pending::Build(pp, _) if !pp.urgent())).count();
        for _ in background..BACKGROUND_IN_FLIGHT {
            let Some((purpose, req)) = self.queue.pop_front() else { break };
            self.send_build(purpose, req);
        }
        out
    }

    fn tick_watch(&mut self) {
        if let Some(rx) = &self.watch_rx {
            let hit = rx.try_iter().filter_map(Result::ok).any(|e| e.paths.iter().any(|p| Self::relevant(p)));
            if hit {
                self.changed_at = Some(Instant::now());
            }
        }
        if let Some(t) = self.changed_at {
            if t.elapsed() >= DEBOUNCE && self.compile_rx.is_none() {
                self.compile();
            } else {
                self.ctx.request_repaint_after(DEBOUNCE);
            }
        }
    }

    fn tick_compile(&mut self, out: &mut Vec<Reply>) {
        let Some(rx) = &self.compile_rx else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.compile_rx = None;
        let startup = std::mem::replace(&mut self.startup, false);
        if result.ok {
            self.note("Generador al día.", false);
            if result.changed || self.process.is_none() {
                self.spawn();
                out.push(Reply::Reloaded { quiet: startup });
            }
            if self.process.is_some() {
                self.status = Status::Ready;
            }
        } else {
            let n = result.errors.len().max(1);
            for e in &result.errors {
                self.note(format!("{}({},{}): {} {}", e.file, e.line, e.col, e.code, e.message), true);
            }
            if result.errors.is_empty() {
                self.note(result.log.trim().to_string(), true);
            }
            self.status = if self.process.is_some() {
                Status::Broken(n)
            } else {
                Status::Off("El generador no compila; mira la consola.".into())
            };
        }
        self.last_compile = Some(result);
    }

    fn tick_process(&mut self, out: &mut Vec<Reply>) {
        let Some(p) = &self.process else { return };
        let events: Vec<Event> = p.events.try_iter().collect();
        for ev in events {
            match ev {
                Event::Log(line) => self.note(line, false),
                Event::Exited(why) => {
                    self.note(why.clone(), true);
                    self.process = None;
                    self.pending.clear();
                    self.status = Status::Off(why);
                }
                Event::Frame(frame) => self.on_frame(frame, out),
            }
        }
    }

    fn on_frame(&mut self, frame: bridge::Frame, out: &mut Vec<Reply>) {
        let Some(pending) = self.pending.remove(&frame.id) else { return };
        let error = frame.error();
        match pending {
            Pending::Hello => {
                if let Some(e) = error {
                    self.note(e.message, true);
                } else if let Ok(h) = serde_json::from_value::<proto::Hello>(frame.header) {
                    self.note(format!("Generador listo (Unity {}, protocolo {}).", h.unity_version, h.protocol), false);
                }
            }
            Pending::Catalog => match error {
                Some(e) => self.note(format!("{}: {}", e.message, e.detail), true),
                None => match serde_json::from_value::<Catalog>(frame.header) {
                    Ok(c) => {
                        if self.compile_rx.is_none() && !matches!(self.status, Status::Broken(_)) {
                            self.status = Status::Ready;
                        }
                        out.push(Reply::Catalog(std::sync::Arc::new(c)));
                    }
                    Err(e) => self.note(format!("Catálogo ilegible: {e}"), true),
                },
            },
            Pending::Validate(revision, grid) => {
                let result = match error {
                    Some(e) => Err(e),
                    None => serde_json::from_value::<BuildHeader>(frame.header)
                        .map_err(|e| e.to_string())
                        .and_then(|h| proto::decode_build(h, &frame.payload, 0, Some(*grid)).map_err(|e| e.to_string()))
                        .map_err(|e| ErrorInfo { message: "La validación ha respondido algo que no entiendo.".into(), detail: e }),
                };
                out.push(Reply::Validated(revision, result));
            }
            Pending::Export => {
                out.push(Reply::Exported(match error {
                    Some(e) => Err(e),
                    None => Ok(frame.header),
                }));
            }
            Pending::Tools => match error {
                // Un generador anterior a la fase 3 no tiene herramientas: no es un fallo.
                Some(_) => {}
                None => match serde_json::from_value::<Vec<proto::ToolInfo>>(frame.header["tools"].clone()) {
                    Ok(t) => out.push(Reply::Tools(t)),
                    Err(e) => self.note(format!("Catálogo de herramientas ilegible: {e}"), true),
                },
            },
            Pending::Furniture => match error {
                Some(_) => {}
                None => match serde_json::from_value::<proto::FurnitureCatalog>(frame.header) {
                    Ok(c) => out.push(Reply::Furniture(c)),
                    Err(e) => self.note(format!("Catálogo de muebles ilegible: {e}"), true),
                },
            },
            Pending::Look => {
                if error.is_none()
                    && let Ok(roles) = serde_json::from_value(frame.header["roles"].clone())
                {
                    out.push(Reply::Look(roles));
                }
            }
            Pending::Tool(seq) => {
                if frame.id != self.latest_tool {
                    return; // hay una vista previa más nueva en camino
                }
                let result = match error {
                    Some(e) => Err(e),
                    None => serde_json::from_value::<BuildHeader>(frame.header)
                        .map_err(|e| e.to_string())
                        .and_then(|h| proto::decode_build(h, &frame.payload, 0, None).map_err(|e| e.to_string()))
                        .map(std::sync::Arc::new)
                        .map_err(|e| ErrorInfo { message: "El generador ha respondido algo que no entiendo.".into(), detail: e }),
                };
                out.push(Reply::Tool(seq, result));
            }
            Pending::Stamp(key) => {
                let result = match error {
                    Some(e) => Err(e),
                    None => proto::decode_stamp(&frame.header, &frame.payload)
                        .map_err(|e| ErrorInfo { message: "Pieza ilegible.".into(), detail: e.to_string() }),
                };
                out.push(Reply::Stamp(key, result));
            }
            Pending::Build(purpose, req) => {
                if purpose.urgent() && self.latest.get(&purpose) != Some(&frame.id) {
                    return; // hay una petición más nueva en camino
                }
                let result = match error {
                    Some(e) => {
                        self.note(format!("{} ({} · semilla {}): {}", e.message, req.kind, req.seed, e.detail), true);
                        Err(e)
                    }
                    None => serde_json::from_value::<BuildHeader>(frame.header)
                        .map_err(|e| e.to_string())
                        .and_then(|h| proto::decode_build(h, &frame.payload, req.seed, None).map_err(|e| e.to_string()))
                        .map(std::sync::Arc::new)
                        .map_err(|e| ErrorInfo { message: "El generador ha respondido algo que no entiendo.".into(), detail: e }),
                };
                out.push(Reply::Built(purpose, req, result));
            }
        }
    }

    pub fn compile_errors(&self) -> &[CompileError] {
        self.last_compile.as_ref().map_or(&[], |c| &c.errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_recompila_por_lo_que_usa_el_puente() {
        let a = Path::new("/x/a_game/Assets/_Game/Scripts/Generation/WorldGen1/Structures/City/CityHouse.cs");
        assert!(Host::relevant(a));
        assert!(Host::relevant(Path::new("/x/Assets/_Game/Scripts/Rendering/Terrain3D/StructurePalette.cs")));
        assert!(!Host::relevant(Path::new("/x/Assets/_Game/Scripts/UI/Hud/TerrainHudText.cs")));
        assert!(!Host::relevant(Path::new("/x/Assets/_Game/Scripts/World/Structures/VoxelGrid.cs.meta")));
    }
}
