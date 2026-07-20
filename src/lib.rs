pub mod graph;
pub mod midi;
pub mod module;
pub mod modules;
pub mod patch;
pub mod rt;
pub mod synth;

pub use synth::Synth;

/// Every voice graph is built to handle blocks up to this many frames in
/// one call. Chosen generously relative to typical audio-callback buffer
/// sizes (usually tens to a few hundred frames); a callback buffer larger
/// than this is simply split into sub-blocks by the caller (see
/// `main.rs`), so this bound only affects how much fixed scratch memory
/// is preallocated up front — never a runtime allocation or a resize.
pub const MAX_BLOCK_FRAMES: usize = 1024;

pub enum AppMessage {
    MidiMessage(midi::MidiMessage),
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
