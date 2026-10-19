use std::fs;
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "com.lswi.app";

fn plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn user_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "501".into())
}

pub fn apply(enabled: bool) -> Result<(), String> {
    let path = plist_path();
    let path_str = path.to_string_lossy().into_owned();
    let domain = format!("gui/{}", user_uid());
    if enabled {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = xml_escape(&exe.to_string_lossy());
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#
        );
        fs::write(&path, plist).map_err(|e| e.to_string())?;
        let _ = Command::new("launchctl")
            .args(["unload", &path_str])
            .status();
        let status = Command::new("launchctl")
            .args(["load", &path_str])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain, &path_str])
                .status();
        }
    } else {
        let _ = Command::new("launchctl")
            .args(["unload", &path_str])
            .status();
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &path_str])
            .status();
        let _ = fs::remove_file(path);
    }
    Ok(())
}
