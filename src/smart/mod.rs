mod dict;
#[cfg(windows)]
mod model;
mod punct;
mod tokenize;

use crate::layout;
use dict::Dict;
use tokenize::Token;

pub use punct::polish_voice_ex;

pub fn ensure_model(progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    #[cfg(windows)]
    {
        model::ensure(progress)
    }
    #[cfg(not(windows))]
    {
        let _ = progress;
        Ok(())
    }
}

pub fn model_punctuate(text: &str) -> Option<String> {
    #[cfg(windows)]
    {
        model::punctuate(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        None
    }
}

pub fn model_ready() -> bool {
    #[cfg(windows)]
    {
        model::ready()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn release_model() {
    #[cfg(windows)]
    {
        model::release();
    }
}

pub fn model_error() -> String {
    #[cfg(windows)]
    {
        model::last_error()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

pub fn dict_ready() -> bool {
    Dict::global().len() > 100
}

const REMAP_MARGIN: u16 = 3;
const MIN_REMAP_SCORE: u8 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartOutcome {
    Converted(String),
    Unchanged,
    Uncertain,
}

#[allow(dead_code)]
pub fn smart_convert(text: &str) -> Option<String> {
    match convert_outcome(text) {
        SmartOutcome::Converted(s) => Some(s),
        SmartOutcome::Unchanged | SmartOutcome::Uncertain => None,
    }
}

pub fn convert_outcome(text: &str) -> SmartOutcome {
    if text.is_empty() {
        return SmartOutcome::Uncertain;
    }

    let dict = Dict::global();
    let tokens = tokenize::tokenize(text);
    let majority = majority_script(&tokens, dict);
    let mut out = String::with_capacity(text.len());
    let mut remapped = 0u32;
    let mut known_ok = 0u32;
    let mut letters = 0u32;
    let mut skipped = 0u32;

    for token in &tokens {
        match token {
            Token::Skip(s) => {
                skipped += 1;
                out.push_str(s);
            }
            Token::Other(s) => out.push_str(s),
            Token::Word(word) => {
                letters += 1;
                match decide_word(word, dict, majority) {
                    Decision::Remap => {
                        out.push_str(&layout::toggle_en_ru(word));
                        remapped += 1;
                    }
                    Decision::Replace(s) => {
                        if s != *word {
                            remapped += 1;
                        } else {
                            known_ok += 1;
                        }
                        out.push_str(&s);
                    }
                    Decision::Leave => {
                        out.push_str(word);
                        if is_known(word, dict) {
                            known_ok += 1;
                        }
                    }
                }
            }
        }
    }

    if remapped > 0 && out != text {
        return SmartOutcome::Converted(out);
    }
    if remapped == 0 && (known_ok > 0 || (skipped > 0 && letters == 0)) {
        return SmartOutcome::Unchanged;
    }
    SmartOutcome::Uncertain
}

enum Decision {
    Remap,
    Leave,
    Replace(String),
}

const TECH: &[&str] = &[
    "json", "js", "ts", "jsx", "tsx", "html", "xml", "css", "scss",
    "http", "https", "sql", "api", "url", "uri", "utf", "ascii",
    "exe", "dll", "png", "jpg", "svg", "pdf", "git", "npm", "ssh",
    "cpu", "gpu", "ram", "usb", "yaml", "toml", "rust", "node",
    "github", "linux", "windows", "macos", "config", "debug", "error",
    "null", "true", "false", "class", "const", "function", "return",
    "import", "export", "async", "await", "string", "number", "boolean",
    "object", "array", "index",
];

fn majority_script(tokens: &[Token], dict: &Dict) -> Option<bool> {
    let mut latin = 0u32;
    let mut cyr = 0u32;
    for token in tokens {
        let Token::Word(word) = token else {
            continue;
        };
        if !is_known(word, dict) {
            continue;
        }
        match script_of(word) {
            Some(true) => cyr += 1,
            Some(false) => latin += 1,
            None => {}
        }
    }
    if cyr > latin {
        Some(true)
    } else if latin > cyr {
        Some(false)
    } else {
        None
    }
}

fn script_of(word: &str) -> Option<bool> {
    let (latin, cyr) = word.chars().fold((0, 0), |acc, ch| {
        if ch.is_ascii_alphabetic() {
            (acc.0 + 1, acc.1)
        } else if ('\u{0400}'..='\u{04FF}').contains(&ch) {
            (acc.0, acc.1 + 1)
        } else {
            acc
        }
    });
    if cyr == 0 && latin == 0 {
        None
    } else {
        Some(cyr > latin)
    }
}

fn decide_word(word: &str, dict: &Dict, majority: Option<bool>) -> Decision {
    let orig_key = word.to_lowercase();
    let remap_key = layout::toggle_en_ru(&orig_key);
    if remap_key == orig_key {
        return Decision::Leave;
    }
    let orig = dict.score(&orig_key);
    let remap = dict.score(&remap_key);
    let known = is_known(word, dict);
    if !known && remap >= MIN_REMAP_SCORE && u16::from(remap) > u16::from(orig) * REMAP_MARGIN {
        return Decision::Remap;
    }
    if let Some(cyr) = script_of(word) {
        if majority == Some(!cyr) {
            return Decision::Remap;
        }
    }
    if known {
        return Decision::Leave;
    }
    if let Some(split) = split_glued(word, dict) {
        return Decision::Replace(split);
    }
    if looks_like_layout_mistype(word) {
        return Decision::Remap;
    }
    Decision::Leave
}

fn is_known(word: &str, dict: &Dict) -> bool {
    let key = word.to_lowercase();
    dict.score(&key) > 0 || TECH.contains(&key.as_str())
}

fn looks_like_layout_mistype(word: &str) -> bool {
    let mut letters = 0u32;
    for ch in word.chars() {
        if ch.is_ascii_alphabetic() {
            letters += 1;
        } else if !layout::is_layout_letter_punct(ch) {
            return false;
        }
    }
    if letters < 4 {
        return false;
    }
    !word.bytes().any(|b| matches!(b.to_ascii_lowercase(), b'a' | b'e' | b'i' | b'o' | b'u' | b'y'))
}

fn longest_known_prefix(text: &str, dict: &Dict) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 3 {
        return None;
    }
    for len in (3..=chars.len()).rev() {
        let prefix: String = chars[..len].iter().collect();
        if is_known(&prefix, dict) {
            return Some(len);
        }
    }
    None
}

fn take_chars(text: &str, n: usize) -> (String, String) {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(n).collect();
    (head, chars.collect())
}

fn push_part(out: &mut String, part: &str) {
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(part);
}

fn split_glued(word: &str, dict: &Dict) -> Option<String> {
    if word.chars().any(|ch| !ch.is_ascii_alphabetic()) {
        return None;
    }
    let mut rest = word.to_string();
    let mut out = String::new();
    let mut peeled = 0u32;
    while !rest.is_empty() {
        let Some(len) = longest_known_prefix(&rest.to_lowercase(), dict) else {
            break;
        };
        if len == rest.chars().count() && peeled == 0 {
            return None;
        }
        let (head, tail) = take_chars(&rest, len);
        push_part(&mut out, &head);
        rest = tail;
        peeled += 1;
    }
    if peeled == 0 {
        return None;
    }
    if !rest.is_empty() {
        let tail = if is_known(&rest, dict) {
            rest
        } else {
            layout::toggle_en_ru(&rest)
        };
        push_part(&mut out, &tail);
    }
    (out != word).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{convert_outcome, smart_convert, SmartOutcome};

    #[test]
    fn remaps_typical_mistype() {
        assert_eq!(smart_convert("ghbdtn").as_deref(), Some("привет"));
        assert_eq!(smart_convert("Ghbdtn").as_deref(), Some("Привет"));
    }

    #[test]
    fn leaves_correct_russian() {
        assert_eq!(convert_outcome("привет"), SmartOutcome::Unchanged);
        let src = "Запятая внутри слова — это почти наверняка русская «б», а не пунктуация. Теперь такие слова конвертируются всегда, даже если их нет в словаре.";
        let got = convert_outcome(src);
        assert_eq!(got, SmartOutcome::Unchanged, "{got:?}");
    }

    #[test]
    fn mixed_sentence() {
        assert_eq!(
            smart_convert("ghbdtn, how are you").as_deref(),
            Some("привет, how are you")
        );
    }

    #[test]
    fn remaps_words_with_layout_letter_punct() {
        assert_eq!(
            smart_convert("vjq ltym vjz cdj,jlf").as_deref(),
            Some("мой день моя свобода")
        );
        assert_eq!(smart_convert("cdj,jlf").as_deref(), Some("свобода"));
        assert_eq!(smart_convert(",jkmit").as_deref(), Some("больше"));
        assert_eq!(smart_convert("k.,jdm").as_deref(), Some("любовь"));
        assert_eq!(smart_convert(";bpym").as_deref(), Some("жизнь"));
        assert_eq!(smart_convert("j,]trn").as_deref(), Some("объект"));
        assert_eq!(smart_convert("hs,f").as_deref(), Some("рыба"));
        assert_eq!(convert_outcome("akm,f"), SmartOutcome::Uncertain);
    }

    #[test]
    fn keeps_login_and_url() {
        assert_eq!(convert_outcome("login: admin"), SmartOutcome::Unchanged);
        assert_eq!(
            convert_outcome("https://github.com/ghbdtn"),
            SmartOutcome::Unchanged
        );
        assert_eq!(convert_outcome("user@mail.ru"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("JSON"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("UTF-8"), SmartOutcome::Unchanged);
    }

    #[test]
    fn numbers_are_uncertain() {
        assert_eq!(convert_outcome("123"), SmartOutcome::Uncertain);
        assert_eq!(smart_convert("123"), None);
    }

    #[test]
    fn keeps_json_html_and_remaps_tail() {
        assert_eq!(
            smart_convert("jsonhtmlgbmcrf").as_deref(),
            Some("json html пиьска")
        );
        assert_eq!(
            smart_convert("json html gbmcrf").as_deref(),
            Some("json html пиьска")
        );
    }

    #[test]
    fn does_not_split_real_english_word() {
        assert_eq!(convert_outcome("something"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("json"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("html"), SmartOutcome::Unchanged);
    }

    #[test]
    fn leaves_known_and_russian_words() {
        assert_eq!(convert_outcome("json"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("js"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("hello"), SmartOutcome::Unchanged);
        assert_eq!(convert_outcome("хуй"), SmartOutcome::Unchanged);
    }

    #[test]
    fn remaps_only_foreign_words_in_english_phrase() {
        assert_eq!(
            smart_convert("json js hello Андрей").as_deref(),
            Some("json js hello Fylhtq")
        );
        assert_eq!(
            smart_convert("json js хуй").as_deref(),
            Some("json js [eq")
        );
    }
}
