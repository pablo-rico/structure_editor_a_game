//! Fase 3 de punta a punta con el generador real: herramientas de construcción, muebles y sustituir un diseño del juego.
//! Necesita un a_game con estructuras a mano: `AGAME_ROOT=../a_game-autoria cargo test --test phase3_live -- --ignored`.
//! Deja `ArtSource/Structures` como estaba.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use structure_editor_a_game::agvox::{self, Meta, Replaces};
use structure_editor_a_game::bridge::proto::{self, BuildHeader, BuildRequest, Catalog, FurnitureCatalog, ToolInfo};
use structure_editor_a_game::bridge::{self, Event, Paths, Process};
use structure_editor_a_game::editor::{next_id, source_dir};
use structure_editor_a_game::model::Structure;

fn wait(p: &Process) -> bridge::Frame {
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end {
        match p.events.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Frame(f)) => return f,
            Ok(Event::Exited(why)) => panic!("{why}"),
            _ => {}
        }
    }
    panic!("el puente no responde");
}

fn ask(p: &mut Process, op: &str, body: serde_json::Value, payload: &[u8]) -> bridge::Frame {
    p.request(op, body, payload).unwrap();
    wait(p)
}

fn start(paths: &Paths) -> (Process, Catalog) {
    let out = bridge::compile(paths);
    assert!(out.ok, "no compila: {:?}", out.errors);
    let mut p = Process::spawn(paths, || {}).unwrap();
    let cat = serde_json::from_value(ask(&mut p, "catalog", json!({}), &[]).header).unwrap();
    (p, cat)
}

fn build(p: &mut Process, req: &BuildRequest) -> (BuildHeader, Structure) {
    let f = ask(p, "build", serde_json::to_value(req).unwrap(), &[]);
    assert!(f.error().is_none(), "{:?}", f.error());
    let h: BuildHeader = serde_json::from_value(f.header).unwrap();
    let s = proto::decode_build(h.clone(), &f.payload, req.seed, None).unwrap();
    (h, s)
}

fn doc(s: &Structure) -> (serde_json::Value, Vec<u8>) {
    proto::doc_request(s, None)
}

#[test]
#[ignore]
fn herramientas_muebles_y_sustituciones() {
    let paths = Paths::new(Paths::default_agame_root());
    let (mut p, cat) = start(&paths);
    assert_eq!(serde_json::from_value::<proto::Hello>(ask(&mut p, "hello", json!({}), &[]).header).unwrap().protocol, 2);
    assert!(!cat.districts.is_empty() && !cat.tiers.is_empty(), "el catálogo trae barrios y clases");

    // Herramientas: cada una sobre la Casa de las fixtures responde (bien, o con un mensaje claro).
    let tools: Vec<ToolInfo> = serde_json::from_value(ask(&mut p, "tools", json!({}), &[]).header["tools"].clone()).unwrap();
    assert!(tools.len() >= 10, "{} herramientas", tools.len());
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let (_, home) = proto::load_fixture(&fixtures, "home_northern").unwrap();
    let roles = ask(&mut p, "look", json!({"style": "Northern", "district": cat.districts[0].name, "tier": cat.tiers[0].name}), &[]).header["roles"].clone();
    assert!(roles.is_object(), "look da papeles");
    for t in &tools {
        let target = match t.target.as_str() {
            "rect" | "line" => json!({"rect": [0, 0, 1, 0]}),
            _ => json!({"cell": [1, 1], "dir": 3, "y": 9}),
        };
        let params: serde_json::Map<String, serde_json::Value> = t.params.iter().map(|f| (f.key.clone(), json!(f.default))).collect();
        let (mut h, payload) = doc(&home);
        h["name"] = json!(t.id);
        h["params"] = json!(params);
        h["roles"] = roles.clone();
        h["target"] = target;
        h["floorY"] = json!(8);
        let f = ask(&mut p, "tool", h, &payload);
        match f.error() {
            Some(e) => assert!(!e.message.is_empty(), "{}: error sin mensaje", t.id),
            None => {
                let hb: BuildHeader = serde_json::from_value(f.header).unwrap();
                let s = proto::decode_build(hb, &f.payload, 0, None).unwrap();
                assert!(s.grid.size[1] >= home.grid.size[1], "{}", t.id);
            }
        }
    }

    // Muebles: catálogo y una cama girada.
    let furn: FurnitureCatalog = serde_json::from_value(ask(&mut p, "furniture", json!({}), &[]).header).unwrap();
    assert!(furn.pieces.len() >= 50, "{} muebles", furn.pieces.len());
    let bed = furn.pieces.iter().find(|x| x.id == "Bed").expect("hay cama");
    let f = ask(&mut p, "stamp", json!({"piece": bed.id, "facing": 0, "seed": 0}), &[]);
    assert!(f.error().is_none(), "{:?}", f.error());
    let stamp = proto::decode_stamp(&f.header, &f.payload).unwrap();
    assert!(stamp.grid.data.iter().any(|&m| m != 0));
    assert!(stamp.interactables.iter().any(|i| i.kind == "Bed"));

    // Sustituir el Ayuntamiento norteño (diseño 0, 10×10): el juego usa el editado; `procedural` da el original.
    let req = BuildRequest {
        kind: "TownHall".into(),
        style: "Northern".into(),
        fields: [("design".to_string(), 0)].into(),
        seed: 7,
        width: 10,
        depth: 10,
        facing: 3,
        foundation: 1,
        damage: "Intact".into(),
        dynamic: 0,
        party: 0,
        procedural: true,
    };
    let (_, original) = build(&mut p, &req);
    let mut edited = original.clone();
    let top = edited.grid.size[1] - 1;
    edited.grid.set(1, top, 1, cat.materials.iter().position(|m| m == "Gold").unwrap() as u8);
    let dir = source_dir(&paths.agame_root);
    let id = next_id(&dir, &cat.materials);
    let meta = Meta {
        id: Some(id),
        name: "Ayuntamiento de prueba".into(),
        style: "Northern".into(),
        replaces: Some(Replaces { kind: "TownHall".into(), style: "Northern".into(), design: 0, size: [10, 10], party: 0, ..Default::default() }),
        look: None,
        recipe: None,
    };
    let file = dir.join("ayuntamiento_de_prueba.agvox");
    std::fs::write(&file, agvox::save(&meta, &edited, &cat.materials).unwrap()).unwrap();
    let exported = ask(&mut p, "export", json!({"sourceDir": dir}), &[]).header;
    let ok = exported["exported"].as_bool() == Some(true);
    let mut checked = false;
    if ok {
        drop(p);
        let (mut p2, _) = start(&paths);
        let (h, used) = build(&mut p2, &BuildRequest { procedural: false, ..req.clone() });
        let (h0, orig) = build(&mut p2, &req);
        assert!(h.override_by.is_some(), "el juego usa la sustitución");
        assert!(h0.override_by.is_none(), "procedural da el original");
        assert_eq!(used.grid.data, edited.grid.data);
        assert_eq!(orig.grid.data, original.grid.data);
        checked = true;
        p = p2;
    }
    std::fs::remove_file(&file).unwrap();
    let after = ask(&mut p, "export", json!({"sourceDir": dir}), &[]).header;
    drop(p);
    let _ = bridge::compile(&paths);
    assert!(ok && checked, "export rechazado: {exported}");
    assert_eq!(after["exported"].as_bool(), Some(true));
}
