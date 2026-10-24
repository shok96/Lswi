#!/usr/bin/env python3
"""Pack EN/RU frequency lists into src/smart/dict.bin (LSWD v1)."""

from __future__ import annotations

import os
import struct
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src" / "smart" / "dict.bin"

EN_URLS = [
    "https://cdn.jsdelivr.net/gh/hermitdave/FrequencyWords@master/content/2018/en/en_50k.txt",
    "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/en/en_50k.txt",
]
RU_URLS = [
    "https://cdn.jsdelivr.net/gh/hermitdave/FrequencyWords@master/content/2018/ru/ru_50k.txt",
    "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/ru/ru_50k.txt",
]

# Always present so tests and everyday mistypes work if the download fails.
CORE = [
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i",
    "it", "for", "not", "on", "with", "he", "as", "you", "do", "at",
    "this", "but", "his", "by", "from", "they", "we", "say", "her", "she",
    "or", "an", "will", "my", "one", "all", "would", "there", "their", "what",
    "so", "up", "out", "if", "about", "who", "get", "which", "go", "me",
    "when", "make", "can", "like", "time", "no", "just", "him", "know", "take",
    "people", "into", "year", "your", "good", "some", "could", "them", "see", "other",
    "than", "then", "now", "look", "only", "come", "its", "over", "think", "also",
    "back", "after", "use", "two", "how", "our", "work", "first", "well", "way",
    "even", "new", "want", "because", "any", "these", "give", "day", "most", "us",
    "hello", "hi", "please", "thanks", "thank", "yes", "ok", "okay", "sorry", "hey",
    "login", "admin", "user", "password", "email", "world", "test", "true", "false",
    "error", "warning", "debug", "info", "file", "open", "save", "edit", "view",
    "help", "home", "search", "settings", "window", "linux", "windows", "mac",
    "я", "и", "в", "не", "на", "что", "он", "с", "как", "а",
    "то", "все", "она", "так", "его", "но", "да", "ты", "к", "у",
    "же", "вы", "за", "бы", "по", "только", "ее", "мне", "было", "вот",
    "от", "меня", "еще", "нет", "о", "из", "ему", "теперь", "когда", "даже",
    "ну", "вдруг", "ли", "если", "уже", "или", "ни", "быть", "был", "него",
    "до", "вас", "нибудь", "опять", "уж", "вам", "ведь", "там", "потом", "себя",
    "ничего", "ей", "может", "они", "тут", "где", "есть", "надо", "ней", "для",
    "мы", "тебя", "их", "чем", "была", "сам", "чтоб", "без", "будто", "чего",
    "раз", "тоже", "себе", "под", "будет", "ж", "тогда", "кто", "этот", "того",
    "потому", "этого", "какой", "совсем", "ним", "здесь", "этом", "один", "почти", "мой",
    "тем", "чтобы", "нее", "сейчас", "были", "куда", "зачем", "сказать", "ведь",
    "привет", "пока", "спасибо", "пожалуйста", "здравствуйте", "добрый", "день",
    "утро", "вечер", "ночь", "дела", "хорошо", "плохо", "давай", "ладно",
    "извините", "прости", "можно", "конечно", "почему", "зачем", "сколько",
    "сегодня", "завтра", "вчера", "очень", "просто", "здесь", "там",
]


def is_word(token: str) -> bool:
    if not token or len(token) > 48:
        return False
    letters = 0
    for ch in token:
        if ch.isalpha():
            letters += 1
        elif ch in "-'":
            continue
        else:
            return False
    return letters >= 1


def score_from_rank(rank: int, total: int) -> int:
    if total <= 1:
        return 255
    # Higher frequency (lower rank) → higher score. Keep a floor so rare words still win vs unknown.
    return max(8, 255 - (rank * 247) // (total - 1))


def load_freq(urls: list[str], limit: int, local: Path | None = None) -> list[str]:
    raw = ""
    if local is not None and local.exists() and local.stat().st_size > 1000:
        print(f"read {local}", flush=True)
        raw = local.read_text(encoding="utf-8", errors="replace")
    else:
        last_err: Exception | None = None
        for url in urls:
            print(f"download {url}", flush=True)
            req = urllib.request.Request(url, headers={"User-Agent": "lswi-dict-builder"})
            try:
                with urllib.request.urlopen(req, timeout=45) as resp:
                    raw = resp.read().decode("utf-8", errors="replace")
                break
            except Exception as exc:
                last_err = exc
                print(f"  failed: {exc}", flush=True)
        if not raw:
            raise last_err or RuntimeError("no frequency source")
    words: list[str] = []
    seen: set[str] = set()
    for line in raw.splitlines():
        part = line.strip().split()
        if not part:
            continue
        word = part[0].casefold()
        if not is_word(word) or word in seen:
            continue
        seen.add(word)
        words.append(word)
        if len(words) >= limit:
            break
    if len(words) < 1000:
        raise RuntimeError(f"too few words from {url}: {len(words)}")
    return words


def merge(en: list[str], ru: list[str]) -> dict[str, int]:
    scores: dict[str, int] = {}
    for rank, word in enumerate(en):
        scores[word] = max(scores.get(word, 0), score_from_rank(rank, len(en)))
    for rank, word in enumerate(ru):
        scores[word] = max(scores.get(word, 0), score_from_rank(rank, len(ru)))
    for word in CORE:
        key = word.casefold()
        scores[key] = max(scores.get(key, 0), 250)
    return scores


def pack(scores: dict[str, int]) -> bytes:
    items = sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))
    out = bytearray()
    out.extend(b"LSWD")
    out.extend(struct.pack("<H", 1))
    out.extend(struct.pack("<I", len(items)))
    for word, score in items:
        raw = word.encode("utf-8")
        if not raw or len(raw) > 255:
            continue
        out.append(len(raw))
        out.extend(raw)
        out.append(min(255, max(1, score)))
    return bytes(out)


def main() -> int:
    try:
        temp = Path(os.environ.get("TEMP") or os.environ.get("TMP") or "/tmp")
        en = load_freq(EN_URLS, 40_000, temp / "en_50k.txt")
        ru = load_freq(RU_URLS, 40_000, temp / "ru_50k.txt")
    except Exception as exc:
        print(f"download failed ({exc}), using core list only", file=sys.stderr)
        en, ru = [], []
    scores = merge(en, ru)
    data = pack(scores)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(data)
    print(f"wrote {OUT} ({len(data)} bytes, {len(scores)} words)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
