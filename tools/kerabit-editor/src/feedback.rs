//! Short UI cues through Kerabit's audio engine.
//!
//! Sounds are synthesized (no asset files) and played with [`kerabit::AudioEngine::play_pcm`].
//! Volume stays modest. The same cue will not retrigger inside a short gap.

use std::time::{Duration, Instant};

use kerabit::{AudioEngine, MixBus};

const SAMPLE_RATE: u32 = 22_050;
const RATE: f32 = SAMPLE_RATE as f32;

/// Editor actions that earn a sound. Not every status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Select,
    Place,
    Save,
    Undo,
    Redo,
    Play,
    Stop,
    Error,
    Delete,
}

pub struct EditorSfx {
    engine: AudioEngine,
    last: Option<(Cue, Instant)>,
}

impl EditorSfx {
    pub fn new() -> Self {
        let mut engine = AudioEngine::new();
        // Modest. Samples are already quiet; this keeps speakers from jumping.
        engine.set_master_volume(0.5);
        engine.set_bus_volume(MixBus::Sfx, 0.55);
        Self { engine, last: None }
    }

    /// Play `cue` unless we just played it (or anything, a moment ago).
    pub fn play(&mut self, cue: Cue) {
        let now = Instant::now();
        if let Some((prev, at)) = self.last {
            let elapsed = now.saturating_duration_since(at);
            if elapsed < Duration::from_millis(45) {
                return;
            }
            if prev == cue && elapsed < Duration::from_millis(120) {
                return;
            }
        }
        self.last = Some((cue, now));
        let samples = render(cue);
        let _ = self.engine.play_pcm(&samples, SAMPLE_RATE, 1, 1.0);
    }

    pub fn maintain(&mut self) {
        self.engine.maintain();
    }
}

impl Default for EditorSfx {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a status line to a cue. Most lines stay silent on purpose.
pub fn cue_for_status(status: &str) -> Option<Cue> {
    let s = status.trim();
    let lower = s.to_ascii_lowercase();
    if lower.contains("fail") || lower.contains("error") {
        return Some(Cue::Error);
    }
    if s == "Undo" {
        return Some(Cue::Undo);
    }
    if s == "Redo" {
        return Some(Cue::Redo);
    }
    if lower.starts_with("saved") {
        return Some(Cue::Save);
    }
    if lower.starts_with("playing") {
        return Some(Cue::Play);
    }
    if lower.starts_with("play stopped") || lower.starts_with("play exited") {
        return Some(Cue::Stop);
    }
    if lower.starts_with("placed")
        || lower.starts_with("added entity")
        || lower.starts_with("duplicated")
        || lower.starts_with("instanced")
    {
        return Some(Cue::Place);
    }
    if lower.starts_with("deleted") {
        return Some(Cue::Delete);
    }
    if lower.starts_with("selected") {
        return Some(Cue::Select);
    }
    None
}

struct Note {
    freq: f32,
    end_freq: f32,
    start: f32,
    dur: f32,
    amp: f32,
}

fn render(cue: Cue) -> Vec<f32> {
    let notes: &[Note] = match cue {
        Cue::Select => &[Note {
            freq: 1318.0,
            end_freq: 1568.0,
            start: 0.0,
            dur: 0.04,
            amp: 0.34,
        }],
        Cue::Place => &[
            Note {
                freq: 523.0,
                end_freq: 523.0,
                start: 0.0,
                dur: 0.06,
                amp: 0.28,
            },
            Note {
                freq: 784.0,
                end_freq: 784.0,
                start: 0.045,
                dur: 0.07,
                amp: 0.24,
            },
        ],
        Cue::Save => &[
            Note {
                freq: 523.0,
                end_freq: 523.0,
                start: 0.0,
                dur: 0.07,
                amp: 0.2,
            },
            Note {
                freq: 659.0,
                end_freq: 659.0,
                start: 0.05,
                dur: 0.08,
                amp: 0.18,
            },
            Note {
                freq: 784.0,
                end_freq: 784.0,
                start: 0.1,
                dur: 0.1,
                amp: 0.16,
            },
        ],
        Cue::Undo => &[Note {
            freq: 494.0,
            end_freq: 349.0,
            start: 0.0,
            dur: 0.08,
            amp: 0.26,
        }],
        Cue::Redo => &[Note {
            freq: 349.0,
            end_freq: 494.0,
            start: 0.0,
            dur: 0.08,
            amp: 0.26,
        }],
        Cue::Play => &[
            Note {
                freq: 523.0,
                end_freq: 523.0,
                start: 0.0,
                dur: 0.08,
                amp: 0.18,
            },
            Note {
                freq: 659.0,
                end_freq: 659.0,
                start: 0.055,
                dur: 0.08,
                amp: 0.16,
            },
            Note {
                freq: 784.0,
                end_freq: 784.0,
                start: 0.11,
                dur: 0.1,
                amp: 0.15,
            },
        ],
        Cue::Stop => &[Note {
            freq: 440.0,
            end_freq: 294.0,
            start: 0.0,
            dur: 0.1,
            amp: 0.22,
        }],
        Cue::Error => &[
            Note {
                freq: 196.0,
                end_freq: 174.0,
                start: 0.0,
                dur: 0.11,
                amp: 0.22,
            },
            Note {
                freq: 392.0,
                end_freq: 370.0,
                start: 0.0,
                dur: 0.08,
                amp: 0.06,
            },
        ],
        Cue::Delete => &[Note {
            freq: 196.0,
            end_freq: 130.0,
            start: 0.0,
            dur: 0.07,
            amp: 0.28,
        }],
    };
    mix(notes)
}

fn mix(notes: &[Note]) -> Vec<f32> {
    let end = notes
        .iter()
        .map(|n| n.start + n.dur)
        .fold(0.0_f32, f32::max);
    let len = ((end * RATE) as usize).clamp(1, SAMPLE_RATE as usize);
    let mut out = vec![0.0_f32; len];
    for note in notes {
        let mut phase = 0.0_f32;
        let start_i = (note.start * RATE) as usize;
        let count = (note.dur * RATE).round().max(1.0) as usize;
        for i in 0..count {
            let idx = start_i + i;
            if idx >= out.len() {
                break;
            }
            let k = i as f32 / (count.saturating_sub(1).max(1) as f32);
            let freq = note.freq + (note.end_freq - note.freq) * k;
            phase = (phase + freq / RATE) % 1.0;
            let env = (-(i as f32) / (count as f32 * 0.42).max(1.0)).exp() * (1.0 - k * 0.35);
            out[idx] += (phase * std::f32::consts::TAU).sin() * env * note.amp;
        }
    }
    for sample in &mut out {
        *sample = sample.clamp(-0.75, 0.75);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_cues_stay_sparse() {
        assert_eq!(cue_for_status("Undo"), Some(Cue::Undo));
        assert_eq!(cue_for_status("Redo"), Some(Cue::Redo));
        assert_eq!(
            cue_for_status("Saved levels/a.kerabit.json"),
            Some(Cue::Save)
        );
        assert_eq!(
            cue_for_status("Saved prefab (1 entity) → x"),
            Some(Cue::Save)
        );
        assert_eq!(cue_for_status("Snap on (size 0.50) saved"), None);
        assert_eq!(
            cue_for_status("Playing (saved scene) — Esc in play window or Stop; selection kept"),
            Some(Cue::Play)
        );
        assert_eq!(
            cue_for_status("Play failed: write temp: nope"),
            Some(Cue::Error)
        );
        assert_eq!(cue_for_status("Placed cube"), Some(Cue::Place));
        assert_eq!(cue_for_status("Added entity"), Some(Cue::Place));
        assert_eq!(cue_for_status("Duplicated 1 entity"), Some(Cue::Place));
        assert_eq!(cue_for_status("Instanced hazard from a"), Some(Cue::Place));
        assert_eq!(cue_for_status("Deleted cube"), Some(Cue::Delete));
        assert_eq!(cue_for_status("Selected \"cube\""), Some(Cue::Select));
        assert_eq!(
            cue_for_status("Play stopped — back to edit"),
            Some(Cue::Stop)
        );
        assert_eq!(cue_for_status("Script checks OK (types + host API)"), None);
        assert_eq!(cue_for_status("Aligned to primary X"), None);
        assert_eq!(cue_for_status("New scene"), None);
        assert_eq!(cue_for_status("Opened /tmp/a.kerabit.json"), None);
        assert_eq!(
            cue_for_status("Rename failed: empty name"),
            Some(Cue::Error)
        );
    }

    #[test]
    fn cues_are_short_and_not_harsh() {
        for cue in [
            Cue::Select,
            Cue::Place,
            Cue::Save,
            Cue::Undo,
            Cue::Redo,
            Cue::Play,
            Cue::Stop,
            Cue::Error,
            Cue::Delete,
        ] {
            let pcm = render(cue);
            assert!(!pcm.is_empty(), "{cue:?} silent");
            assert!(
                pcm.len() < SAMPLE_RATE as usize / 2,
                "{cue:?} longer than half a second"
            );
            let peak = pcm.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
            assert!(peak > 0.02, "{cue:?} inaudible");
            assert!(peak <= 0.75, "{cue:?} peak {peak} too hot");
        }
    }
}
