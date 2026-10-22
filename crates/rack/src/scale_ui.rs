//! Pitch-class masks for quantizer tile glyphs.
//!
//! Order matches `dsp::modules::scale::SCALES` / engine `SCALE_NAMES` (index =
//! Scale param). Masks are duplicated here so the TUI need not depend on dsp.

const fn mask_from(degrees: &[u8]) -> u16 {
    let mut m: u16 = 0;
    let mut i = 0;
    while i < degrees.len() {
        let d = degrees[i] as u16 % 12;
        m |= 1 << d;
        i += 1;
    }
    m
}

/// 12-bit pitch-class masks, index-aligned with the engine quantizer Scale param.
static MASKS: &[u16] = &[
    0x0FFF, // Chromatic
    mask_from(&[0, 2, 4, 5, 7, 9, 11]), // Major
    mask_from(&[0, 2, 3, 5, 7, 8, 10]), // Natural Minor
    mask_from(&[0, 2, 3, 5, 7, 8, 11]), // Harmonic Minor
    mask_from(&[0, 2, 3, 5, 7, 9, 11]), // Melodic Minor
    mask_from(&[0, 2, 3, 5, 7, 9, 10]), // Dorian
    mask_from(&[0, 1, 3, 5, 7, 8, 10]), // Phrygian
    mask_from(&[0, 2, 4, 6, 7, 9, 11]), // Lydian
    mask_from(&[0, 2, 4, 5, 7, 9, 10]), // Mixolydian
    mask_from(&[0, 1, 3, 5, 6, 8, 10]), // Locrian
    mask_from(&[0, 2, 4, 7, 9]),        // Major Pentatonic
    mask_from(&[0, 3, 5, 7, 10]),       // Minor Pentatonic
    mask_from(&[0, 3, 5, 6, 7, 10]),    // Blues
    mask_from(&[0, 2, 4, 6, 8, 10]),    // Whole Tone
    mask_from(&[0, 2, 3, 5, 6, 8, 9, 11]), // Whole-Half Dim
    mask_from(&[0, 1, 3, 4, 6, 7, 9, 10]), // Half-Whole Dim
    mask_from(&[0, 3, 4, 7, 8, 11]),    // Augmented
    mask_from(&[0, 2, 3, 6, 7, 8, 11]), // Hungarian Minor
    mask_from(&[0, 1, 4, 5, 7, 8, 11]), // Double Harmonic
    mask_from(&[0, 1, 4, 5, 7, 8, 10]), // Phrygian Dominant
    mask_from(&[0, 1, 4, 5, 6, 8, 11]), // Persian
    mask_from(&[0, 1, 4, 6, 8, 10, 11]), // Enigmatic
    mask_from(&[0, 2, 3, 7, 8]),        // Hirajoshi
    mask_from(&[0, 1, 5, 7, 10]),       // In Sen
    mask_from(&[0, 1, 5, 6, 10]),       // Iwato
    mask_from(&[0, 2, 5, 7, 9]),        // Yo
    mask_from(&[0, 1, 3, 7, 8]),        // Pelog (approx)
    mask_from(&[0, 2, 4, 6, 9, 10]),    // Prometheus
    mask_from(&[0, 1, 4, 6, 7, 10]),    // Tritone
    mask_from(&[0, 1, 3, 4, 6, 8, 10]), // Super Locrian
];

/// Pitch-class mask for Scale param index (defaults to Major).
pub fn mask_for_index(index: usize) -> u16 {
    MASKS.get(index).copied().unwrap_or(MASKS[1])
}
