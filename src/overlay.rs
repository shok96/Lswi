use crate::util::{hwnd_from_isize, hwnd_to_isize, wide};
use crate::voice;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, GetDC,
    GetMonitorInfoW, MonitorFromPoint, ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor,
    AC_SRC_OVER, BLENDFUNCTION, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER, DT_END_ELLIPSIS,
    DT_SINGLELINE, DT_VCENTER, FW_SEMIBOLD, HBRUSH, HGDIOBJ, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, KillTimer,
    LoadCursorW, RegisterClassW, SetTimer, SetWindowPos, ShowWindow, UpdateLayeredWindow,
    HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    ULW_ALPHA, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_PAINT,
    WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: PCWSTR = w!("LswiVoiceOverlay");
const WIDTH: i32 = 340;
const HEIGHT: i32 = 128;
const TIMER_PULSE: usize = 1;
const MA_NOACTIVATE: isize = 3;

static HWND_OVERLAY: AtomicIsize = AtomicIsize::new(0);
static PHASE: AtomicU32 = AtomicU32::new(0);

fn overlay_hwnd() -> HWND {
    hwnd_from_isize(HWND_OVERLAY.load(Ordering::SeqCst))
}

pub fn register_class(instance: windows::Win32::Foundation::HINSTANCE) {
    let class = WNDCLASSW {
        style: Default::default(),
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        lpszClassName: CLASS,
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&class);
    }
}

pub fn show(instance: windows::Win32::Foundation::HINSTANCE) {
    unsafe {
        let mut hwnd = overlay_hwnd();
        if hwnd.is_invalid() || hwnd.0.is_null() {
            hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                CLASS,
                w!("LSWI Voice"),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                None,
                None,
                instance,
                None,
            )
            .unwrap_or_default();
            if hwnd.is_invalid() {
                return;
            }
            HWND_OVERLAY.store(hwnd_to_isize(hwnd), Ordering::SeqCst);
        }
        position(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetTimer(hwnd, TIMER_PULSE, 200, None);
        compose(hwnd);
    }
}

pub fn hide() {
    unsafe {
        let hwnd = overlay_hwnd();
        if hwnd.is_invalid() || hwnd.0.is_null() {
            return;
        }
        let _ = KillTimer(hwnd, TIMER_PULSE);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn refresh() {
    let hwnd = overlay_hwnd();
    if hwnd.is_invalid() || hwnd.0.is_null() {
        return;
    }
    compose(hwnd);
}

pub fn destroy() {
    unsafe {
        let hwnd = overlay_hwnd();
        if hwnd.is_invalid() || hwnd.0.is_null() {
            return;
        }
        let _ = KillTimer(hwnd, TIMER_PULSE);
        let _ = DestroyWindow(hwnd);
        HWND_OVERLAY.store(0, Ordering::SeqCst);
    }
}

fn position(hwnd: HWND) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
        let monitor: HMONITOR = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - WIDTH) / 2;
        let y = work.bottom - HEIGHT - 28;
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            WIDTH,
            HEIGHT,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            PHASE.fetch_add(1, Ordering::Relaxed);
            compose(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            voice::stop();
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE),
        WM_NCHITTEST => LRESULT(1),
        WM_DESTROY => {
            HWND_OVERLAY.store(0, Ordering::SeqCst);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn compose(hwnd: HWND) {
    if hwnd.is_invalid() || hwnd.0.is_null() {
        return;
    }
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let width = rc.right - rc.left;
        let height = rc.bottom - rc.top;
        if width <= 0 || height <= 0 {
            return;
        }

        let screen = GetDC(HWND::default());
        if screen.is_invalid() {
            return;
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, width, height);
        if mem.is_invalid() || bmp.is_invalid() {
            if !mem.is_invalid() {
                let _ = DeleteDC(mem);
            }
            if !bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            let _ = ReleaseDC(HWND::default(), screen);
            return;
        }
        let old_bmp = SelectObject(mem, HGDIOBJ(bmp.0));

        let bg = CreateSolidBrush(COLORREF(0x001A140F));
        let _ = FillRect(mem, &rc, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let border = CreatePen(PS_SOLID, 1, COLORREF(0x00C26A2D));
        let fill = CreateSolidBrush(COLORREF(0x00221812));
        let old_pen = SelectObject(mem, HGDIOBJ(border.0));
        let old_brush = SelectObject(mem, HGDIOBJ(fill.0));
        let _ = RoundRect(mem, 1, 1, rc.right - 1, rc.bottom - 1, 22, 22);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        let _ = DeleteObject(HGDIOBJ(border.0));
        let _ = DeleteObject(HGDIOBJ(fill.0));

        let ui = voice::ui_snapshot();
        let pulse = ((PHASE.load(Ordering::Relaxed) % 8) as i32 - 4).unsigned_abs() as i32;
        let radius = 16 + pulse / 4;
        let cx = 42;
        let cy = 46;
        let color = if ui.error {
            COLORREF(0x003C3CFF)
        } else {
            COLORREF(0x002D3CCC)
        };
        let mic_brush = CreateSolidBrush(color);
        let null_pen = CreatePen(PS_SOLID, 1, color);
        let old_pen = SelectObject(mem, HGDIOBJ(null_pen.0));
        let old_brush = SelectObject(mem, HGDIOBJ(mic_brush.0));
        let _ = Ellipse(mem, cx - radius, cy - radius, cx + radius, cy + radius);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        let _ = DeleteObject(HGDIOBJ(mic_brush.0));
        let _ = DeleteObject(HGDIOBJ(null_pen.0));

        let font = CreateFontW(
            20,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
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
        let small = CreateFontW(
            16,
            0,
            0,
            0,
            400,
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
        SetBkMode(mem, TRANSPARENT);
        SetTextColor(mem, COLORREF(0x00FFFFFF));
        let old_font = SelectObject(mem, HGDIOBJ(font.0));
        let mut status = wide(&ui.status);
        let mut status_rc = RECT {
            left: 72,
            top: 16,
            right: rc.right - 16,
            bottom: 44,
        };
        let _ = DrawTextW(
            mem,
            &mut status,
            &mut status_rc,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );

        SelectObject(mem, HGDIOBJ(small.0));
        SetTextColor(mem, COLORREF(0x00D8D0C8));
        let preview = if ui.partial.is_empty() {
            "Нажмите клавишу, Esc или кликните, чтобы остановить".to_string()
        } else {
            ui.partial
        };
        let mut preview_wide = wide(&preview);
        let mut preview_rc = RECT {
            left: 72,
            top: 46,
            right: rc.right - 16,
            bottom: 78,
        };
        let _ = DrawTextW(
            mem,
            &mut preview_wide,
            &mut preview_rc,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );

        let level = voice::level() as i32;
        let bar = RECT {
            left: 24,
            top: 96,
            right: rc.right - 24,
            bottom: 108,
        };
        let track = CreateSolidBrush(COLORREF(0x00332C26));
        let _ = FillRect(mem, &bar, track);
        let _ = DeleteObject(HGDIOBJ(track.0));
        let fill_r = bar.left + (bar.right - bar.left) * level / 100;
        if fill_r > bar.left {
            let value = RECT {
                left: bar.left,
                top: bar.top,
                right: fill_r,
                bottom: bar.bottom,
            };
            let accent = CreateSolidBrush(COLORREF(0x00C26A2D));
            let _ = FillRect(mem, &value, accent);
            let _ = DeleteObject(HGDIOBJ(accent.0));
        }

        let mut hint = wide("Русский");
        let mut hint_rc = RECT {
            left: 24,
            top: 108,
            right: rc.right - 24,
            bottom: 124,
        };
        SetTextColor(mem, COLORREF(0x009A9088));
        let _ = DrawTextW(mem, &mut hint, &mut hint_rc, DT_SINGLELINE | DT_CENTER);

        SelectObject(mem, old_font);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(small.0));

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 235,
            AlphaFormat: 0,
        };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let src = POINT { x: 0, y: 0 };
        let _ = UpdateLayeredWindow(
            hwnd,
            screen,
            None,
            Some(&size),
            mem,
            Some(&src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        SelectObject(mem, old_bmp);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(HWND::default(), screen);
    }
}
