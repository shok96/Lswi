const EN: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?";
const RU: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,";

#[allow(dead_code)]
pub fn convert_text(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let mapped = toggle_en_ru(text);
    (mapped != text).then_some(mapped)
}

#[allow(dead_code)]
pub fn looks_cyrillic(text: &str) -> bool {
    let (latin, cyr) = script_counts(text);
    cyr > latin
}

pub fn toggle_en_ru(text: &str) -> String {
    let (latin, cyr) = script_counts(text);
    let en_to_ru = latin >= cyr;
    text.chars()
        .map(|ch| {
            if en_to_ru {
                pair_map(EN, RU, ch)
            } else {
                pair_map(RU, EN, ch)
            }
        })
        .collect()
}

fn script_counts(text: &str) -> (usize, usize) {
    let mut latin = 0;
    let mut cyr = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            latin += 1;
        } else if ('\u{0400}'..='\u{04FF}').contains(&ch) {
            cyr += 1;
        }
    }
    (latin, cyr)
}

fn pair_map(from: &str, to: &str, ch: char) -> char {
    from.chars()
        .zip(to.chars())
        .find(|(a, _)| *a == ch)
        .map(|(_, b)| b)
        .unwrap_or(ch)
}

/// QWERTY punctuation that types a letter on ЙЦУКЕН (`,` → `б`, `.` → `ю`, …).
pub fn is_layout_letter_punct(ch: char) -> bool {
    EN.chars()
        .zip(RU.chars())
        .any(|(en, ru)| en == ch && ru.is_alphabetic())
}

fn change_score(original: &str, converted: &str) -> i32 {
    original
        .chars()
        .zip(converted.chars())
        .filter(|(a, b)| a != b && (a.is_alphabetic() || b.is_alphabetic()))
        .count() as i32
}

#[cfg(windows)]
mod win {
    use super::{change_score, target_for_text, toggle_en_ru};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ActivateKeyboardLayout, GetKeyboardLayout, GetKeyboardLayoutList, MapVirtualKeyExW,
        ToUnicodeEx, VkKeyScanExW, ACTIVATE_KEYBOARD_LAYOUT_FLAGS, HKL, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_INPUTLANGCHANGEREQUEST,
    };

    pub fn foreground_layout() -> HKL {
        unsafe {
            let hwnd = GetForegroundWindow();
            let tid = GetWindowThreadProcessId(hwnd, None);
            GetKeyboardLayout(tid)
        }
    }

    pub fn installed_layouts() -> Vec<HKL> {
        unsafe {
            let count = GetKeyboardLayoutList(None);
            if count <= 0 {
                return Vec::new();
            }
            let mut list = vec![HKL::default(); count as usize];
            let wrote = GetKeyboardLayoutList(Some(&mut list));
            list.truncate(wrote.max(0) as usize);
            list
        }
    }

    pub fn convert_selection(text: &str, current: HKL, layouts: &[HKL]) -> Option<(String, HKL)> {
        if text.is_empty() {
            return None;
        }

        let mut best: Option<(String, HKL, i32)> = None;

        let mapped = toggle_en_ru(text);
        if mapped != text {
            let score = change_score(text, &mapped) + 8;
            let target = target_for_text(&mapped, current, layouts);
            best = Some((mapped, target, score));
        }

        if layouts.len() >= 2 {
            for &from in layouts {
                for &to in layouts {
                    if from == to {
                        continue;
                    }
                    let converted = remap_text(text, from, to);
                    if converted == text {
                        continue;
                    }
                    let score = change_score(text, &converted) + i32::from(from == current);
                    let better = match &best {
                        None => true,
                        Some((_, _, s)) => score > *s,
                    };
                    if better {
                        best = Some((converted, to, score));
                    }
                }
            }
        }

        best.map(|(text, hkl, _)| (text, hkl))
    }

    fn remap_text(text: &str, from: HKL, to: HKL) -> String {
        text.chars().map(|ch| remap_char(ch, from, to)).collect()
    }

    fn remap_char(ch: char, from: HKL, to: HKL) -> char {
        if ch.is_control() || ch == '\n' || ch == '\r' || ch == '\t' {
            return ch;
        }
        let mut utf16 = [0u16; 2];
        let encoded = ch.encode_utf16(&mut utf16);
        if encoded.len() != 1 {
            return ch;
        }
        let scanned = unsafe { VkKeyScanExW(encoded[0], from) };
        if scanned == -1 {
            return ch;
        }
        let vk = (scanned as u16) & 0xff;
        let mods = ((scanned as u16) >> 8) as u8;
        let mut state = [0u8; 256];
        if mods & 1 != 0 {
            state[VK_SHIFT.0 as usize] = 0x80;
        }
        if mods & 2 != 0 {
            state[VK_CONTROL.0 as usize] = 0x80;
        }
        if mods & 4 != 0 {
            state[VK_MENU.0 as usize] = 0x80;
        }
        let scan = unsafe { MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC, from) };

        let mut buf = [0u16; 8];
        let n = unsafe { ToUnicodeEx(vk as u32, scan, &state, &mut buf, 0, to) };
        if n < 0 {
            let mut flush = [0u16; 8];
            unsafe {
                ToUnicodeEx(
                    VIRTUAL_KEY(0x20).0 as u32,
                    0,
                    &[0u8; 256],
                    &mut flush,
                    0,
                    to,
                );
            }
            return ch;
        }
        if n == 0 {
            return ch;
        }
        String::from_utf16_lossy(&buf[..n as usize])
            .chars()
            .next()
            .unwrap_or(ch)
    }

    pub fn activate_layout(target: HKL) {
        unsafe {
            let hwnd = GetForegroundWindow();
            let _ = PostMessageW(
                hwnd,
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(target.0 as isize),
            );
            let _ = ActivateKeyboardLayout(target, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0));
        }
    }
}

#[cfg(windows)]
pub use win::{activate_layout, convert_selection, foreground_layout, installed_layouts};

#[cfg(windows)]
pub fn layout_for_text(
    text: &str,
    current: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
    layouts: &[windows::Win32::UI::Input::KeyboardAndMouse::HKL],
) -> windows::Win32::UI::Input::KeyboardAndMouse::HKL {
    target_for_text(text, current, layouts)
}

#[cfg(windows)]
fn target_for_text(
    text: &str,
    current: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
    layouts: &[windows::Win32::UI::Input::KeyboardAndMouse::HKL],
) -> windows::Win32::UI::Input::KeyboardAndMouse::HKL {
    let (_, cyr) = script_counts(text);
    let want_ru = cyr > 0;
    layouts
        .iter()
        .copied()
        .find(|hkl| is_russian(*hkl) == want_ru)
        .unwrap_or(next_layout(layouts, current).unwrap_or(current))
}

#[cfg(windows)]
fn next_layout(
    layouts: &[windows::Win32::UI::Input::KeyboardAndMouse::HKL],
    current: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
) -> Option<windows::Win32::UI::Input::KeyboardAndMouse::HKL> {
    if layouts.is_empty() {
        return None;
    }
    let idx = layouts.iter().position(|h| *h == current).unwrap_or(0);
    Some(layouts[(idx + 1) % layouts.len()])
}

#[cfg(windows)]
fn is_russian(hkl: windows::Win32::UI::Input::KeyboardAndMouse::HKL) -> bool {
    (hkl.0 as usize & 0x3ff) == 0x19
}

#[cfg(test)]
mod tests {
    use super::{change_score, convert_text, toggle_en_ru};

    #[test]
    fn scores_letter_changes() {
        assert!(change_score("ghbdtn", "привет") > 0);
        assert_eq!(change_score("123", "123"), 0);
    }

    #[test]
    fn toggles_typical_mistype() {
        assert_eq!(toggle_en_ru("ghbdtn"), "привет");
        assert_eq!(toggle_en_ru("привет"), "ghbdtn");
        assert_eq!(toggle_en_ru("Ghbdtn"), "Привет");
        assert_eq!(toggle_en_ru("cdj,jlf"), "свобода");
        assert_eq!(convert_text("ghbdtn").as_deref(), Some("привет"));
        assert_eq!(convert_text("123"), None);
    }

    #[test]
    fn qwerty_punct_that_types_russian_letters() {
        use super::is_layout_letter_punct;
        for ch in [',', '.', ';', '\'', '[', ']', '`'] {
            assert!(is_layout_letter_punct(ch), "{ch}");
        }
        for ch in ['/', '?', '-', '!', ' '] {
            assert!(!is_layout_letter_punct(ch), "{ch}");
        }
    }
}
