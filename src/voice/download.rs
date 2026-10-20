use crate::util::wide;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_DIRECT, INTERNET_OPEN_TYPE_PRECONFIG_WITH_NO_AUTOPROXY,
    INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
};
use windows::Win32::Storage::FileSystem::GetShortPathNameW;

const VOSK_VERSION: &str = "0.3.45";
const MODEL_NAME: &str = "vosk-model-small-ru-0.22";

const DLL_URLS: &[&str] = &[
    "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip",
];
const MODEL_URLS: &[&str] = &[
    "https://huggingface.co/rhasspy/vosk-models/resolve/main/ru/vosk-model-small-ru-0.22.zip",
    "https://archive.org/download/vosk-models-small/vosk-model-small-ru-0.22.zip",
    "https://alphacephei.com/vosk/models/vosk-model-small-ru-0.22.zip",
];

pub struct VoiceAssets {
    pub dll: PathBuf,
    pub model: PathBuf,
}

pub fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("Lswi");
    dir.push("voice");
    dir
}

pub fn native_dir() -> PathBuf {
    data_dir().join("native")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn ensure(progress: &mut dyn FnMut(&str)) -> Result<VoiceAssets, String> {
    fs::create_dir_all(native_dir()).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    fs::create_dir_all(models_dir()).map_err(|e| format!("Не удалось создать папку: {e}"))?;

    let dll = ensure_dll(progress)?;
    let model = ensure_model(progress)?;
    Ok(VoiceAssets { dll, model })
}

fn ensure_dll(progress: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let dir = native_dir();
    let dll = dir.join("libvosk.dll");
    let marker = dir.join("VERSION");
    let ready = dll.exists()
        && fs::read_to_string(&marker)
            .map(|s| s.trim() == VOSK_VERSION)
            .unwrap_or(false);
    if ready {
        return Ok(dll);
    }

    let zip_path = dir.join("vosk-win64.zip");
    download_first(DLL_URLS, &zip_path, "библиотеки распознавания", progress)?;
    progress("Распаковка библиотеки…");
    extract_dlls(&zip_path, &dir)?;
    let _ = fs::remove_file(&zip_path);
    if !dll.exists() {
        return Err("В архиве нет libvosk.dll".into());
    }
    fs::write(&marker, VOSK_VERSION).map_err(|e| e.to_string())?;
    Ok(dll)
}

fn ensure_model(progress: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let dir = models_dir().join(MODEL_NAME);
    if model_looks_ready(&dir) {
        return Ok(dir);
    }
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }

    let zip_path = models_dir().join(format!("{MODEL_NAME}.zip"));
    download_first(MODEL_URLS, &zip_path, "русской модели (~45 МБ)", progress)?;
    progress("Распаковка модели…");
    extract_tree(&zip_path, &models_dir())?;
    let _ = fs::remove_file(&zip_path);
    if !model_looks_ready(&dir) {
        return Err("Модель распознавания повреждена".into());
    }
    Ok(dir)
}

fn model_looks_ready(dir: &Path) -> bool {
    dir.join("am").exists() && (dir.join("conf").exists() || dir.join("graph").exists())
}

fn download_first(
    urls: &[&str],
    dest: &Path,
    label: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let mut last_err = "Нет адресов для скачивания".to_string();
    for (i, url) in urls.iter().enumerate() {
        if i == 0 {
            progress(&format!("Скачивание {label}…"));
        } else {
            progress(&format!("Другой сервер, скачивание {label}…"));
        }
        match download(url, dest, progress) {
            Ok(()) => return Ok(()),
            Err(err) => last_err = err,
        }
    }
    Err(last_err)
}

fn download(url: &str, dest: &Path, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    let tmp = dest.with_extension("part");
    let _ = fs::remove_file(&tmp);
    unsafe { wininet_download(url, &tmp, progress) }?;
    let size = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if size < 1024 {
        let _ = fs::remove_file(&tmp);
        return Err("Сервер вернул пустой файл".into());
    }
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

unsafe fn wininet_download(
    url: &str,
    dest: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let url_w = wide(url);
    let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_SECURE;
    let access_types = [
        INTERNET_OPEN_TYPE_PRECONFIG_WITH_NO_AUTOPROXY,
        INTERNET_OPEN_TYPE_DIRECT.0,
    ];

    let mut last = "Не удалось открыть соединение".to_string();
    for access in access_types {
        let session = InternetOpenW(
            w!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) LSWI"),
            access,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            last = format!("Нет интернет-клиента Windows: {}", last_error());
            continue;
        }
        let _session = InetHandle(session);
        set_timeout(session, INTERNET_OPTION_CONNECT_TIMEOUT, 30_000);
        set_timeout(session, INTERNET_OPTION_RECEIVE_TIMEOUT, 60_000);

        let request = InternetOpenUrlW(session, PCWSTR(url_w.as_ptr()), None, flags, 0);
        if request.is_null() {
            last = format!("Нет сети или сервер недоступен: {}", last_error());
            continue;
        }
        let _request = InetHandle(request);
        set_timeout(request, INTERNET_OPTION_RECEIVE_TIMEOUT, 60_000);

        let status = query_u32(request, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER).unwrap_or(0);
        if status != 0 && !(200..300).contains(&status) {
            last = format!("Сервер вернул ошибку HTTP {status}");
            continue;
        }

        let total = query_u32(request, HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER)
            .map(u64::from)
            .unwrap_or(0);
        progress(&progress_text(0, total));

        let mut file = File::create(dest).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 64 * 1024];
        let mut done = 0u64;
        let mut last_pct = 0u64;
        loop {
            let mut read = 0u32;
            InternetReadFile(request, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read)
                .map_err(|e| format!("Обрыв загрузки: {e}"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read as usize]).map_err(|e| e.to_string())?;
            done += u64::from(read);
            let pct = if total > 0 {
                done.saturating_mul(100) / total
            } else {
                done / (256 * 1024)
            };
            if pct != last_pct {
                last_pct = pct;
                progress(&progress_text(done, total));
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        if done < 1024 {
            last = "Сервер вернул пустой файл".into();
            continue;
        }
        return Ok(());
    }
    Err(last)
}

fn set_timeout(handle: *mut core::ffi::c_void, option: u32, ms: u32) {
    unsafe {
        let mut value = ms;
        let _ = InternetSetOptionW(
            Some(handle),
            option,
            Some((&mut value as *mut u32).cast()),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

unsafe fn query_u32(request: *mut core::ffi::c_void, query: u32) -> Option<u32> {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    HttpQueryInfoW(
        request,
        query,
        Some((&mut value as *mut u32).cast()),
        &mut size,
        None,
    )
    .ok()?;
    Some(value)
}

fn progress_text(done: u64, total: u64) -> String {
    if total > 0 {
        let pct = (done.saturating_mul(100) / total).min(99);
        format!(
            "Скачивание… {pct}% ({:.1} / {:.1} МБ)",
            done as f64 / 1_000_000.0,
            total as f64 / 1_000_000.0
        )
    } else {
        format!("Скачивание… {:.1} МБ", done as f64 / 1_000_000.0)
    }
}

fn last_error() -> String {
    unsafe { format!("{}", GetLastError().0) }
}

struct InetHandle(*mut core::ffi::c_void);

impl Drop for InetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }
}

fn extract_dlls(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = item.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name() else {
            continue;
        };
        if !file_name.to_string_lossy().ends_with(".dll") {
            continue;
        }
        let out_path = dest.join(file_name);
        let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
        io::copy(&mut item, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_tree(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = item.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(rel);
        if item.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
        io::copy(&mut item, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn short_path(path: &Path) -> Result<String, String> {
    let raw = path.to_string_lossy();
    let wide_path = wide(&raw);
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetShortPathNameW(PCWSTR(wide_path.as_ptr()), Some(&mut buf)) };
    if n > 0 && (n as usize) < buf.len() {
        let short = String::from_utf16_lossy(&buf[..n as usize]);
        if !short.is_empty() {
            return Ok(short);
        }
    }
    if raw.chars().all(|c| c.is_ascii()) {
        return Ok(raw.into_owned());
    }
    Err("Путь к модели содержит символы, которые движок распознавания не принимает".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn downloads_vosk_library() {
        let dir = std::env::temp_dir().join("lswi-dl-test");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("vosk-win64.zip");
        let _ = fs::remove_file(&dest);
        let mut last = String::new();
        download(DLL_URLS[0], &dest, &mut |s| last = s.to_string()).expect(&last);
        let size = fs::metadata(&dest).unwrap().len();
        let _ = fs::remove_file(&dest);
        assert!(size > 1_000_000, "size={size} last={last}");
    }
}
