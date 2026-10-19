use crate::keys;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const APP_DIR: &str = "Lswi";
const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Слушать горячую клавишу и конвертировать выделенный текст.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Виртуальный код клавиши (VK_*), которую нужно нажать дважды.
    #[serde(default = "default_hotkey")]
    pub hotkey_vk: u16,
    /// Максимальный интервал между двумя нажатиями, мс.
    #[serde(default = "default_interval")]
    pub double_tap_ms: u32,
    /// Автозапуск при входе в систему (Windows Run / LaunchAgent / .desktop).
    #[serde(default = "default_true")]
    pub start_with_windows: bool,
    /// После замены текста активировать целевую раскладку в активном окне.
    #[serde(default = "default_false")]
    pub switch_layout_after: bool,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_hotkey() -> u16 {
    keys::VK_LSHIFT
}

fn default_interval() -> u32 {
    400
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            hotkey_vk: default_hotkey(),
            double_tap_ms: default_interval(),
            start_with_windows: true,
            switch_layout_after: false,
        }
    }
}

impl Settings {
    pub fn load() -> (Self, bool) {
        let path = settings_path();
        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Settings>(&raw) {
                Ok(mut s) => {
                    s.sanitize();
                    (s, false)
                }
                Err(_) => (Settings::default(), true),
            },
            Err(_) => (Settings::default(), true),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut copy = self.clone();
        copy.sanitize();
        let raw = serde_json::to_string_pretty(&copy).map_err(|e| e.to_string())?;
        fs::write(path, raw).map_err(|e| e.to_string())
    }

    pub fn sanitize(&mut self) {
        if self.double_tap_ms < 150 {
            self.double_tap_ms = 150;
        }
        if self.double_tap_ms > 2000 {
            self.double_tap_ms = 2000;
        }
        if self.hotkey_vk == 0 {
            self.hotkey_vk = default_hotkey();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_interval() {
        let mut s = Settings {
            double_tap_ms: 10,
            hotkey_vk: 0,
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.double_tap_ms, 150);
        assert_ne!(s.hotkey_vk, 0);
    }
}

pub fn settings_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push(APP_DIR);
    dir.push(FILE_NAME);
    dir
}
