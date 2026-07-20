//! Offline real-time-headroom check: renders audio through the synth as
//! fast as possible (no actual audio device involved) and reports how
//! much faster than real time it ran, across a few block sizes and load
//! levels. A real-time audio callback must finish a block well inside
//! `block_frames / sample_rate` seconds *every single time* — average
//! speedup alone can hide an occasional slow block, so this also tracks
//! the worst block observed, which is the number that actually matters
//! for whether the callback deadline gets missed.
//!
//! Run in release mode for a meaningful number:
//!   cargo run --release --example bench_throughput

use std::time::Instant;
use dsp::Synth;
use wmidi::{Note, Velocity};

#[derive(Clone, Copy)]
enum Load {
    /// No notes held: every voice's envelope is idle.
    Idle,
    /// One voice active, three idle.
    OneNote,
    /// All four voices active (worst case for this fixed-size voice pool).
    FullChord,
}

impl Load {
    fn label(self) -> &'static str {
        match self {
            Load::Idle => "idle (no notes)",
            Load::OneNote => "1 note held",
            Load::FullChord => "4 notes held (full chord)",
        }
    }

    fn notes(self) -> &'static [u8] {
        match self {
            Load::Idle => &[],
            Load::OneNote => &[69],
            Load::FullChord => &[60, 64, 67, 71],
        }
    }
}

fn bench_one(sample_rate: f32, block_frames: usize, load: Load, seconds_to_render: f32) {
    let blocks = (seconds_to_render * sample_rate / block_frames as f32) as usize;

    let mut synth = Synth::new(sample_rate);
    for &n in load.notes() {
        synth.note_on(Note::try_from(n).unwrap(), Velocity::try_from(100).unwrap());
    }

    // Warm up (first-touch page faults, branch predictors, etc.) before
    // timing so the measurement reflects steady-state performance.
    for _ in 0..64 {
        synth.process_block(block_frames);
    }

    let deadline_us = block_frames as f64 / f64::from(sample_rate) * 1e6;
    let mut total = std::time::Duration::ZERO;
    let mut worst = std::time::Duration::ZERO;

    for _ in 0..blocks {
        let start = Instant::now();
        synth.process_block(block_frames);
        std::hint::black_box(synth.output_channel(0)); // keep the optimizer from eliding the work
        let elapsed = start.elapsed();

        total += elapsed;
        worst = worst.max(elapsed);
    }

    let avg_us = total.as_secs_f64() * 1e6 / blocks as f64;
    let worst_us = worst.as_secs_f64() * 1e6;

    println!(
        "  {:>10} frames | {:<26} | avg {:>7.2}us ({:>6.1}x) | worst {:>7.2}us ({:>6.1}x) | deadline {:.1}us",
        block_frames,
        load.label(),
        avg_us,
        deadline_us / avg_us,
        worst_us,
        deadline_us / worst_us,
        deadline_us,
    );
}

fn main() {
    let sample_rate = 48_000.0f32;
    let seconds_to_render = 3.0;

    println!("Rendering {seconds_to_render:.1}s per scenario at {sample_rate:.0}Hz.\n");

    for &block_frames in &[32, 64, 128, 256, 512, 1024] {
        for &load in &[Load::Idle, Load::OneNote, Load::FullChord] {
            bench_one(sample_rate, block_frames, load, seconds_to_render);
        }
        println!();
    }
}
