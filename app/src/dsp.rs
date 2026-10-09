//! Cadeia de tratamento: RNNoise -> noise gate -> compressor -> limitador de pico.

use nnnoiseless::DenoiseState;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

pub const SAMPLE_RATE: f32 = 48_000.0;
/// RNNoise processa frames de 10 ms a 48 kHz.
pub const FRAME: usize = DenoiseState::FRAME_SIZE;

const GATE_HOLD_FRAMES: u32 = 20; // 200 ms
const GATE_HYSTERESIS_DB: f32 = 4.0;
const GATE_FLOOR_DB: f32 = -30.0;
const COMP_ATTACK_MS: f32 = 5.0;
const COMP_RELEASE_MS: f32 = 120.0;
const LIMITER_RELEASE_MS: f32 = 50.0;
const LIMITER_CEILING_DB: f32 = -1.0;

pub struct Params {
    pub denoise: AtomicBool,
    pub gate: AtomicBool,
    pub compressor: AtomicBool,

    gate_db: AtomicU32,
    comp_threshold_db: AtomicU32,
    comp_ratio: AtomicU32,
    makeup_db: AtomicU32,

    /// RMS linear da entrada e da saída.
    pub in_level: AtomicU32,
    pub out_level: AtomicU32,
}

fn atomic_f32(v: f32) -> AtomicU32 {
    AtomicU32::new(v.to_bits())
}

fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

fn finite_clamp(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            denoise: AtomicBool::new(true),
            gate: AtomicBool::new(true),
            compressor: AtomicBool::new(true),

            gate_db: atomic_f32(-55.0),
            comp_threshold_db: atomic_f32(-20.0),
            comp_ratio: atomic_f32(6.0),
            makeup_db: atomic_f32(10.0),

            in_level: atomic_f32(0.0),
            out_level: atomic_f32(0.0),
        }
    }
}

impl Params {
    pub fn set_gate_db(&self, v: f32) {
        self.gate_db
            .store(finite_clamp(v, -80.0, -10.0, -55.0).to_bits(), Relaxed);
    }

    pub fn set_comp_threshold_db(&self, v: f32) {
        self.comp_threshold_db
            .store(finite_clamp(v, -60.0, 0.0, -20.0).to_bits(), Relaxed);
    }

    pub fn set_comp_ratio(&self, v: f32) {
        self.comp_ratio
            .store(finite_clamp(v, 1.0, 20.0, 6.0).to_bits(), Relaxed);
    }

    pub fn set_makeup_db(&self, v: f32) {
        self.makeup_db
            .store(finite_clamp(v, 0.0, 24.0, 10.0).to_bits(), Relaxed);
    }

    pub fn in_level(&self) -> f32 {
        load_f32(&self.in_level)
    }

    pub fn out_level(&self) -> f32 {
        load_f32(&self.out_level)
    }

    fn gate_threshold(&self) -> f32 {
        finite_clamp(load_f32(&self.gate_db), -80.0, -10.0, -55.0)
    }

    fn comp_threshold(&self) -> f32 {
        finite_clamp(load_f32(&self.comp_threshold_db), -60.0, 0.0, -20.0)
    }

    fn comp_ratio(&self) -> f32 {
        finite_clamp(load_f32(&self.comp_ratio), 1.0, 20.0, 6.0)
    }

    fn makeup_db(&self) -> f32 {
        finite_clamp(load_f32(&self.makeup_db), 0.0, 24.0, 10.0)
    }
}

fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converte amplitude linear em dBFS sem produzir -inf no silêncio.
fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Coeficiente exponencial para suavização por amostra.
fn coef(ms: f32) -> f32 {
    if !ms.is_finite() || ms <= 0.0 {
        return 1.0;
    }

    1.0 - (-1.0 / (ms * 0.001 * SAMPLE_RATE)).exp()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum = samples.iter().map(|s| s * s).sum::<f32>();

    (sum / samples.len() as f32).sqrt()
}

pub struct Processor {
    denoiser: Box<DenoiseState<'static>>,

    gate_gain: f32,
    gate_open: bool,
    gate_hold: u32,

    // Detector de amplitude do compressor.
    comp_env: f32,

    // Ganho suavizado do limitador.
    limiter_gain: f32,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            denoiser: DenoiseState::new(),

            gate_gain: 0.0,
            gate_open: false,
            gate_hold: 0,

            comp_env: 0.0,
            limiter_gain: 1.0,
        }
    }

    /// Processa exatamente FRAME amostras mono normalizadas (-1.0..1.0).
    /// A função modifica o buffer in-place.
    pub fn process(&mut self, frame: &mut [f32], p: &Params) {
        debug_assert_eq!(frame.len(), FRAME);

        // Não processa buffers incompatíveis em builds de release.
        if frame.len() != FRAME {
            return;
        }

        // Impede NaN e infinitos de contaminarem os estágios seguintes.
        for sample in frame.iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
        }

        p.in_level.store(rms(frame).to_bits(), Relaxed);

        // 1. Redução de ruído.
        if p.denoise.load(Relaxed) {
            let mut input = [0.0_f32; FRAME];
            let mut output = [0.0_f32; FRAME];

            // nnnoiseless espera amostras na escala de i16 em f32.
            for (dst, src) in input.iter_mut().zip(frame.iter()) {
                *dst = *src * 32768.0;
            }

            self.denoiser.process_frame(&mut output, &input);

            for (dst, src) in frame.iter_mut().zip(output.iter()) {
                *dst = if src.is_finite() {
                    (*src / 32768.0).clamp(-4.0, 4.0)
                } else {
                    0.0
                };
            }
        }

        // 2. Noise gate.
        if p.gate.load(Relaxed) {
            self.gate(frame, p.gate_threshold());
        } else {
            // Evita reabertura lenta caso o gate seja desligado na UI.
            self.gate_open = true;
            self.gate_hold = 0;
            self.gate_gain = 1.0;
        }

        // 3. Compressor.
        if p.compressor.load(Relaxed) {
            self.compress(frame, p.comp_threshold(), p.comp_ratio());
        }

        // 4. Ganho de compensação e limitador de pico.
        let makeup = db_to_lin(p.makeup_db());
        let ceiling = db_to_lin(LIMITER_CEILING_DB);
        let release = coef(LIMITER_RELEASE_MS);

        for sample in frame.iter_mut() {
            let amplified = *sample * makeup;

            // Reduz o ganho quando a amostra ultrapassa o teto.
            // O ataque imediato garante que a saída não exceda o ceiling.
            let peak = amplified.abs();
            let target_gain = if peak > ceiling { ceiling / peak } else { 1.0 };

            if target_gain < self.limiter_gain {
                self.limiter_gain = target_gain;
            } else {
                self.limiter_gain += release * (target_gain - self.limiter_gain);
            }

            *sample = amplified * self.limiter_gain;

            // Proteção final contra picos e valores inválidos.
            if !sample.is_finite() {
                *sample = 0.0;
                self.limiter_gain = 1.0;
            } else {
                *sample = sample.clamp(-ceiling, ceiling);
            }
        }

        p.out_level.store(rms(frame).to_bits(), Relaxed);
    }

    fn gate(&mut self, frame: &mut [f32], threshold_db: f32) {
        let level_db = lin_to_db(rms(frame));

        if level_db >= threshold_db {
            self.gate_open = true;
            self.gate_hold = GATE_HOLD_FRAMES;
        } else if level_db < threshold_db - GATE_HYSTERESIS_DB {
            if self.gate_hold > 0 {
                self.gate_hold -= 1;
            } else {
                self.gate_open = false;
            }
        }

        // O gate atenua o sinal em vez de zerá-lo completamente.
        let target = if self.gate_open {
            1.0
        } else {
            db_to_lin(GATE_FLOOR_DB)
        };

        let attack = coef(3.0);
        let release = coef(80.0);
        let a = if target > self.gate_gain {
            attack
        } else {
            release
        };

        for sample in frame.iter_mut() {
            self.gate_gain += a * (target - self.gate_gain);
            *sample *= self.gate_gain;
        }
    }

    fn compress(&mut self, frame: &mut [f32], threshold_db: f32, ratio: f32) {
        let attack = coef(COMP_ATTACK_MS);
        let release = coef(COMP_RELEASE_MS);
        let ratio = finite_clamp(ratio, 1.0, 20.0, 6.0);

        for sample in frame.iter_mut() {
            let level = sample.abs();

            // Detector de amplitude com ataque e release separados.
            let a = if level > self.comp_env {
                attack
            } else {
                release
            };

            self.comp_env += a * (level - self.comp_env);

            let env_db = lin_to_db(self.comp_env);
            let over_db = (env_db - threshold_db).max(0.0);

            // Relação de compressão: 1:1 não reduz o ganho.
            let gain_reduction_db = over_db * (1.0 - 1.0 / ratio);

            let gain = db_to_lin(-gain_reduction_db);
            *sample *= gain;
        }
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}
