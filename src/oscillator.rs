use std::f32::consts::PI;

pub fn sine(frequency: f32, time: f32, sample_rate: f32) -> f32 {
    (frequency * time * 2.0 * PI / sample_rate).sin()
}
