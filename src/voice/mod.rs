mod capture;
mod download;
mod vosk;

use crate::clipboard;
use crate::util::{hwnd_from_isize, hwnd_to_isize};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU16, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_VOICE_SHOW: u32 = WM_APP + 20;
pub const WM_VOICE_HIDE: u32 = WM_APP + 21;
pub const WM_VOICE_REFRESH: u32 = WM_APP + 22;
pub const WM_MIC_LEVEL: u32 = WM_APP + 23;

static HOST: AtomicIsize = AtomicIsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static METER: AtomicBool = AtomicBool::new(false);
static METER_STOP: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(true);
static HOTKEY_VK: AtomicU16 = AtomicU16::new(0);
static HOTKEY_MODS: AtomicU16 = AtomicU16::new(0);
static LEVEL: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID: Mutex<String> = Mutex::new(String::new());
static UI: Mutex<UiState> = Mutex::new(UiState::new());

#[derive(Clone)]
pub struct UiState {
    pub status: String,
    pub partial: String,
    pub error: bool,
}

impl UiState {
    const fn new() -> Self {
        Self {
            status: String::new(),
            partial: String::new(),
            error: false,
        }
    }
}

pub use capture::list_microphones;

pub fn set_host(hwnd: HWND) {
    HOST.store(hwnd_to_isize(hwnd), Ordering::SeqCst);
}

pub fn apply_settings(enabled: bool, vk: u16, mods: u16, microphone_id: &str) {
    ENABLED.store(enabled, Ordering::SeqCst);
    HOTKEY_VK.store(vk, Ordering::SeqCst);
    HOTKEY_MODS.store(mods, Ordering::SeqCst);
    if let Ok(mut id) = DEVICE_ID.lock() {
        *id = microphone_id.to_string();
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn hotkey() -> (u16, u16) {
    (
        HOTKEY_VK.load(Ordering::SeqCst),
        HOTKEY_MODS.load(Ordering::SeqCst),
    )
}

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

pub fn level() -> u32 {
    LEVEL.load(Ordering::SeqCst)
}

pub fn ui_snapshot() -> UiState {
    UI.lock().map(|g| g.clone()).unwrap_or_else(|_| UiState::new())
}

pub fn selected_device() -> String {
    DEVICE_ID.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn toggle() {
    if ACTIVE.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        return;
    }
    start();
}

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

pub fn start_meter(device_id: &str) {
    stop_meter();
    let id = device_id.to_string();
    std::thread::Builder::new()
        .name("lswi-mic-meter".into())
        .spawn(move || run_meter(&id))
        .ok();
}

pub fn stop_meter() {
    METER.store(false, Ordering::SeqCst);
    METER_STOP.store(true, Ordering::SeqCst);
}

fn start() {
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    STOP.store(false, Ordering::SeqCst);
    stop_meter();
    std::thread::sleep(Duration::from_millis(50));
    let device_id = selected_device();
    std::thread::Builder::new()
        .name("lswi-voice".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_session(&device_id);
            }));
            if result.is_err() {
                set_ui("Сбой голосового ввода", "", true);
            }
            ACTIVE.store(false, Ordering::SeqCst);
            LEVEL.store(0, Ordering::SeqCst);
            post(WM_VOICE_HIDE, 0, 0);
        })
        .ok();
}

fn run_session(device_id: &str) {
    set_ui("Подготовка…", "", false);
    post(WM_VOICE_SHOW, 0, 0);

    let mut last_status = String::new();
    let assets = match download::ensure(&mut |msg| {
        if msg == last_status {
            return;
        }
        last_status = msg.to_string();
        set_ui(msg, "", false);
        post(WM_VOICE_REFRESH, 0, 0);
    }) {
        Ok(assets) => assets,
        Err(err) => {
            set_ui(&err, "", true);
            post(WM_VOICE_REFRESH, 0, 0);
            std::thread::sleep(Duration::from_secs(4));
            return;
        }
    };

    set_ui("Загрузка русской модели…", "", false);
    post(WM_VOICE_REFRESH, 0, 0);
    if let Err(err) = vosk::load(&assets.dll, &assets.model) {
        set_ui(&err, "", true);
        post(WM_VOICE_REFRESH, 0, 0);
        std::thread::sleep(Duration::from_secs(4));
        return;
    }

    let recognizer = match vosk::recognizer(16_000.0) {
        Ok(rec) => rec,
        Err(err) => {
            set_ui(&err, "", true);
            post(WM_VOICE_REFRESH, 0, 0);
            std::thread::sleep(Duration::from_secs(3));
            return;
        }
    };

    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(6);
    let stream = match capture::start_stream(device_id, tx, &LEVEL, &STOP) {
        Ok((stream, rate)) => {
            set_ui("Говорите…", "", false);
            post(WM_VOICE_REFRESH, 0, 0);
            clipboard::wait_modifiers_up();
            run_recognize(recognizer, rx, rate);
            drop(stream);
        }
        Err(err) => {
            set_ui(&err, "", true);
            post(WM_VOICE_REFRESH, 0, 0);
            std::thread::sleep(Duration::from_secs(3));
        }
    };
    let _ = stream;
}

fn run_recognize(recognizer: vosk::Recognizer, rx: mpsc::Receiver<Vec<f32>>, rate: u32) {
    let mut acc = 0.0f32;
    let mut injector = Injector::new();
    while !STOP.load(Ordering::SeqCst) {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(80)) else {
            continue;
        };
        let pcm = capture::resample_to_16k(&chunk, rate, &mut acc);
        if pcm.is_empty() {
            continue;
        }
        match recognizer.accept(&pcm) {
            Ok(true) => {
                if let Ok(text) = recognizer.result_text() {
                    injector.commit(&text);
                }
            }
            Ok(false) => {
                if let Ok(partial) = recognizer.partial_text() {
                    injector.preview(&partial);
                }
            }
            Err(err) => {
                set_ui(&err, "", true);
                post(WM_VOICE_REFRESH, 0, 0);
                break;
            }
        }
    }
    if let Ok(text) = recognizer.final_text() {
        injector.commit(&text);
    }
}

fn run_meter(device_id: &str) {
    METER.store(true, Ordering::SeqCst);
    METER_STOP.store(false, Ordering::SeqCst);
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(2);
    let Ok((stream, _)) = capture::start_stream(device_id, tx, &LEVEL, &METER_STOP) else {
        METER.store(false, Ordering::SeqCst);
        LEVEL.store(0, Ordering::SeqCst);
        post(WM_MIC_LEVEL, 0, 0);
        return;
    };
    while METER.load(Ordering::SeqCst) && !METER_STOP.load(Ordering::SeqCst) {
        let _ = rx.recv_timeout(Duration::from_millis(50));
        post(WM_MIC_LEVEL, level() as usize, 0);
        std::thread::sleep(Duration::from_millis(40));
    }
    METER_STOP.store(true, Ordering::SeqCst);
    drop(stream);
    METER.store(false, Ordering::SeqCst);
    LEVEL.store(0, Ordering::SeqCst);
    post(WM_MIC_LEVEL, 0, 0);
}

fn set_ui(status: &str, partial: &str, error: bool) {
    if let Ok(mut ui) = UI.lock() {
        ui.status = status.to_string();
        ui.partial = partial.to_string();
        ui.error = error;
    }
}

fn post(msg: u32, wparam: usize, lparam: isize) {
    let hwnd = hwnd_from_isize(HOST.load(Ordering::SeqCst));
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        let _ = PostMessageW(hwnd, msg, WPARAM(wparam), LPARAM(lparam));
    }
}

struct Injector {
    tail: String,
    started: bool,
    last: String,
}

impl Injector {
    fn new() -> Self {
        Self {
            tail: String::new(),
            started: false,
            last: String::new(),
        }
    }

    fn preview(&mut self, text: &str) {
        let next = normalize(text);
        if next == self.tail || next == self.last {
            return;
        }
        sync(&self.tail, &next);
        self.tail = next.clone();
        set_ui("Говорите…", &next, false);
        post(WM_VOICE_REFRESH, 0, 0);
    }

    fn commit(&mut self, text: &str) {
        let mut next = normalize(text);
        if next.is_empty() {
            if !self.tail.is_empty() {
                sync(&self.tail, "");
                self.tail.clear();
                set_ui("Говорите…", "", false);
                post(WM_VOICE_REFRESH, 0, 0);
            }
            return;
        }
        if !self.started {
            next = capitalize(&next);
            self.started = true;
        }
        if !next.ends_with(' ') {
            next.push(' ');
        }
        sync(&self.tail, &next);
        self.last = next.trim().to_string();
        self.tail.clear();
        set_ui("Говорите…", next.trim(), false);
        post(WM_VOICE_REFRESH, 0, 0);
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn sync(current: &str, target: &str) {
    let common = current
        .chars()
        .zip(target.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let back = current.chars().count().saturating_sub(common);
    clipboard::backspace(back);
    let typed: String = target.chars().skip(common).collect();
    clipboard::type_unicode(&typed);
}
