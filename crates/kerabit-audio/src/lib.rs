//! Audio playback for Kerabit (rodio / cpal).
//!
//! - Non-spatial SFX (`play` / `play_with`) on the **sfx** bus
//! - Stereo positional attenuation (`play_at` / `play_at_with`) via rodio `SpatialSink`
//! - Mix buses: **master** × **sfx** / **music**
//! - Streaming music (`play_music` / `play_music_with`) — WAV decode without full-file buffer
//! - Null-safe when no output device (`AudioEngine::null` / `new`)

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use kerabit_math::Vec3;
use rodio::source::{Buffered, Source};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, SpatialSink};
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

/// Mix bus for gain staging (multiplied by master).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MixBus {
    /// Short one-shots and spatial SFX.
    Sfx,
    /// Background / streamed music.
    Music,
}

/// World-space listener used for stereo positional attenuation.
#[derive(Clone, Copy, Debug)]
pub struct AudioListener {
    /// Head position.
    pub position: Vec3,
    /// Facing direction (normalized when applied).
    pub forward: Vec3,
    /// Up vector (normalized when applied).
    pub up: Vec3,
    /// Distance between ears in world units (default `0.25`).
    pub ear_separation: f32,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: -Vec3::Z,
            up: Vec3::Y,
            ear_separation: 0.25,
        }
    }
}

impl AudioListener {
    /// Build a listener from an eye / look-at / up triple (e.g. camera).
    pub fn from_look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let forward = (target - eye).normalize_or_zero();
        let forward = if forward.length_squared() < 1e-8 {
            -Vec3::Z
        } else {
            forward
        };
        Self {
            position: eye,
            forward,
            up: if up.length_squared() < 1e-8 {
                Vec3::Y
            } else {
                up.normalize()
            },
            ear_separation: 0.25,
        }
    }

    fn ear_positions(self) -> ([f32; 3], [f32; 3]) {
        let forward = self.forward.normalize_or_zero();
        let mut up = self.up.normalize_or_zero();
        if up.length_squared() < 1e-8 {
            up = Vec3::Y;
        }
        let mut right = forward.cross(up);
        if right.length_squared() < 1e-8 {
            right = Vec3::X;
        } else {
            right = right.normalize();
        }
        let half = (self.ear_separation.max(1e-4)) * 0.5;
        let left = self.position - right * half;
        let right_ear = self.position + right * half;
        // Ensure ears are never identical (rodio asserts).
        if (left - right_ear).length_squared() < 1e-10 {
            (
                (self.position - Vec3::X * half).to_array(),
                (self.position + Vec3::X * half).to_array(),
            )
        } else {
            (left.to_array(), right_ear.to_array())
        }
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

enum VoiceOut {
    Flat(Sink),
    Spatial(SpatialSink),
}

impl VoiceOut {
    fn set_volume(&self, volume: f32) {
        match self {
            Self::Flat(s) => s.set_volume(volume),
            Self::Spatial(s) => s.set_volume(volume),
        }
    }

    fn stop(self) {
        match self {
            Self::Flat(s) => s.stop(),
            Self::Spatial(s) => s.stop(),
        }
    }

    fn empty(&self) -> bool {
        match self {
            Self::Flat(s) => s.empty(),
            Self::Spatial(s) => s.empty(),
        }
    }
}

struct Voice {
    id: SoundId,
    bus: MixBus,
    /// Per-voice gain before bus × master.
    base_volume: f32,
    out: VoiceOut,
    /// When set, voice is spatial and tracks this emitter.
    emitter: Option<Vec3>,
    /// Streamed looping music: path to reopen when the decoder ends.
    stream_loop: Option<PathBuf>,
}

/// Audio engine. Keeps the output stream alive for the process lifetime.
///
/// If device init fails, [`AudioEngine::try_new`] returns an error; callers may
/// fall back to [`AudioEngine::null`] so games still run without speakers.
pub struct AudioEngine {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    master_volume: f32,
    sfx_volume: f32,
    music_volume: f32,
    listener: AudioListener,
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
            sfx_volume: 1.0,
            music_volume: 1.0,
            listener: AudioListener::default(),
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
            sfx_volume: 1.0,
            music_volume: 1.0,
            listener: AudioListener::default(),
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

    /// Current listener (ears derived from position / forward / up).
    #[inline]
    pub fn listener(&self) -> AudioListener {
        self.listener
    }

    /// Replace the listener and refresh active spatial voices.
    pub fn set_listener(&mut self, listener: AudioListener) {
        self.listener = listener;
        self.apply_listener_to_spatial();
    }

    /// Convenience: listener follows a camera look-at triple.
    pub fn follow_look_at(&mut self, eye: Vec3, target: Vec3, up: Vec3) {
        self.set_listener(AudioListener::from_look_at(eye, target, up));
    }

    /// Master gain applied to every bus (`0.0..=1.0+`).
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.max(0.0);
        self.reapply_all_volumes();
    }

    /// Current master volume.
    #[inline]
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Per-bus gain (multiplied by master).
    pub fn set_bus_volume(&mut self, bus: MixBus, volume: f32) {
        let v = volume.max(0.0);
        match bus {
            MixBus::Sfx => self.sfx_volume = v,
            MixBus::Music => self.music_volume = v,
        }
        self.reapply_all_volumes();
    }

    /// Current bus volume (before master).
    #[inline]
    pub fn bus_volume(&self, bus: MixBus) -> f32 {
        match bus {
            MixBus::Sfx => self.sfx_volume,
            MixBus::Music => self.music_volume,
        }
    }

    /// Play a sound file once on the sfx bus at full voice gain.
    pub fn play(&mut self, path: impl AsRef<Path>) -> Result<SoundId, AudioError> {
        self.play_with(path, 1.0, false)
    }

    /// Play a sound file with per-voice volume and optional loop (sfx bus, non-spatial).
    ///
    /// Short SFX are fully buffered so loops use rodio `repeat_infinite`.
    pub fn play_with(
        &mut self,
        path: impl AsRef<Path>,
        volume: f32,
        loop_: bool,
    ) -> Result<SoundId, AudioError> {
        self.play_on_bus(path, MixBus::Sfx, volume, loop_, None, false)
    }

    /// Play a one-shot at `position` with stereo distance attenuation (sfx bus).
    pub fn play_at(
        &mut self,
        path: impl AsRef<Path>,
        position: Vec3,
    ) -> Result<SoundId, AudioError> {
        self.play_at_with(path, position, 1.0, false)
    }

    /// Spatial play with per-voice volume and optional loop (sfx bus).
    pub fn play_at_with(
        &mut self,
        path: impl AsRef<Path>,
        position: Vec3,
        volume: f32,
        loop_: bool,
    ) -> Result<SoundId, AudioError> {
        self.play_on_bus(path, MixBus::Sfx, volume, loop_, Some(position), false)
    }

    /// Stream a music file once on the music bus (WAV; not fully buffered).
    pub fn play_music(&mut self, path: impl AsRef<Path>) -> Result<SoundId, AudioError> {
        self.play_music_with(path, 1.0, false)
    }

    /// Stream music with volume and optional loop (re-opens the file each cycle).
    pub fn play_music_with(
        &mut self,
        path: impl AsRef<Path>,
        volume: f32,
        loop_: bool,
    ) -> Result<SoundId, AudioError> {
        self.play_on_bus(path, MixBus::Music, volume, loop_, None, true)
    }

    /// Move a spatial emitter; no-op for non-spatial / unknown ids.
    pub fn set_emitter_position(&mut self, id: SoundId, position: Vec3) {
        let (left, right) = self.listener.ear_positions();
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            if let (Some(_), VoiceOut::Spatial(sink)) = (&voice.emitter, &voice.out) {
                voice.emitter = Some(position);
                sink.set_emitter_position(position.to_array());
                sink.set_left_ear_position(left);
                sink.set_right_ear_position(right);
            }
        }
    }

    /// Set volume on an active voice (multiplied by bus × master).
    pub fn set_volume(&mut self, id: SoundId, volume: f32) {
        let base = volume.max(0.0);
        let gain = if let Some(voice) = self.voices.iter().find(|v| v.id == id) {
            self.effective_gain(voice.bus, base)
        } else {
            return;
        };
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.base_volume = base;
            voice.out.set_volume(gain);
        }
    }

    /// Stop and drop a voice.
    pub fn stop(&mut self, id: SoundId) {
        if let Some(i) = self.voices.iter().position(|v| v.id == id) {
            let voice = self.voices.swap_remove(i);
            voice.out.stop();
        }
    }

    /// Stop all voices on a bus.
    pub fn stop_bus(&mut self, bus: MixBus) {
        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].bus == bus {
                let voice = self.voices.swap_remove(i);
                voice.out.stop();
            } else {
                i += 1;
            }
        }
    }

    /// Stop all voices.
    pub fn stop_all(&mut self) {
        for voice in self.voices.drain(..) {
            voice.out.stop();
        }
    }

    /// GC finished one-shots and restart streamed looping music.
    ///
    /// Called automatically from the Kerabit frame loop; safe to call manually.
    pub fn maintain(&mut self) {
        self.restart_stream_loops();
        self.gc_finished();
    }

    fn play_on_bus(
        &mut self,
        path: impl AsRef<Path>,
        bus: MixBus,
        volume: f32,
        loop_: bool,
        emitter: Option<Vec3>,
        stream: bool,
    ) -> Result<SoundId, AudioError> {
        self.maintain();

        let base = volume.max(0.0);
        let id = SoundId(self.next_id);
        self.next_id += 1;

        let Some(handle) = self.handle.as_ref() else {
            return Ok(id);
        };

        let path = path.as_ref();
        let gain = self.effective_gain(bus, base);
        let stream_loop = if stream && loop_ {
            Some(path.to_path_buf())
        } else {
            None
        };

        let out = if let Some(pos) = emitter {
            let (left, right) = self.listener.ear_positions();
            let sink = SpatialSink::try_new(handle, pos.to_array(), left, right)
                .map_err(|e| AudioError::Device(e.to_string()))?;
            sink.set_volume(gain);
            if stream {
                append_stream(&sink, path, loop_ && stream_loop.is_none())?;
            } else {
                append_buffered_spatial(&sink, path, loop_)?;
            }
            VoiceOut::Spatial(sink)
        } else {
            let sink = Sink::try_new(handle).map_err(|e| AudioError::Device(e.to_string()))?;
            sink.set_volume(gain);
            if stream {
                append_stream_flat(&sink, path, loop_ && stream_loop.is_none())?;
            } else {
                append_buffered_flat(&sink, path, loop_)?;
            }
            VoiceOut::Flat(sink)
        };

        self.voices.push(Voice {
            id,
            bus,
            base_volume: base,
            out,
            emitter,
            stream_loop,
        });
        Ok(id)
    }

    fn effective_gain(&self, bus: MixBus, base: f32) -> f32 {
        (base * self.bus_volume(bus) * self.master_volume).max(0.0)
    }

    fn reapply_all_volumes(&mut self) {
        let master = self.master_volume;
        let sfx = self.sfx_volume;
        let music = self.music_volume;
        for voice in &self.voices {
            let bus_v = match voice.bus {
                MixBus::Sfx => sfx,
                MixBus::Music => music,
            };
            voice
                .out
                .set_volume((voice.base_volume * bus_v * master).max(0.0));
        }
    }

    fn apply_listener_to_spatial(&mut self) {
        let (left, right) = self.listener.ear_positions();
        for voice in &self.voices {
            if let (Some(pos), VoiceOut::Spatial(sink)) = (voice.emitter, &voice.out) {
                sink.set_emitter_position(pos.to_array());
                sink.set_left_ear_position(left);
                sink.set_right_ear_position(right);
            }
        }
    }

    fn restart_stream_loops(&mut self) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let master = self.master_volume;
        let sfx = self.sfx_volume;
        let music = self.music_volume;
        let (left, right) = self.listener.ear_positions();

        // Collect restart work first so we can recreate sinks without fighting borrows.
        let mut restarts: Vec<(usize, PathBuf, MixBus, f32, Option<Vec3>)> = Vec::new();
        for (i, voice) in self.voices.iter().enumerate() {
            let Some(path) = voice.stream_loop.clone() else {
                continue;
            };
            if !voice.out.empty() {
                continue;
            }
            restarts.push((i, path, voice.bus, voice.base_volume, voice.emitter));
        }

        for (i, path, bus, base, emitter) in restarts {
            let bus_v = match bus {
                MixBus::Sfx => sfx,
                MixBus::Music => music,
            };
            let gain = (base * bus_v * master).max(0.0);
            let voice = &mut self.voices[i];
            match (emitter, &mut voice.out) {
                (Some(pos), VoiceOut::Spatial(sink)) => {
                    if append_stream(sink, &path, false).is_ok() {
                        sink.set_volume(gain);
                        sink.set_emitter_position(pos.to_array());
                        sink.set_left_ear_position(left);
                        sink.set_right_ear_position(right);
                    }
                }
                (None, VoiceOut::Flat(sink)) => {
                    if append_stream_flat(sink, &path, false).is_ok() {
                        sink.set_volume(gain);
                    }
                }
                (Some(pos), VoiceOut::Flat(_)) => {
                    if let Ok(sink) = SpatialSink::try_new(&handle, pos.to_array(), left, right) {
                        let _ = append_stream(&sink, &path, false);
                        sink.set_volume(gain);
                        voice.out = VoiceOut::Spatial(sink);
                    }
                }
                (None, VoiceOut::Spatial(_)) => {
                    if let Ok(sink) = Sink::try_new(&handle) {
                        let _ = append_stream_flat(&sink, &path, false);
                        sink.set_volume(gain);
                        voice.out = VoiceOut::Flat(sink);
                    }
                }
            }
        }
    }

    fn gc_finished(&mut self) {
        self.voices
            .retain(|v| v.stream_loop.is_some() || !v.out.empty());
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn load_error(path: &Path, err: impl std::error::Error + Send + Sync + 'static) -> AudioError {
    AudioError::Load {
        path: path.to_path_buf(),
        source: Box::new(err),
    }
}

fn open_decoder(path: &Path) -> Result<Decoder<BufReader<File>>, AudioError> {
    let file = File::open(path).map_err(|e| load_error(path, e))?;
    let reader = BufReader::new(file);
    Decoder::new(reader).map_err(|e| load_error(path, e))
}

fn load_buffered(path: &Path) -> Result<Samples, AudioError> {
    Ok(open_decoder(path)?.buffered())
}

fn append_buffered_flat(sink: &Sink, path: &Path, loop_: bool) -> Result<(), AudioError> {
    let source = load_buffered(path)?;
    if loop_ {
        sink.append(source.repeat_infinite());
    } else {
        sink.append(source);
    }
    Ok(())
}

fn append_buffered_spatial(
    sink: &SpatialSink,
    path: &Path,
    loop_: bool,
) -> Result<(), AudioError> {
    let source = load_buffered(path)?;
    if loop_ {
        sink.append(source.repeat_infinite());
    } else {
        sink.append(source);
    }
    Ok(())
}

fn append_stream_flat(sink: &Sink, path: &Path, loop_buffered: bool) -> Result<(), AudioError> {
    // Streaming path: decode from disk without `.buffered()`.
    // `loop_buffered` is only used when callers request in-memory infinite loop
    // (music uses stream_loop reopen instead).
    if loop_buffered {
        append_buffered_flat(sink, path, true)
    } else {
        let decoder = open_decoder(path)?;
        sink.append(decoder);
        Ok(())
    }
}

fn append_stream(sink: &SpatialSink, path: &Path, loop_buffered: bool) -> Result<(), AudioError> {
    if loop_buffered {
        append_buffered_spatial(sink, path, true)
    } else {
        let decoder = open_decoder(path)?;
        sink.append(decoder);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_engine_play_apis_are_safe() {
        let mut audio = AudioEngine::null();
        assert!(!audio.is_active());
        let a = audio.play("/no/such.wav").unwrap();
        let b = audio
            .play_at("/no/such.wav", Vec3::new(1.0, 0.0, 0.0))
            .unwrap();
        let c = audio.play_music("/no/such.wav").unwrap();
        assert_ne!(a, b);
        assert_ne!(b, c);
        audio.set_master_volume(0.5);
        audio.set_bus_volume(MixBus::Sfx, 0.25);
        audio.set_bus_volume(MixBus::Music, 0.8);
        assert_eq!(audio.bus_volume(MixBus::Sfx), 0.25);
        audio.follow_look_at(Vec3::new(0.0, 1.0, 5.0), Vec3::ZERO, Vec3::Y);
        audio.set_emitter_position(b, Vec3::ONE);
        audio.set_volume(a, 0.1);
        audio.stop(a);
        audio.stop_bus(MixBus::Music);
        audio.stop_all();
        audio.maintain();
    }

    #[test]
    fn listener_ears_are_separated() {
        let listener = AudioListener::from_look_at(
            Vec3::new(0.0, 1.0, 5.0),
            Vec3::ZERO,
            Vec3::Y,
        );
        let (left, right) = listener.ear_positions();
        let dx = left[0] - right[0];
        let dy = left[1] - right[1];
        let dz = left[2] - right[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        assert!(dist > 0.1, "ears too close: {dist}");
    }

    #[test]
    fn play_existing_wav_on_null_still_ok() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/beep.wav");
        assert!(path.is_file(), "fixture missing: {}", path.display());
        let mut audio = AudioEngine::null();
        assert!(audio.play(&path).is_ok());
        assert!(audio.play_music_with(&path, 0.5, true).is_ok());
    }
}
