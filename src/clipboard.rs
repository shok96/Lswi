use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Controls::EM_GETSEL;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_C,
    VK_CONTROL, VK_INSERT, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT,
    VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, SendMessageTimeoutW, SendMessageW, GUITHREADINFO, SMTO_ABORTIFHUNG,
    WM_COPY, WM_PASTE,
};

const CF_UNICODETEXT: u32 = 13;

fn open_clipboard_retry() -> bool {
    for _ in 0..40 {
        if unsafe { OpenClipboard(HWND::default()) }.is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

pub fn read_unicode() -> Option<String> {
    if !open_clipboard_retry() {
        return None;
    }
    let result = unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
        if handle.is_invalid() {
            None
        } else {
            let hmem = HGLOBAL(handle.0);
            let size = GlobalSize(hmem);
            if size < 2 {
                None
            } else {
                let ptr = GlobalLock(hmem);
                if ptr.is_null() {
                    None
                } else {
                    let bytes = std::slice::from_raw_parts(ptr as *const u16, size / 2);
                    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
                    let text = String::from_utf16_lossy(&bytes[..end]);
                    let _ = GlobalUnlock(hmem);
                    Some(text)
                }
            }
        }
    };
    unsafe {
        let _ = CloseClipboard();
    }
    result.filter(|s| !s.is_empty())
}

pub fn write_unicode(text: &str) -> bool {
    let mut encoded: Vec<u16> = text.encode_utf16().collect();
    encoded.push(0);
    let bytes = encoded.len() * 2;
    if !open_clipboard_retry() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let alloc = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseClipboard();
                return false;
            }
        };
        let ptr = GlobalLock(alloc);
        if ptr.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), ptr as *mut u16, encoded.len());
        let _ = GlobalUnlock(alloc);
        if SetClipboardData(CF_UNICODETEXT, HANDLE(alloc.0)).is_err() {
            let _ = CloseClipboard();
            return false;
        }
        let _ = CloseClipboard();
    }
    true
}

fn focused_control() -> HWND {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(tid, &mut info).is_ok() {
            if !info.hwndFocus.is_invalid() {
                return info.hwndFocus;
            }
            if !info.hwndCaret.is_invalid() {
                return info.hwndCaret;
            }
        }
        fg
    }
}

fn read_edit_selection(hwnd: HWND) -> Option<String> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut start: u32 = 0;
    let mut end: u32 = 0;
    unsafe {
        let _ = SendMessageW(
            hwnd,
            EM_GETSEL,
            WPARAM(&mut start as *mut u32 as usize),
            LPARAM(&mut end as *mut u32 as isize),
        );
    }
    if end <= start {
        return None;
    }
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    let from = (start as usize).min(buf.len());
    let to = (end as usize).min(buf.len());
    if to <= from {
        return None;
    }
    let text = String::from_utf16_lossy(&buf[from..to]);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(i32::from(vk.0)) as u16 & 0x8000 != 0 }
}

pub fn wait_modifiers_up() {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        let busy = [
            VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU,
        ]
        .into_iter()
        .any(key_down);
        if !busy {
            break;
        }
        thread::sleep(Duration::from_millis(15));
    }
    thread::sleep(Duration::from_millis(40));
}

fn clear_clipboard() {
    if open_clipboard_retry() {
        unsafe {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }
}

fn is_extended(vk: VIRTUAL_KEY) -> bool {
    matches!(vk, VK_INSERT | VK_RCONTROL | VK_RMENU)
}

fn send_vk_batch(events: &[(VIRTUAL_KEY, bool)]) {
    let mut inputs: Vec<INPUT> = events
        .iter()
        .map(|(vk, up)| {
            let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as u16;
            let mut flags = if *up {
                KEYEVENTF_KEYUP
            } else {
                KEYBD_EVENT_FLAGS(0)
            };
            if is_extended(*vk) {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: *vk,
                        wScan: scan,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        })
        .collect();
    unsafe {
        let _ = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn send_combo(vk: VIRTUAL_KEY) {
    send_vk_batch(&[
        (VK_SHIFT, true),
        (VK_LSHIFT, true),
        (VK_RSHIFT, true),
        (VK_MENU, true),
        (VK_CONTROL, true),
        (VK_CONTROL, false),
        (vk, false),
        (vk, true),
        (VK_CONTROL, true),
    ]);
}

fn post_copy_or_paste(hwnd: HWND, paste: bool) {
    if hwnd.is_invalid() {
        return;
    }
    let msg = if paste { WM_PASTE } else { WM_COPY };
    unsafe {
        let _ = SendMessageTimeoutW(hwnd, msg, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 200, None);
    }
}

fn wait_clipboard_text(before: u32, ms: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < deadline {
        if unsafe { GetClipboardSequenceNumber() } != before {
            if let Some(text) = read_unicode() {
                return Some(text);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

pub fn copy_selection() -> Option<String> {
    let hwnd = focused_control();
    if let Some(text) = read_edit_selection(hwnd) {
        return Some(text);
    }

    clear_clipboard();
    let before = unsafe { GetClipboardSequenceNumber() };
    post_copy_or_paste(hwnd, false);
    if let Some(text) = wait_clipboard_text(before, 200) {
        return Some(text);
    }

    send_combo(VK_C);
    if let Some(text) = wait_clipboard_text(before, 350) {
        return Some(text);
    }

    send_combo(VK_INSERT);
    wait_clipboard_text(before, 350)
}

pub fn paste_text(text: &str) -> bool {
    if !write_unicode(text) {
        return false;
    }
    let hwnd = focused_control();
    thread::sleep(Duration::from_millis(30));
    post_copy_or_paste(hwnd, true);
    send_combo(VK_V);
    true
}
