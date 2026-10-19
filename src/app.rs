use crate::settings::Settings;
use crate::util::{hwnd_from_isize, hwnd_to_isize};
use crate::{autostart, clipboard, keys, layout, tray, ui};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, InsertMenuW, KillTimer, LoadCursorW, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    SetMenuDefaultItem, SetTimer, SetWindowsHookExW, ShowWindow, TrackPopupMenu, TranslateMessage,
    UnhookWindowsHookEx, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HHOOK, HICON, IDC_ARROW,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_UP, MF_BYPOSITION, MSG, SW_HIDE, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WH_KEYBOARD_LL, WINDOW_EX_STYLE,
    WM_APP, WM_DESTROY, WM_KEYUP, WM_LBUTTONUP, WM_RBUTTONUP, WM_SYSKEYUP, WM_TIMER, WNDCLASSW,
    WS_OVERLAPPED,
};

pub const WM_CONVERT: u32 = WM_APP + 2;
pub const WM_BEGIN_CAPTURE: u32 = WM_APP + 3;
pub const WM_SAVE_SETTINGS: u32 = WM_APP + 4;
const WM_OPEN_SETTINGS: u32 = WM_APP + 6;
const IDM_SETTINGS: u32 = 1;
const IDM_TOGGLE: u32 = 2;
const IDM_EXIT: u32 = 3;
const TIMER_RESTORE: usize = 2;

static HOST: AtomicIsize = AtomicIsize::new(0);
static ENABLED: AtomicBool = AtomicBool::new(true);
static CAPTURING: AtomicBool = AtomicBool::new(false);
static HOTKEY_VK: AtomicU16 = AtomicU16::new(0);
static INTERVAL_MS: AtomicU32 = AtomicU32::new(400);
static PENDING_TAP: AtomicBool = AtomicBool::new(false);
static KEY_HELD: AtomicBool = AtomicBool::new(false);
static CHORDED: AtomicBool = AtomicBool::new(false);
static LAST_UP_MS: AtomicU64 = AtomicU64::new(0);
static EAT_KEYUP: AtomicU16 = AtomicU16::new(0);
static FIRE_ON_UP: AtomicBool = AtomicBool::new(false);

struct Inner {
    settings: Settings,
    settings_hwnd: isize,
    pending_hotkey: Option<u16>,
    clipboard_restore: Option<String>,
    icon: isize,
    hook: isize,
    first_run: bool,
}

static INNER: Mutex<Option<Inner>> = Mutex::new(None);

pub fn run() -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host_wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("LswiHost"),
            ..Default::default()
        };
        RegisterClassW(&wc);
        ui::register_class(instance.into());

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("LswiHost"),
            w!("LSWI"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            instance,
            None,
        )?;
        let _ = ShowWindow(hwnd, SW_HIDE);
        HOST.store(hwnd_to_isize(hwnd), Ordering::SeqCst);

        let (settings, first_run) = Settings::load();
        ENABLED.store(settings.enabled, Ordering::SeqCst);
        HOTKEY_VK.store(settings.hotkey_vk, Ordering::SeqCst);
        INTERVAL_MS.store(settings.double_tap_ms, Ordering::SeqCst);
        let _ = autostart::apply(settings.start_with_windows);

        let icon = tray::create_icon();
        tray::add(hwnd, icon);

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard), None, 0)?;

        *INNER.lock().unwrap() = Some(Inner {
            settings,
            settings_hwnd: 0,
            pending_hotkey: None,
            clipboard_restore: None,
            icon: icon.0 as isize,
            hook: hook.0 as isize,
            first_run,
        });

        if first_run {
            let _ = PostMessageW(hwnd, WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

fn host() -> HWND {
    hwnd_from_isize(HOST.load(Ordering::SeqCst))
}

fn icon_of(value: isize) -> HICON {
    HICON(value as *mut core::ffi::c_void)
}

fn hook_of(value: isize) -> HHOOK {
    HHOOK(value as *mut core::ffi::c_void)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

unsafe extern "system" fn low_level_keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam.0 != 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if (info.flags.0 & LLKHF_INJECTED.0) != 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let msg = wparam.0 as u32;
        let vk = info.vkCode as u16;
        let is_up = msg == WM_KEYUP
            || msg == WM_SYSKEYUP
            || (info.flags.0 & LLKHF_UP.0) != 0;
        if handle_key(vk, is_up) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn handle_key(vk: u16, is_up: bool) -> bool {
    if EAT_KEYUP.load(Ordering::SeqCst) == vk && is_up {
        EAT_KEYUP.store(0, Ordering::SeqCst);
        return true;
    }

    if CAPTURING.load(Ordering::SeqCst) {
        if is_up {
            return false;
        }
        CAPTURING.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = INNER.lock() {
            if let Some(inner) = guard.as_mut() {
                inner.pending_hotkey = Some(vk);
                if inner.settings_hwnd != 0 {
                    ui::set_hotkey_name(hwnd_from_isize(inner.settings_hwnd), vk);
                }
            }
        }
        EAT_KEYUP.store(vk, Ordering::SeqCst);
        return true;
    }

    if !ENABLED.load(Ordering::SeqCst) {
        return false;
    }

    let hotkey = HOTKEY_VK.load(Ordering::SeqCst);
    if !keys::matches_hotkey(vk, hotkey) {
        if !is_up {
            PENDING_TAP.store(false, Ordering::SeqCst);
            if KEY_HELD.load(Ordering::SeqCst) {
                CHORDED.store(true, Ordering::SeqCst);
            }
        }
        return false;
    }

    if is_up {
        KEY_HELD.store(false, Ordering::SeqCst);
        if FIRE_ON_UP.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = PostMessageW(host(), WM_CONVERT, WPARAM(0), LPARAM(0));
            }
            return true;
        }
        if CHORDED.swap(false, Ordering::SeqCst) {
            PENDING_TAP.store(false, Ordering::SeqCst);
        } else {
            PENDING_TAP.store(true, Ordering::SeqCst);
            LAST_UP_MS.store(now_ms(), Ordering::SeqCst);
        }
        return false;
    }

    if KEY_HELD.swap(true, Ordering::SeqCst) {
        return false;
    }

    let pending = PENDING_TAP.swap(false, Ordering::SeqCst);
    let elapsed = now_ms().saturating_sub(LAST_UP_MS.load(Ordering::SeqCst));
    if pending && elapsed <= u64::from(INTERVAL_MS.load(Ordering::SeqCst)) {
        FIRE_ON_UP.store(true, Ordering::SeqCst);
        return true;
    }
    false
}

unsafe extern "system" fn host_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    static TASKBAR: AtomicU32 = AtomicU32::new(0);
    let taskbar = TASKBAR.load(Ordering::Relaxed);
    if taskbar == 0 {
        let id = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR.store(id, Ordering::Relaxed);
    }
    if msg == TASKBAR.load(Ordering::Relaxed) && msg != 0 {
        if let Ok(guard) = INNER.lock() {
            if let Some(inner) = guard.as_ref() {
                tray::add(hwnd, icon_of(inner.icon));
            }
        }
        return LRESULT(0);
    }

    match msg {
        tray::WM_TRAY => {
            let mouse = lparam.0 as u32;
            if mouse == WM_LBUTTONUP {
                open_settings(hwnd);
            } else if mouse == WM_RBUTTONUP {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_OPEN_SETTINGS | WM_BEGIN_CAPTURE => {
            if msg == WM_BEGIN_CAPTURE {
                CAPTURING.store(true, Ordering::SeqCst);
                if let Ok(mut guard) = INNER.lock() {
                    if let Some(inner) = guard.as_mut() {
                        if inner.settings_hwnd != 0 {
                            ui::begin_capture(hwnd_from_isize(inner.settings_hwnd));
                        }
                    }
                }
            } else {
                open_settings(hwnd);
            }
            LRESULT(0)
        }
        WM_SAVE_SETTINGS => {
            save_from_ui();
            LRESULT(0)
        }
        WM_CONVERT => {
            std::thread::spawn(|| {
                clipboard::wait_modifiers_up();
                convert_now();
            });
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_RESTORE {
                let _ = KillTimer(hwnd, TIMER_RESTORE);
                restore_clipboard();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            shutdown();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn open_settings(host: HWND) {
    CAPTURING.store(false, Ordering::SeqCst);
    let instance = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    if let Ok(mut guard) = INNER.lock() {
        if let Some(inner) = guard.as_mut() {
            inner.pending_hotkey = None;
            inner.settings_hwnd = hwnd_to_isize(ui::open(host, instance.into(), &inner.settings));
        }
    }
}

fn save_from_ui() {
    let mut close_hwnd = HWND::default();
    if let Ok(mut guard) = INNER.lock() {
        if let Some(inner) = guard.as_mut() {
            if inner.settings_hwnd == 0 {
                return;
            }
            let settings_hwnd = hwnd_from_isize(inner.settings_hwnd);
            let mut next = ui::collect(settings_hwnd, &inner.settings);
            if let Some(vk) = inner.pending_hotkey {
                next.hotkey_vk = vk;
            }
            if next.save().is_ok() {
                let _ = autostart::apply(next.start_with_windows);
                ENABLED.store(next.enabled, Ordering::SeqCst);
                HOTKEY_VK.store(next.hotkey_vk, Ordering::SeqCst);
                INTERVAL_MS.store(next.double_tap_ms, Ordering::SeqCst);
                inner.settings = next;
                close_hwnd = settings_hwnd;
                inner.settings_hwnd = 0;
                inner.pending_hotkey = None;
                inner.first_run = false;
            }
        }
    }
    CAPTURING.store(false, Ordering::SeqCst);
    PENDING_TAP.store(false, Ordering::SeqCst);
    FIRE_ON_UP.store(false, Ordering::SeqCst);
    if !close_hwnd.is_invalid() {
        unsafe {
            let _ = DestroyWindow(close_hwnd);
        }
    }
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        if menu.is_invalid() {
            return;
        }
        let enabled = ENABLED.load(Ordering::SeqCst);
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION, IDM_SETTINGS as usize, w!("Настройки"));
        let toggle = if enabled { w!("Выключить") } else { w!("Включить") };
        let _ = InsertMenuW(menu, 1, MF_BYPOSITION, IDM_TOGGLE as usize, toggle);
        let _ = InsertMenuW(menu, 2, MF_BYPOSITION, IDM_EXIT as usize, w!("Выход"));
        let _ = SetMenuDefaultItem(menu, IDM_SETTINGS, 0);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match cmd.0 as u32 {
            IDM_SETTINGS => open_settings(hwnd),
            IDM_TOGGLE => toggle_enabled(),
            IDM_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

fn toggle_enabled() {
    if let Ok(mut guard) = INNER.lock() {
        if let Some(inner) = guard.as_mut() {
            inner.settings.enabled = !inner.settings.enabled;
            ENABLED.store(inner.settings.enabled, Ordering::SeqCst);
            let _ = inner.settings.save();
        }
    }
}

fn balloon(text: &str) {
    if let Ok(guard) = INNER.lock() {
        if let Some(inner) = guard.as_ref() {
            tray::balloon(host(), icon_of(inner.icon), "LSWI", text);
        }
    }
}

fn restore_clipboard() {
    let original = INNER
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(|i| i.clipboard_restore.take()));
    if let Some(text) = original {
        let _ = clipboard::write_unicode(&text);
    }
}

fn convert_now() {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let switch_after = INNER
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.settings.switch_layout_after))
        .unwrap_or(false);

    let layouts = layout::installed_layouts();
    let saved = clipboard::read_unicode();
    let Some(text) = clipboard::copy_selection() else {
        balloon("Не удалось прочитать выделенный текст. Выделите текст и повторите.");
        return;
    };
    if text.len() > 50_000 {
        return;
    }
    let current = layout::foreground_layout();
    let Some((converted, target)) = layout::convert_selection(&text, current, &layouts) else {
        balloon("Не получилось сменить раскладку этого текста.");
        return;
    };
    if !clipboard::paste_text(&converted) {
        balloon("Не удалось вставить преобразованный текст.");
        return;
    }
    if let Ok(mut guard) = INNER.lock() {
        if let Some(inner) = guard.as_mut() {
            inner.clipboard_restore = saved;
        }
    }
    unsafe {
        let _ = SetTimer(host(), TIMER_RESTORE, 500, None);
    }
    if switch_after && layouts.len() >= 2 {
        layout::activate_layout(target);
    }
}

fn shutdown() {
    PENDING_TAP.store(false, Ordering::SeqCst);
    FIRE_ON_UP.store(false, Ordering::SeqCst);
    CAPTURING.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = INNER.lock() {
        if let Some(inner) = guard.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook_of(inner.hook));
            }
            tray::remove(host());
            tray::destroy_icon(icon_of(inner.icon));
        }
    }
}
