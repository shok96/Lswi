use super::map;
use crate::keys;
use rdev::{listen, Event, EventType};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tao::event_loop::EventLoopProxy;

use super::AppEvent;

pub static ENABLED: AtomicBool = AtomicBool::new(true);
pub static CAPTURING: AtomicBool = AtomicBool::new(false);
pub static CONVERTING: AtomicBool = AtomicBool::new(false);
pub static HOTKEY_VK: AtomicU16 = AtomicU16::new(0);
pub static INTERVAL_MS: AtomicU32 = AtomicU32::new(400);
static PENDING_TAP: AtomicBool = AtomicBool::new(false);
static KEY_HELD: AtomicBool = AtomicBool::new(false);
static CHORDED: AtomicBool = AtomicBool::new(false);
static LAST_UP_MS: AtomicU64 = AtomicU64::new(0);
static FIRE_ON_UP: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn start(proxy: EventLoopProxy<AppEvent>) {
    std::thread::Builder::new()
        .name("lswi-listen".into())
        .spawn(move || {
            if let Err(err) = listen(move |event| on_event(&proxy, event)) {
                eprintln!("LSWI: не удалось слушать клавиатуру: {err:?}");
                eprintln!("macOS: дайте приложению доступ в Системные настройки → Конфиденциальность → Универсальный доступ.");
                eprintln!("Linux: глобальный перехват работает в X11; на чистом Wayland поддержка ограничена.");
                let _ = proxy.send_event(AppEvent::ListenFailed);
            }
        })
        .ok();
}

fn on_event(proxy: &EventLoopProxy<AppEvent>, event: Event) {
    if CONVERTING.load(Ordering::SeqCst) {
        return;
    }
    let (vk, is_up) = match event.event_type {
        EventType::KeyPress(key) => (map::key_to_vk(key), false),
        EventType::KeyRelease(key) => (map::key_to_vk(key), true),
        _ => return,
    };
    if vk == 0 {
        return;
    }

    if CAPTURING.load(Ordering::SeqCst) {
        if is_up {
            return;
        }
        CAPTURING.store(false, Ordering::SeqCst);
        let _ = proxy.send_event(AppEvent::KeyAssigned(vk));
        return;
    }

    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }

    let hotkey = HOTKEY_VK.load(Ordering::SeqCst);
    if !keys::matches_hotkey(vk, hotkey) {
        if !is_up {
            PENDING_TAP.store(false, Ordering::SeqCst);
            if KEY_HELD.load(Ordering::SeqCst) {
                CHORDED.store(true, Ordering::SeqCst);
            }
        }
        return;
    }

    if is_up {
        KEY_HELD.store(false, Ordering::SeqCst);
        if FIRE_ON_UP.swap(false, Ordering::SeqCst) {
            let _ = proxy.send_event(AppEvent::Convert);
            return;
        }
        if CHORDED.swap(false, Ordering::SeqCst) {
            PENDING_TAP.store(false, Ordering::SeqCst);
        } else {
            PENDING_TAP.store(true, Ordering::SeqCst);
            LAST_UP_MS.store(now_ms(), Ordering::SeqCst);
        }
        return;
    }

    if KEY_HELD.swap(true, Ordering::SeqCst) {
        return;
    }

    let pending = PENDING_TAP.swap(false, Ordering::SeqCst);
    let elapsed = now_ms().saturating_sub(LAST_UP_MS.load(Ordering::SeqCst));
    if pending && elapsed <= u64::from(INTERVAL_MS.load(Ordering::SeqCst)) {
        FIRE_ON_UP.store(true, Ordering::SeqCst);
    }
}

pub fn reset_taps() {
    PENDING_TAP.store(false, Ordering::SeqCst);
    FIRE_ON_UP.store(false, Ordering::SeqCst);
    KEY_HELD.store(false, Ordering::SeqCst);
    CHORDED.store(false, Ordering::SeqCst);
}
