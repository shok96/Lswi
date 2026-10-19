mod clipboard;
mod listen;
mod map;

use crate::keys;
use crate::layout;
use crate::settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

static SWITCH_LAYOUT: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub enum AppEvent {
    Convert,
    ListenFailed,
    KeyAssigned(u16),
    Menu(MenuEvent),
    Tray(TrayIconEvent),
}

struct Ui {
    tray: TrayIcon,
    enabled_item: CheckMenuItem,
    autostart_item: CheckMenuItem,
    switch_item: CheckMenuItem,
    assign_item: MenuItem,
    quit_item: MenuItem,
    interval_items: Vec<(u32, CheckMenuItem)>,
    settings: Settings,
}

pub fn run() {
    let _lock = match singleton_lock() {
        Some(file) => file,
        None => {
            eprintln!("LSWI уже запущена");
            return;
        }
    };

    #[cfg(target_os = "linux")]
    {
        if gtk::init().is_err() {
            eprintln!("LSWI: не удалось инициализировать GTK — иконка в трее может не появиться.");
        }
    }

    let (mut settings, first_run) = Settings::load();
    if first_run {
        let _ = settings.save();
    }
    let _ = crate::autostart::apply(settings.start_with_windows);
    apply_runtime(&settings);

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    listen::start(proxy.clone());

    TrayIconEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(AppEvent::Tray(event));
        }
    }));
    MenuEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(AppEvent::Menu(event));
        }
    }));

    let mut ui = match build_ui(&settings) {
        Ok(ui) => ui,
        Err(err) => {
            eprintln!("LSWI: не удалось создать иконку в трее: {err}");
            return;
        }
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let Event::UserEvent(app_event) = event else {
            return;
        };
        match app_event {
            AppEvent::Convert => {
                std::thread::spawn(convert_now);
            }
            AppEvent::ListenFailed => {
                let _ = ui.tray.set_tooltip(Some(
                    "LSWI: нет доступа к клавиатуре. macOS — Универсальный доступ; Linux — лучше X11.",
                ));
            }
            AppEvent::KeyAssigned(vk) => {
                ui.settings.hotkey_vk = vk;
                save_settings(&mut ui);
                refresh_labels(&ui);
            }
            AppEvent::Menu(event) => handle_menu(&mut ui, event, control_flow),
            AppEvent::Tray(_) => {}
        }
    });
}

fn apply_runtime(settings: &Settings) {
    listen::ENABLED.store(settings.enabled, Ordering::SeqCst);
    listen::HOTKEY_VK.store(settings.hotkey_vk, Ordering::SeqCst);
    listen::INTERVAL_MS.store(settings.double_tap_ms, Ordering::SeqCst);
    SWITCH_LAYOUT.store(settings.switch_layout_after, Ordering::SeqCst);
}

fn save_settings(ui: &mut Ui) {
    ui.settings.sanitize();
    let _ = ui.settings.save();
    let _ = crate::autostart::apply(ui.settings.start_with_windows);
    apply_runtime(&ui.settings);
    listen::reset_taps();
}

fn handle_menu(ui: &mut Ui, event: MenuEvent, control_flow: &mut ControlFlow) {
    if event.id == *ui.enabled_item.id() {
        ui.settings.enabled = ui.enabled_item.is_checked();
        save_settings(ui);
        return;
    }
    if event.id == *ui.autostart_item.id() {
        ui.settings.start_with_windows = ui.autostart_item.is_checked();
        save_settings(ui);
        return;
    }
    if event.id == *ui.switch_item.id() {
        ui.settings.switch_layout_after = ui.switch_item.is_checked();
        save_settings(ui);
        return;
    }
    if event.id == *ui.assign_item.id() {
        listen::CAPTURING.store(true, Ordering::SeqCst);
        ui.assign_item.set_text("Нажмите клавишу…");
        return;
    }
    for (ms, item) in &ui.interval_items {
        if event.id == *item.id() {
            ui.settings.double_tap_ms = *ms;
            save_settings(ui);
            sync_interval_checks(ui);
            return;
        }
    }
    if event.id == *ui.quit_item.id() {
        *control_flow = ControlFlow::Exit;
    }
}

fn sync_interval_checks(ui: &Ui) {
    for (ms, item) in &ui.interval_items {
        item.set_checked(*ms == ui.settings.double_tap_ms);
    }
}

fn refresh_labels(ui: &Ui) {
    ui.assign_item.set_text(assign_label(&ui.settings));
    let _ = ui.tray.set_tooltip(Some(tooltip(&ui.settings)));
}

fn assign_label(settings: &Settings) -> String {
    format!("Назначить клавишу ({})", keys::display_name(settings.hotkey_vk))
}

fn tooltip(settings: &Settings) -> String {
    format!("LSWI — {}", keys::display_name(settings.hotkey_vk))
}

fn build_ui(settings: &Settings) -> Result<Ui, Box<dyn std::error::Error>> {
    let enabled_item = CheckMenuItem::with_id("enabled", "Включено", true, settings.enabled, None);
    let autostart_item = CheckMenuItem::with_id(
        "autostart",
        "Запускать при входе в систему",
        true,
        settings.start_with_windows,
        None,
    );
    let switch_item = CheckMenuItem::with_id(
        "switch",
        "Менять раскладку после замены текста",
        true,
        settings.switch_layout_after,
        None,
    );
    let assign_item = MenuItem::with_id("assign", assign_label(settings), true, None);
    let quit_item = MenuItem::with_id("quit", "Выход", true, None);

    let presets = [250_u32, 400, 600, 800, 1000];
    let interval_items: Vec<(u32, CheckMenuItem)> = presets
        .into_iter()
        .map(|ms| {
            let item = CheckMenuItem::with_id(
                format!("int-{ms}"),
                format!("{ms} мс"),
                true,
                settings.double_tap_ms == ms,
                None,
            );
            (ms, item)
        })
        .collect();

    let interval_menu = Submenu::new("Интервал двойного нажатия", true);
    for (_, item) in &interval_items {
        interval_menu.append(item)?;
    }

    let menu = Menu::new();
    menu.append(&enabled_item)?;
    menu.append(&autostart_item)?;
    menu.append(&switch_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&assign_item)?;
    menu.append(&interval_menu)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tooltip(settings))
        .with_icon(app_icon())
        .build()?;

    Ok(Ui {
        tray,
        enabled_item,
        autostart_item,
        switch_item,
        assign_item,
        quit_item,
        interval_items,
        settings: settings.clone(),
    })
}

fn app_icon() -> Icon {
    const S: u32 = 32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            let i = ((y * S + x) * 4) as usize;
            let dx = x as i32 - 15;
            let dy = y as i32 - 15;
            if dx * dx + dy * dy <= 13 * 13 {
                rgba[i] = 0x2B;
                rgba[i + 1] = 0x6C;
                rgba[i + 2] = 0xEF;
                rgba[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, S, S).expect("icon")
}

fn convert_now() {
    if listen::CONVERTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = clipboard::snapshot();
    std::thread::sleep(Duration::from_millis(40));
    let converted = clipboard::copy_selection().and_then(|text| layout::convert_text(&text));
    if let Some(converted) = converted {
        let _ = clipboard::paste_text(&converted);
        if SWITCH_LAYOUT.load(Ordering::SeqCst) {
            switch_input_layout(layout::looks_cyrillic(&converted));
        }
    }
    clipboard::restore_later(previous);
    listen::CONVERTING.store(false, Ordering::SeqCst);
}

fn switch_input_layout(cyrillic: bool) {
    #[cfg(target_os = "linux")]
    {
        let ibus = if cyrillic { "xkb:ru::rus" } else { "xkb:us::eng" };
        let ok = std::process::Command::new("ibus")
            .args(["engine", ibus])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            let layout = if cyrillic { "ru" } else { "us" };
            let _ = std::process::Command::new("xkb-switch")
                .args(["-s", layout])
                .status();
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = cyrillic;
        let _ = std::process::Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to keystroke space using {control down}"#,
            ])
            .status();
    }
}

fn singleton_lock() -> Option<std::fs::File> {
    let mut dir = dirs::runtime_dir().or_else(dirs::config_dir)?;
    dir.push("Lswi");
    std::fs::create_dir_all(&dir).ok()?;
    dir.push("lswi.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&dir)
        .ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        const LOCK_EX: i32 = 2;
        const LOCK_NB: i32 = 4;
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc != 0 {
            return None;
        }
    }
    Some(file)
}
