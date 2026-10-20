/// Портативные коды совпадают с Windows VK, чтобы настройки были общими.
pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_PAUSE: u16 = 0x13;
pub const VK_CAPITAL: u16 = 0x14;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_SPACE: u16 = 0x20;
pub const VK_PRIOR: u16 = 0x21;
pub const VK_NEXT: u16 = 0x22;
pub const VK_END: u16 = 0x23;
pub const VK_HOME: u16 = 0x24;
pub const VK_LEFT: u16 = 0x25;
pub const VK_UP: u16 = 0x26;
pub const VK_RIGHT: u16 = 0x27;
pub const VK_DOWN: u16 = 0x28;
pub const VK_SNAPSHOT: u16 = 0x2C;
pub const VK_INSERT: u16 = 0x2D;
pub const VK_DELETE: u16 = 0x2E;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_NUMPAD0: u16 = 0x60;
pub const VK_F1: u16 = 0x70;
pub const VK_F12: u16 = 0x7B;
pub const VK_F24: u16 = 0x87;
pub const VK_NUMLOCK: u16 = 0x90;
pub const VK_SCROLL: u16 = 0x91;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
pub const VK_OEM_1: u16 = 0xBA;
pub const VK_OEM_PLUS: u16 = 0xBB;
pub const VK_OEM_COMMA: u16 = 0xBC;
pub const VK_OEM_MINUS: u16 = 0xBD;
pub const VK_OEM_PERIOD: u16 = 0xBE;
pub const VK_OEM_2: u16 = 0xBF;
pub const VK_OEM_3: u16 = 0xC0;
pub const VK_OEM_4: u16 = 0xDB;
pub const VK_OEM_5: u16 = 0xDC;
pub const VK_OEM_6: u16 = 0xDD;
pub const VK_OEM_7: u16 = 0xDE;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_BACK: u16 = 0x08;

#[allow(dead_code)]
pub fn is_modifier(vk: u16) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_CONTROL
            | VK_MENU
            | VK_LWIN
            | VK_RWIN
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
    )
}

fn shift_family(vk: u16) -> bool {
    matches!(vk, VK_SHIFT | VK_LSHIFT | VK_RSHIFT)
}

fn ctrl_family(vk: u16) -> bool {
    matches!(vk, VK_CONTROL | VK_LCONTROL | VK_RCONTROL)
}

fn alt_family(vk: u16) -> bool {
    matches!(vk, VK_MENU | VK_LMENU | VK_RMENU)
}

pub const MOD_CTRL: u16 = 0x01;
pub const MOD_SHIFT: u16 = 0x02;
pub const MOD_ALT: u16 = 0x04;
pub const MOD_WIN: u16 = 0x08;

pub fn modifier_flag(vk: u16) -> Option<u16> {
    if ctrl_family(vk) {
        Some(MOD_CTRL)
    } else if shift_family(vk) {
        Some(MOD_SHIFT)
    } else if alt_family(vk) {
        Some(MOD_ALT)
    } else if vk == VK_LWIN || vk == VK_RWIN {
        Some(MOD_WIN)
    } else {
        None
    }
}

/// Левый и правый Shift/Ctrl/Alt считаются одной клавишей.
pub fn matches_hotkey(pressed: u16, configured: u16) -> bool {
    if pressed == 0 || configured == 0 {
        return false;
    }
    if pressed == configured {
        return true;
    }
    (shift_family(pressed) && shift_family(configured))
        || (ctrl_family(pressed) && ctrl_family(configured))
        || (alt_family(pressed) && alt_family(configured))
}

pub fn matches_combo(pressed: u16, pressed_mods: u16, configured: u16, configured_mods: u16) -> bool {
    matches_hotkey(pressed, configured) && pressed_mods == configured_mods
}

pub fn combo_name(vk: u16, mods: u16) -> String {
    let mut parts: Vec<String> = Vec::new();
    if mods & MOD_CTRL != 0 {
        parts.push("Ctrl".into());
    }
    if mods & MOD_SHIFT != 0 {
        parts.push("Shift".into());
    }
    if mods & MOD_ALT != 0 {
        parts.push("Alt".into());
    }
    if mods & MOD_WIN != 0 {
        parts.push("Win".into());
    }
    parts.push(display_name(vk));
    parts.join(" + ")
}

pub fn display_name(vk: u16) -> String {
    if (VK_F1..=VK_F12).contains(&vk) {
        return format!("F{}", vk - VK_F1 + 1);
    }
    if (VK_F12 + 1..=VK_F24).contains(&vk) {
        return format!("F{}", vk - VK_F1 + 1);
    }
    if (0x30..=0x39).contains(&vk) {
        return char::from((vk - 0x30) as u8 + b'0').to_string();
    }
    if (0x41..=0x5A).contains(&vk) {
        return char::from((vk - 0x41) as u8 + b'A').to_string();
    }
    if (VK_NUMPAD0..=VK_NUMPAD0 + 9).contains(&vk) {
        return format!("Num {}", vk - VK_NUMPAD0);
    }
    match vk {
        VK_PAUSE => "Pause".into(),
        VK_SCROLL => "Scroll Lock".into(),
        VK_CAPITAL => "Caps Lock".into(),
        VK_NUMLOCK => "Num Lock".into(),
        VK_SNAPSHOT => "Print Screen".into(),
        VK_SPACE => "Space".into(),
        VK_TAB => "Tab".into(),
        VK_RETURN => "Enter".into(),
        VK_ESCAPE => "Esc".into(),
        VK_BACK => "Backspace".into(),
        VK_INSERT => "Insert".into(),
        VK_DELETE => "Delete".into(),
        VK_HOME => "Home".into(),
        VK_END => "End".into(),
        VK_PRIOR => "Page Up".into(),
        VK_NEXT => "Page Down".into(),
        VK_LEFT => "Left".into(),
        VK_RIGHT => "Right".into(),
        VK_UP => "Up".into(),
        VK_DOWN => "Down".into(),
        VK_SHIFT | VK_LSHIFT => "Shift".into(),
        VK_RSHIFT => "Right Shift".into(),
        VK_CONTROL | VK_LCONTROL => "Ctrl".into(),
        VK_RCONTROL => "Right Ctrl".into(),
        VK_MENU | VK_LMENU => "Alt".into(),
        VK_RMENU => "Right Alt".into(),
        VK_LWIN | VK_RWIN => "Win".into(),
        VK_OEM_1 => ";".into(),
        VK_OEM_PLUS => "+".into(),
        VK_OEM_COMMA => ",".into(),
        VK_OEM_MINUS => "-".into(),
        VK_OEM_PERIOD => ".".into(),
        VK_OEM_2 => "/".into(),
        VK_OEM_3 => "`".into(),
        VK_OEM_4 => "[".into(),
        VK_OEM_5 => "\\".into(),
        VK_OEM_6 => "]".into(),
        VK_OEM_7 => "'".into(),
        _ => format!("VK 0x{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_common_keys() {
        assert_eq!(display_name(VK_PAUSE), "Pause");
        assert_eq!(display_name(0x41), "A");
        assert_eq!(display_name(VK_LCONTROL), "Ctrl");
        assert!(is_modifier(VK_CONTROL));
        assert!(is_modifier(VK_LCONTROL));
        assert!(!is_modifier(VK_PAUSE));
        assert!(matches_hotkey(VK_LSHIFT, VK_RSHIFT));
        assert!(matches_hotkey(VK_SHIFT, VK_LSHIFT));
        assert!(!matches_hotkey(VK_LSHIFT, VK_PAUSE));
        assert_eq!(combo_name(VK_SPACE, MOD_CTRL | MOD_SHIFT), "Ctrl + Shift + Space");
        assert_eq!(combo_name(VK_PAUSE, 0), "Pause");
        assert!(matches_combo(0x41, MOD_CTRL, 0x41, MOD_CTRL));
        assert!(!matches_combo(0x41, MOD_CTRL, 0x41, 0));
        assert_eq!(modifier_flag(VK_LCONTROL), Some(MOD_CTRL));
    }
}
