use crate::util::{copy_wide, wide};
use windows::Win32::Foundation::{COLORREF, HWND, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW,
    FillRect, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, HGDIOBJ, TRANSPARENT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW, NOTIFY_ICON_MESSAGE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, ICONINFO, WM_APP, HICON,
};

pub const TRAY_ID: u32 = 1;
pub const WM_TRAY: u32 = WM_APP + 1;

pub fn add(hwnd: HWND, icon: HICON) {
    notify(hwnd, icon, NIM_ADD, None);
}

pub fn remove(hwnd: HWND) {
    let mut data = nid(hwnd, HICON::default());
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &mut data);
    }
}

pub fn balloon(hwnd: HWND, icon: HICON, title: &str, text: &str) {
    notify(hwnd, icon, NIM_MODIFY, Some((title, text)));
}

fn notify(hwnd: HWND, icon: HICON, action: NOTIFY_ICON_MESSAGE, info: Option<(&str, &str)>) {
    let mut data = nid(hwnd, icon);
    if let Some((title, text)) = info {
        data.uFlags |= NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, text);
    }
    unsafe {
        let _ = Shell_NotifyIconW(action, &mut data);
    }
}

fn nid(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    copy_wide(&mut data.szTip, "LSWI — смена раскладки выделенного текста");
    data
}

pub fn create_icon() -> HICON {
    unsafe {
        let hdc_screen = GetDC(HWND::default());
        let hdc = CreateCompatibleDC(hdc_screen);
        let color = CreateCompatibleBitmap(hdc_screen, 32, 32);
        let mask = CreateCompatibleBitmap(hdc_screen, 32, 32);
        let old = SelectObject(hdc, HGDIOBJ(color.0));
        let brush = CreateSolidBrush(COLORREF(0x00C26A2D));
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 32,
            bottom: 32,
        };
        let _ = FillRect(hdc, &rc, brush);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut label = wide("LS");
        rc.left = 1;
        rc.top = 2;
        let _ = DrawTextW(hdc, &mut label, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        let old_mask = SelectObject(hdc, HGDIOBJ(mask.0));
        let black = CreateSolidBrush(COLORREF(0));
        let mask_rc = RECT {
            left: 0,
            top: 0,
            right: 32,
            bottom: 32,
        };
        let _ = FillRect(hdc, &mask_rc, black);
        SelectObject(hdc, old_mask);
        let _ = DeleteObject(HGDIOBJ(black.0));
        let _ = DeleteDC(hdc);
        let _ = ReleaseDC(HWND::default(), hdc_screen);

        let info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon
    }
}

pub fn destroy_icon(icon: HICON) {
    if !icon.is_invalid() {
        unsafe {
            let _ = DestroyIcon(icon);
        }
    }
}

