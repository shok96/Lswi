use std::fs;
use std::path::PathBuf;

fn desktop_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
        .join("lswi.desktop")
}

fn exec_line(path: &str) -> String {
    if path.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\\')) {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

pub fn apply(enabled: bool) -> Result<(), String> {
    let path = desktop_path();
    if enabled {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exec = exec_line(&exe.to_string_lossy());
        let desktop = format!(
            "\
[Desktop Entry]
Type=Application
Version=1.0
Name=LSWI
Comment=Смена раскладки выделенного текста
Exec={exec}
Terminal=false
X-GNOME-Autostart-enabled=true
"
        );
        fs::write(path, desktop).map_err(|e| e.to_string())
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
