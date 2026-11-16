//! Los modelos de vóxeles de un a_game de verdad (`AGAME_ROOT` o `../a_game`): abrir y guardar cada uno sin tocarlo
//! deja el mismo texto, salvo la marca de editado. Ignorado por defecto: `cargo test --test models_live -- --ignored`.

use std::path::{Path, PathBuf};

use structure_editor_a_game::vmodel::{self, EDITED_MARK};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with("_model.json"))
        {
            out.push(p);
        }
    }
}

#[test]
#[ignore]
fn los_280_ida_y_vuelta() {
    let root = std::env::var("AGAME_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../a_game"));
    let mut files = Vec::new();
    collect(&root.join("Assets/_Game/Art"), &mut files);
    assert!(files.len() >= 280, "solo he visto {} modelos", files.len());
    let mut bad = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let want = text.replacen("\"_\": \"", &format!("\"_\": \"{EDITED_MARK}"), 1);
        match vmodel::load(&text).and_then(|(m, s)| vmodel::save(&m, &s)) {
            Ok(got) if got == want => {}
            Ok(got) => {
                let line = got.lines().zip(want.lines()).position(|(a, b)| a != b);
                bad.push(format!(
                    "{}: distinto en la línea {:?}",
                    f.display(),
                    line.map(|l| l + 1)
                ));
            }
            Err(e) => bad.push(format!("{}: {e:#}", f.display())),
        }
    }
    assert!(
        bad.is_empty(),
        "{} de {} no vuelven iguales:\n{}",
        bad.len(),
        files.len(),
        bad.join("\n")
    );
}
