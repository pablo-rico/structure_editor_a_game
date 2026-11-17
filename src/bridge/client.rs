//! Cliente síncrono del puente para lo que no tiene ventana (`cantero`, tests con el generador real): compila si
//! falta la DLL, lanza `serve` y espera cada respuesta. La app usa `Host`, que no bloquea.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::proto::{self, BuildHeader, BuildRequest, Catalog, FurnitureCatalog, Stamp, ToolInfo};
use super::{Event, Frame, Paths, Process, compile};
use crate::agvox::Replaces;
use crate::model::Structure;

/// Plazo de una respuesta (una catedral tarda ~100 ms; el primer `catalog`, ~1 s).
const TIMEOUT: Duration = Duration::from_secs(90);

pub struct Client {
    process: Process,
    pub paths: Paths,
}

impl Client {
    /// Lanza el generador. Con `rebuild` compila siempre (el árbol de trabajo del juego puede haber cambiado); sin él,
    /// solo si aún no hay DLL.
    pub fn start(paths: Paths, rebuild: bool) -> Result<Self> {
        Paths::check_agame(&paths.agame_root).map_err(anyhow::Error::msg)?;
        if rebuild || !paths.dll().is_file() {
            let out = compile(&paths);
            if !out.ok {
                let first = out.errors.first().map_or_else(
                    || out.log.lines().last().unwrap_or("").to_string(),
                    |e| format!("{}({}): {}", e.file, e.line, e.message),
                );
                bail!("el generador no compila: {first}");
            }
        }
        let process = Process::spawn(&paths, || {})?;
        Ok(Self { process, paths })
    }

    fn wait(&self, id: u64) -> Result<Frame> {
        let end = Instant::now() + TIMEOUT;
        while Instant::now() < end {
            match self.process.events.recv_timeout(Duration::from_millis(200)) {
                Ok(Event::Frame(f)) if f.id == id => return Ok(f),
                Ok(Event::Exited(why)) => bail!("{why}"),
                _ => {}
            }
        }
        bail!("el generador no responde")
    }

    /// Petición y respuesta, sin mirar `ok` (quien llama decide qué hacer con el error).
    pub fn ask_raw(&mut self, op: &str, body: Value, payload: &[u8]) -> Result<Frame> {
        let id = self.process.request(op, body, payload)?;
        self.wait(id)
    }

    /// Petición y respuesta; `ok: false` es un error con el mensaje del generador.
    pub fn ask(&mut self, op: &str, body: Value, payload: &[u8]) -> Result<Frame> {
        let f = self.ask_raw(op, body, payload)?;
        if let Some(e) = f.error() {
            // El detalle de un fallo del generador es su traza de .NET: no ayuda a quien escribe una receta.
            let trace = e.detail.contains("Exception") || e.detail.contains('\n');
            bail!("{}{}", e.message, if e.detail.is_empty() || trace { String::new() } else { format!(" ({})", e.detail) });
        }
        Ok(f)
    }

    pub fn catalog(&mut self) -> Result<Catalog> {
        serde_json::from_value(self.ask("catalog", json!({}), &[])?.header).context("catálogo del generador")
    }

    pub fn tools(&mut self) -> Result<Vec<ToolInfo>> {
        serde_json::from_value(self.ask("tools", json!({}), &[])?.header["tools"].clone()).context("herramientas")
    }

    pub fn furniture(&mut self) -> Result<FurnitureCatalog> {
        serde_json::from_value(self.ask("furniture", json!({}), &[])?.header).context("muebles")
    }

    /// Papeles de material de un Aspecto (`look`).
    pub fn look(&mut self, style: &str, district: &str, tier: &str) -> Result<Value> {
        Ok(self.ask("look", json!({ "style": style, "district": district, "tier": tier }), &[])?.header["roles"].clone())
    }

    pub fn build(&mut self, req: &BuildRequest) -> Result<Structure> {
        let f = self.ask("build", serde_json::to_value(req)?, &[])?;
        let h: BuildHeader = serde_json::from_value(f.header).context("cabecera de build")?;
        proto::decode_build(h, &f.payload, req.seed, None)
    }

    /// Navegación, huella y comprobaciones de un documento (la rejilla es la de `s`).
    pub fn validate(&mut self, s: &Structure, replaces: Option<&Replaces>) -> Result<Structure> {
        let (header, payload) = proto::doc_request(s, replaces);
        let f = self.ask("validate", header, &payload)?;
        let h: BuildHeader = serde_json::from_value(f.header).context("cabecera de validate")?;
        proto::decode_build(h, &f.payload, s.seed, Some(s.grid.clone()))
    }

    /// Herramienta de construcción sobre el documento entero. `extra` lleva `name`, `params`, `roles`, `target`,
    /// `floorY` y `style`. Devuelve el documento resultante.
    pub fn tool(&mut self, s: &Structure, replaces: Option<&Replaces>, extra: Value) -> Result<Structure> {
        let (mut header, payload) = proto::doc_request(s, replaces);
        if let (Some(h), Value::Object(e)) = (header.as_object_mut(), extra) {
            h.extend(e);
        }
        let f = self.ask("tool", header, &payload)?;
        let h: BuildHeader = serde_json::from_value(f.header).context("cabecera de tool")?;
        proto::decode_build(h, &f.payload, s.seed, None)
    }

    /// Mueble del juego recortado (`stamp`).
    pub fn stamp(&mut self, piece: &str, facing: i32, width: Option<i32>, seed: i32) -> Result<Stamp> {
        let mut body = json!({ "piece": piece, "facing": facing, "seed": seed });
        if let Some(w) = width {
            body["width"] = json!(w);
        }
        let f = self.ask("stamp", body, &[])?;
        proto::decode_stamp(&f.header, &f.payload)
    }
}
