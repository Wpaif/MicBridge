//! Cadeia de tratamento: redução de ruído (RNNoise) -> noise gate -> compressor -> limitador.

use nnnoiseless::DenoiseState;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

pub const SAMPLE_RATE: f32 = 48_000.0;
/// Tamanho de frame do RNNoise (10 ms a 48 kHz).
pub const FRAME: usize = DenoiseState::FRAME_SIZE;

/// Parâmetros ajustáveis pela UI enquanto o áudio roda.
pub struct Params {
    pub denoise: AtomicBool,
    pub gate: AtomicBool,
    pub compressor: AtomicBool,
    gate_db: AtomicU32,
    comp_threshold_db: AtomicU32,
    comp_ratio: AtomicU32,
    makeup_db: AtomicU32,
    pub in_level: AtomicU32,
    pub out_level: AtomicU32,
}

fn f(v: f32) -> AtomicU32 {
    AtomicU32::new(v.to_bits())
}
fn get(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

impl Default for Params {
    fn default() -> Self {
        Self {
            denoise: AtomicBool::new(true),
            gate: AtomicBool::new(true),
            compressor: AtomicBool::new(true),
            gate_db: f(-55.0),
            comp_threshold_db: f(-20.0),
            comp_ratio: f(3.0),
            makeup_db: f(4.0),
            in_level: f(0.0),
            out_level: f(0.0),
        }
    }
}

impl Params {
    pub fn set_gate_db(&self, v: f32) {
        self.gate_db.store(v.to_bits(), Relaxed)
    }
    pub fn set_comp_threshold_db(&self, v: f32) {
        self.comp_threshold_db.store(v.to_bits(), Relaxed)
    }
    pub fn set_comp_ratio(&self, v: f32) {
        self.comp_ratio.store(v.to_bits(), Relaxed)
    }
    pub fn set_makeup_db(&self, v: f32) {
        self.makeup_db.store(v.to_bits(), Relaxed)
    }
    pub fn in_level(&self) -> f32 {
        get(&self.in_level)
    }
    pub fn out_level(&self) -> f32 {
        get(&self.out_level)
    }
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
fn lin_to_db(x: f32) -> f32 {
    20.0 * (x + 1e-9).log10()
}
fn coef(ms: f32) -> f32 {
    1.0 - (-1.0 / (ms * 0.001 * SAMPLE_RATE)).exp()
}
fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

pub struct Processor {
    denoiser: Box<DenoiseState<'static>>,
    first_frame: bool,
    gate_gain: f32,
    gate_open: bool,
    gate_hold: u32,
    comp_env_db: f32,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            denoiser: DenoiseState::new(),
            first_frame: true,
            gate_gain: 0.0,
            gate_open: false,
            gate_hold: 0,
            comp_env_db: -90.0,
        }
    }

    /// Processa exatamente `FRAME` amostras mono (-1.0..1.0) in-place.
    pub fn process(&mut self, frame: &mut [f32], p: &Params) {
        debug_assert_eq!(frame.len(), FRAME);
        p.in_level.store(rms(frame).to_bits(), Relaxed);

        if p.denoise.load(Relaxed) {
            // RNNoise espera escala de i16.
            let mut input = [0f32; FRAME];
            for (d, s) in input.iter_mut().zip(frame.iter()) {
                *d = s * 32768.0;
            }
            let mut out = [0f32; FRAME];
            self.denoiser.process_frame(&mut out, &input);
            if self.first_frame {
                // A primeira saída do RNNoise tem artefato de fade-in.
                self.first_frame = false;
                out.fill(0.0);
            }
            for (d, s) in frame.iter_mut().zip(out.iter()) {
                *d = s / 32768.0;
            }
        }

        if p.gate.load(Relaxed) {
            self.gate(frame, get(&p.gate_db));
        }
        if p.compressor.load(Relaxed) {
            self.compress(frame, get(&p.comp_threshold_db), get(&p.comp_ratio).max(1.0));
        }

        let makeup = db_to_lin(get(&p.makeup_db));
        for s in frame.iter_mut() {
            // Limitador suave: nunca passa de ~-1 dBFS.
            *s = (*s * makeup).clamp(-4.0, 4.0).tanh() * 0.89;
        }
        p.out_level.store(rms(frame).to_bits(), Relaxed);
    }

    fn gate(&mut self, frame: &mut [f32], threshold_db: f32) {
        let level = lin_to_db(rms(frame));
        if level > threshold_db {
            self.gate_open = true;
            self.gate_hold = 20; // 200 ms de hold
        } else if level < threshold_db - 4.0 {
            if self.gate_hold > 0 {
                self.gate_hold -= 1;
            } else {
                self.gate_open = false;
            }
        }
        let target = if self.gate_open { 1.0 } else { 0.0 };
        let a = if self.gate_open { coef(3.0) } else { coef(80.0) };
        for s in frame.iter_mut() {
            self.gate_gain += a * (target - self.gate_gain);
            *s *= self.gate_gain;
        }
    }

    fn compress(&mut self, frame: &mut [f32], threshold_db: f32, ratio: f32) {
        let (att, rel) = (coef(5.0), coef(120.0));
        for s in frame.iter_mut() {
            let lvl = lin_to_db(s.abs());
            let a = if lvl > self.comp_env_db { att } else { rel };
            self.comp_env_db += a * (lvl - self.comp_env_db);
            let over = self.comp_env_db - threshold_db;
            if over > 0.0 {
                *s *= db_to_lin(-over * (1.0 - 1.0 / ratio));
            }
        }
    }
}
