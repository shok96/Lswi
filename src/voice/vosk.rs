use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;
use std::ptr;
use std::sync::Mutex;

type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut std::ffi::c_void;
type ModelFree = unsafe extern "C" fn(*mut std::ffi::c_void);
type RecNew = unsafe extern "C" fn(*mut std::ffi::c_void, f32) -> *mut std::ffi::c_void;
type RecFree = unsafe extern "C" fn(*mut std::ffi::c_void);
type RecAcceptS = unsafe extern "C" fn(*mut std::ffi::c_void, *const i16, c_int) -> c_int;
type RecStr = unsafe extern "C" fn(*mut std::ffi::c_void) -> *const c_char;
type SetLog = unsafe extern "C" fn(c_int);

struct Api {
    _lib: Library,
    model: *mut std::ffi::c_void,
    rec_new: RecNew,
    rec_free: RecFree,
    rec_accept_s: RecAcceptS,
    rec_result: RecStr,
    rec_partial: RecStr,
    rec_final: RecStr,
}

unsafe impl Send for Api {}

static API: Mutex<Option<Api>> = Mutex::new(None);

pub struct Recognizer {
    ptr: *mut std::ffi::c_void,
}

unsafe impl Send for Recognizer {}

pub fn load(dll: &Path, model_dir: &Path) -> Result<(), String> {
    let mut guard = API.lock().map_err(|e| e.to_string())?;
    if guard.as_ref().map(|api| !api.model.is_null()).unwrap_or(false) {
        return Ok(());
    }

    let dll_dir = dll.parent().ok_or("Нет папки с libvosk.dll")?;
    crate::util::set_dll_directory(dll_dir)?;

    let lib = unsafe { Library::new(dll) }.map_err(|e| format!("Не удалось загрузить vosk: {e}"))?;
    unsafe {
        let set_log: Symbol<SetLog> = lib.get(b"vosk_set_log_level\0").map_err(|e| e.to_string())?;
        set_log(-1);
    }

    let model_new: ModelNew = *unsafe { lib.get(b"vosk_model_new\0") }.map_err(|e| e.to_string())?;
    let model_free: ModelFree =
        *unsafe { lib.get(b"vosk_model_free\0") }.map_err(|e| e.to_string())?;
    let rec_new: RecNew =
        *unsafe { lib.get(b"vosk_recognizer_new\0") }.map_err(|e| e.to_string())?;
    let rec_free: RecFree =
        *unsafe { lib.get(b"vosk_recognizer_free\0") }.map_err(|e| e.to_string())?;
    let rec_accept_s: RecAcceptS =
        *unsafe { lib.get(b"vosk_recognizer_accept_waveform_s\0") }.map_err(|e| e.to_string())?;
    let rec_result: RecStr =
        *unsafe { lib.get(b"vosk_recognizer_result\0") }.map_err(|e| e.to_string())?;
    let rec_partial: RecStr =
        *unsafe { lib.get(b"vosk_recognizer_partial_result\0") }.map_err(|e| e.to_string())?;
    let rec_final: RecStr =
        *unsafe { lib.get(b"vosk_recognizer_final_result\0") }.map_err(|e| e.to_string())?;

    let model_path = super::download::short_path(model_dir)?;
    let c_path = CString::new(model_path).map_err(|_| "Некорректный путь к модели")?;
    let model = unsafe { model_new(c_path.as_ptr()) };
    if model.is_null() {
        return Err("Не удалось открыть русскую модель распознавания".into());
    }

    *guard = Some(Api {
        _lib: lib,
        model,
        rec_new,
        rec_free,
        rec_accept_s,
        rec_result,
        rec_partial,
        rec_final,
    });
    let _ = (model_free, model_new);
    Ok(())
}

pub fn recognizer(sample_rate: f32) -> Result<Recognizer, String> {
    let guard = API.lock().map_err(|e| e.to_string())?;
    let api = guard.as_ref().ok_or("Движок распознавания не загружен")?;
    let ptr = unsafe { (api.rec_new)(api.model, sample_rate) };
    if ptr.is_null() {
        return Err("Не удалось создать распознаватель".into());
    }
    Ok(Recognizer { ptr })
}

impl Recognizer {
    pub fn accept(&self, pcm: &[i16]) -> Result<bool, String> {
        let guard = API.lock().map_err(|e| e.to_string())?;
        let api = guard.as_ref().ok_or("Движок распознавания не загружен")?;
        let rc = unsafe { (api.rec_accept_s)(self.ptr, pcm.as_ptr(), pcm.len() as c_int) };
        if rc < 0 {
            return Err("Ошибка распознавания".into());
        }
        Ok(rc == 1)
    }

    pub fn partial_text(&self) -> Result<String, String> {
        let guard = API.lock().map_err(|e| e.to_string())?;
        let api = guard.as_ref().ok_or("Движок распознавания не загружен")?;
        parse_json_field(unsafe { (api.rec_partial)(self.ptr) }, "partial")
    }

    pub fn result_text(&self) -> Result<String, String> {
        let guard = API.lock().map_err(|e| e.to_string())?;
        let api = guard.as_ref().ok_or("Движок распознавания не загружен")?;
        parse_json_field(unsafe { (api.rec_result)(self.ptr) }, "text")
    }

    pub fn final_text(&self) -> Result<String, String> {
        let guard = API.lock().map_err(|e| e.to_string())?;
        let api = guard.as_ref().ok_or("Движок распознавания не загружен")?;
        parse_json_field(unsafe { (api.rec_final)(self.ptr) }, "text")
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        if let Ok(guard) = API.lock() {
            if let Some(api) = guard.as_ref() {
                unsafe { (api.rec_free)(self.ptr) };
            }
        }
        self.ptr = ptr::null_mut();
    }
}

fn parse_json_field(ptr: *const c_char, field: &str) -> Result<String, String> {
    if ptr.is_null() {
        return Ok(String::new());
    }
    let raw = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    let value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}));
    Ok(value
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string())
}
