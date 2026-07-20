pub mod graph;
pub mod midi;
pub mod module;
pub mod modules;
pub mod patch;
pub mod rt;
pub mod synth;

pub use patch::{
    IoDirection, IoEndpoint, ModuleInfo, ParamInfo, Patch, IO_DEVICE_DEFAULT, IO_ENDPOINT_AUDIO_IN,
    IO_ENDPOINT_AUDIO_OUT,
};
pub use synth::{Synth, MASTER_IO_MODULE};

/// Every voice graph is built to handle blocks up to this many frames in
/// one call. Chosen generously relative to typical audio-callback buffer
/// sizes (usually tens to a few hundred frames); a callback buffer larger
/// than this is simply split into sub-blocks by the caller (see
/// `main.rs`), so this bound only affects how much fixed scratch memory
/// is preallocated up front — never a runtime allocation or a resize.
pub const MAX_BLOCK_FRAMES: usize = 1024;

/// Max IO endpoints whose peaks `EngineMeters` tracks (audio_out + audio_in).
pub const MAX_IO_METER_ENDPOINTS: usize = 2;
/// Max channels per IO endpoint for peak meters.
pub const MAX_IO_METER_CHANNELS: usize = 8;

/// Everything that can reach the audio thread from the outside: hardware
/// MIDI, and now also a gRPC client (see the `engine` host). Sent over an
/// `mpsc::Sender<Command>` and drained with `try_recv` at the top of each
/// audio callback — never blocking, so any producer (the MIDI thread, a
/// tonic request handler) is safe to call from off the audio thread.
pub enum Command {
    Midi(midi::MidiMessage),
    SetParam { module: usize, param: usize, value: f32 },
    SetOutputCalibration { channel: usize, scale: f32, offset: f32 },
    /// Update patch-owned device binding for an IO endpoint. The engine
    /// realizes the new device off the audio thread after this (or in
    /// parallel); the audio path only records the name on `Synth`.
    SetIoDevice {
        endpoint_id: String,
        device_name: String,
    },
    NoteOn { note: wmidi::Note, velocity: wmidi::Velocity },
    NoteOff { note: wmidi::Note },
}

/// Live status of one IO endpoint for control surfaces (device name is
/// not written on the audio thread).
#[derive(Clone, Debug, Default)]
pub struct IoEndpointStatus {
    pub endpoint_id: String,
    pub device_name: String,
    pub connected: bool,
    pub channels: usize,
}

/// Engine state published by the audio thread for anything that wants to
/// observe it without touching the audio path itself (e.g. a gRPC state
/// stream). Wait-free: the audio callback only ever does atomic stores here
/// after `process_block`, and readers only ever do atomic loads — no locks,
/// no allocation, so this can't jeopardize the real-time deadline.
pub struct EngineMeters {
    /// Peak absolute sample value of the most recent block's mixed audio
    /// output (channel 0), stored as `f32::to_bits`.
    output_peak_bits: std::sync::atomic::AtomicU32,
    active_voices: std::sync::atomic::AtomicU32,
    /// Most recent block of the mixed audio output, for a live oscilloscope
    /// view (see the `rack` TUI's scope panel). A `Mutex` (not an atomic)
    /// because it's a whole buffer, not a scalar; the audio-thread writer
    /// uses `try_lock` so it never blocks — under contention it simply
    /// drops a frame, which is harmless for a 30 Hz visual — and the
    /// reader (the gRPC stream task) is off the audio thread, where the
    /// occasional `clone` allocation is fine.
    waveform: std::sync::Mutex<Vec<f32>>,
    /// Per-endpoint, per-channel abs peak (bits). Endpoint order is fixed:
    /// 0 = audio_out, 1 = audio_in.
    io_peak_bits: [[std::sync::atomic::AtomicU32; MAX_IO_METER_CHANNELS]; MAX_IO_METER_ENDPOINTS],
    io_channel_counts: [std::sync::atomic::AtomicU32; MAX_IO_METER_ENDPOINTS],
    /// Resolved device names + connected flags (control thread writes;
    /// gRPC stream reads). Not touched on the audio callback.
    io_status: std::sync::Mutex<Vec<IoEndpointStatus>>,
}

impl Default for EngineMeters {
    fn default() -> Self {
        Self {
            output_peak_bits: std::sync::atomic::AtomicU32::new(0.0f32.to_bits()),
            active_voices: std::sync::atomic::AtomicU32::new(0),
            waveform: std::sync::Mutex::new(Vec::with_capacity(MAX_BLOCK_FRAMES)),
            io_peak_bits: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0.0f32.to_bits()))
            }),
            io_channel_counts: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
            io_status: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl EngineMeters {
    /// Endpoint slot for master audio out meters.
    pub const SLOT_AUDIO_OUT: usize = 0;
    /// Endpoint slot for external audio in meters.
    pub const SLOT_AUDIO_IN: usize = 1;

    /// Called from the audio callback after each `process_block`.
    pub fn update(&self, output_peak: f32, active_voices: u32) {
        self.output_peak_bits
            .store(output_peak.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.active_voices
            .store(active_voices, std::sync::atomic::Ordering::Relaxed);
    }

    /// Publishes per-channel peaks for one IO endpoint slot (audio thread).
    pub fn update_io_peaks(&self, slot: usize, peaks: &[f32]) {
        if slot >= MAX_IO_METER_ENDPOINTS {
            return;
        }
        let n = peaks.len().min(MAX_IO_METER_CHANNELS);
        self.io_channel_counts[slot].store(n as u32, std::sync::atomic::Ordering::Relaxed);
        for (i, &peak) in peaks.iter().take(n).enumerate() {
            self.io_peak_bits[slot][i]
                .store(peak.to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Reads peaks for one IO endpoint slot.
    pub fn io_peaks(&self, slot: usize) -> Vec<f32> {
        if slot >= MAX_IO_METER_ENDPOINTS {
            return Vec::new();
        }
        let n = self.io_channel_counts[slot].load(std::sync::atomic::Ordering::Relaxed) as usize;
        (0..n.min(MAX_IO_METER_CHANNELS))
            .map(|i| {
                f32::from_bits(
                    self.io_peak_bits[slot][i].load(std::sync::atomic::Ordering::Relaxed),
                )
            })
            .collect()
    }

    /// Control-thread update of resolved device status for StreamState.
    pub fn set_io_status(&self, status: Vec<IoEndpointStatus>) {
        if let Ok(mut guard) = self.io_status.lock() {
            *guard = status;
        }
    }

    pub fn io_status(&self) -> Vec<IoEndpointStatus> {
        self.io_status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Publishes the most recent block's mixed-output samples for the scope
    /// view. Wait-free on the audio thread: `try_lock` never blocks, and
    /// `copy_from_slice` writes into a preallocated buffer (no growth, no
    /// allocation). Returns `false` if the lock was contended (the
    /// outstanding frame is simply skipped).
    pub fn publish_waveform(&self, samples: &[f32]) -> bool {
        match self.waveform.try_lock() {
            Ok(mut buf) => {
                buf.clear();
                // Capacity is reserved at construction (MAX_BLOCK_FRAMES);
                // extend_from_slice must not reallocate for normal blocks.
                buf.extend_from_slice(samples);
                true
            }
            Err(_) => false,
        }
    }

    /// Reads back the most recently published waveform (a copy — the caller
    /// is off the audio thread, so the allocation here is fine).
    pub fn waveform(&self) -> Vec<f32> {
        self.waveform
            .lock()
            .map(|buf| buf.clone())
            .unwrap_or_default()
    }

    pub fn output_peak(&self) -> f32 {
        f32::from_bits(self.output_peak_bits.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn active_voices(&self) -> u32 {
        self.active_voices.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Counting allocator wrapping the system allocator, active only in test
/// builds. Lets tests assert that a real-time-critical path (the audio
/// callback's `process_block`) makes zero heap allocations after setup —
/// the actual real-time-safety property that matters, not just "no
/// allocation visible in the source."
///
/// The count is kept **per-thread** (not a single process-wide counter):
/// `cargo test` runs tests concurrently on a pool of threads, and a
/// global counter would pick up unrelated allocations from whichever
/// other tests happen to run at the same time. Scoping to the calling
/// thread makes the measurement depend only on what this test itself did.
#[cfg(test)]
mod alloc_guard {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATION_COUNT.try_with(|count| count.set(count.get() + 1));
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
        }
    }

    /// Runs `f` on the current thread, returning how many heap
    /// allocations it made.
    pub fn count_allocations(f: impl FnOnce()) -> usize {
        let before = ALLOCATION_COUNT.with(Cell::get);
        f();
        ALLOCATION_COUNT.with(Cell::get) - before
    }
}

#[cfg(test)]
#[global_allocator]
static GLOBAL: alloc_guard::CountingAllocator = alloc_guard::CountingAllocator;

#[cfg(test)]
mod rt_safety_tests {
    use super::alloc_guard::count_allocations;
    use crate::Synth;
    use wmidi::{Note, Velocity};

    #[test]
    fn process_block_makes_no_heap_allocations() {
        let mut synth = Synth::new(48000.0);
        // Exercise both a held note and the allocator/voice-stealing path
        // before measuring, so the block under test reflects steady-state
        // playing, not first-use setup.
        synth.note_on(Note::try_from(60).unwrap(), Velocity::try_from(100).unwrap());
        synth.process_block(256);

        let allocations = count_allocations(|| {
            for _ in 0..50 {
                // Also exercises external-input staging and multi-channel
                // output reads, not just the voice-render path.
                synth.write_external_input(0, &[0.1; 256]);
                synth.write_external_input(1, &[0.1; 256]);
                synth.process_block(256);
                for channel in 0..Synth::NUM_OUTPUT_CHANNELS {
                    std::hint::black_box(synth.output_channel(channel));
                }
            }
        });

        assert_eq!(
            allocations, 0,
            "process_block allocated on the heap — this would risk missing the audio callback deadline"
        );
    }
}
