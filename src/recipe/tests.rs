use std::collections::BTreeSet;

use super::*;

/// Carpeta temporal que hace de «juego» y de borrador.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("recetas-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_text(dir: &Path, script: &str, force: bool) -> Result<Outcome> {
    let recipe = dir.join("receta.rhai");
    std::fs::write(&recipe, script).unwrap();
    run(&recipe, Game::new(dir.to_path_buf(), false), Options { out_dir: Some(dir.join("salida")), force })
}

const LAMP: &str = r#"
new_model("Objects/Voxel/farol", 8, 12, 8);
tones("hierro", "5a5a66", "7a7a88", "3e3e4a", "202028");
cylinder(4, 4, 2, 0, 2);
box(3, 2, 3, 5, 9, 5);
color("ffd060");
box(3, 6, 3, 5, 8, 5);
color("hierro");
cone(4, 4, 3, 0.5, 9, 12);
print("farol de " + count() + " vóxeles");
"#;

#[test]
fn la_referencia_cubre_todo_lo_registrado() {
    let shared = Rc::new(RefCell::new(Canvas {
        game: Game::new(PathBuf::new(), false),
        opts: Options::default(),
        recipe: String::new(),
        doc: None,
        target: None,
        generative: false,
        before: None,
        opened_size: None,
        is_new: false,
        brush: 0,
        detail: false,
        mirror: (false, false),
        named: HashMap::new(),
        tones: HashMap::new(),
        floor_y: None,
        roles: serde_json::Map::new(),
        clipped: 0,
        log: Vec::new(),
        wraps: Vec::new(),
    }));
    let mut engine = Engine::new();
    api::register(&mut engine, &shared);
    let registered: BTreeSet<String> = engine
        .gen_fn_signatures(false)
        .into_iter()
        .map(|s| format!("{}/{}", &s[..s.find('(').unwrap()], s.matches("_:").count()))
        .collect();
    let documented: BTreeSet<String> = API
        .iter()
        .map(|(_, sig, _)| {
            let (name, args) = sig.split_once('(').unwrap();
            let args = args.trim_end_matches(')');
            format!("{name}/{}", if args.is_empty() { 0 } else { args.split(',').count() })
        })
        .collect();
    assert_eq!(registered, documented);
    assert!(reference().contains("cylinder(cx, cz, r, y0, y1)"));
    // La referencia de la skill es la salida de `cantero api`.
    let skill = include_str!("../../.claude/skills/cantero/referencia.md").replace("\r\n", "\n");
    assert_eq!(skill.trim_end(), reference().trim_end(), "rehaz la referencia: cargo run --bin cantero -- api > .claude/skills/cantero/referencia.md");
}

#[test]
fn receta_generativa_de_un_modelo() {
    let dir = scratch("generativa");
    let out = run_text(&dir, LAMP, false).unwrap();
    assert_eq!(out.target, dir.join("salida/farol_model.json"));
    assert_eq!(out.summary["change"]["new"], true);
    assert_eq!(out.summary["log"][0], format!("farol de {} vóxeles", out.summary["solids"]));
    assert!(out.summary["checks"].as_array().unwrap().iter().any(|c| c["code"] == "model-new"));
    // Lo escrito se lee como modelo, lleva la marca, la receta y su huella, y está sombreado.
    let (m, s) = vmodel::load(&std::fs::read_to_string(&out.target).unwrap()).unwrap();
    assert!(m.edited());
    assert_eq!(noted_fingerprint(m.note()), out.summary["fingerprint"].as_str());
    assert!(m.note().contains("receta.rhai"));
    assert!(!s.face_colors.is_empty(), "las tapas y lo de debajo del sombrero van en su tono");
    assert_eq!(s.grid.get(3, 6, 3), m.palette.iter().position(|c| *c == [0xff, 0xd0, 0x60]).unwrap() as u8 + 1);
    // Repetirla no cambia nada.
    let again = run_text(&dir, LAMP, false).unwrap();
    assert_eq!(again.summary["unchanged"], true);
    assert_eq!(again.summary["change"]["added"], 0);
    // Cambiar la receta dice cuánto cambia.
    let taller = LAMP.replace("box(3, 2, 3, 5, 9, 5);", "box(3, 2, 3, 5, 9, 5);\nvoxel(0, 0, 0);");
    let third = run_text(&dir, &taller, false).unwrap();
    assert_eq!(third.summary["change"]["added"], 1);
    assert_eq!(third.summary["change"]["box"], json!([[0, 0, 0], [1, 1, 1]]));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_pisa_lo_retocado_a_mano() {
    let dir = scratch("guarda");
    let out = run_text(&dir, LAMP, false).unwrap();
    // Alguien lo retoca en el editor: la huella apuntada ya no casa.
    let (mut m, mut s) = vmodel::load(&std::fs::read_to_string(&out.target).unwrap()).unwrap();
    let i = s.grid.data.iter().position(|&b| b != 0).unwrap();
    s.grid.data[i] = 0;
    std::fs::write(&out.target, vmodel::save(&m, &s).unwrap()).unwrap();
    let e = run_text(&dir, LAMP, false).err().unwrap().to_string();
    assert!(e.contains("retocado") && e.contains("--forzar"), "{e}");
    assert!(run_text(&dir, LAMP, true).is_ok(), "forzando, se rehace");
    // Un modelo que no viene de una receta tampoco se pisa.
    m.set_note("De un script de arte.");
    std::fs::write(&out.target, vmodel::save(&m, &s).unwrap()).unwrap();
    let e = run_text(&dir, LAMP, false).err().unwrap().to_string();
    assert!(e.contains("no lo generó una receta"), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn receta_de_parche_sobre_el_cofre() {
    let dir = scratch("parche");
    let chest = dir.join("chest_0_model.json");
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/models/chest_0_model.json"), &chest).unwrap();
    let base = open_doc(&chest, &mut Game::new(dir.clone(), false)).unwrap().fingerprint();
    let script = format!(
        r#"open("chest_0_model.json", "{base}");
           color("c4c4d0");
           mirror(true, false);
           box(1, 13, 7, 3, 14, 9);   // se sale por arriba: no cabe
           box(1, 12, 7, 3, 13, 9);
           let n = replace("141418", "202030");
           print("cerradura: " + n);"#
    );
    let out = run_text(&dir, &script, false).unwrap();
    assert_eq!(out.target, dir.join("salida/chest_0_model.json"));
    assert!(out.summary["outOfBounds"].as_str().unwrap().starts_with("8 "), "{}", out.summary);
    assert_eq!(out.summary["log"][0], "cerradura: 12");
    let ch = &out.summary["change"];
    // Los ocho vóxeles de la tapa (ya eran sólidos) y los de la cerradura que tienen alguna cara a la vista.
    assert!(ch["added"] == 0 && ch["recolored"].as_u64().unwrap() >= 8, "{ch}");
    // El original no se toca con borrador, y conserva su nota de script (un parche no apunta huella).
    assert_eq!(open_doc(&chest, &mut Game::new(dir.clone(), false)).unwrap().fingerprint(), base);
    let (m, _) = vmodel::load(&std::fs::read_to_string(&out.target).unwrap()).unwrap();
    assert!(m.edited() && m.note().contains("furniture_models.py"));
    // Aplicarlo otra vez: ahora lee el del borrador, que ya no es la base.
    let e = run_text(&dir, &script, false).err().unwrap().to_string();
    assert!(e.contains("ya está aplicado"), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn errores_con_linea_y_en_llano() {
    let dir = scratch("errores");
    for (script, expect) in [
        ("box(0, 0, 0, 1, 1, 1);", "new_model"),
        ("new_model(\"a/b\", 4, 4, 4);\ncolor(\"rojo\");", "rrggbb"),
        ("new_model(\"a/b\", 4, 4, 4);\ncolor(\"ff0000\");\nbox(2, 0, 0, 1, 1, 1);", "caja vacía"),
        ("new_model(\"a/b\", 4, 4, 4);\nmaterial(\"Stone\");", "solo para estructuras"),
        ("new_model(\"a/b\", 4, 4, 4);\ncolor(\"ff0000\");\nvoxel(1.5, 0, 0);", "entero"),
        ("new_model(\"a/b\", 4, 4, 4);\nloop { }", "operations"),
        ("new_model(\"a/b\", 400, 4, 4);", "tamaño no válido"),
    ] {
        let e = run_text(&dir, script, false).err().unwrap_or_else(|| panic!("debería fallar: {script}")).to_string();
        assert!(e.contains(expect), "{script}\n→ {e}");
    }
    let e = run_text(&dir, "new_model(\"a/b\", 4, 4, 4);\n\ncolor(\"rojo\");", false).err().unwrap().to_string();
    assert!(e.contains("line 3"), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn copiar_pegar_y_formas() {
    let dir = scratch("formas");
    let script = r#"
        new_model("x/formas", 16, 8, 16);
        color("804020");
        hollow(0, 0, 0, 4, 4, 4);
        let c = copy(0, 0, 0, 4, 4, 4);
        paste(rotated(c), 8, 0, 8);
        color("20a040");
        sphere(12, 4, 4, 2);
        roof(0, 4, 0, 4, 4, "x");
        fill(1, 1, 1);
        speckle(0, 0, 0, 16, 8, 16, 30, 7, "804020");
        if get(1, 1, 1) != "20a040" { throw "el relleno no ha llegado al hueco"; }
        if size() != [16, 8, 16] { throw "tamaño"; }
        let b = solid_box();
        if b[0] != 0 || b[4] > 8 { throw "caja"; }
    "#;
    let out = run_text(&dir, script, false).unwrap();
    // 4³ macizo (cáscara + relleno) + su copia hueca (56) + esfera + tejado a dos aguas (16 + 8).
    assert!(out.summary["solids"].as_u64().unwrap() > 64 + 56 + 24, "{}", out.summary);
    std::fs::remove_dir_all(&dir).unwrap();
}
