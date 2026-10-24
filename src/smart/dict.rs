use std::collections::HashMap;
use std::sync::OnceLock;

const PACKED: &[u8] = include_bytes!("dict.bin");

static DICT: OnceLock<Dict> = OnceLock::new();

pub struct Dict {
    scores: HashMap<String, u8>,
}

impl Dict {
    pub fn global() -> &'static Self {
        DICT.get_or_init(load)
    }

    pub fn score(&self, word: &str) -> u8 {
        if word.is_empty() {
            return 0;
        }
        self.scores.get(&word.to_lowercase()).copied().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.scores.len()
    }
}

fn load() -> Dict {
    parse_packed(PACKED).unwrap_or_else(|_| Dict {
        scores: HashMap::new(),
    })
}

fn parse_packed(data: &[u8]) -> Result<Dict, &'static str> {
    if data.len() < 10 || &data[..4] != b"LSWD" {
        return Err("bad magic");
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != 1 {
        return Err("bad version");
    }
    let count = u32::from_le_bytes(data[6..10].try_into().unwrap()) as usize;
    let mut scores = HashMap::with_capacity(count);
    let mut i = 10;
    for _ in 0..count {
        if i >= data.len() {
            return Err("truncated");
        }
        let n = data[i] as usize;
        i += 1;
        if i + n + 1 > data.len() {
            return Err("truncated word");
        }
        let word = std::str::from_utf8(&data[i..i + n]).map_err(|_| "utf8")?;
        i += n;
        let score = data[i];
        i += 1;
        scores.insert(word.to_string(), score);
    }
    Ok(Dict { scores })
}

#[cfg(test)]
mod tests {
    use super::Dict;

    #[test]
    fn knows_common_words() {
        let dict = Dict::global();
        assert!(dict.len() > 100);
        assert!(dict.score("привет") > 0);
        assert!(dict.score("hello") > 0);
        assert_eq!(dict.score("ghbdtn"), 0);
    }
}
