#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word(String),
    Skip(String),
    Other(String),
}

pub fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut space = String::new();

    let flush_space = |tokens: &mut Vec<Token>, space: &mut String| {
        if !space.is_empty() {
            tokens.push(Token::Other(std::mem::take(space)));
        }
    };

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !buf.is_empty() {
                flush_space(&mut tokens, &mut space);
                tokens.extend(split_chunk(&buf));
                buf.clear();
            }
            space.push(ch);
            continue;
        }
        if !space.is_empty() && buf.is_empty() {
            flush_space(&mut tokens, &mut space);
        }
        buf.push(ch);
    }
    if !buf.is_empty() {
        flush_space(&mut tokens, &mut space);
        tokens.extend(split_chunk(&buf));
    } else {
        flush_space(&mut tokens, &mut space);
    }
    tokens
}

fn split_chunk(chunk: &str) -> Vec<Token> {
    if is_protected(chunk) {
        return vec![Token::Skip(chunk.to_string())];
    }

    let mut out = Vec::new();
    let mut word = String::new();
    let mut other = String::new();
    let flush_word = |out: &mut Vec<Token>, word: &mut String| {
        if !word.is_empty() {
            out.push(Token::Word(std::mem::take(word)));
        }
    };
    let flush_other = |out: &mut Vec<Token>, other: &mut String| {
        if !other.is_empty() {
            out.push(Token::Other(std::mem::take(other)));
        }
    };

    let chars: Vec<char> = chunk.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();
        if is_word_char(ch, next, !word.is_empty(), &chars[i + 1..]) {
            flush_other(&mut out, &mut other);
            word.push(ch);
        } else {
            flush_word(&mut out, &mut word);
            other.push(ch);
        }
        i += 1;
    }
    flush_word(&mut out, &mut word);
    flush_other(&mut out, &mut other);
    out
}

fn is_word_char(ch: char, next: Option<char>, in_word: bool, rest: &[char]) -> bool {
    if ch.is_alphabetic() {
        return true;
    }
    if ch == '-' {
        return in_word && next.is_some_and(char::is_alphabetic);
    }
    crate::layout::is_layout_letter_punct(ch) && rest_has_letter(rest)
}

fn rest_has_letter(rest: &[char]) -> bool {
    for &c in rest {
        if c.is_alphabetic() {
            return true;
        }
        if !(crate::layout::is_layout_letter_punct(c) || c == '-') {
            return false;
        }
    }
    false
}

fn is_protected(chunk: &str) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let lower = chunk.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") {
        return true;
    }
    if let Some(at) = chunk.find('@') {
        if chunk[at + 1..].contains('.') {
            return true;
        }
    }
    if chunk.contains('\\') {
        return true;
    }
    if chunk.starts_with('/') && chunk.matches('/').count() >= 2 {
        return true;
    }
    if lower.starts_with("0x")
        && chunk.len() > 2
        && chunk[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return true;
    }
    if chunk.contains('_') && chunk.chars().any(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    if is_camel_case(chunk) {
        return true;
    }
    if is_all_caps_ascii(chunk) {
        return true;
    }
    if has_file_ext(chunk) {
        return true;
    }
    if chunk.chars().any(|c| c.is_ascii_digit()) && chunk.chars().any(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    false
}

fn is_all_caps_ascii(chunk: &str) -> bool {
    let mut letters = 0;
    for ch in chunk.chars() {
        if ch.is_ascii_alphabetic() {
            if !ch.is_ascii_uppercase() {
                return false;
            }
            letters += 1;
        } else if ch != '-' {
            return false;
        }
    }
    letters >= 2
}

fn is_camel_case(chunk: &str) -> bool {
    let mut saw_lower = false;
    for ch in chunk.chars() {
        if ch.is_ascii_lowercase() {
            saw_lower = true;
        } else if saw_lower && ch.is_ascii_uppercase() {
            return true;
        }
    }
    false
}

const FILE_EXTS: &[&str] = &[
    "txt", "md", "rs", "go", "py", "js", "ts", "jsx", "tsx", "c", "h", "cc",
    "cpp", "hpp", "cs", "java", "kt", "swift", "php", "rb", "lua", "sh",
    "ps1", "bat", "cmd", "exe", "dll", "html", "htm", "css", "scss", "xml",
    "json", "yaml", "yml", "toml", "ini", "cfg", "log", "png", "jpg", "jpeg",
    "gif", "svg", "ico", "webp", "mp3", "wav", "mp4", "mkv", "pdf", "doc",
    "docx", "xls", "xlsx", "ppt", "pptx", "csv", "zip", "rar", "7z", "tar",
    "gz", "sql", "db", "env",
];

fn has_file_ext(chunk: &str) -> bool {
    let Some((_, ext)) = chunk.rsplit_once('.') else {
        return false;
    };
    if ext.is_empty() || !FILE_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
        return false;
    }
    let stem = &chunk[..chunk.len() - ext.len() - 1];
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::{tokenize, Token};

    #[test]
    fn keeps_spaces_and_punct() {
        let tokens = tokenize("ghbdtn, how");
        assert!(matches!(&tokens[0], Token::Word(w) if w == "ghbdtn"));
        assert!(matches!(&tokens[1], Token::Other(s) if s == ","));
        assert!(matches!(&tokens[2], Token::Other(s) if s == " "));
        assert!(matches!(&tokens[3], Token::Word(w) if w == "how"));
    }

    #[test]
    fn keeps_layout_letter_punct_inside_words() {
        let tokens = tokenize("cdj,jlf ,jkmit k.,jdm ;bpym j,]trn");
        let words: Vec<&str> = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Word(w) => Some(w.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(words, ["cdj,jlf", ",jkmit", "k.,jdm", ";bpym", "j,]trn"]);
    }

    #[test]
    fn still_skips_real_filenames() {
        let tokens = tokenize("readme.txt main.rs");
        assert!(tokens.iter().any(|t| matches!(t, Token::Skip(s) if s == "readme.txt")));
        assert!(tokens.iter().any(|t| matches!(t, Token::Skip(s) if s == "main.rs")));
    }

    #[test]
    fn skips_url_and_email() {
        let tokens = tokenize("см. https://github.com/ghbdtn и user@mail.ru");
        assert!(tokens.iter().any(|t| matches!(t, Token::Skip(s) if s.contains("github"))));
        assert!(tokens.iter().any(|t| matches!(t, Token::Skip(s) if s.contains('@'))));
    }
}
