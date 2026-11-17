//! `cantero`: el editor sin ventana, para trabajar estructuras y modelos desde la línea de comandos (`docs/recetas.md`).

fn main() {
    std::process::exit(structure_editor_a_game::cli::main(std::env::args().skip(1).collect()));
}
