//! Ableton Link session clock — tempo / phase / start-stop synced peers.
//!
//! One process-wide [`AblLink`] client (Ableton expects a single Link instance
//! per app). Each module instance captures the audio session state on the
//! real-time path and emits modular CV: clock triggers, quantum phase, and
//! a play gate. Networking runs on Link-managed threads; `process` only uses
//! RT-safe capture/commit APIs.
//!
//! Host time is sample-anchored to Link's `clock_micros()` at first process
//! (and soft-resynced). That stays correct both in real-time audio and in
//! offline/unit tests that render faster than wall-clock — unlike
//! `HostTimeFilter`, which assumes sample time tracks wall time 1:1.

use std::sync::OnceLock;

use rusty_link::{AblLink, SessionState};

use crate::module::{BlockContext, Module};

/// Shared Link client for the whole process.
static SHARED_LINK: OnceLock<AblLink> = OnceLock::new();

/// `capture_audio_session_state` is not thread-safe; production uses a single
/// audio thread. Unit tests may run in parallel, so they share this lock for
/// the whole test body (see tests module).
#[cfg(test)]
static TEST_LINK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn shared_link(initial_bpm: f64) -> &'static AblLink {
    SHARED_LINK.get_or_init(|| {
        let link = AblLink::new(initial_bpm);
        // enable* / app commit are not RT-safe — only here (first module create).
        link.enable(true);
        link.enable_start_stop_sync(true);
        // Start transport so a freshly added Link module clocks immediately
        // without requiring a Play toggle (still overridable via PARAM_PLAY).
        let mut state = SessionState::new();
        link.capture_app_session_state(&mut state);
        let t = link.clock_micros();
        state.set_tempo(initial_bpm, t);
        state.set_is_playing(true, t);
        state.request_beat_at_start_playing_time(0.0, 4.0);
        link.commit_app_session_state(&state);
        link
    })
}

/// Ableton Link panel: session-synced clock, phase, and play gate.
pub struct Link {
    sample_rate: f32,
    /// Monotonic sample counter since module creation.
    sample_time: u64,
    /// Host micros corresponding to `origin_sample`.
    host_origin: i64,
    origin_sample: u64,
    anchored: bool,
    /// Preallocated; never created on the audio path.
    session: SessionState,
    bpm: f64,
    quantum: f64,
    /// Clocks per beat: 1, 2, 4, or 8.
    div: u32,
    play_req: bool,
    dirty_tempo: bool,
    dirty_play: bool,
    last_tick: i64,
    tick_init: bool,
    pulse_left: u32,
}

impl Link {
    pub const OUT_CLOCK: usize = 0;
    pub const OUT_PHASE: usize = 1;
    pub const OUT_PLAY: usize = 2;

    pub const PARAM_BPM: usize = 0;
    pub const PARAM_QUANTUM: usize = 1;
    /// Discrete index 0..=3 → div 1/2/4/8 (see registry choices).
    pub const PARAM_DIV: usize = 2;
    pub const PARAM_PLAY: usize = 3;

    const PULSE_MS: f32 = 5.0;
    const DIV_TABLE: [u32; 4] = [1, 2, 4, 8];

    pub fn new(sample_rate: f32) -> Self {
        let bpm = 120.0;
        // Ensure the process-wide client exists before the audio thread runs.
        let _ = shared_link(bpm);
        Self {
            sample_rate: sample_rate.max(1.0),
            sample_time: 0,
            host_origin: 0,
            origin_sample: 0,
            anchored: false,
            session: SessionState::new(),
            bpm,
            quantum: 4.0,
            div: 4,
            play_req: true,
            // Don't auto-commit on first process — avoids fighting peers /
            // other Link instances. Factory `set_param` marks dirty when the
            // patch actually wants a different BPM/Play.
            dirty_tempo: false,
            dirty_play: false,
            last_tick: 0,
            tick_init: false,
            pulse_left: 0,
        }
    }

    fn pulse_width(&self) -> u32 {
        ((self.sample_rate * Self::PULSE_MS / 1000.0).round() as u32).max(1)
    }

    fn micros_per_sample(&self) -> f64 {
        1_000_000.0 / self.sample_rate as f64
    }

    fn host_at(&self, sample: u64) -> i64 {
        let delta = sample.saturating_sub(self.origin_sample) as f64 * self.micros_per_sample();
        self.host_origin + delta.round() as i64
    }

    fn ensure_anchor(&mut self, link: &AblLink) {
        if !self.anchored {
            self.host_origin = link.clock_micros();
            self.origin_sample = self.sample_time;
            self.anchored = true;
        }
    }

    fn div_from_param(value: f32) -> u32 {
        let idx = value.round().clamp(0.0, 3.0) as usize;
        Self::DIV_TABLE[idx]
    }

    #[cfg(test)]
    fn div_to_param(div: u32) -> f32 {
        match div {
            1 => 0.0,
            2 => 1.0,
            4 => 2.0,
            _ => 3.0,
        }
    }
}

impl Module for Link {
    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let link = shared_link(self.bpm);
        self.ensure_anchor(link);

        let width = self.pulse_width();
        let quantum = self.quantum.max(1.0);
        let div = self.div.max(1) as f64;
        let mps = self.micros_per_sample();

        // Apply local tempo / play requests (RT-safe commit path).
        // Use wall-clock micros for start/stop so the change takes effect
        // immediately even when audio is rendered faster than real time.
        if self.dirty_tempo || self.dirty_play {
            link.capture_audio_session_state(&mut self.session);
            let wall = link.clock_micros();
            if self.dirty_tempo {
                self.session.set_tempo(self.bpm, wall);
            }
            if self.dirty_play {
                self.session.set_is_playing(self.play_req, wall);
                if self.play_req {
                    self.session
                        .request_beat_at_start_playing_time(0.0, quantum);
                }
            }
            link.commit_audio_session_state(&self.session);
            self.dirty_tempo = false;
            self.dirty_play = false;
        }

        link.capture_audio_session_state(&mut self.session);

        let host_at_block = self.host_at(self.sample_time);
        let (clock_out, rest) = outputs.split_at_mut(ctx.frames);
        let (phase_out, play_out) = rest.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            let host_t = host_at_block + (i as f64 * mps).round() as i64;
            let beat = self.session.beat_at_time(host_t, quantum);
            let phase_q = self.session.phase_at_time(host_t, quantum);
            let phase01 = (phase_q / quantum).clamp(0.0, 1.0) as f32;
            let playing = self.session.is_playing();

            let tick = (beat * div).floor() as i64;
            if !self.tick_init {
                self.last_tick = tick;
                self.tick_init = true;
            } else if tick != self.last_tick {
                if playing {
                    self.pulse_left = width;
                }
                self.last_tick = tick;
            }

            clock_out[i] = if self.pulse_left > 0 && playing {
                self.pulse_left = self.pulse_left.saturating_sub(1);
                1.0
            } else {
                if self.pulse_left > 0 {
                    self.pulse_left = self.pulse_left.saturating_sub(1);
                }
                0.0
            };
            phase_out[i] = phase01;
            play_out[i] = if playing { 1.0 } else { 0.0 };

            self.sample_time = self.sample_time.saturating_add(1);
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_BPM => {
                let bpm = value.clamp(20.0, 300.0) as f64;
                if (bpm - self.bpm).abs() > 1e-6 {
                    self.bpm = bpm;
                    self.dirty_tempo = true;
                }
            }
            Self::PARAM_QUANTUM => {
                self.quantum = value.round().clamp(1.0, 16.0) as f64;
            }
            Self::PARAM_DIV => {
                self.div = Self::div_from_param(value);
                // Force re-arm tick tracking after division change.
                self.tick_init = false;
            }
            Self::PARAM_PLAY => {
                // Always re-commit: another instance (or peer) may have changed
                // the shared session's transport since our last process.
                self.play_req = value > 0.5;
                self.dirty_play = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(sample_rate: f32, frames: usize) -> BlockContext {
        BlockContext {
            sample_rate,
            frames,
        }
    }

    fn lock_link() -> std::sync::MutexGuard<'static, ()> {
        TEST_LINK_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn phase_in_unit_interval() {
        let _g = lock_link();
        let sr = 48_000.0;
        let frames = 256;
        let mut link = Link::new(sr);
        link.set_param(Link::PARAM_BPM, 120.0);
        link.set_param(Link::PARAM_PLAY, 1.0);
        let mut outputs = vec![0.0; 3 * frames];
        for _ in 0..20 {
            link.process(&[], &mut outputs, &ctx(sr, frames));
        }
        let phase = &outputs[frames..2 * frames];
        for &p in phase {
            assert!(p.is_finite(), "phase non-finite");
            assert!((0.0..=1.0).contains(&p), "phase out of range: {p}");
        }
    }

    #[test]
    fn clock_fires_near_bpm_at_div1() {
        let _g = lock_link();
        let sr = 48_000.0;
        let frames = 256;
        let mut link = Link::new(sr);
        link.set_param(Link::PARAM_BPM, 120.0);
        link.set_param(Link::PARAM_DIV, Link::div_to_param(1));
        link.set_param(Link::PARAM_QUANTUM, 4.0);
        link.set_param(Link::PARAM_PLAY, 1.0);

        let mut outputs = vec![0.0; 3 * frames];
        let mut rising = 0u32;
        let mut prev = 0.0f32;
        // ~2 seconds of audio.
        let blocks = ((sr as usize) * 2) / frames;
        for _ in 0..blocks {
            link.process(&[], &mut outputs, &ctx(sr, frames));
            let clock = &outputs[..frames];
            for &s in clock {
                if s > 0.5 && prev <= 0.5 {
                    rising += 1;
                }
                prev = s;
            }
        }
        // 120 BPM = 2 beats/s → ~4 clocks in 2 s at div=1 (skip first tick).
        assert!(
            (2..=6).contains(&rising),
            "expected ~4 clocks in 2s @ 120 BPM div1, got {rising}"
        );
    }

    #[test]
    fn play_off_suppresses_clock() {
        let _g = lock_link();
        let sr = 48_000.0;
        let frames = 256;
        let mut link = Link::new(sr);
        link.set_param(Link::PARAM_BPM, 120.0);
        link.set_param(Link::PARAM_DIV, Link::div_to_param(1));
        link.set_param(Link::PARAM_PLAY, 1.0);

        let mut outputs = vec![0.0; 3 * frames];
        // Warm up and establish play.
        for _ in 0..10 {
            link.process(&[], &mut outputs, &ctx(sr, frames));
        }
        link.set_param(Link::PARAM_PLAY, 0.0);
        let mut high = 0u32;
        for _ in 0..100 {
            link.process(&[], &mut outputs, &ctx(sr, frames));
            high += outputs[..frames].iter().filter(|&&s| s > 0.5).count() as u32;
        }
        let play_high = outputs[2 * frames..3 * frames]
            .iter()
            .filter(|&&s| s > 0.5)
            .count();
        assert_eq!(play_high, 0, "play gate should be low when stopped");
        assert_eq!(high, 0, "clock should be silent when stopped, high_samples={high}");
    }
}
