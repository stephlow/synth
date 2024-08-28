#[derive(Debug)]
pub struct Oscillator {
    pub sample_rate: f32,
    pub waveform: Waveform,
    pub current_sample_index: f32,
    pub frequency_hz: f32,
    pub gate: Gate,
}

#[derive(Debug)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
}

#[derive(Debug)]
pub enum Gate {
    High,
    Low,
}

impl Oscillator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            waveform: Waveform::Sine,
            sample_rate,
            current_sample_index: 0.0,
            frequency_hz: 440.0,
            gate: Gate::Low,
        }
    }

    pub fn note_on(&mut self, freq: f32) {
        self.frequency_hz = freq;
        self.gate = Gate::High;
    }

    pub fn note_off(&mut self) {
        self.gate = Gate::Low;
    }

    pub fn set_frequency(&mut self, frequency_hz: f32) {
        self.frequency_hz = frequency_hz;
    }

    fn advance_sample(&mut self) {
        self.current_sample_index = (self.current_sample_index + 1.0) % self.sample_rate;
    }

    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    fn calculate_sine_output_from_freq(&self, freq: f32) -> f32 {
        let two_pi = 2.0 * std::f32::consts::PI;
        (self.current_sample_index * freq * two_pi / self.sample_rate).sin()
    }

    fn is_multiple_of_freq_above_nyquist(&self, multiple: f32) -> bool {
        self.frequency_hz * multiple > self.sample_rate / 2.0
    }

    fn sine_wave(&mut self) -> f32 {
        self.advance_sample();
        self.calculate_sine_output_from_freq(self.frequency_hz)
    }

    fn generative_waveform(&mut self, harmonic_index_increment: i32, gain_exponent: f32) -> f32 {
        self.advance_sample();
        let mut output = 0.0;
        let mut i = 1;
        while !self.is_multiple_of_freq_above_nyquist(i as f32) {
            let gain = 1.0 / (i as f32).powf(gain_exponent);
            output += gain * self.calculate_sine_output_from_freq(self.frequency_hz * i as f32);
            i += harmonic_index_increment;
        }
        output
    }

    fn square_wave(&mut self) -> f32 {
        self.generative_waveform(2, 1.0)
    }

    fn saw_wave(&mut self) -> f32 {
        self.generative_waveform(1, 1.0)
    }

    fn triangle_wave(&mut self) -> f32 {
        self.generative_waveform(2, 2.0)
    }

    pub fn tick(&mut self) -> f32 {
        match self.gate {
            Gate::High => match self.waveform {
                Waveform::Sine => self.sine_wave(),
                Waveform::Square => self.square_wave(),
                Waveform::Saw => self.saw_wave(),
                Waveform::Triangle => self.triangle_wave(),
            },
            Gate::Low => 0.,
        }
    }
}
