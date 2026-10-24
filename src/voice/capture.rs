use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Mutex;
use windows::core::{implement, Result as WinResult, PCWSTR};
use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

pub struct Microphone {
    pub id: String,
    pub name: String,
}

pub fn list_microphones() -> Vec<Microphone> {
    ensure_com();
    match list_wasapi() {
        Ok(list) if !list.is_empty() => list,
        _ => list_cpal(),
    }
}

fn ensure_com() {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = hr.is_ok() || hr == RPC_E_CHANGED_MODE;
    }
}

fn list_wasapi() -> windows::core::Result<Vec<Microphone>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let collection = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let count = collection.GetCount()?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let device = collection.Item(i)?;
            let id = device_id(&device).unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let name = device_name(&device).unwrap_or_else(|| format!("Микрофон {}", i + 1));
            out.push(Microphone { id, name });
        }
        Ok(out)
    }
}

fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    unsafe {
        let pwstr = device.GetId()?;
        let id = pwstr.to_string()?;
        CoTaskMemFree(Some(pwstr.0 as *const _));
        Ok(id)
    }
}

fn device_name(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store
            .GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _)
            .ok()?;
        let inner = &value.as_raw().Anonymous.Anonymous;
        let name = if inner.vt == VT_LPWSTR.0 {
            let ptr = *(&inner.Anonymous as *const _ as *const *const u16);
            if ptr.is_null() {
                None
            } else {
                let mut len = 0isize;
                while *ptr.offset(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len as usize);
                Some(OsString::from_wide(slice).to_string_lossy().into_owned())
            }
        } else {
            None
        };
        let _ = PropVariantClear(&mut value);
        name
    }
}

fn list_cpal() -> Vec<Microphone> {
    let host = cpal::default_host();
    let Ok(devices) = host.devices() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for device in devices {
        let Ok(name) = device.name() else {
            continue;
        };
        if device.default_input_config().is_err() && device.supported_input_configs().map(|mut i| i.next().is_none()).unwrap_or(true)
        {
            continue;
        }
        out.push(Microphone {
            id: name.clone(),
            name,
        });
    }
    out
}

pub fn start_stream(
    device_id: &str,
    tx: SyncSender<Vec<f32>>,
    level: &'static AtomicU32,
    stop: &'static AtomicBool,
) -> Result<(Stream, u32), String> {
    let device = input_device(device_id)?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Микрофон недоступен: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let stream_config = config.config();

    let err_fn = move |err| {
        eprintln!("LSWI: ошибка микрофона: {err}");
        stop.store(true, Ordering::SeqCst);
        super::on_capture_error();
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &stream_config, channels, tx, level, stop, err_fn)?,
        SampleFormat::I16 => build::<i16>(&device, &stream_config, channels, tx, level, stop, err_fn)?,
        SampleFormat::I32 => build::<i32>(&device, &stream_config, channels, tx, level, stop, err_fn)?,
        SampleFormat::U16 => build::<u16>(&device, &stream_config, channels, tx, level, stop, err_fn)?,
        SampleFormat::U8 => build::<u8>(&device, &stream_config, channels, tx, level, stop, err_fn)?,
        other => return Err(format!("Формат микрофона {other} не поддерживается")),
    };
    stream.play().map_err(|e| format!("Не удалось начать запись: {e}"))?;
    Ok((stream, sample_rate))
}

fn input_device(device_id: &str) -> Result<cpal::Device, String> {
    ensure_com();
    let host = cpal::default_host();
    if device_id.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| "Системный микрофон не найден".into());
    }
    let friendly = list_wasapi()
        .ok()
        .and_then(|list| {
            list.into_iter()
                .find(|mic| mic.id == device_id)
                .map(|mic| mic.name)
        });
    let mut devices = host
        .devices()
        .map_err(|e| format!("Нет списка микрофонов: {e}"))?;
    devices
        .find(|d| {
            let Ok(name) = d.name() else {
                return false;
            };
            name == device_id || friendly.as_deref() == Some(name.as_str())
        })
        .or_else(|| host.default_input_device())
        .ok_or_else(|| "Выбранный микрофон не найден".into())
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    tx: SyncSender<Vec<f32>>,
    level: &'static AtomicU32,
    stop: &'static AtomicBool,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample + ToF32 + Send + 'static,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let mono = to_mono(data, channels);
                if let Some(peak) = mono
                    .iter()
                    .map(|s| s.abs())
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                {
                    let pct = (peak.sqrt() * 100.0).clamp(0.0, 100.0) as u32;
                    level.store(pct, Ordering::SeqCst);
                }
                let _ = tx.try_send(mono);
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Не удалось открыть микрофон: {e}"))
}

fn to_mono<T: ToF32>(data: &[T], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.iter().copied().map(ToF32::to_f32).collect();
    }
    data.chunks(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().copied().map(ToF32::to_f32).sum();
            sum / channels as f32
        })
        .collect()
}

trait ToF32: Copy {
    fn to_f32(self) -> f32;
}

impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}

impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / 32768.0
    }
}

impl ToF32 for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / 2_147_483_648.0
    }
}

impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        self as f32 / 32768.0 - 1.0
    }
}

impl ToF32 for u8 {
    fn to_f32(self) -> f32 {
        self as f32 / 128.0 - 1.0
    }
}

pub fn resample_to_16k(input: &[f32], from_rate: u32, acc: &mut f32) -> Vec<i16> {
    if input.is_empty() || from_rate == 0 {
        return Vec::new();
    }
    if from_rate == 16_000 {
        return input
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
    }
    let step = from_rate as f32 / 16_000.0;
    let mut out = Vec::with_capacity(((input.len() as f32) / step) as usize + 1);
    let mut pos = *acc;
    while pos < input.len() as f32 - 1.0 {
        let i = pos.floor() as usize;
        let frac = pos - i as f32;
        let a = input[i];
        let b = input.get(i + 1).copied().unwrap_or(a);
        let sample = a + (b - a) * frac;
        out.push((sample.clamp(-1.0, 1.0) * 32767.0) as i16);
        pos += step;
    }
    *acc = pos - input.len() as f32;
    out
}

struct Watch {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

unsafe impl Send for Watch {}
unsafe impl Sync for Watch {}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);
static LAST_DEFAULT: Mutex<String> = Mutex::new(String::new());

#[implement(IMMNotificationClient)]
struct NotificationClient;

impl IMMNotificationClient_Impl for NotificationClient_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> WinResult<()> {
        super::notify_device_change();
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> WinResult<()> {
        super::notify_device_change();
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> WinResult<()> {
        super::notify_device_change();
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, _role: ERole, _id: &PCWSTR) -> WinResult<()> {
        if flow == eCapture {
            super::notify_device_change();
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> WinResult<()> {
        Ok(())
    }
}

pub fn start_watch() {
    ensure_com();
    stop_watch();
    snapshot_default();
    unsafe {
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return;
        };
        let client: IMMNotificationClient = NotificationClient.into();
        if enumerator
            .RegisterEndpointNotificationCallback(&client)
            .is_err()
        {
            return;
        }
        if let Ok(mut guard) = WATCH.lock() {
            *guard = Some(Watch { enumerator, client });
        }
    }
}

pub fn stop_watch() {
    if let Ok(mut guard) = WATCH.lock() {
        if let Some(watch) = guard.take() {
            unsafe {
                let _ = watch
                    .enumerator
                    .UnregisterEndpointNotificationCallback(&watch.client);
            }
        }
    }
}

pub fn default_changed() -> bool {
    let now = default_capture_id().unwrap_or_default();
    let Ok(mut last) = LAST_DEFAULT.lock() else {
        return false;
    };
    if *last == now {
        return false;
    }
    *last = now;
    true
}

fn snapshot_default() {
    let now = default_capture_id().unwrap_or_default();
    if let Ok(mut last) = LAST_DEFAULT.lock() {
        *last = now;
    }
}

fn default_capture_id() -> Option<String> {
    ensure_com();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .ok()?;
        device_id(&device).ok()
    }
}
