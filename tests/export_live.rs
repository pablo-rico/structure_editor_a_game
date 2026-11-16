//! Exportar al juego de punta a punta: `.agvox` → `export` → recompilar → «Hechas a mano» en el catálogo → construir
//! y comparar con el original. Necesita un a_game con estructuras a mano (la rama `editor-estructuras-autoria`):
//! `AGAME_ROOT=../a_game-autoria cargo test --test export_live -- --ignored`. Deja la carpeta como estaba.

use std::path::Path;
use std::time::{Duration, Instant};

use structure_editor_a_game::agvox::{self, Meta};
use structure_editor_a_game::bridge::proto::{self, BuildHeader, BuildRequest, Catalog};
use structure_editor_a_game::bridge::{self, Event, Paths, Process};
use structure_editor_a_game::editor::{next_id, source_dir};

fn wait(p: &Process) -> bridge::Frame {
    let end = Instant::now() + Duration::from_secs(60);
    while Instant::now() < end {
        match p.events.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Frame(f)) => return f,
            Ok(Event::Exited(why)) => panic!("{why}"),
            _ => {}
        }
    }
    panic!("el puente no responde");
}

fn start(paths: &Paths) -> (Process, Catalog) {
    let out = bridge::compile(paths);
    assert!(out.ok, "no compila: {:?}", out.errors);
    let mut p = Process::spawn(paths, || {}).unwrap();
    p.request("catalog", serde_json::json!({}), &[]).unwrap();
    let cat = serde_json::from_value(wait(&p).header).unwrap();
    (p, cat)
}

fn export(p: &mut Process, dir: &Path) -> serde_json::Value {
    p.request("export", serde_json::json!({ "sourceDir": dir }), &[]).unwrap();
    let f = wait(p);
    assert!(f.error().is_none(), "{:?}", f.error());
    f.header
}

#[test]
#[ignore]
fn exportar_y_construir() {
    let paths = Paths::new(Paths::default_agame_root());
    let dir = source_dir(&paths.agame_root);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let (req, home) = proto::load_fixture(&fixtures, "home_northern").unwrap();
    let (mut p, cat) = start(&paths);
    let id = next_id(&dir, &cat.materials);
    let meta = Meta { id: Some(id), name: "Prueba del editor".into(), style: req.style.clone(), ..Default::default() };
    let file = dir.join("prueba_del_editor.agvox");
    std::fs::write(&file, agvox::save(&meta, &home, &cat.materials).unwrap()).unwrap();

    let h = export(&mut p, &dir);
    let ok = h["exported"].as_bool() == Some(true);
    if ok {
        drop(p);
        let (mut p2, cat2) = start(&paths);
        let kind = cat2.kind("Authored").expect("«Hechas a mano» en el catálogo");
        assert_eq!(kind.group, "authored");
        let build = BuildRequest {
            kind: "Authored".into(),
            style: req.style.clone(),
            fields: [("id".to_string(), id as i64)].into(),
            seed: id as i32,
            width: home.grid.cells()[0],
            depth: home.grid.cells()[1],
            facing: 3,
            foundation: 1,
            damage: "Intact".into(),
            dynamic: 0,
            party: 0,
            procedural: false,
        };
        p2.request("build", serde_json::to_value(&build).unwrap(), &[]).unwrap();
        let f = wait(&p2);
        assert!(f.error().is_none(), "{:?}", f.error());
        let hb: BuildHeader = serde_json::from_value(f.header).unwrap();
        let s = proto::decode_build(hb, &f.payload, build.seed, None).unwrap();
        assert_eq!(s.grid.data, home.grid.data, "la hecha a mano no coincide con la original");
        p = p2;
    }
    // Limpieza: sin la fuente, export borra lo generado.
    std::fs::remove_file(&file).unwrap();
    let after = export(&mut p, &dir);
    drop(p);
    assert!(ok, "export rechazado: {h}");
    assert_eq!(after["exported"].as_bool(), Some(true));
    let _ = bridge::compile(&paths);
}
