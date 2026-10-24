use crate::fetch;
use crate::util;
use ort::environment::Environment;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const VERSION: &str = "1.31.0";
const DLL_NAME: &str = "onnxruntime.dll";
const URLS: &[&str] = &[
    "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime/1.31.0",
    "https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-1.31.0.zip",
];

static ENV: Mutex<Option<Environment>> = Mutex::new(None);

fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("Lswi");
    dir.push("onnx");
    dir.push(VERSION);
    dir
}

pub fn environment(progress: &mut dyn FnMut(&str)) -> Result<Environment, String> {
    let mut guard = ENV.lock().map_err(|e| e.to_string())?;
    if let Some(env) = guard.as_ref() {
        return Ok(env.clone());
    }
    let dll = ensure_dll(progress)?;
    if let Some(dir) = dll.parent() {
        util::set_dll_directory(dir)?;
    }
    let env = ort::init_from(&dll)
        .map_err(|e| format!("Не удалось открыть ONNX Runtime: {e}"))?
        .with_name("lswi")
        .build()
        .map_err(|e| format!("Не удалось запустить ONNX Runtime: {e}"))?;
    *guard = Some(env.clone());
    Ok(env)
}

fn ensure_dll(progress: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    let dll = dir.join(DLL_NAME);
    let marker = dir.join("VERSION");
    let ready = dll.exists()
        && fs::metadata(&dll).map(|m| m.len() > 1_000_000).unwrap_or(false)
        && fs::read_to_string(&marker)
            .map(|s| s.trim() == VERSION)
            .unwrap_or(false);
    if ready {
        return Ok(dll);
    }

    let zip_path = dir.join("onnxruntime.zip");
    let mut last = "Не удалось скачать ONNX Runtime".to_string();
    for url in URLS {
        if let Err(err) = fetch::download(url, &zip_path, progress) {
            last = err;
            continue;
        }
        progress("Распаковка движка моделей…");
        match extract_dlls(&zip_path, &dir) {
            Ok(()) if fs::metadata(&dll).map(|m| m.len()).unwrap_or(0) > 1_000_000 => {
                let _ = fs::remove_file(&zip_path);
                fs::write(&marker, VERSION).map_err(|e| e.to_string())?;
                return Ok(dll);
            }
            Ok(()) => last = "В архиве нет onnxruntime.dll".into(),
            Err(err) => last = err,
        }
        let _ = fs::remove_file(&zip_path);
    }
    Err(last)
}

fn extract_dlls(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut found = false;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = item.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if !file_name.ends_with(".dll") {
            continue;
        }
        let mut out = File::create(dest.join(&file_name)).map_err(|e| e.to_string())?;
        io::copy(&mut item, &mut out).map_err(|e| e.to_string())?;
        if file_name == DLL_NAME {
            found = true;
        }
    }
    if found {
        Ok(())
    } else {
        Err("В архиве нет onnxruntime.dll".into())
    }
}
