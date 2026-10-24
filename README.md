# LSWI

Tray application that rewrites selected text typed in the wrong keyboard layout, and, on Windows, types Russian speech into the focused field. Select `ghbdtn` and double-tap the hotkey: the selection becomes `привет`. Press the voice hotkey, speak, and press it again: the recognized text is typed where the caret is.

It runs on **Windows**, **macOS**, and **Linux**. Layout correction is on every platform. Voice input, the settings window, and the on-screen voice notice are Windows-only. macOS and Linux keep every option in the tray menu.

Closing the settings window does not exit the process. The tray item **Выход** does. A second launch on Windows finds the running process, brings it forward, and opens **Настройки**. A second launch on macOS or Linux prints `LSWI уже запущена` and exits.

## How to use

1. Select text that was typed in the wrong layout.
2. Double-tap the layout hotkey. The default is **Shift**. Left and right Shift, Ctrl, and Alt count as the same key.
3. The selection is replaced. With smart conversion on, each word is decided on its own. With it off, the whole selection is remapped.

The second tap is swallowed. The replacement runs when that key goes up, after Shift, Ctrl, and Alt have been released. A chord cancels the pending tap: if another key goes down while the hotkey is still held, that pair is ordinary typing. Keys this process injects are ignored, so the paste does not count as another tap. While voice input is recording, the layout hotkey does nothing.

The system keyboard layout stays where it is unless **switch after replace** is turned on.

### Voice input (Windows)

The voice hotkey defaults to **F8** with no modifiers. One press starts a session, the same combo or **Esc** stops it. The first session downloads the recognizer, the Russian model, the voice-activity model, and, when punctuation is on, the punctuation model. A small notice follows the session: status, then the partial text.

Speech is captured from the selected microphone, or from the system default when the device id is empty. Audio is resampled to 16 kHz. Silence is dropped by a voice-activity gate before it reaches the recognizer. Partial results are typed into the focused control as they arrive. A correction deletes the characters that no longer match and types the remainder. When the session ends, the utterance is punctuated and the typed text is updated to match.

Spoken words that name punctuation are inserted as that mark and skip the neural punctuator: `точка` / `dot`, `запятая` / `comma`, `вопрос` / `вопросительный`, `восклицание` / `восклицательный`, `тире`, `newline` / `enter`.

## What it does

| | Windows | macOS | Linux |
|---|---|---|---|
| Layout hotkey | Low-level keyboard hook (`WH_KEYBOARD_LL`) | `rdev` global listener | `rdev` global listener |
| Selection | Native edit control, then `WM_COPY`, then Ctrl+C, then Ctrl+Insert | Cmd+C through the clipboard | Ctrl+C through the clipboard |
| Replace | Ctrl+V | Cmd+V | Ctrl+V |
| Smart words | Embedded dictionary | Embedded dictionary | Embedded dictionary |
| Installed layouts | Each pair of installed HKLs is scored | EN ↔ RU table | EN ↔ RU table |
| Switch layout after | `WM_INPUTLANGCHANGEREQUEST` toward RU (`0x419`) or the other installed layout | Control+Space through System Events | `ibus engine`, then `xkb-switch` |
| Voice | Vosk small Russian, Silero VAD, optional punctuation | — | — |
| Settings UI | Win32 window from the tray | Tray menu | Tray menu |
| Single instance | Mutex `Local\LswiSingleton` | `flock` on `lswi.lock` | `flock` on `lswi.lock` |
| Start at login | `HKCU\...\Run` value `Lswi` | `~/Library/LaunchAgents/com.lswi.app.plist` | `~/.config/autostart/lswi.desktop` |

A selection longer than 50 000 bytes is left alone on Windows. The previous clipboard text is put back about 800 ms after a successful paste (500 ms on macOS and Linux). If the clipboard cannot be read, Windows shows a tray balloon. macOS and Linux leave the selection unchanged and do not show a balloon.

## Layout conversion

The fixed table is QWERTY ↔ ЙЦУКЕН, including the punctuation that types a letter on the other side (`,` → `б`, `.` → `ю`, `;` → `ж`, `'` → `э`, `[` → `х`, `]` → `ъ`, `` ` `` → `ё`, and the shifted pairs). Characters outside the table stay as they are. The whole-string direction follows whichever script has more letters. A string that does not change is not pasted.

Smart conversion is the default (`smart_layout`). It runs when the packed dictionary has loaded (more than 100 entries). The dictionary is compiled into the binary as `src/smart/dict.bin` (`LSWD` version 1: count, then length-prefixed UTF-8 words with a frequency score). `tools/build_dict.py` rebuilds that file from the Hermit Dave 2018 English and Russian 50k lists, with a small core list used when the download fails.

Each whitespace-separated chunk is classified, then words are decided:

- A URL (`://` or `www.`), an email, a path (`\` or two or more `/`), a hex literal, an identifier with `_`, CamelCase, an ALL-CAPS token, a file extension, or a token that mixes letters and digits is copied through.
- A word whose remapped form scores at least 16 and more than three times the original score is remapped.
- A word whose script disagrees with the majority of *known* words in the selection is remapped, so one mistyped word inside a Russian sentence follows the sentence.
- A known dictionary word, or a word on the short technical list (`json`, `http`, `null`, `true`, …), stays.
- A glued run of known prefixes can be split. The leftover tail is kept when it is known and remapped otherwise.
- A Latin word of four or more letters with no `a e i o u y` is treated as a layout mistype. Layout punctuation that produces a Russian letter is allowed inside that word (`cdj,jlf` → `свобода`).
- Anything still undecided stays. If nothing was remapped and nothing was recognized, the outcome is uncertain and the selection is not touched. A selection that is already dictionary text is reported as unchanged (Windows shows a balloon).

On Windows, with smart conversion off, the EN ↔ RU candidate is scored against a remap through every pair of installed keyboard layouts (`VkKeyScanExW` on the source HKL, `ToUnicodeEx` on the destination). The candidate with the highest letter-change score is pasted. The EN ↔ RU candidate gets a fixed bonus so the table wins ties.

Case follows the source character: `Ghbdtn` becomes `Привет`.

## Voice models

Downloaded on first use, and only on Windows. Nothing in this repository is the recognizer or the models. Each archive is fetched over HTTPS, checked for a minimum size, unpacked, and marked with a `VERSION` file so a later run reuses it.

| Piece | Version marker | Location under `%LOCALAPPDATA%\Lswi` | Source |
|---|---|---|---|
| Vosk library (`libvosk.dll` and the DLLs beside it) | `0.3.45` | `voice\native` | `vosk-win64-0.3.45.zip` from the alphacep GitHub release |
| Russian acoustic model | directory `vosk-model-small-ru-0.22` with `am/` and `conf/` or `graph/` | `voice\models` | Hugging Face `rhasspy/vosk-models`, then archive.org, then alphacephei.com (~45 MB) |
| Silero VAD | `silero-vad-5` | `vad\silero_vad.onnx` | snakers4/silero-vad (~2 MB) |
| ONNX Runtime | `1.31.0` | `onnx\1.31.0\onnxruntime.dll` | NuGet `Microsoft.ML.OnnxRuntime` 1.31.0, then the official win-x64 zip |
| Punctuation | `rupunct-small-int8-1` | `smart\rupunct_small_int8.onnx` and `tokenizer.json` | Hugging Face `ekhodzitsky/rupunct-small-onnx`, then hf-mirror (~29 MB) |

The Vosk path has to be ASCII. When the user profile path is not, the short 8.3 path is used. A model directory the recognizer cannot open fails that session and the notice stays up for a few seconds.

The recognizer is created at 16 kHz. The VAD gate uses 512-sample windows, a 64-sample context, enter threshold 0.5, exit threshold 0.35, two warm-up frames, and a short hang before speech is considered over. If the VAD file cannot be loaded, chunks pass through unfiltered.

Punctuation runs when `voice_polish` is on. The ONNX model labels each token (period, comma, question mark, dash, colon, ellipsis, and the letter case of the following word). A spoken punctuation command bypasses the model and uses the rule table instead. If the model is missing, the rules capitalize the start of a sentence, insert a comma after a greeting and before `а` / `но` / `что` / `если`, and add `?` or `.` when the utterance is finished. Final punctuation is applied in windows of 60 words.

Vosk, the VAD session, and the punctuation session are released after three minutes without a recording. The next voice hotkey loads them again.

The microphone list is refreshed from the endpoint notification. A saved device that disappears is cleared, the session stops, and a balloon says the microphone was disconnected. The level meter in **Настройки** is the same capture path with recognition skipped.

## Settings

`settings.json` is written with `serde_json`. Missing fields take the defaults below. `double_tap_ms` is clamped to 150–2000. A zero hotkey code is replaced with the default. Voice modifier bits outside Ctrl, Shift, Alt, and Win are cleared, and a modifier that is the voice key itself is removed from the modifier mask.

| | Settings |
|---|---|
| Windows | `%APPDATA%\Lswi\settings.json` |
| macOS | `~/Library/Application Support/Lswi/settings.json` |
| Linux | `$XDG_CONFIG_HOME/Lswi/settings.json`, or `~/.config/Lswi/settings.json` |

The lock file on macOS and Linux is `$XDG_RUNTIME_DIR/Lswi/lswi.lock`, or the config directory when the runtime directory is unset.

| Field | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Listen for the layout hotkey |
| `hotkey_vk` | `160` (`VK_LSHIFT`) | Double-tap key. Codes match Windows virtual-key codes on every platform |
| `double_tap_ms` | `400` | Maximum gap between the two key-ups |
| `start_with_windows` | `true` | Run at login on the current platform. The name is historical |
| `switch_layout_after` | `false` | After a successful replace, move the system layout toward the script of the result |
| `smart_layout` | `true` | Decide per word. Off uses the whole-string remap |
| `voice_enabled` | `true` | Listen for the voice hotkey. Windows only |
| `voice_hotkey_vk` | `119` (`VK_F8`) | Voice key |
| `voice_hotkey_mods` | `0` | `1` Ctrl, `2` Shift, `4` Alt, `8` Win |
| `microphone_id` | `""` | Capture device id. Empty is the system default |
| `voice_polish` | `true` | Punctuate and fix case after a voice session |

On Windows the tray left click and **Настройки** open the window. The first launch with no settings file opens it immediately. **Назначить** waits for the next key-down and stores that virtual-key code. The voice assign button stores a key plus the modifiers held with it. macOS and Linux assign a single key from the tray item **Назначить клавишу**, and the double-tap gap is one of 250, 400, 600, 800, or 1000 ms.

Start at login stores the current executable path. On Windows the `Run` value is the quoted path. On macOS the LaunchAgent label is `com.lswi.app` and it is bootstrapped into `gui/<uid>`. On Linux the autostart entry is `lswi.desktop` with `Terminal=false`.

## Build

Rust stable and Cargo. `Cargo.lock` is part of the repository. The release profile enables fat LTO, one codegen unit, and symbol stripping. The Windows binary is built with the Windows subsystem, so it has no console window.

Voice, ONNX Runtime, and the punctuation stack are `cfg`'d to Windows and are loaded at run time (`load-dynamic`). Compiling on macOS or Linux does not need those libraries. Compiling on Windows does not need them either: the `windows` crate and the Rust MSVC toolchain are enough.

### Quick build

Scripts in `scripts/` compile a release binary and copy it to `dist/`.

| System | Command | Output |
|---|---|---|
| Windows (PowerShell) | `.\scripts\build.ps1` | `dist\lswi.exe` |
| Windows (cmd) | `scripts\build.bat` | `dist\lswi.exe` |
| macOS | `./scripts/build.sh` | `dist/lswi` |
| Linux | `./scripts/build.sh` | `dist/lswi` |

On macOS and Linux, make the script executable once: `chmod +x scripts/build.sh`.

Optional flags (same idea on every script):

```
# Windows
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Run

# macOS / Linux
./scripts/build.sh --test
./scripts/build.sh --run
```

`-Test` / `--test` runs `cargo test` before the release build. `-Run` / `--run` starts the app from `dist/` after a successful build.

The same result without a script:

```
cargo build --release
```

The cargo binary is `target/release/lswi` (`target\release\lswi.exe` on Windows). The scripts also leave a copy in `dist/` so you do not have to dig through `target/`.

### Windows

Use the MSVC toolchain, `x86_64-pc-windows-msvc`. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select the **Desktop development with C++** workload, which provides MSVC and the Windows SDK. Then:

```
rustup toolchain install stable
rustup default stable
.\scripts\build.ps1
```

No extra system libraries are required to compile or to correct layout. Voice input downloads its libraries on first use. The global hook needs a normal user session; it does not need an extra capability prompt.

### macOS

Xcode Command Line Tools supply `clang` and the system frameworks used by the tray icon.

```
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
chmod +x scripts/build.sh
./scripts/build.sh
```

The global hotkey needs **System Settings → Privacy & Security → Accessibility** for LSWI, or for Terminal / iTerm when the binary is started from there. Without that grant the listener exits and the tray tooltip says the keyboard is not available. `switch_layout_after` sends Control+Space, which is the usual input-source shortcut. If that shortcut was changed in Keyboard settings, the menu item follows the system shortcut and may not land on Russian.

The script ad-hoc signs `dist/lswi` so a local unsigned binary can be started from Terminal.

### Linux

The tray icon links GTK 3 and libayatana-appindicator (or the older libappindicator). The global hook uses X11. Install those before the build script.

Debian and Ubuntu:

```
sudo apt install build-essential pkg-config libgtk-3-dev libxdo-dev libayatana-appindicator3-dev libx11-dev libxtst-dev
chmod +x scripts/build.sh
./scripts/build.sh
```

If `libayatana-appindicator3-dev` is not in the archive, use `libappindicator3-dev` instead.

Fedora:

```
sudo dnf install gcc pkgconf-pkg-config gtk3-devel libxdo-devel libayatana-appindicator-gtk3-devel libX11-devel libXtst-devel
./scripts/build.sh
```

Arch Linux:

```
sudo pacman -S --needed base-devel pkgconf gtk3 xdotool libayatana-appindicator libx11 libxtst
./scripts/build.sh
```

`libappindicator-gtk3` is the alternative to `libayatana-appindicator` when the Ayatana package is absent.

The global hook is reliable on **X11**. On a Wayland session the listener often cannot see keys typed in other applications, and the tray tooltip says so. Clipboard access for the replace uses the data-control portal where the compositor provides it. `switch_layout_after` runs `ibus engine xkb:ru::rus` or `xkb:us::eng`. When ibus is missing or refuses the engine, it tries `xkb-switch -s ru` or `xkb-switch -s us`.

### Check

```
cargo test
```

Or through the scripts: `.\scripts\build.ps1 -Test` / `./scripts/build.sh --test`.

`cargo test` covers the EN ↔ RU table, smart word decisions (including layout punctuation and skipped URLs), dictionary load, hotkey names, the double-tap interval clamp, and the rule-based voice punctuation. It does not download Vosk, ONNX Runtime, or the punctuation model.
