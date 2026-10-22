use crate::module::{BlockContext, Module};

/// Noise source: white, pink (Paul Kellet), and red (brown) simultaneous outs.
pub struct Noise {
    state: u32,
    // Pink filter state (Paul Kellet refined method).
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
    b6: f32,
    // Brown (red) integrator.
    brown: f32,
}

impl Noise {
    pub const OUT_WHITE: usize = 0;
    pub const OUT_PINK: usize = 1;
    pub const OUT_RED: usize = 2;

    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B9 } else { seed },
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            b3: 0.0,
            b4: 0.0,
            b5: 0.0,
            b6: 0.0,
            brown: 0.0,
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_white(&mut self) -> f32 {
        let unit = (self.next_u32() as f64 / f64::from(u32::MAX)) as f32;
        unit * 2.0 - 1.0
    }
}

impl Module for Noise {
    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        3
    }

    fn process(&mut self, _inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let (white_out, rest) = outputs.split_at_mut(ctx.frames);
        let (pink_out, red_out) = rest.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            let white = self.next_white();
            white_out[i] = white;

            // Paul Kellet pink noise.
            self.b0 = 0.99886 * self.b0 + white * 0.0555179;
            self.b1 = 0.99332 * self.b1 + white * 0.0750759;
            self.b2 = 0.96900 * self.b2 + white * 0.1538520;
            self.b3 = 0.86650 * self.b3 + white * 0.3104856;
            self.b4 = 0.55000 * self.b4 + white * 0.5329522;
            self.b5 = -0.7616 * self.b5 - white * 0.0168980;
            let pink = self.b0
                + self.b1
                + self.b2
                + self.b3
                + self.b4
                + self.b5
                + self.b6
                + white * 0.5362;
            self.b6 = white * 0.115926;
            pink_out[i] = (pink * 0.11).clamp(-1.0, 1.0);

            // Brown/red: leaky integrate white.
            self.brown = (self.brown + white * 0.02).clamp(-1.0, 1.0);
            red_out[i] = self.brown * 3.5;
            red_out[i] = red_out[i].clamp(-1.0, 1.0);
        }
    }
}
