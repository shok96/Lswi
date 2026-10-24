use crate::fetch;
use crate::util::wide;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
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
    fetch::download_first(DLL_URLS, &zip_path, "библиотеки распознавания", progress)?;
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
    fetch::download_first(MODEL_URLS, &zip_path, "русской модели (~45 МБ)", progress)?;
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
        crate::fetch::download(DLL_URLS[0], &dest, &mut |s| last = s.to_string()).expect(&last);
        let size = fs::metadata(&dest).unwrap().len();
        let _ = fs::remove_file(&dest);
        assert!(size > 1_000_000, "size={size} last={last}");
    }
}
