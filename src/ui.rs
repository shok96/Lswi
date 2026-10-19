use crate::keys;
use crate::settings::Settings;
use crate::util::wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, GetStockObject, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_GUI_FONT, FW_NORMAL,
    HBRUSH, HFONT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, GetWindowLongPtrW,
    GetWindowTextW, PostMessageW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowTextW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BN_CLICKED,
    BS_AUTOCHECKBOX, BS_PUSHBUTTON, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, ES_NUMBER,
    ES_READONLY, GWLP_USERDATA, HMENU, MINMAXINFO, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_DESTROY, WM_GETMINMAXINFO, WM_SETFONT, WNDCLASSW, WS_BORDER,
    WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

pub const IDC_ENABLED: i32 = 101;
pub const IDC_AUTOSTART: i32 = 102;
pub const IDC_SWITCH: i32 = 103;
pub const IDC_HOTKEY: i32 = 104;
pub const IDC_ASSIGN: i32 = 105;
pub const IDC_INTERVAL: i32 = 106;
pub const IDC_SAVE: i32 = 107;
pub const IDC_CANCEL: i32 = 108;

const CLASS: PCWSTR = w!("LswiSettings");

struct Handles {
    enabled: HWND,
    autostart: HWND,
    switch_layout: HWND,
    hotkey: HWND,
    assign: HWND,
    interval: HWND,
}

pub fn register_class(instance: windows::Win32::Foundation::HINSTANCE) {
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
            470,
            380,
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
        109,
        20,
        16,
        420,
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
        72,
        420,
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
        102,
        420,
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
        132,
        420,
        24,
        "Менять раскладку клавиатуры после замены текста",
    );
    let _ = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        110,
        20,
        172,
        420,
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
        196,
        260,
        26,
        &keys::display_name(settings.hotkey_vk),
    );
    let assign = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_ASSIGN,
        290,
        194,
        140,
        28,
        "Назначить",
    );
    let _ = child(
        hwnd,
        instance,
        w!("STATIC"),
        WS_CHILD | WS_VISIBLE,
        111,
        20,
        236,
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
        232,
        120,
        26,
        &settings.double_tap_ms.to_string(),
    );
    let save = child(
        hwnd,
        instance,
        w!("BUTTON"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        IDC_SAVE,
        230,
        290,
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
        350,
        290,
        80,
        30,
        "Отмена",
    );

    unsafe {
        for h in [hint, enabled, autostart, switch_layout, hotkey, assign, interval, save, cancel] {
            SendMessageW(h, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        set_check(enabled, settings.enabled);
        set_check(autostart, settings.start_with_windows);
        set_check(switch_layout, settings.switch_layout_after);
        let boxed = Box::new(Handles {
            enabled,
            autostart,
            switch_layout,
            hotkey,
            assign,
            interval,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(boxed) as isize);
    }
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

pub fn fill(hwnd: HWND, settings: &Settings) {
    let Some(h) = handles_of(hwnd) else { return };
    set_check(h.enabled, settings.enabled);
    set_check(h.autostart, settings.start_with_windows);
    set_check(h.switch_layout, settings.switch_layout_after);
    set_text(h.hotkey, &keys::display_name(settings.hotkey_vk));
    set_text(h.interval, &settings.double_tap_ms.to_string());
    set_text(h.assign, "Назначить");
}

pub fn set_hotkey_name(hwnd: HWND, vk: u16) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.hotkey, &keys::display_name(vk));
    set_text(h.assign, "Назначить");
}

pub fn begin_capture(hwnd: HWND) {
    let Some(h) = handles_of(hwnd) else { return };
    set_text(h.hotkey, "Нажмите клавишу…");
    set_text(h.assign, "Жду…");
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
    next.sanitize();
    next
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
                    IDC_SAVE => {
                        let _ = PostMessageW(
                            owner_of(hwnd),
                            crate::app::WM_SAVE_SETTINGS,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    IDC_CANCEL => {
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
                (*info).ptMinTrackSize.x = 470;
                (*info).ptMinTrackSize.y = 380;
                (*info).ptMaxTrackSize.x = 470;
                (*info).ptMaxTrackSize.y = 380;
            }
            LRESULT(0)
        }
        WM_DESTROY => {
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
