use std::f32::consts::PI;

const SAMPLE_RATE: f32 = 44_100.0;

fn sine(frequency: f32, time: f32) -> f32 {
    (frequency * time * 2.0 * PI / SAMPLE_RATE).sin()
}

fn main() {
    let mut sample_clock: f32 = 0.0;
    let mut buffer: [f32; 1024] = [0.0; 1024];

    loop {
        sample_clock = (sample_clock + 1.0) % SAMPLE_RATE;
        for index in 0..buffer.len() {
            buffer[index] = sine(440.0, sample_clock);
        }
    }
}
