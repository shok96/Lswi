use ort::session::Session;
use ort::value::Tensor;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

const VERSION: &str = "silero-vad-5";
const FILE_NAME: &str = "silero_vad.onnx";
const URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/gh/snakers4/silero-vad@master/src/silero_vad/data/silero_vad.onnx",
    "https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx",
];

const WINDOW: usize = 512;
const CONTEXT: usize = 64;
const STATE_LEN: usize = 2 * 128;
const ENTER: f32 = 0.5;
const EXIT: f32 = 0.35;
const WARMUP: u8 = 2;
const HANG: u8 = 24;
const PREROLL: usize = 14;
const END_SILENCE: usize = WINDOW * 10;

struct Engine {
    _env: ort::environment::Environment,
    session: Session,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("Lswi");
    dir.push("vad");
    dir
}

pub fn ensure(progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    let model = crate::fetch::ensure_cached(
        &data_dir(),
        FILE_NAME,
        VERSION,
        URLS,
        "детектора речи (~2 МБ)",
        200_000,
        progress,
    )?;
    let env = crate::onnx::environment(progress)?;
    let mut guard = ENGINE.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let session = Session::builder(&env)
        .map_err(|e| e.to_string())?
        .with_intra_threads(1)
        .map_err(|e| e.to_string())?
        .commit_from_file(&model)
        .map_err(|e| format!("Не удалось открыть детектор речи: {e}"))?;
    *guard = Some(Engine { _env: env, session });
    Ok(())
}

pub fn release() {
    if let Ok(mut guard) = ENGINE.lock() {
        *guard = None;
    }
}

pub struct Gate {
    pending: Vec<i16>,
    preroll: VecDeque<Vec<i16>>,
    context: Vec<f32>,
    state: Vec<f32>,
    track: SpeechTrack,
    disabled: bool,
}

impl Gate {
    pub fn open() -> Option<Self> {
        let guard = ENGINE.lock().ok()?;
        guard.as_ref()?;
        Some(Self {
            pending: Vec::new(),
            preroll: VecDeque::new(),
            context: vec![0.0; CONTEXT],
            state: vec![0.0; STATE_LEN],
            track: SpeechTrack::default(),
            disabled: false,
        })
    }

    pub fn push(&mut self, pcm: &[i16]) -> Vec<i16> {
        if self.disabled {
            return pcm.to_vec();
        }
        self.pending.extend_from_slice(pcm);
        let mut out = Vec::new();
        while self.pending.len() >= WINDOW {
            let frame: Vec<i16> = self.pending.drain(..WINDOW).collect();
            let speech = match probability(&frame, &mut self.context, &mut self.state) {
                Ok(prob) => self.track.hears(prob),
                Err(_) => {
                    self.disabled = true;
                    out.extend(frame);
                    out.extend(self.pending.drain(..));
                    return out;
                }
            };
            self.track.observe(speech);
            if self.track.take_preroll {
                for pre in self.preroll.drain(..) {
                    out.extend(pre);
                }
            }
            if self.track.emit {
                out.extend_from_slice(&frame);
            } else if self.track.ended {
                out.extend(std::iter::repeat(0).take(END_SILENCE));
            } else {
                self.preroll.push_back(frame);
                while self.preroll.len() > PREROLL {
                    self.preroll.pop_front();
                }
            }
        }
        out
    }

    pub fn finish(&mut self) -> Vec<i16> {
        if self.disabled {
            return std::mem::take(&mut self.pending);
        }
        if self.track.emit && !self.pending.is_empty() {
            let mut tail = std::mem::take(&mut self.pending);
            tail.extend(std::iter::repeat(0).take(END_SILENCE));
            tail
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
struct SpeechTrack {
    speaking: bool,
    hang: u8,
    warm: u8,
    emit: bool,
    take_preroll: bool,
    ended: bool,
}

impl SpeechTrack {
    fn hears(&self, prob: f32) -> bool {
        if self.speaking {
            prob >= EXIT
        } else {
            prob >= ENTER
        }
    }

    fn observe(&mut self, speech: bool) {
        let was = self.emit;
        self.emit = step_speech(&mut self.speaking, &mut self.hang, &mut self.warm, speech);
        self.take_preroll = self.emit && !was;
        self.ended = was && !self.emit;
    }
}

fn step_speech(speaking: &mut bool, hang: &mut u8, warm: &mut u8, speech: bool) -> bool {
    if speech {
        *warm = warm.saturating_add(1);
        if *warm >= WARMUP {
            *speaking = true;
            *hang = HANG;
        }
    } else {
        *warm = 0;
        if *hang > 0 {
            *hang -= 1;
            return true;
        }
        *speaking = false;
    }
    *speaking || *hang > 0
}

fn probability(frame: &[i16], context: &mut [f32], state: &mut [f32]) -> Result<f32, String> {
    let mut input = Vec::with_capacity(CONTEXT + WINDOW);
    input.extend_from_slice(context);
    input.extend(frame.iter().map(|s| *s as f32 / 32768.0));
    let tail = input[input.len() - CONTEXT..].to_vec();

    let mut guard = ENGINE.lock().map_err(|e| e.to_string())?;
    let engine = guard.as_mut().ok_or("Детектор речи не загружен")?;
    let input_t = Tensor::from_array(([1usize, input.len()], input)).map_err(|e| e.to_string())?;
    let state_t = Tensor::from_array(([2usize, 1, 128], state.to_vec())).map_err(|e| e.to_string())?;
    let sr_t = Tensor::from_array((Vec::<i64>::new(), vec![16_000i64])).map_err(|e| e.to_string())?;
    let outputs = engine
        .session
        .run(ort::inputs![
            "input" => input_t,
            "state" => state_t,
            "sr" => sr_t
        ])
        .map_err(|e| e.to_string())?;

    let mut prob = None;
    let mut next_state = None;
    for (_name, value) in outputs.iter() {
        let Ok(array) = value.try_extract_array::<f32>() else {
            continue;
        };
        let Some(slice) = array.as_slice() else {
            continue;
        };
        if slice.len() == state.len() {
            next_state = Some(slice.to_vec());
        } else if !slice.is_empty() && slice.len() <= 4 {
            prob = Some(slice[0]);
        }
    }
    let prob = prob.ok_or("Детектор речи не вернул вероятность")?;
    if let Some(next) = next_state {
        state.copy_from_slice(&next);
    }
    context.copy_from_slice(&tail);
    Ok(prob)
}

#[cfg(test)]
mod tests {
    use super::step_speech;

    #[test]
    fn waits_for_two_speech_frames_then_hangs() {
        let mut speaking = false;
        let mut hang = 0;
        let mut warm = 0;
        assert!(!step_speech(&mut speaking, &mut hang, &mut warm, true));
        assert!(step_speech(&mut speaking, &mut hang, &mut warm, true));
        for _ in 0..super::HANG {
            assert!(step_speech(&mut speaking, &mut hang, &mut warm, false));
        }
        assert!(!step_speech(&mut speaking, &mut hang, &mut warm, false));
    }
}
