const GREETINGS: &[&str] = &[
    "привет",
    "здравствуйте",
    "здравствуй",
    "hello",
    "hi",
    "hey",
];

const QUESTION: &[&str] = &[
    "как",
    "что",
    "чем",
    "кто",
    "где",
    "когда",
    "зачем",
    "почему",
    "сколько",
    "какой",
    "какая",
    "какое",
    "какие",
    "how",
    "what",
    "when",
    "why",
    "who",
    "where",
    "which",
];

const COMMA_BEFORE: &[&str] = &["а", "но", "что", "чтобы", "потому", "если", "когда"];

#[allow(dead_code)]
pub fn polish_voice(text: &str) -> String {
    polish_voice_ex(text, true)
}

pub fn polish_voice_ex(text: &str, finished: bool) -> String {
    if finished && !cfg!(test) && !has_spoken_command(text) {
        if let Some(out) = crate::smart::model_punctuate(text) {
            return out;
        }
    }
    polish_rules(text, finished)
}

fn has_spoken_command(text: &str) -> bool {
    text.split_whitespace().any(|w| spoken_command(w).is_some())
}

fn polish_rules(text: &str, finished: bool) -> String {
    let mut words = Vec::new();
    for raw in text.split_whitespace() {
        if let Some(cmd) = spoken_command(raw) {
            words.push(cmd.to_string());
        } else {
            words.push(raw.to_string());
        }
    }
    if words.is_empty() {
        return String::new();
    }
    if words.len() == 1 && is_punct_token(&words[0]) {
        return words[0].clone();
    }

    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if is_punct_token(word) {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push_str(word);
            if word != "\n" {
                out.push(' ');
            }
            continue;
        }
        if i > 0 && should_comma_before(word) && !out.trim_end().ends_with(',') {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push_str(", ");
        } else if i == 1 && is_greeting(&words[0]) && !out.trim_end().ends_with(',') {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push_str(", ");
        } else if !out.is_empty() && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        out.push_str(word);
    }

    let trimmed = out.trim().to_string();
    let mut polished = capitalize_sentences(&trimmed);
    if finished && !polished.is_empty() && !ends_with_sentence_punct(&polished) {
        if looks_question(&words) {
            polished.push('?');
        } else if words.len() > 1 {
            polished.push('.');
        }
    }
    polished
}

fn spoken_command(word: &str) -> Option<&'static str> {
    match word.to_lowercase().as_str() {
        "точка" | "dot" => Some("."),
        "запятая" | "comma" => Some(","),
        "вопрос" | "вопросительный" => Some("?"),
        "восклицание" | "восклицательный" => Some("!"),
        "тире" => Some("—"),
        "newline" | "enter" => Some("\n"),
        _ => None,
    }
}

fn is_punct_token(word: &str) -> bool {
    matches!(word, "." | "," | "?" | "!" | "—" | "\n")
}

fn is_greeting(word: &str) -> bool {
    let key = strip_punct(word).to_lowercase();
    GREETINGS.contains(&key.as_str())
}

fn should_comma_before(word: &str) -> bool {
    COMMA_BEFORE.contains(&word.to_lowercase().as_str())
}

fn looks_question(words: &[String]) -> bool {
    words.iter().any(|w| {
        let key = strip_punct(w).to_lowercase();
        QUESTION.contains(&key.as_str())
    })
}

fn ends_with_sentence_punct(text: &str) -> bool {
    text.chars()
        .rev()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| matches!(c, '.' | '?' | '!' | '…'))
}

fn strip_punct(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphabetic() || *c == '-')
        .collect()
}

fn capitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for ch in text.chars() {
        if start && ch.is_alphabetic() {
            for up in ch.to_uppercase() {
                out.push(up);
            }
            start = false;
            continue;
        }
        out.push(ch);
        if matches!(ch, '.' | '?' | '!' | '\n') {
            start = true;
        } else if !ch.is_whitespace() {
            start = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{polish_voice, polish_voice_ex};

    #[test]
    fn polishes_greeting_question() {
        assert_eq!(polish_voice("привет как дела"), "Привет, как дела?");
    }

    #[test]
    fn spoken_period() {
        assert_eq!(polish_voice("привет точка"), "Привет.");
    }

    #[test]
    fn no_period_until_finished() {
        assert_eq!(polish_voice_ex("релиз собран", false), "Релиз собран");
        assert_eq!(polish_voice_ex("релиз собран", true), "Релиз собран.");
    }
}
