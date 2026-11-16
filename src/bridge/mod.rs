//! Cliente del puente C# (`bridge/`): compilarlo con dotnet, lanzarlo como proceso hijo y hablarle por tramas
//! (`docs/contrato.md`). Todo lo lento va en hilos; la UI solo encola peticiones y recoge eventos con `poll`.

pub mod proto;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// Rutas del juego y del puente.
#[derive(Clone, Debug)]
pub struct Paths {
    pub agame_root: PathBuf,
    pub editor_root: PathBuf,
}

/// Fuentes del puente dentro del binario: una versión publicada no trae el repo, así que las escribe en la carpeta de
/// datos y las compila allí (`Paths::prepare`).
const BRIDGE_SOURCES: [(&str, &str); 9] = [
    ("bridge/AGame.StructureBridge.csproj", include_str!("../../bridge/AGame.StructureBridge.csproj")),
    ("bridge/src/Program.cs", include_str!("../../bridge/src/Program.cs")),
    ("bridge/src/Catalog.cs", include_str!("../../bridge/src/Catalog.cs")),
    ("bridge/src/Builder.cs", include_str!("../../bridge/src/Builder.cs")),
    ("bridge/src/Checks.cs", include_str!("../../bridge/src/Checks.cs")),
    ("bridge/src/Authored.cs", include_str!("../../bridge/src/Authored.cs")),
    ("bridge/src/Doc.cs", include_str!("../../bridge/src/Doc.cs")),
    ("bridge/src/Tools.cs", include_str!("../../bridge/src/Tools.cs")),
    ("bridge/src/Furniture.cs", include_str!("../../bridge/src/Furniture.cs")),
];

impl Paths {
    /// En desarrollo (hay checkout del repo) el puente se compila en `bridge/`; en una versión publicada, en la carpeta
    /// de datos, por versión.
    pub fn new(agame_root: PathBuf) -> Self {
        // dotnet resuelve las rutas relativas desde bridge/, no desde aquí: siempre absoluta.
        let agame_root = std::path::absolute(&agame_root).unwrap_or(agame_root);
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let editor_root = if repo.join("bridge/AGame.StructureBridge.csproj").is_file() {
            repo
        } else {
            crate::platform::data_dir().join(format!("bridge-{}", env!("CARGO_PKG_VERSION")))
        };
        Self { agame_root, editor_root }
    }

    /// Escribe las fuentes del puente si faltan o son de otra versión (no toca un checkout del repo).
    pub fn prepare(&self) -> std::io::Result<()> {
        for (rel, text) in BRIDGE_SOURCES {
            let path = self.editor_root.join(rel);
            if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
                continue;
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, text)?;
        }
        Ok(())
    }

    /// Carpeta de a_game por defecto: `AGAME_ROOT` o `../a_game` junto al editor.
    pub fn default_agame_root() -> PathBuf {
        std::env::var_os("AGAME_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../a_game"))
    }

    /// Comprueba que la carpeta es a_game y que Unity la ha abierto alguna vez. Mensaje para artistas si no.
    pub fn check_agame(root: &Path) -> Result<(), String> {
        if !root.join("Assets/_Game/Scripts/World/Structures/VoxelGrid.cs").is_file() {
            return Err("Esta carpeta no es la del juego (falta Assets/_Game/Scripts/World/Structures).".into());
        }
        if !root.join("Library/ScriptAssemblies/Unity.Mathematics.dll").is_file() {
            return Err("Abre el proyecto en Unity una vez: falta Library/ScriptAssemblies.".into());
        }
        Ok(())
    }

    pub fn csproj(&self) -> PathBuf {
        self.editor_root.join("bridge/AGame.StructureBridge.csproj")
    }

    pub fn dll(&self) -> PathBuf {
        self.editor_root.join("bridge/bin/Release/net8.0/AGame.StructureBridge.dll")
    }

    /// Carpeta con `UnityEngine.CoreModule.dll` para la versión de Unity del proyecto (Unity Hub en su sitio de cada
    /// sistema, o `UNITY_MANAGED`).
    pub fn unity_managed(&self) -> Result<PathBuf> {
        if let Some(dir) = std::env::var_os("UNITY_MANAGED").map(PathBuf::from) {
            return Ok(dir);
        }
        let text = std::fs::read_to_string(self.agame_root.join("ProjectSettings/ProjectVersion.txt"))
            .context("no encuentro ProjectSettings/ProjectVersion.txt")?;
        let version = text
            .lines()
            .find_map(|l| l.strip_prefix("m_EditorVersion:"))
            .map(str::trim)
            .context("ProjectVersion.txt sin m_EditorVersion")?;
        let home = crate::platform::home_dir();
        let candidates = [
            PathBuf::from(format!(
                "/Applications/Unity/Hub/Editor/{version}/Unity.app/Contents/Resources/Scripting/Managed/UnityEngine"
            )),
            PathBuf::from(format!(r"C:\Program Files\Unity\Hub\Editor\{version}\Editor\Data\Managed\UnityEngine")),
            home.join(format!("Unity/Hub/Editor/{version}/Editor/Data/Managed/UnityEngine")),
        ];
        candidates
            .into_iter()
            .find(|d| d.join("UnityEngine.CoreModule.dll").is_file())
            .with_context(|| format!("No está instalado Unity {version} (lo pide el proyecto)."))
    }
}

/// Ejecutable de dotnet: una app lanzada desde el Finder no hereda el PATH de la terminal.
pub fn dotnet() -> Option<PathBuf> {
    let exe = format!("dotnet{}", std::env::consts::EXE_SUFFIX);
    let known = [
        PathBuf::from("/usr/local/share/dotnet/dotnet"),
        PathBuf::from("/opt/homebrew/bin/dotnet"),
        PathBuf::from("/usr/local/bin/dotnet"),
        PathBuf::from("/usr/share/dotnet/dotnet"),
        PathBuf::from("/usr/lib/dotnet/dotnet"),
        PathBuf::from(r"C:\Program Files\dotnet\dotnet.exe"),
        crate::platform::home_dir().join(".dotnet").join(&exe),
    ];
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).map(|d| d.join(&exe)).collect::<Vec<_>>())
        .chain(known)
        .find(|p| p.is_file())
}

/// Error de compilación de C# (`file(line,col): error CSxxxx: mensaje`).
#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub code: String,
    pub message: String,
}

/// Errores de la salida de `dotnet build`, sin repetidos (MSBuild repite cada error en el resumen).
pub fn parse_compile_errors(output: &str) -> Vec<CompileError> {
    let mut out: Vec<CompileError> = Vec::new();
    for line in output.lines() {
        let Some((loc, rest)) = line.split_once("): error ") else { continue };
        let Some((file, pos)) = loc.trim().rsplit_once('(') else { continue };
        let mut nums = pos.split(',').map(|n| n.trim().parse::<u32>().unwrap_or(0));
        let (line_no, col) = (nums.next().unwrap_or(0), nums.next().unwrap_or(0));
        let (code, msg) = rest.split_once(": ").unwrap_or(("", rest));
        let message = msg.rsplit_once(" [").map_or(msg, |(m, _)| m).trim().to_string();
        let e = CompileError { file: file.trim().into(), line: line_no, col, code: code.trim().into(), message };
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// Resultado de una compilación del puente.
#[derive(Clone, Debug)]
pub struct CompileOutcome {
    pub ok: bool,
    pub errors: Vec<CompileError>,
    pub log: String,
    /// La DLL cambió (hay que relanzar el proceso).
    pub changed: bool,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// `dotnet build` del puente contra la carpeta del juego. Bloquea: llámalo desde un hilo.
pub fn compile(paths: &Paths) -> CompileOutcome {
    let fail = |log: String| CompileOutcome { ok: false, errors: Vec::new(), log, changed: false };
    let Some(dotnet) = dotnet() else { return fail("No encuentro dotnet: instala el SDK de .NET 8.".into()) };
    let managed = match paths.unity_managed() {
        Ok(m) => m,
        Err(e) => return fail(e.to_string()),
    };
    if let Err(e) = paths.prepare() {
        return fail(format!("No he podido preparar el generador en {}: {e}", paths.editor_root.display()));
    }
    let before = mtime(&paths.dll());
    let out = crate::platform::no_console(&mut Command::new(dotnet))
        .arg("build")
        .arg(paths.csproj())
        .args(["-c", "Release", "-nologo", "-v", "q", "-clp:NoSummary"])
        .arg(format!("-p:AGameRoot={}", paths.agame_root.display()))
        .arg(format!("-p:UnityManaged={}", managed.display()))
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .env("DOTNET_NOLOGO", "1")
        .output();
    match out {
        Err(e) => fail(format!("No he podido lanzar dotnet: {e}")),
        Ok(o) => {
            let log = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let errors = parse_compile_errors(&log);
            CompileOutcome { ok: o.status.success(), errors, log, changed: mtime(&paths.dll()) != before }
        }
    }
}

/// Respuesta del puente: cabecera JSON y carga binaria.
#[derive(Debug)]
pub struct Frame {
    pub id: u64,
    pub header: Value,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Error en lenguaje llano si `ok` es false.
    pub fn error(&self) -> Option<proto::ErrorInfo> {
        if self.header["ok"].as_bool() == Some(true) {
            return None;
        }
        Some(serde_json::from_value(self.header["error"].clone()).unwrap_or(proto::ErrorInfo {
            message: "El generador ha respondido algo que no entiendo.".into(),
            detail: self.header.to_string(),
        }))
    }
}

/// Lo que llega del proceso del puente.
#[derive(Debug)]
pub enum Event {
    Frame(Frame),
    Log(String),
    /// El proceso terminó (o se rompió la tubería).
    Exited(String),
}

/// Proceso vivo del puente en modo `serve`.
pub struct Process {
    child: Child,
    stdin: ChildStdin,
    next_id: u64,
    pub events: Receiver<Event>,
}

fn read_frame(r: &mut impl Read) -> std::io::Result<(Value, Vec<u8>)> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let mut header = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut header)?;
    r.read_exact(&mut len)?;
    let mut payload = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut payload)?;
    let header = serde_json::from_slice(&header).map_err(std::io::Error::other)?;
    Ok((header, payload))
}

impl Process {
    /// Lanza `serve`. `wake` se llama al llegar cada evento (para pedir un repintado).
    pub fn spawn(paths: &Paths, wake: impl Fn() + Send + Clone + 'static) -> Result<Self> {
        let dotnet = dotnet().context("No encuentro dotnet: instala el SDK de .NET 8.")?;
        let mut child = crate::platform::no_console(&mut Command::new(dotnet))
            .arg(paths.dll())
            .arg("serve")
            .env("AGAME_ROOT", &paths.agame_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("no he podido lanzar el generador")?;
        let stdin = child.stdin.take().context("sin stdin")?;
        let stdout = child.stdout.take().context("sin stdout")?;
        let stderr = child.stderr.take().context("sin stderr")?;
        let (tx, rx) = channel();
        spawn_reader(stdout, tx.clone(), wake.clone());
        thread::spawn({
            let wake = wake.clone();
            move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = tx.send(Event::Log(line));
                    wake();
                }
            }
        });
        Ok(Self { child, stdin, next_id: 1, events: rx })
    }

    /// Encola una petición; devuelve su id. `payload` va tras la línea JSON (`payloadLen`).
    pub fn request(&mut self, op: &str, mut body: Value, payload: &[u8]) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        if !body.is_object() {
            body = json!({});
        }
        body["id"] = json!(id);
        body["op"] = json!(op);
        if !payload.is_empty() {
            body["payloadLen"] = json!(payload.len());
        }
        let mut line = serde_json::to_vec(&body)?;
        line.push(b'\n');
        self.stdin.write_all(&line)?;
        self.stdin.write_all(payload)?;
        self.stdin.flush()?;
        Ok(id)
    }
}

fn spawn_reader(stdout: impl Read + Send + 'static, tx: Sender<Event>, wake: impl Fn() + Send + 'static) {
    thread::spawn(move || {
        let mut r = BufReader::with_capacity(1 << 20, stdout);
        loop {
            match read_frame(&mut r) {
                Ok((header, payload)) => {
                    let id = header["id"].as_u64().unwrap_or(0);
                    let _ = tx.send(Event::Frame(Frame { id, header, payload }));
                }
                Err(e) => {
                    let why = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        "El generador se ha cerrado.".to_string()
                    } else {
                        format!("Se ha roto la comunicación con el generador: {e}")
                    };
                    let _ = tx.send(Event::Exited(why));
                    wake();
                    return;
                }
            }
            wake();
        }
    });
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.request("shutdown", json!({}), &[]);
        // shortcut: no esperamos la respuesta de shutdown; si el proceso se cuelga, lo matamos al soltarlo.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errores_de_dotnet_sin_repetidos() {
        let log = "\
/a/Foo.cs(12,5): error CS1002: ; expected [/b/AGame.StructureBridge.csproj]
/a/Foo.cs(12,5): error CS1002: ; expected [/b/AGame.StructureBridge.csproj]
/a/Bar.cs(3,1): warning CS0168: algo [/b/x.csproj]
/a/Baz.cs(7,9): error CS0103: The name 'x' does not exist [/b/x.csproj]";
        let e = parse_compile_errors(log);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].file, "/a/Foo.cs");
        assert_eq!((e[0].line, e[0].col), (12, 5));
        assert_eq!(e[0].code, "CS1002");
        assert_eq!(e[0].message, "; expected");
        assert_eq!(e[1].message, "The name 'x' does not exist");
    }

    #[test]
    fn el_binario_lleva_todo_el_puente() {
        // Un .cs nuevo en bridge/src que no esté en BRIDGE_SOURCES rompería la versión publicada.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("bridge/src");
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let rel = format!("bridge/src/{}", e.file_name().to_string_lossy());
            if rel.ends_with(".cs") {
                assert!(BRIDGE_SOURCES.iter().any(|(r, _)| *r == rel), "falta {rel} en BRIDGE_SOURCES");
            }
        }
        let paths = Paths { agame_root: PathBuf::new(), editor_root: std::env::temp_dir().join(format!("puente-{}", std::process::id())) };
        paths.prepare().unwrap();
        assert!(paths.csproj().is_file());
        paths.prepare().unwrap(); // la segunda vez no reescribe nada
        std::fs::remove_dir_all(&paths.editor_root).unwrap();
    }

    #[test]
    fn lee_tramas() {
        let header = br#"{"id":7,"ok":true}"#;
        let mut buf = (header.len() as u32).to_le_bytes().to_vec();
        buf.extend(header);
        buf.extend(3u32.to_le_bytes());
        buf.extend([1, 2, 3]);
        let (h, p) = read_frame(&mut buf.as_slice()).unwrap();
        assert_eq!(h["id"], 7);
        assert_eq!(p, [1, 2, 3]);
    }
}
