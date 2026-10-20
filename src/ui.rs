use crate::keys;
use crate::settings::Settings;
use crate::util::wide;
use crate::voice;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, GetStockObject, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_GUI_FONT, FW_NORMAL,
    HBRUSH, HFONT,
};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
    PBM_SETRANGE32, PBM_SETPOS, PBS_SMOOTH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, GetWindowLongPtrW, GetWindowTextW,
    PostMessageW, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_PUSHBUTTON,
    CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CS_HREDRAW,
    CS_VREDRAW, CW_USEDEFAULT, ES_NUMBER, ES_READONLY, GWLP_USERDATA, HMENU, MINMAXINFO,
    SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_DESTROY, WM_GETMINMAXINFO, WM_SETFONT,
    WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};

pub const IDC_ENABLED: i32 = 101;
pub const IDC_AUTOSTART: i32 = 102;
pub const IDC_SWITCH: i32 = 103;
pub const IDC_HOTKEY: i32 = 104;
pub const IDC_ASSIGN: i32 = 105;
pub const IDC_INTERVAL: i32 = 106;
pub const IDC_SAVE: i32 = 107;
pub const IDC_CANCEL: i32 = 108;
pub const IDC_VOICE_ENABLED: i32 = 109;
pub const IDC_VOICE_HOTKEY: i32 = 110;
pub const IDC_VOICE_ASSIGN: i32 = 111;
pub const IDC_MIC: i32 = 112;
pub const IDC_MIC_TEST: i32 = 113;
pub const IDC_MIC_LEVEL: i32 = 114;

const CLASS: PCWSTR = w!("LswiSettings");
const WINDOW_W: i32 = 500;
const WINDOW_H: i32 = 620;
const SS_ETCHEDHORZ: i32 = 0x0010;
const CB_SETMINVISIBLE: u32 = 0x1701;

struct Handles {
    enabled: HWND,
    autostart: HWND,
    switch_layout: HWND,
    hotkey: HWND,
    assign: HWND,
    interval: HWND,
    voice_enabled: HWND,
    voice_hotkey: HWND,
    voice_assign: HWND,
    mic: HWND,
    mic_test: HWND,
    mic_level: HWND,
    mic_ids: Vec<String>,
}

pub fn register_class(instance: windows::Win32::Foundation::HINSTANCE) {
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES,
    };
    unsafe {
        let _ = InitCommonControlsEx(&icc);
    }
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hbrBackground: HBRUSH((windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as *mut core::ffi::c_void),
        lpszClassName: CLASS,
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&class);
    }
}

pub fn open(
    _owner: HWND,
    instance: windows::Win32::Foundation::HINSTANCE,
    settings: &Settings,
) -> HWND {
    if let Ok(existing) = unsafe { FindWindowW(CLASS, None) } {
        if !existing.is_invalid() {
            unsafe {
                let _ = ShowWindow(existing, SW_SHOW);
                let _ = SetForegroundWindow(existing);
            }
            fill(existing, settings);
            return existing;
        }
    }
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            w!("LSWI — настройки"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_W,
            WINDOW_H,
            None,
            None,
            instance,
            None,
        )
        .unwrap_or_default();
        if hwnd.is_invalid() {
            return hwnd;
        }
        create_controls(hwnd, instance, settings);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        hwnd
    }
}

fn create_controls(
    hwnd: HWND,
    instance: windows::Win32::Foundation::HINSTANCE,
    settings: &Settings,
) {
    let font = ui_font();
    let hint = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        200,
        20,
        16,
        440,
        48,
        "Дважды быстро нажмите выбранную клавишу — выделенный текст сменит раскладку.",
    );
    let enabled = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        IDC_ENABLED,
        20,
        68,
        440,
        24,
        "Включено",
    );
    let autostart = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        IDC_AUTOSTART,
        20,
        96,
        440,
        24,
        "Запускать вместе с Windows",
    );
    let switch_layout = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        IDC_SWITCH,
        20,
        124,
        440,
        24,
        "Менять раскладку клавиатуры после замены текста",
    );
    let hotkey_label = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        201,
        20,
        160,
        440,
        20,
        "Горячая клавиша (два нажатия):",
    );
    let hotkey = child(
        hwnd,
        instance,
        w!("EDIT"),
        WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_READONLY as u32),
        IDC_HOTKEY,
        20,
        184,
        280,
        26,
        &keys::display_name(settings.hotkey_vk),
    );
    let assign = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_ASSIGN,
        310,
        182,
        140,
        28,
        "Назначить",
    );
    let interval_label = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        202,
        20,
        220,
        280,
        20,
        "Интервал двойного нажатия, мс:",
    );
    let interval = child(
        hwnd,
        instance,
        w!("EDIT"),
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32),
        IDC_INTERVAL,
        310,
        216,
        140,
        26,
        &settings.double_tap_ms.to_string(),
    );
    let sep = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_ETCHEDHORZ as u32),
        203,
        20,
        256,
        440,
        2,
        "",
    );
    let voice_title = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        204,
        20,
        268,
        440,
        20,
        "Голосовой ввод (русский)",
    );
    let voice_enabled = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        IDC_VOICE_ENABLED,
        20,
        292,
        440,
        24,
        "Включить голосовой ввод",
    );
    let voice_hotkey_label = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        205,
        20,
        324,
        440,
        20,
        "Клавиша или сочетание (одно нажатие — старт и стоп):",
    );
    let voice_hotkey = child(
        hwnd,
        instance,
        w!("EDIT"),
        WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_READONLY as u32),
        IDC_VOICE_HOTKEY,
        20,
        348,
        280,
        26,
        &keys::combo_name(settings.voice_hotkey_vk, settings.voice_hotkey_mods),
    );
    let voice_assign = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_VOICE_ASSIGN,
        310,
        346,
        140,
        28,
        "Назначить",
    );
    let mic_label = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        206,
        20,
        388,
        440,
        20,
        "Микрофон:",
    );
    let mic = child(
        hwnd,
        instance,
        w!("COMBOBOX"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WS_BORDER | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        IDC_MIC,
        20,
        412,
        430,
        200,
        "",
    );
    let mic_test = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_MIC_TEST,
        20,
        450,
        140,
        28,
        "Проверить",
    );
    let mic_level = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("msctls_progress32"),
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(PBS_SMOOTH),
            170,
            454,
            280,
            20,
            hwnd,
            HMENU(IDC_MIC_LEVEL as *mut core::ffi::c_void),
            instance,
            None,
        )
        .unwrap_or_default()
    };
    let save = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_SAVE,
        250,
        510,
        110,
        30,
        "Сохранить",
    );
    let cancel = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_CANCEL,
        370,
        510,
        80,
        30,
        "Отмена",
    );

    unsafe {
        for h in [
            hint,
            enabled,
            autostart,
            switch_layout,
            hotkey_label,
            hotkey,
            assign,
            interval_label,
            interval,
            sep,
            voice_title,
            voice_enabled,
            voice_hotkey_label,
            voice_hotkey,
            voice_assign,
            mic_label,
            mic,
            mic_test,
            save,
            cancel,
        ] {
            SendMessageW(h, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        let _ = SendMessageW(mic, CB_SETMINVISIBLE, WPARAM(8), LPARAM(0));
        let _ = SendMessageW(mic_level, PBM_SETRANGE32, WPARAM(0), LPARAM(100));
        set_check(enabled, settings.enabled);
        set_check(autostart, settings.start_with_windows);
        set_check(switch_layout, settings.switch_layout_after);
        set_check(voice_enabled, settings.voice_enabled);
        let boxed = Box::new(Handles {
            enabled,
            autostart,
            switch_layout,
            hotkey,
            assign,
            interval,
            voice_enabled,
            voice_hotkey,
            voice_assign,
            mic,
            mic_test,
            mic_level,
            mic_ids: Vec::new(),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(boxed) as isize);
    }
    populate_mics(hwnd, &settings.microphone_id);
}

fn child(
    parent: HWND,
    instance: windows::Win32::Foundation::HINSTANCE,
    class: PCWSTR,
    style: WINDOW_STYLE,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
) -> HWND {
    let caption = wide(text);
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(caption.as_ptr()),
            style,
            x,
            y,
            w,
            h,
            parent,
            HMENU(id as *mut core::ffi::c_void),
            instance,
            None,
        )
        .unwrap_or_default()
    }
}

fn ui_font() -> HFONT {
    unsafe {
        let font = CreateFontW(
            20,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            0,
            0,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            w!("Segoe UI"),
        );
        if font.is_invalid() {
            HFONT(GetStockObject(DEFAULT_GUI_FONT).0)
        } else {
            font
        }
    }
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        let _ = SendMessageW(hwnd, BM_SETCHECK, WPARAM(usize::from(checked)), LPARAM(0));
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
}

fn handles_of(hwnd: HWND) -> Option<&'static Handles> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Handles;
        if ptr.is_null() {
            None
        } else {
            Some(&*ptr)
        }
    }
}

fn handles_of_mut(hwnd: HWND) -> Option<&'static mut Handles> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Handles;
        if ptr.is_null() {
            None
        } else {
            Some(&mut *ptr)
        }
    }
}

pub fn fill(hwnd: HWND, settings: &Settings) {
    let Some(h) = handles_of(hwnd) else { return };
    set_check(h.enabled, settings.enabled);
    set_check(h.autostart, settings.start_with_windows);
    set_check(h.switch_layout, settings.switch_layout_after);
    set_text(h.hotkey, &keys::display_name(settings.hotkey_vk));
    set_text(h.interval, &settings.double_tap_ms.to_string());
    set_text(h.assign, "Назначить");
    set_check(h.voice_enabled, settings.voice_enabled);
    set_text(
        h.voice_hotkey,
        &keys::combo_name(settings.voice_hotkey_vk, settings.voice_hotkey_mods),
    );
    set_text(h.voice_assign, "Назначить");
    populate_mics(hwnd, &settings.microphone_id);
    set_mic_testing(hwnd, false);
    set_mic_level(hwnd, 0);
}

pub fn set_hotkey_name(hwnd: HWND, vk: u16) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.hotkey, &keys::display_name(vk));
    set_text(h.assign, "Назначить");
}

pub fn set_voice_hotkey_name(hwnd: HWND, vk: u16, mods: u16) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.voice_hotkey, &keys::combo_name(vk, mods));
    set_text(h.voice_assign, "Назначить");
}

pub fn begin_capture(hwnd: HWND) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.hotkey, "Нажмите клавишу…");
    set_text(h.assign, "Жду…");
}

pub fn begin_voice_capture(hwnd: HWND) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.voice_hotkey, "Нажмите клавишу или сочетание…");
    set_text(h.voice_assign, "Жду…");
}

pub fn selected_microphone(hwnd: HWND) -> String {
    let Some(h) = handles_of(hwnd) else {
        return String::new();
    };
    let idx = unsafe { SendMessageW(h.mic, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 };
    if idx < 0 {
        return String::new();
    }
    h.mic_ids.get(idx as usize).cloned().unwrap_or_default()
}

pub fn set_mic_testing(hwnd: HWND, testing: bool) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.mic_test, if testing { "Стоп" } else { "Проверить" });
    if !testing {
        set_mic_level(hwnd, 0);
    }
}

pub fn set_mic_level(hwnd: HWND, level: u32) {
    let Some(h) = handles_of(hwnd) else { return };
    unsafe {
        let _ = SendMessageW(h.mic_level, PBM_SETPOS, WPARAM(level as usize), LPARAM(0));
    }
}

pub fn collect(hwnd: HWND, current: &Settings) -> Settings {
    let Some(h) = handles_of(hwnd) else {
        return current.clone();
    };
    let mut next = current.clone();
    next.enabled = is_checked(h.enabled);
    next.start_with_windows = is_checked(h.autostart);
    next.switch_layout_after = is_checked(h.switch_layout);
    next.double_tap_ms = read_number(h.interval).unwrap_or(current.double_tap_ms);
    next.voice_enabled = is_checked(h.voice_enabled);
    next.microphone_id = selected_microphone(hwnd);
    next.sanitize();
    next
}

fn populate_mics(hwnd: HWND, selected: &str) {
    let Some(h) = handles_of_mut(hwnd) else { return };
    unsafe {
        let _ = SendMessageW(h.mic, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
    }
    h.mic_ids.clear();
    add_mic(h, "", "По умолчанию");
    for mic in voice::list_microphones() {
        add_mic(h, &mic.id, &mic.name);
    }
    if !selected.is_empty() && !h.mic_ids.iter().any(|id| id == selected) {
        add_mic(h, selected, &format!("Не найдено: {selected}"));
    }
    let index = h
        .mic_ids
        .iter()
        .position(|id| id == selected)
        .unwrap_or(0);
    unsafe {
        let _ = SendMessageW(h.mic, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    }
}

fn add_mic(h: &mut Handles, id: &str, name: &str) {
    let caption = wide(name);
    unsafe {
        let _ = SendMessageW(h.mic, CB_ADDSTRING, WPARAM(0), LPARAM(caption.as_ptr() as isize));
    }
    h.mic_ids.push(id.to_string());
}

fn set_text(hwnd: HWND, text: &str) {
    let caption = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(caption.as_ptr()));
    }
}

fn read_number(hwnd: HWND) -> Option<u32> {
    let mut buf = [0u16; 16];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
    if n == 0 {
        return None;
    }
    String::from_utf16_lossy(&buf[..n]).trim().parse().ok()
}

fn owner_of(_hwnd: HWND) -> HWND {
    unsafe { FindWindowW(w!("LswiHost"), None).unwrap_or_default() }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wparam.0 as u16 as i32;
            let notify = (wparam.0 >> 16) as u16;
            if notify == BN_CLICKED as u16 {
                match id {
                    IDC_ASSIGN => {
                        let _ = PostMessageW(
                            owner_of(hwnd),
                            crate::app::WM_BEGIN_CAPTURE,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    IDC_VOICE_ASSIGN => {
                        let _ = PostMessageW(
                            owner_of(hwnd),
                            crate::app::WM_BEGIN_VOICE_CAPTURE,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    IDC_MIC_TEST => {
                        let _ = PostMessageW(
                            owner_of(hwnd),
                            crate::app::WM_MIC_TEST,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    IDC_SAVE => {
                        let _ = PostMessageW(
                            owner_of(hwnd),
                            crate::app::WM_SAVE_SETTINGS,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    IDC_CANCEL => {
                        voice::stop_meter();
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = lparam.0 as *mut MINMAXINFO;
            if !info.is_null() {
                (*info).ptMinTrackSize.x = WINDOW_W;
                (*info).ptMinTrackSize.y = WINDOW_H;
                (*info).ptMaxTrackSize.x = WINDOW_W;
                (*info).ptMaxTrackSize.y = WINDOW_H;
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            voice::stop_meter();
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Handles;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
