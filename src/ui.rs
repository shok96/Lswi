use crate::keys;
use crate::settings::Settings;
use crate::util::wide;
use crate::voice;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, FALSE, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, RoundRect, ScreenToClient,
    SelectObject, SetBkMode, SetTextColor, SetWindowRgn, ANTIALIASED_QUALITY, DEFAULT_CHARSET,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_TOP, DT_VCENTER,
    DT_WORDBREAK, FW_NORMAL, FW_SEMIBOLD, HFONT, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, FindWindowW,
    GetClientRect, GetCursorPos, GetWindowLongPtrW, InsertMenuW, LoadCursorW, PostMessageW,
    RegisterClassW, SetCursor, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TrackPopupMenu, CS_DROPSHADOW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HTCAPTION,
    HTCLIENT, IDC_ARROW, IDC_HAND, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, MINMAXINFO, SW_SHOW,
    SWP_NOZORDER, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE,
    WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCHITTEST, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    WS_VISIBLE,
};

const WM_MOUSELEAVE: u32 = 0x02A3;

const CLASS: PCWSTR = w!("LswiSettings");
const WINDOW_W: i32 = 1000;
const WINDOW_H: i32 = 580;
const TITLE_H: i32 = 46;
const SPLIT_X: i32 = 500;
const ROW_H: i32 = 44;
const ROW_GAP: i32 = 6;
const ROW_X: i32 = 520;
const ROW_W: i32 = 452;
const ROW_START_Y: i32 = 60;
const INTERVALS: [u32; 5] = [250, 400, 600, 800, 1000];

const BG: COLORREF = COLORREF(0x0018130E);
const CARD: COLORREF = COLORREF(0x00261E17);
const CARD_HOVER: COLORREF = COLORREF(0x002E251D);
const CARD_BORDER: COLORREF = COLORREF(0x0040332A);
const DIVIDER: COLORREF = COLORREF(0x002A211A);
const TEXT: COLORREF = COLORREF(0x00F7F5F3);
const MUTED: COLORREF = COLORREF(0x00A3958B);
const DIM: COLORREF = COLORREF(0x0072655C);
const GREEN: COLORREF = COLORREF(0x006AC57E);
const GOLD: COLORREF = COLORREF(0x006AA3C9);
const PILL: COLORREF = COLORREF(0x00211A14);
const PILL_BORDER: COLORREF = COLORREF(0x0040332A);
const DOT_R: COLORREF = COLORREF(0x00575FFF);
const DOT_Y: COLORREF = COLORREF(0x002EBCFE);
const DOT_G: COLORREF = COLORREF(0x0040C828);
const CLOSE_HOVER: COLORREF = COLORREF(0x003C3CFF);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    None,
    Close,
    Enabled,
    Autostart,
    Smart,
    Voice,
    Polish,
    Hotkey,
    VoiceHotkey,
    Switch,
    Interval,
    Mic,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    None,
    Layout,
    Voice,
}

struct Fonts {
    title: HFONT,
    eyebrow: HFONT,
    display: HFONT,
    body: HFONT,
    row: HFONT,
    value: HFONT,
    pill: HFONT,
    badge: HFONT,
}

struct State {
    enabled: bool,
    autostart: bool,
    switch_layout: bool,
    smart_layout: bool,
    hotkey_vk: u16,
    double_tap_ms: u32,
    voice_enabled: bool,
    voice_polish: bool,
    voice_hotkey_vk: u16,
    voice_hotkey_mods: u16,
    microphone_id: String,
    mic_ids: Vec<String>,
    mic_names: Vec<String>,
    mic_testing: bool,
    mic_level: u32,
    capturing: Capture,
    hover: Hit,
    fonts: Fonts,
}

pub fn register_class(instance: windows::Win32::Foundation::HINSTANCE) {
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        hbrBackground: unsafe { CreateSolidBrush(BG) },
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
    let title = wide("LSWI · раскладка и голос");
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
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
        let state = Box::new(State::from_settings(settings));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        apply_frame(hwnd);
        place_window(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        hwnd
    }
}

pub fn fill(hwnd: HWND, settings: &Settings) {
    let Some(s) = state_mut(hwnd) else { return };
    s.apply_settings(settings);
    s.capturing = Capture::None;
    s.mic_testing = false;
    s.mic_level = 0;
    reload_mics(s, &settings.microphone_id);
    redraw(hwnd);
}

pub fn set_hotkey_name(hwnd: HWND, vk: u16) {
    let Some(s) = state_mut(hwnd) else { return };
    s.hotkey_vk = vk;
    s.capturing = Capture::None;
    redraw(hwnd);
}

pub fn set_voice_hotkey_name(hwnd: HWND, vk: u16, mods: u16) {
    let Some(s) = state_mut(hwnd) else { return };
    s.voice_hotkey_vk = vk;
    s.voice_hotkey_mods = mods;
    s.capturing = Capture::None;
    redraw(hwnd);
}

pub fn begin_capture(hwnd: HWND) {
    let Some(s) = state_mut(hwnd) else { return };
    s.capturing = Capture::Layout;
    redraw(hwnd);
}

pub fn begin_voice_capture(hwnd: HWND) {
    let Some(s) = state_mut(hwnd) else { return };
    s.capturing = Capture::Voice;
    redraw(hwnd);
}

pub fn refresh_microphones(hwnd: HWND, selected: &str) {
    let Some(s) = state_mut(hwnd) else { return };
    reload_mics(s, selected);
    redraw(hwnd);
}

pub fn selected_microphone(hwnd: HWND) -> String {
    state_of(hwnd)
        .map(|s| s.microphone_id.clone())
        .unwrap_or_default()
}

pub fn set_mic_testing(hwnd: HWND, testing: bool) {
    let Some(s) = state_mut(hwnd) else { return };
    s.mic_testing = testing;
    if !testing {
        s.mic_level = 0;
    }
    redraw(hwnd);
}

pub fn set_mic_level(hwnd: HWND, level: u32) {
    let Some(s) = state_mut(hwnd) else { return };
    s.mic_level = level.min(100);
    redraw(hwnd);
}

pub fn collect(hwnd: HWND, current: &Settings) -> Settings {
    let Some(s) = state_of(hwnd) else {
        return current.clone();
    };
    let mut next = current.clone();
    next.enabled = s.enabled;
    next.start_with_windows = s.autostart;
    next.switch_layout_after = s.switch_layout;
    next.smart_layout = s.smart_layout;
    next.hotkey_vk = s.hotkey_vk;
    next.double_tap_ms = s.double_tap_ms;
    next.voice_enabled = s.voice_enabled;
    next.voice_polish = s.voice_polish;
    next.voice_hotkey_vk = s.voice_hotkey_vk;
    next.voice_hotkey_mods = s.voice_hotkey_mods;
    next.microphone_id = s.microphone_id.clone();
    next.sanitize();
    next
}

impl State {
    fn from_settings(settings: &Settings) -> Self {
        let mut s = Self {
            enabled: settings.enabled,
            autostart: settings.start_with_windows,
            switch_layout: settings.switch_layout_after,
            smart_layout: settings.smart_layout,
            hotkey_vk: settings.hotkey_vk,
            double_tap_ms: settings.double_tap_ms,
            voice_enabled: settings.voice_enabled,
            voice_polish: settings.voice_polish,
            voice_hotkey_vk: settings.voice_hotkey_vk,
            voice_hotkey_mods: settings.voice_hotkey_mods,
            microphone_id: settings.microphone_id.clone(),
            mic_ids: Vec::new(),
            mic_names: Vec::new(),
            mic_testing: false,
            mic_level: 0,
            capturing: Capture::None,
            hover: Hit::None,
            fonts: Fonts::new(),
        };
        reload_mics(&mut s, &settings.microphone_id);
        s
    }

    fn apply_settings(&mut self, settings: &Settings) {
        self.enabled = settings.enabled;
        self.autostart = settings.start_with_windows;
        self.switch_layout = settings.switch_layout_after;
        self.smart_layout = settings.smart_layout;
        self.hotkey_vk = settings.hotkey_vk;
        self.double_tap_ms = settings.double_tap_ms;
        self.voice_enabled = settings.voice_enabled;
        self.voice_polish = settings.voice_polish;
        self.voice_hotkey_vk = settings.voice_hotkey_vk;
        self.voice_hotkey_mods = settings.voice_hotkey_mods;
        self.microphone_id = settings.microphone_id.clone();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.fonts.destroy();
    }
}

impl Fonts {
    fn new() -> Self {
        Self {
            title: make_font(13, FW_SEMIBOLD.0 as i32),
            eyebrow: make_font(11, FW_SEMIBOLD.0 as i32),
            display: make_font(40, FW_SEMIBOLD.0 as i32),
            body: make_font(15, FW_NORMAL.0 as i32),
            row: make_font(14, FW_SEMIBOLD.0 as i32),
            value: make_font(13, FW_NORMAL.0 as i32),
            pill: make_font(16, FW_SEMIBOLD.0 as i32),
            badge: make_font(12, FW_NORMAL.0 as i32),
        }
    }

    fn destroy(&mut self) {
        unsafe {
            for font in [
                self.title,
                self.eyebrow,
                self.display,
                self.body,
                self.row,
                self.value,
                self.pill,
                self.badge,
            ] {
                if !font.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
            }
        }
    }
}

fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            0,
            0,
            ANTIALIASED_QUALITY.0 as u32,
            0,
            w!("Segoe UI"),
        )
    }
}

fn reload_mics(s: &mut State, selected: &str) {
    s.mic_ids.clear();
    s.mic_names.clear();
    s.mic_ids.push(String::new());
    s.mic_names.push("по умолчанию".into());
    for mic in voice::list_microphones() {
        s.mic_ids.push(mic.id);
        s.mic_names.push(mic.name);
    }
    if !selected.is_empty() && !s.mic_ids.iter().any(|id| id == selected) {
        s.mic_ids.push(selected.to_string());
        s.mic_names.push(format!("не найдено: {selected}"));
    }
    s.microphone_id = selected.to_string();
}

fn mic_label(s: &State) -> String {
    if s.mic_testing {
        return "проверка…".into();
    }
    s.mic_ids
        .iter()
        .position(|id| id == &s.microphone_id)
        .and_then(|i| s.mic_names.get(i).cloned())
        .unwrap_or_else(|| "по умолчанию".into())
}

fn rows() -> [(Hit, &'static str); 10] {
    [
        (Hit::Enabled, "Включено"),
        (Hit::Autostart, "Запуск с Windows"),
        (Hit::Smart, "Умная конвертация"),
        (Hit::Voice, "Голосовой ввод"),
        (Hit::Polish, "Пунктуация в диктовке"),
        (Hit::Hotkey, "Горячая клавиша"),
        (Hit::VoiceHotkey, "Клавиша голоса"),
        (Hit::Switch, "Менять раскладку"),
        (Hit::Interval, "Интервал"),
        (Hit::Mic, "Микрофон"),
    ]
}

fn row_rect(index: usize) -> RECT {
    let y = ROW_START_Y + index as i32 * (ROW_H + ROW_GAP);
    RECT {
        left: ROW_X,
        top: y,
        right: ROW_X + ROW_W,
        bottom: y + ROW_H,
    }
}

fn close_rect() -> RECT {
    RECT {
        left: WINDOW_W - 44,
        top: 12,
        right: WINDOW_W - 16,
        bottom: 36,
    }
}

fn close_dot_rect() -> RECT {
    RECT {
        left: 12,
        top: 12,
        right: 32,
        bottom: 34,
    }
}

fn hit_at(x: i32, y: i32) -> Hit {
    if pt_in(close_rect(), x, y) || pt_in(close_dot_rect(), x, y) {
        return Hit::Close;
    }
    for (i, (hit, _)) in rows().iter().enumerate() {
        if pt_in(row_rect(i), x, y) {
            return *hit;
        }
    }
    Hit::None
}

fn pt_in(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn row_value(s: &State, hit: Hit) -> (String, COLORREF) {
    let on_off = |on: bool| {
        if on {
            ("on".into(), GREEN)
        } else {
            ("off".into(), DIM)
        }
    };
    match hit {
        Hit::Enabled => on_off(s.enabled),
        Hit::Autostart => on_off(s.autostart),
        Hit::Smart => on_off(s.smart_layout),
        Hit::Voice => on_off(s.voice_enabled),
        Hit::Polish => on_off(s.voice_polish),
        Hit::Switch => on_off(s.switch_layout),
        Hit::Hotkey => {
            if s.capturing == Capture::Layout {
                ("нажмите клавишу".into(), MUTED)
            } else {
                (keys::display_name(s.hotkey_vk), GREEN)
            }
        }
        Hit::VoiceHotkey => {
            if s.capturing == Capture::Voice {
                ("нажмите клавишу".into(), MUTED)
            } else {
                (
                    keys::combo_name(s.voice_hotkey_vk, s.voice_hotkey_mods),
                    GREEN,
                )
            }
        }
        Hit::Interval => (format!("{} мс", s.double_tap_ms), GREEN),
        Hit::Mic => (mic_label(s), GREEN),
        Hit::Close | Hit::None => (String::new(), TEXT),
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_invalid() {
            let _ = EndPaint(hwnd, &ps);
            return;
        }
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let width = rc.right - rc.left;
        let height = rc.bottom - rc.top;
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, width, height);
        if mem.is_invalid() || bmp.is_invalid() {
            if !mem.is_invalid() {
                let _ = DeleteDC(mem);
            }
            if !bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            let _ = EndPaint(hwnd, &ps);
            return;
        }
        let old_bmp = SelectObject(mem, HGDIOBJ(bmp.0));
        let bg = CreateSolidBrush(BG);
        let _ = FillRect(mem, &rc, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));
        SetBkMode(mem, TRANSPARENT);

        if let Some(s) = state_of(hwnd) {
            paint_chrome(mem, s);
            paint_hero(mem, s);
            paint_rows(mem, s);
        }

        let _ = BitBlt(hdc, 0, 0, width, height, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

fn paint_chrome(hdc: windows::Win32::Graphics::Gdi::HDC, s: &State) {
    draw_dot(hdc, 22, 23, 6, DOT_R);
    draw_dot(hdc, 42, 23, 6, DOT_Y);
    draw_dot(hdc, 62, 23, 6, DOT_G);
    text(
        hdc,
        s.fonts.title,
        "LSWI  ·  раскладка и голос",
        RECT {
            left: 86,
            top: 0,
            right: WINDOW_W - 56,
            bottom: TITLE_H,
        },
        MUTED,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
    );
    let close = close_rect();
    let close_color = if s.hover == Hit::Close {
        CLOSE_HOVER
    } else {
        MUTED
    };
    text(
        hdc,
        s.fonts.title,
        "✕",
        close,
        close_color,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    fill_rect(hdc, RECT {
        left: SPLIT_X,
        top: TITLE_H,
        right: SPLIT_X + 1,
        bottom: WINDOW_H,
    }, DIVIDER);
}

fn paint_hero(hdc: windows::Win32::Graphics::Gdi::HDC, s: &State) {
    text(
        hdc,
        s.fonts.eyebrow,
        "TRAY  ·  WINDOWS  /  MACOS  /  LINUX",
        RECT {
            left: 36,
            top: 64,
            right: SPLIT_X - 28,
            bottom: 84,
        },
        DIM,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
    );
    text(
        hdc,
        s.fonts.display,
        "ghbdtn  →  привет",
        RECT {
            left: 36,
            top: 92,
            right: SPLIT_X - 28,
            bottom: 148,
        },
        TEXT,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
    );
    let hotkey = keys::display_name(s.hotkey_vk);
    let body = format!(
        "Выделили текст, набранный не в той раскладке: двойной {hotkey} меняет его на месте."
    );
    text(
        hdc,
        s.fonts.body,
        &body,
        RECT {
            left: 36,
            top: 158,
            right: SPLIT_X - 36,
            bottom: 214,
        },
        MUTED,
        DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
    );

    let pill_y = 236;
    let left_pill = RECT {
        left: 36,
        top: pill_y,
        right: 168,
        bottom: pill_y + 48,
    };
    let right_pill = RECT {
        left: 220,
        top: pill_y,
        right: 352,
        bottom: pill_y + 48,
    };
    round(hdc, left_pill, 16, PILL, PILL_BORDER);
    round(hdc, right_pill, 16, PILL, PILL_BORDER);
    text(
        hdc,
        s.fonts.pill,
        "ghbdtn",
        left_pill,
        GOLD,
        DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    text(
        hdc,
        s.fonts.pill,
        "→",
        RECT {
            left: 168,
            top: pill_y,
            right: 220,
            bottom: pill_y + 48,
        },
        GREEN,
        DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    text(
        hdc,
        s.fonts.pill,
        "привет",
        right_pill,
        GREEN,
        DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    let voice = keys::combo_name(s.voice_hotkey_vk, s.voice_hotkey_mods);
    let badges = [
        format!("{hotkey}  ×2"),
        format!("{voice} голос"),
        "EN  ↔  RU".into(),
    ];
    let mut x = 36;
    let badge_y = 304;
    for label in badges {
        let w = 28 + (label.chars().count() as i32) * 7;
        let r = RECT {
            left: x,
            top: badge_y,
            right: x + w,
            bottom: badge_y + 30,
        };
        round(hdc, r, 10, PILL, PILL_BORDER);
        text(
            hdc,
            s.fonts.badge,
            &label,
            r,
            MUTED,
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        x += w + 8;
    }
}

fn paint_rows(hdc: windows::Win32::Graphics::Gdi::HDC, s: &State) {
    for (i, (hit, label)) in rows().iter().enumerate() {
        let r = row_rect(i);
        let fill_color = if s.hover == *hit { CARD_HOVER } else { CARD };
        round(hdc, r, 12, fill_color, CARD_BORDER);
        let label_rc = RECT {
            left: r.left + 18,
            top: r.top,
            right: r.right - 120,
            bottom: r.bottom,
        };
        text(
            hdc,
            s.fonts.row,
            label,
            label_rc,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        let (value, color) = row_value(s, *hit);
        let value_rc = RECT {
            left: r.right - 168,
            top: r.top,
            right: r.right - 18,
            bottom: r.bottom,
        };
        text(
            hdc,
            s.fonts.value,
            &value,
            value_rc,
            color,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        if *hit == Hit::Mic && s.mic_testing {
            let bar = RECT {
                left: r.left + 18,
                top: r.bottom - 6,
                right: r.right - 18,
                bottom: r.bottom - 3,
            };
            fill_rect(hdc, bar, PILL_BORDER);
            let fill_r = bar.left + (bar.right - bar.left) * s.mic_level as i32 / 100;
            if fill_r > bar.left {
                fill_rect(
                    hdc,
                    RECT {
                        left: bar.left,
                        top: bar.top,
                        right: fill_r,
                        bottom: bar.bottom,
                    },
                    GREEN,
                );
            }
        }
    }
}

fn text(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    font: HFONT,
    value: &str,
    mut rc: RECT,
    color: COLORREF,
    format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    unsafe {
        let _ = SelectObject(hdc, HGDIOBJ(font.0));
        SetTextColor(hdc, color);
        let mut buf: Vec<u16> = value.encode_utf16().collect();
        if buf.is_empty() {
            return;
        }
        let centered = format.0 & DT_RIGHT.0 == 0
            && format.0 & DT_LEFT.0 == 0
            && format.0 & DT_CENTER.0 == 0;
        let format = if centered { format | DT_CENTER } else { format };
        let _ = DrawTextW(hdc, &mut buf, &mut rc, format);
    }
}

fn fill_rect(hdc: windows::Win32::Graphics::Gdi::HDC, rc: RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let _ = FillRect(hdc, &rc, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn round(hdc: windows::Win32::Graphics::Gdi::HDC, rc: RECT, radius: i32, fill: COLORREF, border: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, border);
        let brush = CreateSolidBrush(fill);
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ(pen.0));
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn draw_dot(hdc: windows::Win32::Graphics::Gdi::HDC, cx: i32, cy: i32, r: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let _ = Ellipse(hdc, cx - r, cy - r, cx + r, cy + r);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn apply_frame(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            4,
        );
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            4,
        );
        let border: COLORREF = BG;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const COLORREF as *const _,
            4,
        );
        let rgn = CreateRoundRectRgn(0, 0, WINDOW_W + 1, WINDOW_H + 1, 18, 18);
        let _ = SetWindowRgn(hwnd, rgn, TRUE);
    }
}

fn place_window(hwnd: HWND) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let work = info.rcWork;
        let mut x = pt.x - WINDOW_W / 2;
        let mut y = pt.y - WINDOW_H - 18;
        if y < work.top + 12 {
            y = work.top + (work.bottom - work.top - WINDOW_H) / 2;
        }
        x = x.clamp(work.left + 12, (work.right - WINDOW_W - 12).max(work.left + 12));
        y = y.clamp(work.top + 12, (work.bottom - WINDOW_H - 12).max(work.top + 12));
        let _ = SetWindowPos(hwnd, None, x, y, WINDOW_W, WINDOW_H, SWP_NOZORDER);
    }
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(hwnd, None, FALSE);
    }
}

fn owner_of(_hwnd: HWND) -> HWND {
    unsafe { FindWindowW(w!("LswiHost"), None).unwrap_or_default() }
}

fn post_host(hwnd: HWND, msg: u32) {
    unsafe {
        let _ = PostMessageW(owner_of(hwnd), msg, WPARAM(hwnd.0 as usize), LPARAM(0));
    }
}

fn apply_now(hwnd: HWND) {
    redraw(hwnd);
    post_host(hwnd, crate::app::WM_SAVE_SETTINGS);
}

fn cycle_interval(current: u32) -> u32 {
    let idx = INTERVALS.iter().position(|ms| *ms == current).unwrap_or(1);
    INTERVALS[(idx + 1) % INTERVALS.len()]
}

fn click(hwnd: HWND, hit: Hit) {
    match hit {
        Hit::None => {}
        Hit::Close => unsafe {
            voice::stop_meter();
            let _ = DestroyWindow(hwnd);
        },
        Hit::Enabled => {
            if let Some(s) = state_mut(hwnd) {
                s.enabled = !s.enabled;
            }
            apply_now(hwnd);
        }
        Hit::Autostart => {
            if let Some(s) = state_mut(hwnd) {
                s.autostart = !s.autostart;
            }
            apply_now(hwnd);
        }
        Hit::Smart => {
            if let Some(s) = state_mut(hwnd) {
                s.smart_layout = !s.smart_layout;
            }
            apply_now(hwnd);
        }
        Hit::Voice => {
            if let Some(s) = state_mut(hwnd) {
                s.voice_enabled = !s.voice_enabled;
            }
            apply_now(hwnd);
        }
        Hit::Polish => {
            if let Some(s) = state_mut(hwnd) {
                s.voice_polish = !s.voice_polish;
            }
            apply_now(hwnd);
        }
        Hit::Switch => {
            if let Some(s) = state_mut(hwnd) {
                s.switch_layout = !s.switch_layout;
            }
            apply_now(hwnd);
        }
        Hit::Interval => {
            if let Some(s) = state_mut(hwnd) {
                s.double_tap_ms = cycle_interval(s.double_tap_ms);
            }
            apply_now(hwnd);
        }
        Hit::Hotkey => post_host(hwnd, crate::app::WM_BEGIN_CAPTURE),
        Hit::VoiceHotkey => post_host(hwnd, crate::app::WM_BEGIN_VOICE_CAPTURE),
        Hit::Mic => mic_menu(hwnd),
    }
}

fn mic_menu(hwnd: HWND) {
    let Some(s) = state_of(hwnd) else { return };
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        if menu.is_invalid() {
            return;
        }
        for (i, name) in s.mic_names.iter().enumerate() {
            let caption = wide(&name.replace('&', "&&"));
            let _ = InsertMenuW(
                menu,
                i as u32,
                MF_BYPOSITION | MF_STRING,
                i as usize + 1,
                PCWSTR(caption.as_ptr()),
            );
        }
        let sep_at = s.mic_names.len() as u32;
        let _ = InsertMenuW(menu, sep_at, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null());
        let test = if s.mic_testing { "Стоп" } else { "Проверить микрофон" };
        let test_w = wide(test);
        let _ = InsertMenuW(
            menu,
            sep_at + 1,
            MF_BYPOSITION | MF_STRING,
            1000,
            PCWSTR(test_w.as_ptr()),
        );
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        let id = cmd.0 as u32;
        if id == 1000 {
            post_host(hwnd, crate::app::WM_MIC_TEST);
        } else if id > 0 {
            if let Some(s) = state_mut(hwnd) {
                let idx = id as usize - 1;
                if let Some(mic_id) = s.mic_ids.get(idx).cloned() {
                    s.microphone_id = mic_id;
                    apply_now(hwnd);
                }
            }
        }
    }
}

fn set_hover(hwnd: HWND, hit: Hit) {
    let Some(s) = state_mut(hwnd) else { return };
    if s.hover != hit {
        s.hover = hit;
        redraw(hwnd);
    }
}

fn track_leave(hwnd: HWND) {
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe {
        let _ = TrackMouseEvent(&mut tme);
    }
}

fn state_of(hwnd: HWND) -> Option<&'static State> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State;
        if ptr.is_null() {
            None
        } else {
            Some(&*ptr)
        }
    }
}

fn state_mut(hwnd: HWND) -> Option<&'static mut State> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            None
        } else {
            Some(&mut *ptr)
        }
    }
}

fn lparam_point(lparam: LPARAM) -> (i32, i32) {
    let x = lparam.0 as i16 as i32;
    let y = (lparam.0 >> 16) as i16 as i32;
    (x, y)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let (x, y) = lparam_point(lparam);
            set_hover(hwnd, hit_at(x, y));
            track_leave(hwnd);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            set_hover(hwnd, Hit::None);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_point(lparam);
            click(hwnd, hit_at(x, y));
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hover = state_of(hwnd).map(|s| s.hover).unwrap_or(Hit::None);
            let cursor = if hover == Hit::None {
                IDC_ARROW
            } else {
                IDC_HAND
            };
            if let Ok(h) = LoadCursorW(None, cursor) {
                let _ = SetCursor(h);
            }
            LRESULT(1)
        }
        WM_NCHITTEST => {
            let mut pt = POINT {
                x: lparam.0 as i16 as i32,
                y: (lparam.0 >> 16) as i16 as i32,
            };
            let _ = ScreenToClient(hwnd, &mut pt);
            if hit_at(pt.x, pt.y) != Hit::None {
                LRESULT(HTCLIENT as isize)
            } else if pt.y >= 0 && pt.y < TITLE_H {
                LRESULT(HTCAPTION as isize)
            } else {
                LRESULT(HTCLIENT as isize)
            }
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == keys::VK_ESCAPE {
                voice::stop_meter();
                let _ = DestroyWindow(hwnd);
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
            post_host(hwnd, crate::app::WM_SETTINGS_CLOSED);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
