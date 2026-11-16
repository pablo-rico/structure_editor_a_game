//! Prueba de punta a punta con el puente real: compilar, lanzar, catálogo y una construcción.
//! Necesita dotnet, Unity y a_game: `cargo test --test bridge_live -- --ignored`.

use std::time::{Duration, Instant};

use structure_editor_a_game::bridge::proto::{self, BuildHeader, BuildRequest, Catalog};
use structure_editor_a_game::bridge::{self, Event, Paths, Process};

fn wait(p: &Process, secs: u64) -> bridge::Frame {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        match p.events.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Frame(f)) => return f,
            Ok(Event::Exited(why)) => panic!("{why}"),
            _ => {}
        }
    }
    panic!("el puente no responde");
}

#[test]
#[ignore]
fn compila_lanza_y_construye() {
    let paths = Paths::new(Paths::default_agame_root());
    Paths::check_agame(&paths.agame_root).unwrap();
    let out = bridge::compile(&paths);
    assert!(out.ok, "no compila: {:?}\n{}", out.errors, out.log);
    let mut p = Process::spawn(&paths, || {}).unwrap();
    p.request("catalog", serde_json::json!({}), &[]).unwrap();
    let f = wait(&p, 30);
    assert!(f.error().is_none());
    let cat: Catalog = serde_json::from_value(f.header).unwrap();
    let home = cat.kind("Home").unwrap();
    let req = BuildRequest {
        kind: "Home".into(),
        style: home.styles[0].clone(),
        fields: Default::default(),
        seed: 1234,
        width: home.size.unwrap().min[0],
        depth: home.size.unwrap().min[1],
        facing: 3,
        foundation: 1,
        damage: "Intact".into(),
        dynamic: 0,
        party: 0,
        procedural: false,
    };
    p.request("build", serde_json::to_value(&req).unwrap(), &[]).unwrap();
    let f = wait(&p, 30);
    assert!(f.error().is_none(), "{:?}", f.error());
    let h: BuildHeader = serde_json::from_value(f.header).unwrap();
    let s = proto::decode_build(h, &f.payload, req.seed, None).unwrap();
    assert!(s.grid.data.iter().any(|&m| m != 0));
    assert_eq!(s.grid.cells(), [req.width, req.depth]);
}
