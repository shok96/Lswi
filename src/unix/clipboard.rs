use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use std::thread;
use std::time::Duration;

fn modifier_key() -> Key {
    #[cfg(target_os = "macos")]
    {
        Key::Meta
    }
    #[cfg(not(target_os = "macos"))]
    {
        Key::Control
    }
}

fn enigo() -> Option<Enigo> {
    Enigo::new(&Settings::default()).ok()
}

fn combo(letter: char) -> bool {
    let mut enigo = match enigo() {
        Some(e) => e,
        None => return false,
    };
    let modifier = modifier_key();
    let _ = enigo.key(modifier, Press);
    let _ = enigo.key(Key::Unicode(letter), Click);
    let _ = enigo.key(modifier, Release);
    true
}

pub fn copy_selection() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    let sentinel = format!("__lswi_{}", std::process::id());
    clipboard.set_text(&sentinel).ok()?;
    thread::sleep(Duration::from_millis(20));
    if !combo('c') {
        return None;
    }
    for _ in 0..25 {
        thread::sleep(Duration::from_millis(25));
        if let Ok(text) = clipboard.get_text() {
            if text != sentinel && !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

pub fn paste_text(text: &str) -> bool {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => return false,
    };
    if clipboard.set_text(text).is_err() {
        return false;
    }
    thread::sleep(Duration::from_millis(30));
    combo('v')
}

pub fn restore_later(previous: Option<String>) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        if let Ok(mut clipboard) = Clipboard::new() {
            match previous {
                Some(text) => {
                    let _ = clipboard.set_text(text);
                }
                None => {
                    let _ = clipboard.set_text(String::new());
                }
            }
        }
    });
}

pub fn snapshot() -> Option<String> {
    Clipboard::new().ok().and_then(|mut c| c.get_text().ok())
}
