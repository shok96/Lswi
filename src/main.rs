#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod keys;
mod layout;
mod settings;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod util;
#[cfg(windows)]
mod voice;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix;

fn main() {
    #[cfg(windows)]
    windows_main();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    unix::run();

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        eprintln!("LSWI поддерживает Windows, macOS и Linux.");
    }
}

#[cfg(windows)]
fn windows_main() {
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, TRUE};
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, PostMessageW, SetForegroundWindow, WM_APP,
    };

    unsafe {
        let mutex = match CreateMutexW(None, TRUE, w!("Local\\LswiSingleton")) {
            Ok(handle) => handle,
            Err(_) => return,
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(existing) = FindWindowW(w!("LswiHost"), None) {
                let _ = PostMessageW(
                    existing,
                    WM_APP + 6,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
                let _ = SetForegroundWindow(existing);
            }
            let _ = CloseHandle(mutex);
            return;
        }

        let _ = app::run();
        let _ = ReleaseMutex(mutex);
        let _ = CloseHandle(mutex);
    }
}
