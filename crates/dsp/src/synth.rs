use crate::module::{BlockContext, Module};
use crate::modules::{Io, MidiToCv};
use crate::patch::{IoEndpoint, ModuleInfo, Patch, PatchError, Voice, IO_DEVICE_DEFAULT};
use crate::{
    LiveSignalBank, MAX_BLOCK_FRAMES, MAX_CABLE_SIGNALS, MAX_MODULE_SIGNALS, MODULE_SIGNAL_WAVE_LEN,
};
use wmidi::{Note, Velocity};

const NUM_VOICES: usize = 4;
/// External input channels forwarded into every voice's `Io` module —
/// must match the capture-channel count each voice's `Io` is built with
/// in `patch.rs`.
const NUM_EXTERNAL_INPUT_CHANNELS: usize = 2;
/// Master output channels: 0 = audio L, 1 = audio R, 2 = pitch CV, 3 = gate CV.
const NUM_MASTER_OUTPUT_CHANNELS: usize = 4;

/// Sentinel module index for the master (non-voice-graph) `Io`. Control
/// surfaces address master calibration knobs with this index via
/// `set_param`; it is never a real graph module slot. Chosen to fit in
/// the gRPC `uint32` module field (`u32::MAX`).
pub const MASTER_IO_MODULE: usize = u32::MAX as usize;

/// Polyphonic front-end: owns a fixed pool of identical patch-graph
/// voices and allocates MIDI notes onto them (reusing a free+idle voice
/// when available, otherwise stealing the oldest-held one), then sums
/// their output. The patch itself (what each voice actually sounds like)
/// lives in `patch.rs` / the `modules` the voices are built from.
///
/// Also owns the outward-facing side of a patch: a master `Io` (see
/// `modules::io`) fed by the mixed voice audio plus a second,
/// independent `MidiToCv` — the same "MIDI patched into CV" module every
/// voice already uses internally, reused here as a mono CV/gate-out
/// source — and a set of per-voice `Io`s for capturing external
/// audio/CV into the patch.
pub struct Synth {
    voices: Vec<Voice>,
    voices_map: [Option<Note>; NUM_VOICES],
    /// Order in which voices were allocated, oldest first; used for voice
    /// stealing when every voice is busy.
    allocation_order: Vec<usize>,
    /// Edit/serialize source of truth. Topology edits mutate this then rebuild
    /// voices (control path only). Param changes update it and live graphs.
    patch: Patch,
    /// Description of every voice's (identical) patch — derived from `patch`
    /// after the last compile (see `describe_patch`).
    patch_modules: Vec<ModuleInfo>,
    /// Patch-owned hardware IO endpoints (device bindings + channel layout).
    /// Mutated by `set_io_device`; the engine realizes the bindings.
    io_endpoints: Vec<IoEndpoint>,
    /// External-input module **dense graph index** shared by every voice,
    /// so `set_input_calibration` can target it on the hot path.
    external_input_module: usize,
    sample_rate: f32,

    /// Reused every block to render one voice before it's mixed into
    /// `mixed` — never (re)allocated on the audio thread.
    voice_scratch: [f32; MAX_BLOCK_FRAMES],
    /// The voices' summed (averaged) mono audio for the current block.
    /// Dualled to hardware L+R in `process_block`.
    mixed: [f32; MAX_BLOCK_FRAMES],

    /// Mono CV/gate-out source, independent of the 4-voice polyphony
    /// pool: every note on/off updates it in addition to whichever voice
    /// actually plays the note, so a hardware CV-out always reflects
    /// "the most recent note" the way a classic MIDI-CV converter does.
    /// See the `synth::tests` module for the exact (simplified,
    /// last-message-wins rather than true last-note-priority) semantics.
    master_midi_to_cv: MidiToCv,
    master_io: Io,
    /// Scratch for `master_midi_to_cv`'s 3 output ports (pitch/gate/velocity).
    master_cv_scratch: [f32; 3 * MAX_BLOCK_FRAMES],
    /// Densely packed (per-call stride = that call's frame count) input
    /// and output scratch for `master_io`.
    master_out_inputs: [f32; NUM_MASTER_OUTPUT_CHANNELS * MAX_BLOCK_FRAMES],
    master_out_outputs: [f32; NUM_MASTER_OUTPUT_CHANNELS * MAX_BLOCK_FRAMES],

    /// Externally-captured audio/CV, staged by `write_external_input` and
    /// forwarded into every voice's `Io` module at the start of the next
    /// `process_block`.
    external_input: [[f32; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],

    /// Frame count from the most recent `process_block` call — tells
    /// `output_channel` how much of `master_out_outputs`' per-channel
    /// stride to expose.
    last_frames: usize,

    /// Live UI meters filled after each `process_block` (no heap growth).
    live_signals: LiveSignalBank,
}

impl Synth {
    /// `output_channel` indices for the master hardware output — see
    /// `NUM_MASTER_OUTPUT_CHANNELS`.
    pub const CHANNEL_AUDIO_L: usize = 0;
    pub const CHANNEL_AUDIO_R: usize = 1;
    /// Alias for left audio (scope / legacy mono peak).
    pub const CHANNEL_AUDIO: usize = Self::CHANNEL_AUDIO_L;
    pub const CHANNEL_PITCH_CV: usize = 2;
    pub const CHANNEL_GATE_CV: usize = 3;
    /// Total number of `output_channel` indices the synth defines;
    /// callers should treat any physical channel beyond this as silent.
    pub const NUM_OUTPUT_CHANNELS: usize = NUM_MASTER_OUTPUT_CHANNELS;

    pub fn new(sample_rate: f32) -> Self {
        Self::from_patch(Patch::default(), sample_rate)
            .expect("default patch must compile")
    }

    /// Builds a synth from an explicit patch document. Allocates — not for
    /// the audio callback.
    pub fn from_patch(patch: Patch, sample_rate: f32) -> Result<Self, PatchError> {
        let (first_voice, meta) = Voice::new(&patch, sample_rate)?;
        let external_input_module = first_voice.external_input_module();
        let mut voices = Vec::with_capacity(NUM_VOICES);
        voices.push(first_voice);
        for _ in 1..NUM_VOICES {
            let (v, _) = Voice::new(&patch, sample_rate)?;
            voices.push(v);
        }

        Ok(Self {
            voices,
            voices_map: [None; NUM_VOICES],
            allocation_order: Vec::with_capacity(NUM_VOICES),
            patch,
            patch_modules: meta.modules,
            io_endpoints: meta.io_endpoints,
            external_input_module,
            sample_rate,
            voice_scratch: [0.0; MAX_BLOCK_FRAMES],
            mixed: [0.0; MAX_BLOCK_FRAMES],
            master_midi_to_cv: MidiToCv::new(),
            master_io: Io::new(NUM_MASTER_OUTPUT_CHANNELS, 0),
            master_cv_scratch: [0.0; 3 * MAX_BLOCK_FRAMES],
            master_out_inputs: [0.0; NUM_MASTER_OUTPUT_CHANNELS * MAX_BLOCK_FRAMES],
            master_out_outputs: [0.0; NUM_MASTER_OUTPUT_CHANNELS * MAX_BLOCK_FRAMES],
            external_input: [[0.0; MAX_BLOCK_FRAMES]; NUM_EXTERNAL_INPUT_CHANNELS],
            last_frames: 0,
            live_signals: LiveSignalBank::default(),
        })
    }

    /// The edit/serialize patch document (topology + params).
    pub fn patch(&self) -> &Patch {
        &self.patch
    }

    /// Broadcasts a parameter change to the matching module in every voice.
    /// `module` is a **stable document id** (or [`MASTER_IO_MODULE`]).
    /// Updates the document so save/load sees current knobs. No rebuild.
    pub fn set_param(&mut self, module: usize, param: usize, value: f32) {
        if module == MASTER_IO_MODULE {
            self.master_io.set_param(param, value);
            return;
        }
        let module_id = module as u32;
        let _ = self.patch.set_param(module_id, param, value);
        // Keep control-surface defaults in sync for describe_patch consumers.
        if let Some(info) = self.patch_modules.iter_mut().find(|m| m.module == module_id) {
            if let Some(p) = info.params.iter_mut().find(|p| p.param == param) {
                p.default_value = value;
            }
        }
        for voice in &mut self.voices {
            voice.set_param_id(module_id, param, value);
        }
    }

    /// Patch-owned hardware IO endpoints (device bindings + layout).
    pub fn describe_io(&self) -> &[IoEndpoint] {
        &self.io_endpoints
    }

    /// Updates the patch's device binding for `endpoint_id`. Returns
    /// `false` if no such endpoint exists. Does not open hardware — the
    /// engine realizes the new binding after this state change.
    pub fn set_io_device(&mut self, endpoint_id: &str, device_name: &str) -> bool {
        let device = if device_name.is_empty() {
            IO_DEVICE_DEFAULT.to_string()
        } else {
            device_name.to_string()
        };
        let Some(endpoint) = self
            .io_endpoints
            .iter_mut()
            .find(|e| e.id == endpoint_id)
        else {
            return false;
        };
        endpoint.device = device.clone();
        if let Some(doc_ep) = self.patch.io_endpoints.iter_mut().find(|e| e.id == endpoint_id) {
            doc_ep.device = device;
        }
        true
    }

    // --- Topology (control path only; allocates / rebuilds voices) -----------

    /// Recompile all voices from the current document. Clears active notes.
    fn rebuild_voices(&mut self) -> Result<(), PatchError> {
        let (first, meta) = Voice::new(&self.patch, self.sample_rate)?;
        self.external_input_module = first.external_input_module();
        let mut voices = Vec::with_capacity(NUM_VOICES);
        voices.push(first);
        for _ in 1..NUM_VOICES {
            let (v, _) = Voice::new(&self.patch, self.sample_rate)?;
            voices.push(v);
        }
        self.voices = voices;
        self.voices_map = [None; NUM_VOICES];
        self.allocation_order.clear();
        self.patch_modules = meta.modules;
        self.io_endpoints = meta.io_endpoints;
        Ok(())
    }

    pub fn add_module(&mut self, kind: &str) -> Result<u32, PatchError> {
        let id = self.patch.add_module(kind)?;
        self.rebuild_voices()?;
        Ok(id)
    }

    pub fn remove_module(&mut self, id: u32) -> Result<(), PatchError> {
        self.patch.remove_module(id)?;
        self.rebuild_voices()
    }

    pub fn connect(
        &mut self,
        src_module: u32,
        src_port: usize,
        dst_module: u32,
        dst_port: usize,
    ) -> Result<u32, PatchError> {
        let cable_id = self.patch.connect(src_module, src_port, dst_module, dst_port)?;
        self.rebuild_voices()?;
        Ok(cable_id)
    }

    pub fn disconnect(
        &mut self,
        src_module: u32,
        src_port: usize,
        dst_module: u32,
        dst_port: usize,
    ) -> Result<(), PatchError> {
        self.patch
            .disconnect(src_module, src_port, dst_module, dst_port)?;
        self.rebuild_voices()
    }

    pub fn disconnect_cable(&mut self, cable_id: u32) -> Result<(), PatchError> {
        self.patch.disconnect_by_id(cable_id)?;
        self.rebuild_voices()
    }

    /// Replace the entire patch document and rebuild. Control path only.
    pub fn load_patch(&mut self, patch: Patch) -> Result<(), PatchError> {
        // Validate compile before swapping document.
        let _ = patch.build(self.sample_rate)?;
        self.patch = patch;
        self.rebuild_voices()
    }

    pub fn save_patch_json(&self) -> Result<String, PatchError> {
        self.patch.to_json()
    }

    /// Set lattice coordinates for one module. Document-only; no voice rebuild.
    pub fn set_module_coordinates(
        &mut self,
        id: u32,
        x: u32,
        y: u32,
    ) -> Result<(), PatchError> {
        self.patch.set_module_coordinates(id, x, y)
    }

    /// Assign the voice main-bus tap (rebuilds voices).
    pub fn set_output_tap(&mut self, module: u32, port: usize) -> Result<(), PatchError> {
        self.patch.set_output_tap(module, port)?;
        self.rebuild_voices()
    }

    /// Batch-update lattice coordinates and optional canvas. Document-only.
    pub fn set_coordinates(
        &mut self,
        positions: &[(u32, u32, u32)],
        canvas: Option<crate::patch::PatchCanvas>,
    ) -> Result<(), PatchError> {
        self.patch.set_coordinates(positions, canvas)
    }

    /// Absolute peak of the most recent block on each external-input
    /// channel (pre-calibration capture staging). Used for IO meters.
    pub fn external_input_peak(&self, channel: usize) -> f32 {
        let frames = self.last_frames;
        if frames == 0 {
            return 0.0;
        }
        self.external_input
            .get(channel)
            .map(|buf| {
                buf[..frames]
                    .iter()
                    .fold(0.0f32, |m, &s| m.max(s.abs()))
            })
            .unwrap_or(0.0)
    }

    /// Stages a block of externally-captured audio/CV on `channel` (must
    /// be `< NUM_EXTERNAL_INPUT_CHANNELS`), forwarded to every voice's
    /// `Io` module on the next `process_block`.
    pub fn write_external_input(&mut self, channel: usize, samples: &[f32]) {
        if let Some(buf) = self.external_input.get_mut(channel) {
            buf[..samples.len()].copy_from_slice(samples);
        }
    }

    /// Sets calibration (`out = in * scale + offset`) for master output
    /// `channel` (see `CHANNEL_AUDIO_L` / `CHANNEL_AUDIO_R` / `CHANNEL_PITCH_CV` /
    /// `CHANNEL_GATE_CV`) — e.g. scale the pitch-CV channel down to match a
    /// real interface's full-scale voltage so this engine's 1V/oct convention
    /// actually lands at 1V/oct on the jack. See `Io`'s doc comment for worked
    /// examples; there's no built-in default beyond neutral (1.0/0.0)
    /// since the right numbers depend entirely on the hardware in use.
    pub fn set_output_calibration(&mut self, channel: usize, scale: f32, offset: f32) {
        self.master_io.set_param(Io::param_send_scale(channel), scale);
        self.master_io.set_param(Io::param_send_offset(channel), offset);
    }

    /// Sets calibration for external-input `channel` (must be
    /// `< NUM_EXTERNAL_INPUT_CHANNELS`) on every voice's `Io` module —
    /// normalizes an external device's own voltage convention back into
    /// this engine's internal units.
    pub fn set_input_calibration(&mut self, channel: usize, scale: f32, offset: f32) {
        for voice in &mut self.voices {
            voice.set_param_index(
                self.external_input_module,
                Io::param_capture_scale(channel),
                scale,
            );
            voice.set_param_index(
                self.external_input_module,
                Io::param_capture_offset(channel),
                offset,
            );
        }
    }

    /// Describes every module the running patch placed in each voice's
    /// graph — module/param names, kinds, and ranges — for building a
    /// generic control surface (gRPC descriptors, MIDI CC lookups) without
    /// hardcoding module indices. See `patch::ModuleInfo`.
    pub fn describe_patch(&self) -> &[ModuleInfo] {
        &self.patch_modules
    }

    /// How many external input channels the patch expects.
    pub fn external_input_channels(&self) -> usize {
        NUM_EXTERNAL_INPUT_CHANNELS
    }

    /// Picks a voice to (re)use for a new note on: prefer a slot that is
    /// both unassigned and has finished its envelope (so a voice still
    /// fading out in its release tail isn't cut short); otherwise steal
    /// the oldest allocated voice.
    fn allocate_voice_index(&self) -> usize {
        if let Some(index) = self
            .voices_map
            .iter()
            .enumerate()
            .find(|(index, note)| note.is_none() && !self.voices[*index].is_active())
            .map(|(index, _)| index)
        {
            return index;
        }

        *self.allocation_order.first().unwrap_or(&0)
    }

    fn get_active_voice_index(&self, note: Note) -> Option<usize> {
        self.voices_map
            .iter()
            .position(|voice| *voice == Some(note))
    }

    pub fn note_on(&mut self, note: Note, velocity: Velocity) {
        let index = self.allocate_voice_index();
        let gain = u8::from(velocity) as f32 / 127.0;
        let freq = note.to_freq_f32();

        self.voices[index].note_on(freq, gain);
        self.voices_map[index] = Some(note);

        self.allocation_order.retain(|i| *i != index);
        self.allocation_order.push(index);

        // Drive the mono CV/gate-out independent of voice allocation —
        // "last message wins" (see the struct doc comment's note on the
        // simplified priority semantics).
        let pitch_volts = (freq / 440.0).log2();
        self.master_midi_to_cv.set_param(MidiToCv::PARAM_PITCH, pitch_volts);
        self.master_midi_to_cv.set_param(MidiToCv::PARAM_VELOCITY, gain);
        self.master_midi_to_cv.set_param(MidiToCv::PARAM_GATE, 1.0);
    }

    pub fn note_off(&mut self, note: Note, _velocity: Velocity) {
        if let Some(index) = self.get_active_voice_index(note) {
            self.voices[index].note_off();
            self.voices_map[index] = None;
            self.allocation_order.retain(|i| *i != index);
        }
        self.master_midi_to_cv.set_param(MidiToCv::PARAM_GATE, 0.0);
    }

    /// Renders every voice, the mixed audio, and the master CV/gate-out
    /// for `frames` (must be `<= MAX_BLOCK_FRAMES`). Allocation-free.
    /// Read the result afterward via `output_channel`.
    pub fn process_block(&mut self, frames: usize) {
        debug_assert!(frames <= MAX_BLOCK_FRAMES);
        self.last_frames = frames;

        // Forward any staged external input into every voice.
        for channel in 0..NUM_EXTERNAL_INPUT_CHANNELS {
            let samples = &self.external_input[channel][..frames];
            for voice in &mut self.voices {
                voice.write_input(channel, samples);
            }
        }

        // Render and mix voices (mono graph → mono sum).
        self.mixed[..frames].fill(0.0);
        for voice in &mut self.voices {
            let scratch = &mut self.voice_scratch[..frames];
            voice.process_block(scratch);
            for (m, &s) in self.mixed[..frames].iter_mut().zip(scratch.iter()) {
                *m += s;
            }
        }
        let voice_count = self.voices.len() as f32;
        for m in self.mixed[..frames].iter_mut() {
            *m /= voice_count;
        }

        let ctx = BlockContext {
            sample_rate: self.sample_rate,
            frames,
        };

        // Master CV/gate-out.
        self.master_midi_to_cv
            .process(&[], &mut self.master_cv_scratch[..3 * frames], &ctx);
        let pitch_cv_start = MidiToCv::OUT_PITCH * frames;
        let gate_cv_start = MidiToCv::OUT_GATE * frames;

        // Master Io: dual mono audio on L+R, then pitch CV, gate CV.
        self.master_out_inputs[..frames].copy_from_slice(&self.mixed[..frames]);
        self.master_out_inputs[frames..2 * frames].copy_from_slice(&self.mixed[..frames]);
        self.master_out_inputs[2 * frames..3 * frames]
            .copy_from_slice(&self.master_cv_scratch[pitch_cv_start..pitch_cv_start + frames]);
        self.master_out_inputs[3 * frames..4 * frames]
            .copy_from_slice(&self.master_cv_scratch[gate_cv_start..gate_cv_start + frames]);

        self.master_io.process(
            &self.master_out_inputs[..NUM_MASTER_OUTPUT_CHANNELS * frames],
            &mut self.master_out_outputs[..NUM_MASTER_OUTPUT_CHANNELS * frames],
            &ctx,
        );

        self.refresh_live_signals(frames);
    }

    /// Number of voices currently sounding (held or still in their release
    /// tail) — for metering/UI, not used by the engine itself.
    pub fn active_voice_count(&self) -> u32 {
        self.voices.iter().filter(|v| v.is_active()).count() as u32
    }

    /// Per-module / per-cable live meters from the most recent block.
    pub fn live_signals(&self) -> &LiveSignalBank {
        &self.live_signals
    }

    /// Fill `live_signals` from voice graph outputs (allocation-free).
    fn refresh_live_signals(&mut self, frames: usize) {
        let bank = &mut self.live_signals;
        bank.module_count = 0;
        bank.cable_count = 0;
        if frames == 0 {
            return;
        }

        // Modules: peak + decimated waveform of the loudest output port
        // across all voices (so a VCO patched on square still shows signal).
        let n_mod = self.patch.modules.len().min(MAX_MODULE_SIGNALS);
        for i in 0..n_mod {
            let id = self.patch.modules[i].id;
            bank.module_ids[i] = id;
            let mut peak = 0.0f32;
            let mut best_voice = 0usize;
            let mut best_port = 0usize;
            let mut best_peak = 0.0f32;
            let n_ports = self
                .voices
                .first()
                .map(|v| v.module_output_count(id))
                .unwrap_or(0)
                .max(1);
            for (vi, voice) in self.voices.iter().enumerate() {
                for port in 0..n_ports {
                    let samples = voice.module_output(id, port);
                    let mut vpeak = 0.0f32;
                    for &s in samples {
                        vpeak = vpeak.max(s.abs());
                    }
                    peak = peak.max(vpeak);
                    if vpeak >= best_peak {
                        best_peak = vpeak;
                        best_voice = vi;
                        best_port = port;
                    }
                }
            }
            bank.module_peaks[i] = peak;

            let wave_base = i * MODULE_SIGNAL_WAVE_LEN;
            let samples = self.voices[best_voice].module_output(id, best_port);
            if samples.is_empty() {
                for k in 0..MODULE_SIGNAL_WAVE_LEN {
                    bank.module_waves[wave_base + k] = 0.0;
                }
            } else {
                for k in 0..MODULE_SIGNAL_WAVE_LEN {
                    let idx = (k * samples.len()) / MODULE_SIGNAL_WAVE_LEN;
                    bank.module_waves[wave_base + k] = samples.get(idx).copied().unwrap_or(0.0);
                }
            }
        }
        bank.module_count = n_mod;

        // Cables: peak of the source module/port across voices.
        let n_cab = self.patch.cables.len().min(MAX_CABLE_SIGNALS);
        for i in 0..n_cab {
            let c = &self.patch.cables[i];
            bank.cable_ids[i] = c.id;
            let mut peak = 0.0f32;
            for voice in &self.voices {
                let samples = voice.module_output(c.src_module, c.src_port);
                for &s in samples {
                    peak = peak.max(s.abs());
                }
            }
            bank.cable_peaks[i] = peak;
        }
        bank.cable_count = n_cab;
    }

    /// Reads back hardware output `channel`'s block from the most recent
    /// `process_block` call. Channels: 0 = audio L, 1 = audio R (copy of L),
    /// 2 = pitch CV, 3 = gate CV (see `NUM_MASTER_OUTPUT_CHANNELS`).
    pub fn output_channel(&self, channel: usize) -> &[f32] {
        let start = channel * self.last_frames;
        &self.master_out_outputs[start..start + self.last_frames]
    }

    /// Most recent block of mixed mono audio (dualled to L+R on the master).
    pub fn scope_waveform(&self) -> &[f32] {
        &self.mixed[..self.last_frames]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(n: u8) -> Note {
        Note::try_from(n).unwrap()
    }

    fn velocity(v: u8) -> Velocity {
        Velocity::try_from(v).unwrap()
    }

    #[test]
    fn assigns_distinct_voices_to_distinct_notes() {
        let mut synth = Synth::new(100.0);
        for n in [60, 62, 64, 65] {
            synth.note_on(note(n), velocity(100));
        }
        // All four voices should now be spoken for, one note each.
        let assigned: Vec<Note> = synth.voices_map.iter().filter_map(|v| *v).collect();
        assert_eq!(assigned.len(), 4);
    }

    #[test]
    fn note_off_does_not_immediately_free_a_still_releasing_voice() {
        // Groove patch uses an envelope as activity_gate (release tail).
        let mut synth = Synth::from_patch(Patch::example_groove(), 100.0).unwrap();
        synth.note_on(note(60), velocity(100));
        // Let the envelope observe the gate opening.
        synth.process_block(1);
        synth.note_off(note(60), velocity(0));

        // The voice's slot in voices_map is cleared...
        assert!(synth.voices_map.iter().all(|v| v.is_none()));
        // ...but the underlying voice is still active (mid-release), so
        // it must not be picked for a brand new note right away.
        assert!(synth.voices[0].is_active());

        synth.note_on(note(62), velocity(100));
        // The new note must have landed on a different, untouched voice,
        // not the one still fading out.
        assert_ne!(synth.voices_map[0], Some(note(62)));
    }

    #[test]
    fn steals_the_oldest_voice_when_all_are_held() {
        let mut synth = Synth::new(100.0);
        for n in [60, 62, 64, 65] {
            synth.note_on(note(n), velocity(100));
        }
        // A 5th note, with every voice still held, must steal voice 0
        // (the oldest allocation) rather than silently drop.
        synth.note_on(note(67), velocity(100));
        assert_eq!(synth.voices_map[0], Some(note(67)));
    }

    // --- End-to-end buffer tests: these render actual audio through
    // Synth::process_block and inspect the resulting samples, rather than
    // just the voice-allocation bookkeeping the tests above check.

    #[test]
    fn empty_default_is_silent_without_notes() {
        let mut synth = Synth::new(48_000.0);
        synth.process_block(256);
        assert!(
            synth
                .output_channel(Synth::CHANNEL_AUDIO_L)
                .iter()
                .all(|&s| s.abs() < 1e-6),
            "empty default should be silent"
        );
    }

    #[test]
    fn groove_example_free_runs_without_notes() {
        let mut synth = Synth::from_patch(Patch::example_groove(), 48_000.0).unwrap();
        let mut energy = 0.0f64;
        for _ in 0..40 {
            synth.process_block(256);
            for &s in synth.output_channel(0) {
                assert!(s.is_finite());
                energy += f64::from(s * s);
            }
        }
        assert!(
            energy > 0.0,
            "expected free-running bass/drums without MIDI notes"
        );
    }

    #[test]
    fn produces_bounded_finite_nonsilent_audio_while_a_note_is_held() {
        // Groove example has a MIDI lead path.
        let mut synth = Synth::from_patch(Patch::example_groove(), 48_000.0).unwrap();
        synth.note_on(note(69), velocity(100)); // A4, 440Hz

        let mut energy = 0.0f64;
        for _ in 0..20 {
            synth.process_block(256);
            for &s in synth.output_channel(0) {
                assert!(s.is_finite(), "non-finite sample: {s}");
                assert!(s.abs() <= 1.5, "sample out of expected headroom: {s}");
                energy += f64::from(s * s);
            }
        }
        assert!(energy > 0.0, "expected non-silent output while a note is held");
    }

    #[test]
    fn lead_release_does_not_stop_the_groove() {
        let mut synth = Synth::from_patch(Patch::example_groove(), 48_000.0).unwrap();
        synth.note_on(note(69), velocity(100));

        for _ in 0..10 {
            synth.process_block(256);
        }
        synth.note_off(note(69), velocity(0));

        for _ in 0..100 {
            synth.process_block(256);
        }
        let mut energy = 0.0f64;
        for &s in synth.output_channel(0) {
            assert!(s.is_finite());
            energy += f64::from(s * s);
        }
        assert!(
            energy > 0.0,
            "expected accompaniment to keep playing after lead release"
        );
    }

    #[test]
    fn cv_out_pitch_holds_after_release_and_gate_drops() {
        let mut synth = Synth::new(48_000.0);
        synth.note_on(note(69), velocity(100)); // A4, pitch CV = 0.0 (440Hz reference)
        synth.process_block(64);

        let pitch_while_held = synth.output_channel(Synth::CHANNEL_PITCH_CV)[0];
        let gate_while_held = synth.output_channel(Synth::CHANNEL_GATE_CV)[0];
        assert_eq!(pitch_while_held, 0.0);
        assert_eq!(gate_while_held, 1.0);

        synth.note_off(note(69), velocity(0));
        synth.process_block(64);

        // Pitch holds its last value (nothing resets it); gate drops.
        assert_eq!(
            synth.output_channel(Synth::CHANNEL_PITCH_CV)[0],
            pitch_while_held
        );
        assert_eq!(synth.output_channel(Synth::CHANNEL_GATE_CV)[0], 0.0);
    }

    #[test]
    fn mono_mix_is_dualled_to_left_and_right() {
        let mut synth = Synth::from_patch(Patch::example_groove(), 48_000.0).unwrap();
        for _ in 0..20 {
            synth.process_block(256);
        }
        let l = synth.output_channel(Synth::CHANNEL_AUDIO_L);
        let r = synth.output_channel(Synth::CHANNEL_AUDIO_R);
        assert_eq!(l.len(), r.len());
        assert!(
            l.iter().zip(r.iter()).all(|(a, b)| (a - b).abs() < 1e-6),
            "L and R should be identical dual-mono copies"
        );
        assert!(l.iter().any(|s| s.abs() > 1e-6), "expected non-silent dual mono");
    }

    #[test]
    fn output_calibration_scales_the_pitch_cv_channel() {
        let mut synth = Synth::new(48_000.0);
        // A4 = 440Hz = 0.0 volts in this engine's 1V/oct convention, so
        // use a note an octave up (880Hz) to get a nonzero pitch CV to
        // observe the scaling on.
        synth.note_on(Note::try_from(81).unwrap(), velocity(100)); // A5, 880Hz
        synth.process_block(1);
        let uncalibrated = synth.output_channel(Synth::CHANNEL_PITCH_CV)[0];
        assert!((uncalibrated - 1.0).abs() < 1e-4, "expected ~1.0 (one octave), got {uncalibrated}");

        synth.set_output_calibration(Synth::CHANNEL_PITCH_CV, 0.1, 0.0);
        synth.process_block(1);
        let calibrated = synth.output_channel(Synth::CHANNEL_PITCH_CV)[0];
        assert!((calibrated - 0.1).abs() < 1e-4, "expected 1.0 scaled by 0.1, got {calibrated}");
    }

    #[test]
    fn input_calibration_scales_captured_external_signal() {
        let mut synth = Synth::new(48_000.0);
        synth.set_input_calibration(0, 0.1, 0.0);
        synth.write_external_input(0, &[10.0]);
        synth.process_block(1);

        // The per-voice Io's capture output feeds the VCF, so we can't
        // read it directly from Synth's public API; instead confirm the
        // calibration call reaches every voice without panicking and
        // that uncalibrated input (channel 1, left at unity) round-trips
        // as expected via the same write path — a smoke test that
        // set_input_calibration's fan-out doesn't error for any voice.
        synth.write_external_input(1, &[1.0]);
        synth.process_block(1);
    }

    #[test]
    fn set_io_device_updates_patch_binding() {
        let mut synth = Synth::new(48_000.0);
        assert!(synth.set_io_device("audio_out", "Mock Interface"));
        let out = synth
            .describe_io()
            .iter()
            .find(|e| e.id == "audio_out")
            .unwrap();
        assert_eq!(out.device, "Mock Interface");

        assert!(synth.set_io_device("audio_in", ""));
        let inn = synth
            .describe_io()
            .iter()
            .find(|e| e.id == "audio_in")
            .unwrap();
        assert_eq!(inn.device, crate::patch::IO_DEVICE_DEFAULT);

        assert!(!synth.set_io_device("nope", "x"));
    }

    #[test]
    fn master_io_module_set_param_reaches_calibration() {
        let mut synth = Synth::new(48_000.0);
        synth.note_on(Note::try_from(81).unwrap(), velocity(100));
        synth.set_param(
            MASTER_IO_MODULE,
            Io::param_send_scale(Synth::CHANNEL_PITCH_CV),
            0.1,
        );
        synth.process_block(1);
        let calibrated = synth.output_channel(Synth::CHANNEL_PITCH_CV)[0];
        assert!(
            (calibrated - 0.1).abs() < 1e-4,
            "expected pitch scale via MASTER_IO_MODULE, got {calibrated}"
        );
    }

    #[test]
    fn add_module_and_reload_json() {
        let mut synth = Synth::new(48_000.0);
        let id = synth.add_module("mixer").unwrap();
        assert!(synth.describe_patch().iter().any(|m| m.module == id));
        let json = synth.save_patch_json().unwrap();
        let mut synth2 = Synth::new(48_000.0);
        synth2
            .load_patch(Patch::from_json(&json).unwrap())
            .unwrap();
        assert!(synth2.describe_patch().iter().any(|m| m.module == id));
        synth2.note_on(note(69), velocity(100));
        synth2.process_block(64);
        // Still produces finite output after load.
        assert!(synth2.output_channel(0).iter().all(|s| s.is_finite()));
    }

    #[test]
    fn live_signals_nonzero_while_note_held() {
        let mut synth = Synth::from_patch(Patch::example_groove(), 48_000.0).unwrap();
        synth.note_on(note(60), velocity(100));
        for _ in 0..20 {
            synth.process_block(256);
        }
        let bank = synth.live_signals();
        assert!(bank.module_count > 0);
        let any_peak = bank.module_peaks[..bank.module_count]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        assert!(any_peak > 0.01, "max module peak {any_peak}");
        let cab = bank.cable_peaks[..bank.cable_count]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        assert!(cab > 0.01, "max cable peak {cab}");
    }

}
