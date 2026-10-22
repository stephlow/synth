//! Real-time-thread setup helpers.

/// Flushes denormal (subnormal) floating-point results to zero instead of
/// computing them at full (much slower) precision. Call once at the start
/// of the audio callback/thread.
///
/// Denormals show up constantly in audio DSP — a decaying envelope tail,
/// a filter or feedback loop settling toward silence — and on x86
/// hardware without FTZ/DAZ set, arithmetic on them can be 10-100x slower
/// than normal floats. That's exactly the kind of surprise latency spike
/// that blows a real-time audio callback's deadline, so every real-time
/// audio engine sets this. Idempotent and cheap (a couple of register
/// writes), so it's safe to call on every callback invocation rather than
/// tracking whether it's already been set on the current thread.
#[cfg(target_arch = "x86_64")]
pub fn flush_denormals_to_zero() {
    const FLUSH_TO_ZERO: u32 = 1 << 15; // FTZ: flush denormal *results* to zero
    const DENORMALS_ARE_ZERO: u32 = 1 << 6; // DAZ: treat denormal *inputs* as zero
    unsafe {
        let mut mxcsr: u32 = 0;
        std::arch::asm!("stmxcsr [{0}]", in(reg) &mut mxcsr);
        mxcsr |= FLUSH_TO_ZERO | DENORMALS_ARE_ZERO;
        std::arch::asm!("ldmxcsr [{0}]", in(reg) &mxcsr);
    }
}

/// No-op on architectures without an equivalent MXCSR-style control
/// (e.g. aarch64 defaults to flush-to-zero-friendly behavior already for
/// the common case, or would need a different register).
#[cfg(not(target_arch = "x86_64"))]
pub fn flush_denormals_to_zero() {}
