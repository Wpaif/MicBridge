//! Cadeia de tratamento:
//! passa-altas -> RNNoise -> noise gate -> de-esser -> EQ de presença
//! -> compressor -> ganho automático -> ganho manual -> limitador de pico.
//!
//! Para leigos, basta escolher um `Preset`; os controles individuais
//! continuam disponíveis para quem quiser ajustar.

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

const HPF_HZ: f32 = 80.0;
const PRESENCE_HZ: f32 = 3_000.0;
const PRESENCE_DB: f32 = 2.5;
const DEESS_HZ: f32 = 6_000.0;
const DEESS_THRESHOLD_DB: f32 = -32.0;
const DEESS_MAX_REDUCTION_DB: f32 = 8.0;

const AGC_TARGET_DB: f32 = -18.0;
const AGC_MAX_GAIN_DB: f32 = 18.0;
const AGC_MIN_GAIN_DB: f32 = -6.0;
/// Abaixo disso o AGC congela o ganho, para não amplificar ruído de fundo.
const AGC_FREEZE_DB: f32 = -50.0;
const AGC_ATTACK_S: f32 = 0.15;
const AGC_RELEASE_S: f32 = 1.5;

pub struct Params {
    pub denoise: AtomicBool,
    pub gate: AtomicBool,
    pub compressor: AtomicBool,
    /// Remove rumble e vibração de mesa abaixo de ~80 Hz.
    pub highpass: AtomicBool,
    /// Realça a inteligibilidade da voz (~3 kHz).
    pub presence: AtomicBool,
    /// Suaviza sibilantes ("s", "ch") agressivos.
    pub deesser: AtomicBool,
    /// Mantém o volume constante sem ajuste manual (substitui o makeup).
    pub auto_gain: AtomicBool,

    gate_db: AtomicU32,
    comp_threshold_db: AtomicU32,
    comp_ratio: AtomicU32,
    makeup_db: AtomicU32,

    /// RMS linear da entrada e da saída.
    pub in_level: AtomicU32,
    pub out_level: AtomicU32,

    /// Frames processados (com wrap). Quem observa o nível compara com a leitura
    /// anterior para saber se ainda chega áudio, e não um valor antigo.
    frames: AtomicU32,
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
            highpass: AtomicBool::new(true),
            presence: AtomicBool::new(true),
            deesser: AtomicBool::new(true),
            auto_gain: AtomicBool::new(true),

            gate_db: atomic_f32(-55.0),
            comp_threshold_db: atomic_f32(-20.0),
            comp_ratio: atomic_f32(6.0),
            makeup_db: atomic_f32(10.0),

            in_level: atomic_f32(0.0),
            out_level: atomic_f32(0.0),
            frames: AtomicU32::new(0),
        }
    }
}

/// Configurações prontas: um clique em vez de vários controles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Voz limpa e transparente, processamento leve.
    Natural,
    /// Reuniões e chamadas: máxima limpeza de ruído e clareza.
    Chamada,
    /// Podcast/locução: voz encorpada e bem nivelada.
    Podcast,
    /// Jogos/streaming: gate firme para teclado e ventilador.
    Streaming,
    /// Tudo desligado (só o limitador de segurança continua ativo).
    Bruto,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Natural,
        Preset::Chamada,
        Preset::Podcast,
        Preset::Streaming,
        Preset::Bruto,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Natural => "Natural",
            Preset::Chamada => "Chamada",
            Preset::Podcast => "Podcast",
            Preset::Streaming => "Streaming",
            Preset::Bruto => "Original",
        }
    }

    /// Identificador estável, usado para guardar a escolha em disco. Diferente de `name()`,
    /// não muda se o texto exibido for renomeado.
    pub fn key(self) -> &'static str {
        match self {
            Preset::Natural => "natural",
            Preset::Chamada => "chamada",
            Preset::Podcast => "podcast",
            Preset::Streaming => "streaming",
            Preset::Bruto => "bruto",
        }
    }

    pub fn from_key(key: &str) -> Option<Preset> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    pub fn description(self) -> &'static str {
        match self {
            Preset::Natural => "Voz limpa, processamento leve",
            Preset::Chamada => "Reuniões: remove ruído e deixa a voz clara",
            Preset::Podcast => "Voz encorpada e com volume uniforme",
            Preset::Streaming => "Corta teclado e ventilador com firmeza",
            Preset::Bruto => "Sem tratamento: só a proteção contra estouro",
        }
    }
}

impl Params {
    /// Aplica um preset completo.
    pub fn apply_preset(&self, preset: Preset) {
        // (denoise, gate, comp, hpf, presence, deesser, auto_gain, gate_db, thr, ratio)
        let (dn, gt, cp, hp, pr, ds, ag, gate_db, thr, ratio) = match preset {
            Preset::Natural => (
                true, true, true, true, false, false, true, -60.0, -18.0, 2.5,
            ),
            Preset::Chamada => (true, true, true, true, true, true, true, -55.0, -20.0, 4.0),
            Preset::Podcast => (true, true, true, true, true, true, true, -58.0, -22.0, 5.0),
            Preset::Streaming => (true, true, true, true, true, true, true, -45.0, -20.0, 6.0),
            Preset::Bruto => (
                false, false, false, false, false, false, false, -55.0, -20.0, 1.0,
            ),
        };

        self.denoise.store(dn, Relaxed);
        self.gate.store(gt, Relaxed);
        self.compressor.store(cp, Relaxed);
        self.highpass.store(hp, Relaxed);
        self.presence.store(pr, Relaxed);
        self.deesser.store(ds, Relaxed);
        self.auto_gain.store(ag, Relaxed);
        self.set_gate_db(gate_db);
        self.set_comp_threshold_db(thr);
        self.set_comp_ratio(ratio);
        self.set_makeup_db(if ag { 0.0 } else { 6.0 });
    }

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

    pub fn frames(&self) -> u32 {
        self.frames.load(Relaxed)
    }

    pub fn gate_threshold(&self) -> f32 {
        finite_clamp(load_f32(&self.gate_db), -80.0, -10.0, -55.0)
    }

    pub fn comp_threshold(&self) -> f32 {
        finite_clamp(load_f32(&self.comp_threshold_db), -60.0, 0.0, -20.0)
    }

    pub fn comp_ratio(&self) -> f32 {
        finite_clamp(load_f32(&self.comp_ratio), 1.0, 20.0, 6.0)
    }

    pub fn makeup_db(&self) -> f32 {
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

/// Filtro biquad (forma transposta II), coeficientes RBJ.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn normalized(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            ..Self::default()
        }
    }

    fn highpass(freq: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * freq / SAMPLE_RATE;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    fn peaking(freq: f32, q: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / SAMPLE_RATE;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
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

    hpf: Biquad,
    presence: Biquad,

    // De-esser: filtro da banda sibilante e seu detector.
    deess_band: Biquad,
    deess_env: f32,

    // Ganho automático em dB (suavizado por frame).
    agc_gain_db: f32,
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

            hpf: Biquad::highpass(HPF_HZ, 0.707),
            presence: Biquad::peaking(PRESENCE_HZ, 0.9, PRESENCE_DB),

            deess_band: Biquad::highpass(DEESS_HZ, 0.707),
            deess_env: 0.0,

            agc_gain_db: 0.0,
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

        // 0. Passa-altas antes do RNNoise: tira rumble que confunde o detector.
        if p.highpass.load(Relaxed) {
            for sample in frame.iter_mut() {
                *sample = self.hpf.tick(*sample);
            }
        } else {
            self.hpf.reset();
        }

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

        // 3. De-esser e EQ de presença.
        if p.deesser.load(Relaxed) {
            self.deess(frame);
        } else {
            self.deess_band.reset();
            self.deess_env = 0.0;
        }

        if p.presence.load(Relaxed) {
            for sample in frame.iter_mut() {
                *sample = self.presence.tick(*sample);
            }
        } else {
            self.presence.reset();
        }

        // 4. Compressor.
        if p.compressor.load(Relaxed) {
            self.compress(frame, p.comp_threshold(), p.comp_ratio());
        }

        // 5. Ganho automático ou manual, e limitador de pico.
        let makeup = if p.auto_gain.load(Relaxed) {
            self.auto_gain(frame);
            1.0
        } else {
            self.agc_gain_db = 0.0;
            db_to_lin(p.makeup_db())
        };
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
        p.frames.fetch_add(1, Relaxed);
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

    /// Atenua só a banda sibilante quando ela passa do limiar.
    fn deess(&mut self, frame: &mut [f32]) {
        let attack = coef(1.0);
        let release = coef(30.0);
        let max_red = db_to_lin(-DEESS_MAX_REDUCTION_DB);

        for sample in frame.iter_mut() {
            let band = self.deess_band.tick(*sample);
            let level = band.abs();
            let a = if level > self.deess_env {
                attack
            } else {
                release
            };
            self.deess_env += a * (level - self.deess_env);

            let over_db = lin_to_db(self.deess_env) - DEESS_THRESHOLD_DB;
            if over_db > 0.0 {
                let gain = db_to_lin(-over_db).max(max_red);
                // Reduz apenas a banda alta, preservando o resto da voz.
                *sample -= band * (1.0 - gain);
            }
        }
    }

    /// Leva a voz ao nível-alvo com ganho lento, congelado nas pausas.
    fn auto_gain(&mut self, frame: &mut [f32]) {
        let level_db = lin_to_db(rms(frame));
        let prev_db = self.agc_gain_db;

        if level_db > AGC_FREEZE_DB {
            let wanted = (AGC_TARGET_DB - level_db).clamp(AGC_MIN_GAIN_DB, AGC_MAX_GAIN_DB);
            let tau = if wanted < self.agc_gain_db {
                AGC_ATTACK_S
            } else {
                AGC_RELEASE_S
            };
            let k = 1.0 - (-(FRAME as f32 / SAMPLE_RATE) / tau).exp();
            self.agc_gain_db += k * (wanted - self.agc_gain_db);
        }

        // Rampa linear dentro do frame evita estalos na troca de ganho.
        let (from, to) = (db_to_lin(prev_db), db_to_lin(self.agc_gain_db));
        for (i, sample) in frame.iter_mut().enumerate() {
            let t = (i + 1) as f32 / FRAME as f32;
            *sample *= from + (to - from) * t;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames * FRAME)
            .map(|n| amp * (2.0 * std::f32::consts::PI * freq * n as f32 / SAMPLE_RATE).sin())
            .collect()
    }

    fn run(p: &Params, input: &mut [f32]) {
        let mut proc = Processor::new();
        for frame in input.chunks_exact_mut(FRAME) {
            proc.process(frame, p);
        }
    }

    #[test]
    fn saida_nunca_excede_o_teto() {
        let p = Params::default();
        p.set_makeup_db(24.0);
        p.auto_gain.store(false, Relaxed);
        let mut x = sine(440.0, 0.9, 100);
        run(&p, &mut x);
        let ceiling = db_to_lin(LIMITER_CEILING_DB);
        assert!(x.iter().all(|s| s.abs() <= ceiling + 1e-6));
    }

    #[test]
    fn passa_altas_remove_rumble() {
        let p = Params::default();
        p.apply_preset(Preset::Bruto);
        p.highpass.store(true, Relaxed);
        p.set_makeup_db(0.0);
        let mut x = sine(20.0, 0.3, 100);
        run(&p, &mut x);
        assert!(rms(&x[x.len() / 2..]) < 0.3 * 0.707 * 0.1);
    }

    #[test]
    fn auto_gain_sobe_voz_baixa() {
        let p = Params::default();
        p.apply_preset(Preset::Bruto);
        p.auto_gain.store(true, Relaxed);
        let mut x = sine(500.0, 0.02, 600);
        let before = rms(&x[x.len() - FRAME..]);
        run(&p, &mut x);
        assert!(rms(&x[x.len() - FRAME..]) > before * 3.0);
    }

    #[test]
    fn presets_nao_produzem_valores_invalidos() {
        for preset in Preset::ALL {
            let p = Params::default();
            p.apply_preset(preset);
            let mut x = sine(300.0, 0.5, 50);
            run(&p, &mut x);
            assert!(x.iter().all(|s| s.is_finite()), "{preset:?}");
        }
    }
}
