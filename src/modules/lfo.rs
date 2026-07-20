use crate::module::{port, port_mut, BlockContext, Module};

/// Low-frequency oscillator: a modulation-source counterpart to `Vco`,
/// running at sub-audio rates. Emits both bipolar (-1..1) and unipolar
/// (0..1) sine outputs so it can directly drive either a signed CV
/// (e.g. filter cutoff wobble) or an unsigned one (e.g. a VCA level).
pub struct Lfo {
    sample_rate: f32,
    phase: f32,
    rate_hz: f32,
}

impl Lfo {
    pub const IN_RATE_CV: usize = 0;

    pub const OUT_BIPOLAR: usize = 0;
    pub const OUT_UNIPOLAR: usize = 1;

    pub const PARAM_RATE: usize = 0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            rate_hz: 2.0,
        }
    }
}

impl Module for Lfo {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        2
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let rate_cv = port(inputs, Self::IN_RATE_CV, ctx);
        // Split the mutable borrow of `outputs` into its two ports up
        // front so both can be written in the same frame loop below.
        let (bipolar_out, unipolar_out) = outputs.split_at_mut(ctx.frames);
        let bipolar_out = port_mut(bipolar_out, 0, ctx);
        let unipolar_out = port_mut(unipolar_out, 0, ctx);

        for i in 0..ctx.frames {
            let rate = (self.rate_hz * 2f32.powf(rate_cv[i])).max(0.0);
            self.phase = (self.phase + rate / self.sample_rate).fract();
            let bipolar = (self.phase * 2.0 * std::f32::consts::PI).sin();

            bipolar_out[i] = bipolar;
            unipolar_out[i] = (bipolar + 1.0) * 0.5;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        if param == Self::PARAM_RATE {
            self.rate_hz = value.max(0.0);
        }
    }
}
