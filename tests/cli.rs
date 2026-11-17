//! `cantero` de punta a punta, como lo usa un agente: lanzar el binario y leer lo que imprime. Los modelos no
//! necesitan el generador; dibujar sí necesita tarjeta gráfica (`cargo test --test cli -- --ignored`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn cantero(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_cantero")).args(args).output().expect("no se puede lanzar cantero");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn json(args: &[&str]) -> serde_json::Value {
    let (ok, stdout, stderr) = cantero(args);
    assert!(ok, "{args:?}\n{stderr}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{args:?} no ha dado JSON ({e}):\n{stdout}"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cantero-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn recipe() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/recetas/pozo.rhai").to_string_lossy().into_owned()
}

#[test]
fn receta_info_y_capas() {
    let dir = scratch("humo");
    let out = dir.to_string_lossy().into_owned();
    // El juego no hace falta para un modelo en borrador: se le da una carpeta cualquiera.
    let run = json(&["run", &recipe(), "--salida", &out, "--juego", &out]);
    assert_eq!(run["kind"], "model");
    assert_eq!(run["size"], serde_json::json!([24, 30, 24]));
    assert_eq!(run["change"]["new"], true);
    assert!(run["solids"].as_u64().unwrap() > 3000);
    let model = dir.join("pozo_model.json").to_string_lossy().into_owned();
    assert_eq!(run["written"].as_str().map(PathBuf::from), Some(PathBuf::from(&model)));

    let info = json(&["info", &model]);
    assert_eq!(info["fingerprint"], run["fingerprint"]);
    assert_eq!(info["name"], "pozo");
    assert!(info["note"].as_str().unwrap().contains("pozo.rhai"));
    assert!(info["colors"].as_array().unwrap().len() >= 8, "piedra, madera, teja y sus tonos");

    // Repetir la receta no cambia nada.
    assert_eq!(json(&["run", &recipe(), "--salida", &out, "--juego", &out])["unchanged"], true);

    let (ok, slices, _) = cantero(&["capas", &model, "--caja", "0,8,0,24,9,24"]);
    assert!(ok && slices.contains("y = 8") && slices.contains("«.» = aire"), "{slices}");
    assert_eq!(slices.lines().filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit())).count(), 24 + 2, "24 filas y la regla");

    // Los fallos van por la salida de errores, en llano, y el proceso lo dice.
    let (ok, stdout, stderr) = cantero(&["capas", &model]);
    assert!(!ok && stdout.is_empty() && stderr.starts_with("cantero: ") && stderr.contains("--caja"), "{stderr}");
    let (ok, _, stderr) = cantero(&["info", "no-existe_model.json"]);
    assert!(!ok && stderr.contains("no puedo leer"), "{stderr}");
    let (ok, _, stderr) = cantero(&["pintar"]);
    assert!(!ok && stderr.contains("--ayuda"), "{stderr}");

    let (ok, api, _) = cantero(&["api"]);
    assert!(ok && api.contains("new_model(nombre, sx, sy, sz)") && api.contains("tool(nombre, donde)"));
    let (ok, help, _) = cantero(&[]);
    assert!(ok && help.contains("cantero run"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[ignore]
fn lamina_de_la_receta() {
    let dir = scratch("lamina");
    let out = dir.to_string_lossy().into_owned();
    let png = dir.join("pozo.png").to_string_lossy().into_owned();
    let run = json(&["run", &recipe(), "--salida", &out, "--juego", &out, "--render", "--out", &png, "--tam", "800"]);
    assert_eq!(run["image"].as_str().map(PathBuf::from), Some(PathBuf::from(&png)));
    let img = image::open(&png).unwrap().to_rgba8();
    assert_eq!((img.width(), img.height()), (800, 600));
    // No es un rectángulo liso: hay modelo y hay reglas.
    let colors: std::collections::HashSet<[u8; 4]> = img.pixels().map(|p| p.0).collect();
    assert!(colors.len() > 30, "{} colores", colors.len());
    // Una vista suelta y una región.
    let one = json(&["render", &dir.join("pozo_model.json").to_string_lossy(), "--out", &png, "--vista", "frente", "--caja", "0,0,0,24,10,24"]);
    assert_eq!(one["views"], serde_json::json!(["frente"]));
    std::fs::remove_dir_all(&dir).unwrap();
}
