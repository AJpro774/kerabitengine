//! Non-spatial audio playback for Kerabit (rodio / cpal).
//!
//! Play sounds by filesystem path with volume and optional loop. Spatial /
//! attenuation can be layered later without changing the basic play API.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rodio::source::{Buffered, Source};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use thiserror::Error;

/// Handle to a playing (or finished) sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(u64);

impl SoundId {
    /// Raw id (debug).
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Audio playback errors.
#[derive(Debug, Error)]
pub enum AudioError {
    /// No usable output device (headless CI, permissions, etc.).
    #[error("audio output unavailable: {0}")]
    Device(String),
    /// Failed to open or decode a file.
    #[error("failed to load `{path}`: {source}")]
    Load {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

type Samples = Buffered<Decoder<BufReader<File>>>;

struct Voice {
    id: SoundId,
    sink: Sink,
}

/// Audio engine. Keeps the output stream alive for the process lifetime.
///
/// If device init fails, [`AudioEngine::try_new`] returns an error; callers may
/// fall back to [`AudioEngine::null`] so games still run without speakers.
pub struct AudioEngine {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    master_volume: f32,
    next_id: u64,
    voices: Vec<Voice>,
}

impl AudioEngine {
    /// Open the default output device.
    pub fn try_new() -> Result<Self, AudioError> {
        let (stream, handle) =
            OutputStream::try_default().map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(Self {
            _stream: Some(stream),
            handle: Some(handle),
            master_volume: 1.0,
            next_id: 1,
            voices: Vec::new(),
        })
    }

    /// No-op engine (silent). Useful when no device is available.
    pub fn null() -> Self {
        Self {
            _stream: None,
            handle: None,
            master_volume: 1.0,
            next_id: 1,
            voices: Vec::new(),
        }
    }

    /// Prefer a live device; otherwise return a null engine.
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|_| Self::null())
    }

    /// Whether this engine can emit sound.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.handle.is_some()
    }

    /// Master gain applied to new and existing voices (`0.0..=1.0+`).
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.max(0.0);
        for voice in &self.voices {
            voice.sink.set_volume(self.master_volume);
        }
    }

    /// Current master volume.
    #[inline]
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Play a sound file once at master volume.
    pub fn play(&mut self, path: impl AsRef<Path>) -> Result<SoundId, AudioError> {
        self.play_with(path, 1.0, false)
    }

    /// Play a sound file with per-voice volume and optional loop.
    ///
    /// `volume` is relative to master (`1.0` = master only). Looping uses
    /// rodio's `repeat_infinite` on the decoded buffer.
    pub fn play_with(
        &mut self,
        path: impl AsRef<Path>,
        volume: f32,
        loop_: bool,
    ) -> Result<SoundId, AudioError> {
        self.gc_finished();

        let Some(handle) = self.handle.as_ref() else {
            // Silent engine: accept the call but return a dummy id.
            let id = SoundId(self.next_id);
            self.next_id += 1;
            return Ok(id);
        };

        let path = path.as_ref();
        let source = load_source(path)?;
        let sink = Sink::try_new(handle).map_err(|e| AudioError::Device(e.to_string()))?;

        let gain = (volume.max(0.0) * self.master_volume).max(0.0);
        sink.set_volume(gain);

        if loop_ {
            sink.append(source.repeat_infinite());
        } else {
            sink.append(source);
        }

        let id = SoundId(self.next_id);
        self.next_id += 1;
        self.voices.push(Voice { id, sink });
        Ok(id)
    }

    /// Set volume on an active voice (multiplied by master).
    pub fn set_volume(&mut self, id: SoundId, volume: f32) {
        let gain = (volume.max(0.0) * self.master_volume).max(0.0);
        if let Some(voice) = self.voices.iter().find(|v| v.id == id) {
            voice.sink.set_volume(gain);
        }
    }

    /// Stop and drop a voice.
    pub fn stop(&mut self, id: SoundId) {
        if let Some(i) = self.voices.iter().position(|v| v.id == id) {
            let voice = self.voices.swap_remove(i);
            voice.sink.stop();
        }
    }

    /// Stop all voices.
    pub fn stop_all(&mut self) {
        for voice in self.voices.drain(..) {
            voice.sink.stop();
        }
    }

    fn gc_finished(&mut self) {
        self.voices.retain(|v| !v.sink.empty());
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn load_source(path: &Path) -> Result<Samples, AudioError> {
    let file = File::open(path).map_err(|e| AudioError::Load {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let reader = BufReader::new(file);
    let decoder = Decoder::new(reader).map_err(|e| AudioError::Load {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    Ok(decoder.buffered())
}
