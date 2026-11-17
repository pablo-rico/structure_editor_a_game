//! Prueba de punta a punta con el puente real: compilar, lanzar, catálogo y una construcción.
//! Necesita dotnet, Unity y a_game: `cargo test --test bridge_live -- --ignored`.

use structure_editor_a_game::bridge::Paths;
use structure_editor_a_game::bridge::client::Client;
use structure_editor_a_game::bridge::proto::BuildRequest;

#[test]
#[ignore]
fn compila_lanza_y_construye() {
    let mut c = Client::start(Paths::new(Paths::default_agame_root()), true).unwrap();
    let cat = c.catalog().unwrap();
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
    let s = c.build(&req).unwrap();
    assert!(s.grid.data.iter().any(|&m| m != 0));
    assert_eq!(s.grid.cells(), [req.width, req.depth]);
    // Un documento se valida tal cual: la navegación sale de sus sub-cubos.
    let v = c.validate(&s, None).unwrap();
    assert_eq!(v.grid.data, s.grid.data);
    assert_eq!(v.nav.cells_x, req.width);
    assert!(c.tools().unwrap().len() >= 10);
}
