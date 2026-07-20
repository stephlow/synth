use crate::module::{port, BlockContext, Module};

/// AD function generator (West-coast slope): trigger starts attack to 1.0,
/// then decay to 0. No sustain — ideal for plucks into an LPG or as a
/// CV contour. Cycle mode re-triggers when idle (LFO-like).
pub struct Function {
    sample_rate: f32,
    attack_s: f32,
    decay_s: f32,
    cycle: bool,
    stage: Stage,
    level: f32,
    prev_trig: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
}

impl Function {
    pub const IN_TRIG: usize = 0;

    pub const OUT_CV: usize = 0;
    pub const OUT_EOC: usize = 1; // end-of-cycle pulse (decay finished)

    pub const PARAM_ATTACK: usize = 0;
    pub const PARAM_DECAY: usize = 1;
    pub const PARAM_CYCLE: usize = 2; // 0 off, 1 on

    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            attack_s: 0.01,
            decay_s: 0.2,
            cycle: false,
            stage: Stage::Idle,
            level: 0.0,
            prev_trig: false,
        }
    }
}

impl Module for Function {
    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        2
    }

    fn process(&mut self, inputs: &[f32], outputs: &mut [f32], ctx: &BlockContext) {
        let trig = port(inputs, Self::IN_TRIG, ctx);
        let (cv_out, eoc_out) = outputs.split_at_mut(ctx.frames);

        for i in 0..ctx.frames {
            let open = trig[i] > 0.0;
            let rising = open && !self.prev_trig;
            self.prev_trig = open;

            let mut eoc = 0.0;
            if rising {
                self.stage = Stage::Attack;
            } else if self.cycle && self.stage == Stage::Idle {
                self.stage = Stage::Attack;
            }

            match self.stage {
                Stage::Idle => {
                    self.level = 0.0;
                }
                Stage::Attack => {
                    let step = 1.0 / (self.attack_s.max(1e-4) * self.sample_rate);
                    self.level += step;
                    if self.level >= 1.0 {
                        self.level = 1.0;
                        self.stage = Stage::Decay;
                    }
                }
                Stage::Decay => {
                    let step = 1.0 / (self.decay_s.max(1e-4) * self.sample_rate);
                    self.level -= step;
                    if self.level <= 0.0 {
                        self.level = 0.0;
                        self.stage = Stage::Idle;
                        eoc = 1.0;
                    }
                }
            }

            cv_out[i] = self.level;
            eoc_out[i] = eoc;
        }
    }

    fn set_param(&mut self, param: usize, value: f32) {
        match param {
            Self::PARAM_ATTACK => self.attack_s = value.clamp(0.001, 5.0),
            Self::PARAM_DECAY => self.decay_s = value.clamp(0.001, 5.0),
            Self::PARAM_CYCLE => self.cycle = value >= 0.5,
            _ => {}
        }
    }

    fn is_active(&self) -> bool {
        self.stage != Stage::Idle || self.level > 1e-4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_makes_ad_contour() {
        let mut f = Function::new(1000.0);
        f.set_param(Function::PARAM_ATTACK, 0.005);
        f.set_param(Function::PARAM_DECAY, 0.02);
        let frames = 64;
        let mut inputs = vec![0.0; frames];
        inputs[0] = 1.0;
        inputs[1] = 1.0;
        let mut outputs = vec![0.0; frames * 2];
        let ctx = BlockContext {
            sample_rate: 1000.0,
            frames,
        };
        f.process(&inputs, &mut outputs, &ctx);
        let cv = &outputs[..frames];
        let peak = cv.iter().cloned().fold(0.0f32, f32::max);
        assert!(peak > 0.9, "peak {peak}");
        assert!(cv[frames - 1] < 0.2, "should decay, got {}", cv[frames - 1]);
    }
}
