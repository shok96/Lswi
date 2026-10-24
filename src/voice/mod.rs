mod capture;
mod download;
mod vad;
mod vosk;

use crate::clipboard;
use crate::util::{hwnd_from_isize, hwnd_to_isize};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_VOICE_SHOW: u32 = WM_APP + 20;
pub const WM_VOICE_HIDE: u32 = WM_APP + 21;
pub const WM_VOICE_REFRESH: u32 = WM_APP + 22;
pub const WM_MIC_LEVEL: u32 = WM_APP + 23;
pub const WM_MIC_DEVICES: u32 = WM_APP + 24;

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
static POLISH: AtomicBool = AtomicBool::new(true);
static DEVICE_LOST: AtomicBool = AtomicBool::new(false);
static UI: Mutex<UiState> = Mutex::new(UiState::new());
static LAST_USED_MS: AtomicU64 = AtomicU64::new(0);
static IDLE_WATCH: OnceLock<()> = OnceLock::new();
const IDLE_MS: u64 = 3 * 60 * 1000;

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
    IDLE_WATCH.get_or_init(|| {
        std::thread::Builder::new()
            .name("lswi-model-idle".into())
            .spawn(idle_models)
            .ok();
    });
}

pub fn start_watch() {
    capture::start_watch();
}

pub fn stop_watch() {
    capture::stop_watch();
}

pub fn default_capture_changed() -> bool {
    capture::default_changed()
}

pub fn meter_running() -> bool {
    METER.load(Ordering::SeqCst)
}

pub(super) fn on_capture_error() {
    DEVICE_LOST.store(true, Ordering::SeqCst);
    notify_device_change();
}

pub(super) fn notify_device_change() {
    post(WM_MIC_DEVICES, 0, 0);
}

pub fn apply_settings(enabled: bool, vk: u16, mods: u16, microphone_id: &str, polish: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    HOTKEY_VK.store(vk, Ordering::SeqCst);
    HOTKEY_MODS.store(mods, Ordering::SeqCst);
    POLISH.store(polish, Ordering::SeqCst);
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

    let _ = vad::ensure(&mut |msg| {
        if msg == last_status {
            return;
        }
        last_status = msg.to_string();
        set_ui(msg, "", false);
        post(WM_VOICE_REFRESH, 0, 0);
    });
    touch_models();

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

    if POLISH.load(Ordering::SeqCst) {
        let _ = crate::smart::ensure_model(&mut |msg| {
            if msg == last_status {
                return;
            }
            last_status = msg.to_string();
            set_ui(msg, "", false);
            post(WM_VOICE_REFRESH, 0, 0);
        });
    }

    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(48);
    DEVICE_LOST.store(false, Ordering::SeqCst);
    let stream = match capture::start_stream(device_id, tx, &LEVEL, &STOP) {
        Ok((stream, rate)) => {
            set_ui("Говорите…", "", false);
            post(WM_VOICE_REFRESH, 0, 0);
            clipboard::wait_modifiers_up();
            run_recognize(recognizer, rx, rate);
            drop(stream);
            if DEVICE_LOST.swap(false, Ordering::SeqCst) {
                set_ui("Микрофон отключён", "", true);
                post(WM_VOICE_REFRESH, 0, 0);
                std::thread::sleep(Duration::from_secs(2));
            }
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
    let mut gate = vad::Gate::open();
    while !STOP.load(Ordering::SeqCst) {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(80)) else {
            continue;
        };
        let pcm = capture::resample_to_16k(&chunk, rate, &mut acc);
        if pcm.is_empty() {
            continue;
        }
        let voiced = match gate.as_mut() {
            Some(gate) => gate.push(&pcm),
            None => pcm,
        };
        if voiced.is_empty() {
            continue;
        }
        match recognizer.accept(&voiced) {
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
    if let Some(gate) = gate.as_mut() {
        let tail = gate.finish();
        if !tail.is_empty() {
            let _ = recognizer.accept(&tail);
        }
    }
    if let Ok(text) = recognizer.final_text() {
        injector.commit(&text);
    }
    drop(recognizer);
    injector.finish();
    touch_models();
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

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis() as u64
}

fn touch_models() {
    LAST_USED_MS.store(now_ms(), Ordering::SeqCst);
}

fn idle_models() {
    loop {
        std::thread::sleep(Duration::from_secs(30));
        if ACTIVE.load(Ordering::SeqCst) {
            continue;
        }
        let last = LAST_USED_MS.load(Ordering::SeqCst);
        if last == 0 || now_ms().saturating_sub(last) < IDLE_MS {
            continue;
        }
        LAST_USED_MS.store(0, Ordering::SeqCst);
        vosk::unload();
        vad::release();
        crate::smart::release_model();
    }
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
    typed: String,
    tail: String,
    last_raw: String,
}

impl Injector {
    fn new() -> Self {
        Self {
            typed: String::new(),
            tail: String::new(),
            last_raw: String::new(),
        }
    }

    fn preview(&mut self, text: &str) {
        let next = normalize(text);
        if next.is_empty() || next == self.last_raw || next == self.tail {
            return;
        }
        sync(&self.tail, &next);
        self.tail = next.clone();
        set_ui("Говорите…", &next, false);
        post(WM_VOICE_REFRESH, 0, 0);
    }

    fn commit(&mut self, text: &str) {
        let piece = normalize(text);
        if piece.is_empty() {
            if !self.tail.is_empty() {
                sync(&self.tail, "");
                self.tail.clear();
                set_ui("Говорите…", "", false);
                post(WM_VOICE_REFRESH, 0, 0);
            }
            return;
        }
        if piece == self.last_raw {
            return;
        }
        if !self.tail.is_empty() {
            sync(&self.tail, "");
            self.tail.clear();
        }
        let expanded = apply_commands(&piece);
        self.last_raw = piece;
        let mut phrase = punctuate_phrase(&expanded);
        if self.typed.is_empty() || ends_sentence(&self.typed) {
            phrase = capitalize(&phrase);
        }
        if !phrase.ends_with(' ') && !phrase.ends_with('\n') {
            phrase.push(' ');
        }
        clipboard::type_unicode(&phrase);
        self.typed.push_str(&phrase);
        set_ui("Говорите…", phrase.trim(), false);
        post(WM_VOICE_REFRESH, 0, 0);
    }

    fn finish(&mut self) {
        if self.tail.is_empty() {
            return;
        }
        let pending = self.tail.clone();
        self.last_raw.clear();
        self.commit(&pending);
    }
}

fn punctuate_phrase(raw: &str) -> String {
    if raw.is_empty() || !POLISH.load(Ordering::SeqCst) || !crate::smart::model_ready() {
        return raw.to_string();
    }
    let polished = crate::smart::polish_voice_ex(raw, true);
    if same_words(raw, &polished) {
        polished
    } else {
        raw.to_string()
    }
}

fn same_words(original: &str, polished: &str) -> bool {
    word_keys(original) == word_keys(polished)
}

fn word_keys(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let key: String = word
                .chars()
                .filter(|ch| ch.is_alphabetic() || *ch == '-')
                .flat_map(|ch| ch.to_lowercase())
                .collect();
            if key.is_empty() { None } else { Some(key) }
        })
        .collect()
}

fn ends_sentence(text: &str) -> bool {
    text.chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        .is_some_and(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

fn apply_commands(text: &str) -> String {
    text.split_whitespace()
        .map(|w| match w.to_lowercase().as_str() {
            "точка" | "dot" => ".".to_string(),
            "запятая" | "comma" => ",".to_string(),
            "вопрос" | "вопросительный" => "?".to_string(),
            "восклицание" | "восклицательный" => "!".to_string(),
            "тире" => "—".to_string(),
            "newline" | "enter" => "\n".to_string(),
            _ => w.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests {
    use super::same_words;

    #[test]
    fn punctuation_may_not_replace_words() {
        assert!(same_words(
            "привет меня зовут роман",
            "Привет, меня зовут Роман."
        ));
        assert!(!same_words("привет меня зовут роман", "Привет, Роман!"));
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
