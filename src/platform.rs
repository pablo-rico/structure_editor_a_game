//! Lo que cambia según el sistema: carpeta de datos, carpeta personal, mostrar un fichero en el explorador y lanzar
//! procesos sin abrir consola en Windows.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Carpeta personal (`HOME`; en Windows, `USERPROFILE`).
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

/// Carpeta de datos del editor (generador compilado, copia de recuperación).
pub fn data_dir() -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home_dir)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home_dir().join(".local/share"))
    };
    base.join("structure_editor_a_game")
}

/// Texto del enlace que enseña un fichero en el explorador del sistema.
pub const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Mostrar en Finder"
} else if cfg!(windows) {
    "Mostrar en el Explorador"
} else {
    "Abrir la carpeta"
};

/// Enseña `path` en el explorador del sistema.
pub fn reveal(path: &Path) {
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()
    } else if cfg!(windows) {
        Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
    } else {
        Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn()
    };
}

/// En Windows, que un proceso de consola (dotnet) no abra su propia ventana; en el resto no hace nada.
pub fn no_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
