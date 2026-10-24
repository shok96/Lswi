use crate::fetch;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

const VERSION: &str = "rupunct-small-int8-1";
const ONNX_NAME: &str = "rupunct_small_int8.onnx";
const TOK_NAME: &str = "tokenizer.json";

const ONNX_URLS: &[&str] = &[
    "https://huggingface.co/ekhodzitsky/rupunct-small-onnx/resolve/main/rupunct_small_int8.onnx",
    "https://hf-mirror.com/ekhodzitsky/rupunct-small-onnx/resolve/main/rupunct_small_int8.onnx",
];
const TOK_URLS: &[&str] = &[
    "https://huggingface.co/ekhodzitsky/rupunct-small-onnx/resolve/main/tokenizer.json",
    "https://hf-mirror.com/ekhodzitsky/rupunct-small-onnx/resolve/main/tokenizer.json",
];

const LABELS: [&str; 33] = [
    "UPPER_PERIOD",
    "LOWER_PERIOD",
    "UPPER_TOTAL_PERIOD",
    "UPPER_COMMA",
    "LOWER_COMMA",
    "UPPER_TOTAL_COMMA",
    "UPPER_QUESTION",
    "LOWER_QUESTION",
    "UPPER_TOTAL_QUESTION",
    "UPPER_TIRE",
    "LOWER_TIRE",
    "UPPER_TOTAL_TIRE",
    "UPPER_VOSKL",
    "LOWER_VOSKL",
    "UPPER_TOTAL_VOSKL",
    "UPPER_DVOETOCHIE",
    "LOWER_DVOETOCHIE",
    "UPPER_TOTAL_DVOETOCHIE",
    "UPPER_PERIODCOMMA",
    "LOWER_PERIODCOMMA",
    "UPPER_TOTAL_PERIODCOMMA",
    "UPPER_DEFIS",
    "LOWER_DEFIS",
    "UPPER_TOTAL_DEFIS",
    "UPPER_QUESTIONVOSKL",
    "LOWER_QUESTIONVOSKL",
    "UPPER_TOTAL_QUESTIONVOSKL",
    "UPPER_MNOGOTOCHIE",
    "LOWER_MNOGOTOCHIE",
    "UPPER_TOTAL_MNOGOTOCHIE",
    "UPPER_O",
    "LOWER_O",
    "UPPER_TOTAL_O",
];

static ENGINE: OnceLock<Mutex<Option<Engine>>> = OnceLock::new();
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn set_error(msg: impl Into<String>) {
    if let Ok(mut err) = LAST_ERROR.lock() {
        *err = msg.into();
    }
}

pub fn ready() -> bool {
    ENGINE
        .get()
        .and_then(|slot| slot.lock().ok())
        .is_some_and(|g| g.is_some())
}

pub fn last_error() -> String {
    LAST_ERROR.lock().map(|g| g.clone()).unwrap_or_default()
}

struct Engine {
    _env: Environment,
    session: Session,
    tokenizer: Tokenizer,
}

fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("Lswi");
    dir.push("smart");
    dir
}

pub fn ensure(progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    let onnx = dir.join(ONNX_NAME);
    let tok = dir.join(TOK_NAME);
    let marker = dir.join("VERSION");
    let ready = onnx.exists()
        && tok.exists()
        && fs::metadata(&onnx).map(|m| m.len() > 1_000_000).unwrap_or(false)
        && fs::read_to_string(&marker)
            .map(|s| s.trim() == VERSION)
            .unwrap_or(false);
    if !ready {
        fetch::download_first(ONNX_URLS, &onnx, "модели пунктуации (~29 МБ)", progress)?;
        fetch::download_first(TOK_URLS, &tok, "токенизатора", progress)?;
        if fs::metadata(&onnx).map(|m| m.len()).unwrap_or(0) < 1_000_000 {
            return Err("Модель пунктуации повреждена".into());
        }
        fs::write(&marker, VERSION).map_err(|e| e.to_string())?;
    }
    if let Err(err) = crate::onnx::environment(progress) {
        set_error(&err);
        return Err(err);
    }
    match load(&onnx, &tok) {
        Ok(()) => {
            set_error("");
            Ok(())
        }
        Err(err) => {
            set_error(&err);
            Err(err)
        }
    }
}

fn load(onnx: &Path, tok: &Path) -> Result<(), String> {
    let slot = ENGINE.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let env = crate::onnx::environment(&mut |_| {})?;
    let session = Session::builder(&env)
        .map_err(|e| e.to_string())?
        .with_intra_threads(1)
        .map_err(|e| e.to_string())?
        .commit_from_file(onnx)
        .map_err(|e| format!("Не удалось открыть модель пунктуации: {e}"))?;
    let tokenizer = Tokenizer::from_file(tok).map_err(|e| e.to_string())?;
    *guard = Some(Engine {
        _env: env,
        session,
        tokenizer,
    });
    Ok(())
}

pub fn release() {
    if let Some(slot) = ENGINE.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = None;
        }
    }
}

pub fn punctuate(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || words.len() > 80 {
        return None;
    }
    let slot = ENGINE.get()?;
    let mut guard = slot.lock().ok()?;
    let engine = guard.as_mut()?;
    infer(engine, &words).ok().filter(|s| !s.is_empty())
}

fn infer(engine: &mut Engine, words: &[&str]) -> Result<String, String> {
    let joined = words.join(" ");
    let encoding = engine
        .tokenizer
        .encode(words.to_vec(), true)
        .or_else(|_| engine.tokenizer.encode(joined, true))
        .map_err(|e| e.to_string())?;
    let ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
    let mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|id| i64::from(*id))
        .collect();
    let types: Vec<i64> = encoding
        .get_type_ids()
        .iter()
        .map(|id| i64::from(*id))
        .collect();
    let len = ids.len();
    if len == 0 {
        return Ok(String::new());
    }

    let ids_t = Tensor::from_array(([1usize, len], ids)).map_err(|e| e.to_string())?;
    let mask_t = Tensor::from_array(([1usize, len], mask)).map_err(|e| e.to_string())?;
    let types_t = Tensor::from_array(([1usize, len], types)).map_err(|e| e.to_string())?;

    let outputs = engine
        .session
        .run(ort::inputs![
            "input_ids" => ids_t,
            "attention_mask" => mask_t,
            "token_type_ids" => types_t
        ])
        .map_err(|e| e.to_string())?;

    let (_name, value) = outputs
        .iter()
        .next()
        .ok_or_else(|| "Модель не вернула результат".to_string())?;
    let array = value.try_extract_array::<f32>().map_err(|e| e.to_string())?;
    let shape = array.shape();
    let logits = array.as_slice().ok_or("Не удалось прочитать логиты")?;
    let (seq, classes) = match shape {
        [1, s, c] => (*s, *c),
        [s, c] => (*s, *c),
        _ => return Err(format!("Неожиданная форма выхода {shape:?}")),
    };
    if classes != LABELS.len() || seq != len {
        return Err("Размер выхода модели не совпал".into());
    }

    let word_ids = encoding.get_word_ids();
    let mut labels = Vec::new();
    let mut last = None;
    for (i, word_id) in word_ids.iter().enumerate() {
        let Some(wid) = *word_id else {
            continue;
        };
        if last == Some(wid) {
            continue;
        }
        last = Some(wid);
        let row = &logits[i * classes..(i + 1) * classes];
        labels.push(argmax(row));
    }
    if labels.is_empty() {
        return Err("Модель не разметила слова".into());
    }

    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        let label = labels.get(i).copied().unwrap_or(31);
        if !out.is_empty() && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        out.push_str(&apply_label(word, LABELS[label]));
    }
    Ok(out.trim().to_string())
}

fn argmax(row: &[f32]) -> usize {
    row.iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(31)
}

fn apply_label(word: &str, label: &str) -> String {
    let (case, mark) = label.split_once('_').unwrap_or(("LOWER", "O"));
    let (case, mark) = if case == "UPPER" && mark.starts_with("TOTAL_") {
        ("TOTAL", &mark[6..])
    } else {
        (case, mark)
    };
    let body = match case {
        "UPPER" | "TOTAL" => capitalize(word),
        _ => word.to_string(),
    };
    match mark {
        "PERIOD" => format!("{body}."),
        "COMMA" => format!("{body},"),
        "QUESTION" => format!("{body}?"),
        "VOSKL" => format!("{body}!"),
        "TIRE" => {
            if case == "UPPER" {
                format!("{body} —")
            } else {
                format!("{body}—")
            }
        }
        "DVOETOCHIE" => format!("{body}:"),
        "PERIODCOMMA" => format!("{body};"),
        "DEFIS" => format!("{body}-"),
        "MNOGOTOCHIE" => format!("{body}..."),
        "QUESTIONVOSKL" => format!("{body}?!"),
        _ => body,
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure, punctuate, ONNX_NAME, data_dir};

    #[test]
    fn punctuates_when_model_is_installed() {
        if !data_dir().join(ONNX_NAME).exists() {
            return;
        }
        let runtime = dirs::data_local_dir()
            .unwrap_or_default()
            .join("Lswi")
            .join("onnx")
            .join("1.31.0")
            .join("onnxruntime.dll");
        if !runtime.exists() {
            return;
        }
        ensure(&mut |_| {}).expect("load punct model");
        let out = punctuate("привет как дела").expect("infer");
        assert!(
            out.to_lowercase().contains("привет") && out.chars().any(|c| matches!(c, ',' | '?' | '.' | '!')),
            "got {out:?}"
        );
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
