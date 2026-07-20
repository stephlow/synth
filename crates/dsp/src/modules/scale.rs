//! 12-TET scale presets and pitch quantization helpers.
//!
//! Pitch uses this engine's 1V/oct convention (1.0 V = 1 octave). Semitones
//! are therefore `volts * 12.0`. Scales are pitch-class bitmasks: bit `n`
//! set means degree `n` semitones above the root is allowed.

/// One named scale preset.
#[derive(Clone, Copy, Debug)]
pub struct ScaleDef {
    pub name: &'static str,
    /// Bits 0..11: allowed pitch classes relative to root.
    pub mask: u16,
}

/// Build a 12-bit mask from a list of semitone degrees (0..11).
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

/// Stable ordered preset table (param index = position in this slice).
pub static SCALES: &[ScaleDef] = &[
    ScaleDef {
        name: "Chromatic",
        mask: 0x0FFF,
    },
    ScaleDef {
        name: "Major",
        mask: mask_from(&[0, 2, 4, 5, 7, 9, 11]),
    },
    ScaleDef {
        name: "Natural Minor",
        mask: mask_from(&[0, 2, 3, 5, 7, 8, 10]),
    },
    ScaleDef {
        name: "Harmonic Minor",
        mask: mask_from(&[0, 2, 3, 5, 7, 8, 11]),
    },
    ScaleDef {
        name: "Melodic Minor",
        mask: mask_from(&[0, 2, 3, 5, 7, 9, 11]),
    },
    ScaleDef {
        name: "Dorian",
        mask: mask_from(&[0, 2, 3, 5, 7, 9, 10]),
    },
    ScaleDef {
        name: "Phrygian",
        mask: mask_from(&[0, 1, 3, 5, 7, 8, 10]),
    },
    ScaleDef {
        name: "Lydian",
        mask: mask_from(&[0, 2, 4, 6, 7, 9, 11]),
    },
    ScaleDef {
        name: "Mixolydian",
        mask: mask_from(&[0, 2, 4, 5, 7, 9, 10]),
    },
    ScaleDef {
        name: "Locrian",
        mask: mask_from(&[0, 1, 3, 5, 6, 8, 10]),
    },
    ScaleDef {
        name: "Major Pentatonic",
        mask: mask_from(&[0, 2, 4, 7, 9]),
    },
    ScaleDef {
        name: "Minor Pentatonic",
        mask: mask_from(&[0, 3, 5, 7, 10]),
    },
    ScaleDef {
        name: "Blues",
        mask: mask_from(&[0, 3, 5, 6, 7, 10]),
    },
    ScaleDef {
        name: "Whole Tone",
        mask: mask_from(&[0, 2, 4, 6, 8, 10]),
    },
    ScaleDef {
        name: "Whole-Half Dim",
        mask: mask_from(&[0, 2, 3, 5, 6, 8, 9, 11]),
    },
    ScaleDef {
        name: "Half-Whole Dim",
        mask: mask_from(&[0, 1, 3, 4, 6, 7, 9, 10]),
    },
    ScaleDef {
        name: "Augmented",
        mask: mask_from(&[0, 3, 4, 7, 8, 11]),
    },
    ScaleDef {
        name: "Hungarian Minor",
        mask: mask_from(&[0, 2, 3, 6, 7, 8, 11]),
    },
    ScaleDef {
        name: "Double Harmonic",
        mask: mask_from(&[0, 1, 4, 5, 7, 8, 11]),
    },
    ScaleDef {
        name: "Phrygian Dominant",
        mask: mask_from(&[0, 1, 4, 5, 7, 8, 10]),
    },
    ScaleDef {
        name: "Persian",
        mask: mask_from(&[0, 1, 4, 5, 6, 8, 11]),
    },
    ScaleDef {
        name: "Enigmatic",
        mask: mask_from(&[0, 1, 4, 6, 8, 10, 11]),
    },
    ScaleDef {
        name: "Hirajoshi",
        mask: mask_from(&[0, 2, 3, 7, 8]),
    },
    ScaleDef {
        name: "In Sen",
        mask: mask_from(&[0, 1, 5, 7, 10]),
    },
    ScaleDef {
        name: "Iwato",
        mask: mask_from(&[0, 1, 5, 6, 10]),
    },
    ScaleDef {
        name: "Yo",
        mask: mask_from(&[0, 2, 5, 7, 9]),
    },
    ScaleDef {
        name: "Pelog (approx)",
        mask: mask_from(&[0, 1, 3, 7, 8]),
    },
    ScaleDef {
        name: "Prometheus",
        mask: mask_from(&[0, 2, 4, 6, 9, 10]),
    },
    ScaleDef {
        name: "Tritone",
        mask: mask_from(&[0, 1, 4, 6, 7, 10]),
    },
    ScaleDef {
        name: "Super Locrian",
        mask: mask_from(&[0, 1, 3, 4, 6, 8, 10]),
    },
];

/// Index of Major in [`SCALES`] (default for the quantizer).
pub const SCALE_MAJOR: usize = 1;

/// Last valid scale-index param value (`SCALES.len() - 1`). Keep in sync with the table.
pub const SCALE_INDEX_MAX: f32 = 29.0;

/// Scale display names for control surfaces (same order as [`SCALES`]).
pub static SCALE_NAMES: &[&str] = &[
    "Chromatic",
    "Major",
    "Natural Minor",
    "Harmonic Minor",
    "Melodic Minor",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Locrian",
    "Major Pentatonic",
    "Minor Pentatonic",
    "Blues",
    "Whole Tone",
    "Whole-Half Dim",
    "Half-Whole Dim",
    "Augmented",
    "Hungarian Minor",
    "Double Harmonic",
    "Phrygian Dominant",
    "Persian",
    "Enigmatic",
    "Hirajoshi",
    "In Sen",
    "Iwato",
    "Yo",
    "Pelog (approx)",
    "Prometheus",
    "Tritone",
    "Super Locrian",
];

/// Snap continuous 1V/oct pitch CV to the nearest allowed scale degree.
///
/// `root_semitones` shifts the scale root (0 = same as 0 V / A4 reference
/// frame; +1 = one semitone up). Empty masks are treated as chromatic.
pub fn quantize_to_scale(volts: f32, root_semitones: f32, mask: u16) -> f32 {
    let mask = if mask & 0x0FFF == 0 { 0x0FFF } else { mask & 0x0FFF };

    // Work in absolute semitones relative to 0 V.
    let semis = volts * 12.0;
    // Pitch class is measured from the root.
    let relative = semis - root_semitones;

    // Round to nearest candidate among allowed classes in this octave band.
    // Search all 12 classes; for each allowed class, pick the instance
    // (same or neighboring octave) closest to `relative`.
    let mut best_rel = relative;
    let mut best_dist = f32::INFINITY;

    for pc in 0..12u16 {
        if mask & (1 << pc) == 0 {
            continue;
        }
        // Candidate in the octave floor of `relative`, and ±1 octave.
        let base_oct = (relative / 12.0).floor();
        for oct_off in -1i32..=1 {
            let cand = (base_oct + oct_off as f32) * 12.0 + pc as f32;
            let dist = (cand - relative).abs();
            // Prefer smaller distance; on ties prefer lower pitch class / lower cand.
            if dist < best_dist - 1e-6 || ((dist - best_dist).abs() <= 1e-6 && cand < best_rel) {
                best_dist = dist;
                best_rel = cand;
            }
        }
    }

    (best_rel + root_semitones) / 12.0
}

/// Clamp a float scale-index param to a valid [`SCALES`] index.
pub fn clamp_scale_index(value: f32) -> usize {
    let n = SCALES.len();
    if n == 0 {
        return 0;
    }
    let i = value.round() as i32;
    i.clamp(0, (n as i32) - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn major_mask() -> u16 {
        SCALES[SCALE_MAJOR].mask
    }

    #[test]
    fn major_keeps_scale_tones() {
        // 0 V = root of scale when root=0 → degree 0.
        let q = quantize_to_scale(0.0, 0.0, major_mask());
        assert!((q - 0.0).abs() < 1e-5);

        // Two semitones up (major 2nd) is in major: 2/12 V.
        let two = 2.0 / 12.0;
        let q = quantize_to_scale(two, 0.0, major_mask());
        assert!((q - two).abs() < 1e-5);
    }

    #[test]
    fn major_snaps_off_scale_tone() {
        // One semitone (minor 2nd) is not in major → nearer of 0 or 2.
        // 1 is equidistant; algorithm prefers lower cand → 0.
        let one = 1.0 / 12.0;
        let q = quantize_to_scale(one, 0.0, major_mask());
        assert!((q - 0.0).abs() < 1e-5 || (q - 2.0 / 12.0).abs() < 1e-5);

        // 1.4 semis closer to 2 than to 0.
        let almost_two = 1.4 / 12.0;
        let q = quantize_to_scale(almost_two, 0.0, major_mask());
        assert!((q - 2.0 / 12.0).abs() < 1e-5, "got {q}");
    }

    #[test]
    fn root_shifts_scale() {
        // Root +2 semis: the root tone itself sits at 2/12 V and must stick.
        let root = 2.0;
        let root_v = root / 12.0;
        let q = quantize_to_scale(root_v, root, major_mask());
        assert!((q - root_v).abs() < 1e-5, "got {q}");

        // A pitch slightly above the root (off the half-step) snaps back to root,
        // not to the unshifted (root=0) major grid.
        let slightly_above = (root + 0.4) / 12.0;
        let q = quantize_to_scale(slightly_above, root, major_mask());
        assert!((q - root_v).abs() < 1e-5, "got {q}");
    }

    #[test]
    fn hirajoshi_rejects_non_members() {
        let hira = SCALES.iter().find(|s| s.name == "Hirajoshi").unwrap();
        // Degree 4 (major 3rd) not in Hirajoshi {0,2,3,7,8}; nearer is 3.
        let four = 4.0 / 12.0;
        let q = quantize_to_scale(four, 0.0, hira.mask);
        let semis = (q * 12.0).round() as i32;
        let pc = ((semis % 12) + 12) % 12;
        assert!(
            matches!(pc, 0 | 2 | 3 | 7 | 8),
            "quantized to off-scale pc {pc}"
        );
    }

    #[test]
    fn empty_mask_is_chromatic() {
        let q = quantize_to_scale(1.0 / 12.0, 0.0, 0);
        assert!((q - 1.0 / 12.0).abs() < 1e-5);
    }

    #[test]
    fn clamp_scale_index_bounds() {
        assert_eq!(clamp_scale_index(-1.0), 0);
        assert_eq!(clamp_scale_index(1.4), 1);
        assert_eq!(clamp_scale_index(1000.0), SCALES.len() - 1);
    }

    #[test]
    fn scale_index_max_matches_table() {
        assert_eq!(SCALE_INDEX_MAX as usize, SCALES.len() - 1);
        assert_eq!(SCALES.len(), 30);
        assert_eq!(SCALES[SCALE_MAJOR].name, "Major");
        assert_eq!(SCALE_NAMES.len(), SCALES.len());
        for (i, s) in SCALES.iter().enumerate() {
            assert_eq!(SCALE_NAMES[i], s.name, "name mismatch at {i}");
        }
    }
}
