//! Las fixtures que escribe el puente (`AGame.StructureBridge dump fixtures`) se leen con los tipos del contrato.

use std::path::Path;

use structure_editor_a_game::bridge::proto::{load_catalog_fixture, load_fixture};

const SAMPLES: [&str; 5] = ["home_northern", "shop_arabic", "walltower_ports", "schooltower", "harbour"];

fn dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures"))
}

#[test]
fn las_fixtures_casan_con_su_peticion_y_su_carga() {
    let catalog = load_catalog_fixture(dir()).unwrap();
    for name in SAMPLES {
        let (req, s) = load_fixture(dir(), name).unwrap();
        let [sx, sy, sz] = s.grid.size;
        assert_eq!(s.grid.data.len(), (sx * sy * sz) as usize, "{name}: rejilla");
        assert_eq!([s.nav.cells_x, s.nav.cells_z], [sx / 8, sz / 8], "{name}: celdas");
        assert_eq!(s.nav.levels, sy / 8 + 1, "{name}: niveles");
        assert_eq!(s.nav.dirs, ["+X", "-X", "+Z", "-Z"], "{name}: orden de dirs");
        assert_eq!(s.band_starts.len(), s.band_floors.len(), "{name}: franjas");
        assert!(s.grid.data.iter().all(|&m| (m as usize) < catalog.materials.len()), "{name}: material desconocido");
        let kind = catalog.kind(&req.kind).unwrap_or_else(|| panic!("{name}: {} no está en el catálogo", req.kind));
        // Sin girar (facing 3), la huella pedida es la rejilla.
        if let (Some(_), 3) = (kind.size, req.facing) {
            assert_eq!([req.width * 8, req.depth * 8], [sx, sz], "{name}: huella");
        }
        if let Some(f) = &s.footprint {
            assert_eq!(f.len(), (s.nav.cells_x * s.nav.cells_z) as usize, "{name}: huella con huecos");
        }
        for p in &s.parts {
            let b = p.bbox;
            assert!(b[0] < b[3] && b[1] < b[4] && b[2] < b[5] && b[3] <= sx && b[4] <= sy && b[5] <= sz, "{name}: caja {b:?}");
        }
    }
    let (_, school) = load_fixture(dir(), "schooltower").unwrap();
    assert!(school.theme.is_some(), "la torre de escuela lleva tema");
    assert_eq!(catalog.palette_for(school.theme).len(), catalog.materials.len());
    let (_, tower) = load_fixture(dir(), "walltower_ports").unwrap();
    assert!(tower.footprint.is_some(), "la torre de muralla no llena su caja");
}

#[test]
fn el_catalogo_trae_todo_lo_que_pinta_el_editor() {
    let c = load_catalog_fixture(dir()).unwrap();
    assert_eq!(c.palette.len(), c.materials.len());
    assert_eq!(c.elements.len(), 14);
    assert!(c.elements.iter().all(|e| e.palette.len() == c.materials.len()));
    assert_eq!(c.facings.iter().find(|f| f.turns == 0).map(|f| f.value), Some(3));
    for k in &c.kinds {
        assert!(c.groups.iter().any(|g| g.key == k.group), "{}: grupo {}", k.name, k.group);
        assert!(!k.description.is_empty(), "{}: sin descripción", k.name);
        if let Some(s) = k.size {
            assert!(s.min[0] <= s.max[0] && s.min[1] <= s.max[1], "{}: tamaños", k.name);
        }
    }
    assert!(c.kind("PlayerBase").is_none());
    for name in ["Home", "WallTower", "SchoolTower", "Harbour", "Boat", "House"] {
        assert!(c.kind(name).is_some(), "falta {name}");
    }
}
