//! Calibração automática: mede o ruído do ambiente (silêncio) e o nível da voz
//! (leitura) no ponto em que o gate decide, e sugere limiar do gate, redução de
//! ruído e ganho manual. A sugestão só muda o áudio quando a pessoa aplica.
//!
//! As medições são em dBFS, uma a cada 20 ms, só quando chega áudio novo.
use crate::dsp::{AGC_TARGET_DB, Params};
use crate::state::{Link, Shared};
use serde_json::{Value, json};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SILENCE: Duration = Duration::from_secs(5);
const SPEECH: Duration = Duration::from_secs(8);
/// Começo do silêncio descartado: o RNNoise ainda está se acomodando.
const SETTLE: Duration = Duration::from_millis(500);
const TICK: Duration = Duration::from_millis(20);
/// Sem áudio novo por este tempo, a calibração desiste.
const STALL: Duration = Duration::from_secs(1);

/// Ruído acima disto (antes do tratamento) pede redução de ruído.
const NOISY_DB: f32 = -62.0;
/// O limiar fica este tanto acima do ruído...
const GATE_ABOVE_NOISE_DB: f32 = 6.0;
/// ...mas sempre este tanto abaixo da voz, para não cortar sílabas fracas.
const GATE_BELOW_VOICE_DB: f32 = 18.0;
/// Para separar fala de pausa na leitura: o que passa disto acima do ruído é fala.
const VOICED_ABOVE_NOISE_DB: f32 = 3.0;
/// Voz e ruído mais próximos que isto: o ambiente ou a distância está ruim.
const MIN_SEPARATION_DB: f32 = 10.0;

/// Uma medição: nível bruto (entrada) e no ponto do gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub raw_db: f32,
    pub gate_db: f32,
}

/// Ajustes atuais, para comparar com a sugestão e saber se o ganho é manual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Current {
    pub gate_db: f32,
    pub denoise: bool,
    pub auto_gain: bool,
    pub makeup_db: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub gate_db: f32,
    pub denoise: bool,
    /// Só com o ganho automático desligado.
    pub makeup_db: Option<f32>,
    pub noise_db: f32,
    pub voice_db: f32,
    pub warning: Option<String>,
}

fn percentile(values: &mut [f32], p: f32) -> f32 {
    values.sort_by(f32::total_cmp);
    let i = ((values.len() - 1) as f32 * p).round() as usize;
    values[i]
}

/// Transforma as medições em sugestão. Erro quando não deu para ouvir a voz.
pub fn suggest(
    silence: &[Sample],
    speech: &[Sample],
    current: Current,
) -> Result<Suggestion, String> {
    if silence.len() < 50 || speech.len() < 100 {
        return Err("Chegou pouco áudio do celular. Confira a conexão e tente de novo.".into());
    }
    let noise_raw = percentile(
        &mut silence.iter().map(|s| s.raw_db).collect::<Vec<_>>(),
        0.5,
    );
    let noise_gate = percentile(
        &mut silence.iter().map(|s| s.gate_db).collect::<Vec<_>>(),
        0.9,
    );
    // Fala: o que passa bem acima do ruído; o resto são pausas entre palavras.
    let voiced: Vec<&Sample> = speech
        .iter()
        .filter(|s| s.gate_db > noise_gate + VOICED_ABOVE_NOISE_DB)
        .collect();
    if voiced.len() * 4 < speech.len() {
        return Err(
            "Não ouvi sua voz. Leia o texto em voz normal, perto do celular, e tente de novo."
                .into(),
        );
    }
    let voice_gate = percentile(
        &mut voiced.iter().map(|s| s.gate_db).collect::<Vec<_>>(),
        0.5,
    );
    let voice_raw = percentile(
        &mut voiced.iter().map(|s| s.raw_db).collect::<Vec<_>>(),
        0.5,
    );

    // Nunca desliga a redução de ruído que já estava ligada.
    let denoise = current.denoise || noise_raw > NOISY_DB;
    // Sem redução de ruído, o gate escuta praticamente a entrada.
    let (noise, voice) = if denoise {
        (noise_gate, voice_gate)
    } else {
        (noise_raw, voice_raw)
    };
    let separation = voice - noise;
    let mut warning = None;
    let mut gate = (noise + GATE_ABOVE_NOISE_DB).min(voice - GATE_BELOW_VOICE_DB);
    if gate < noise + 3.0 {
        // Pouca folga: fica logo acima do ruído e pode cortar sílabas fracas.
        gate = noise + 3.0;
    }
    if separation < MIN_SEPARATION_DB {
        warning = Some(format!(
            "Sua voz ficou só {separation:.0} dB acima do ruído. Aproxime o celular da boca ou \
             procure um lugar mais silencioso."
        ));
    }
    let gate_db = gate.clamp(-80.0, -20.0).round();
    let makeup_db = (!current.auto_gain).then(|| (AGC_TARGET_DB - voice).clamp(0.0, 24.0).round());
    Ok(Suggestion {
        gate_db,
        denoise,
        makeup_db,
        noise_db: noise.round(),
        voice_db: voice.round(),
        warning,
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    Silence,
    Speech,
    Done(Suggestion),
    Failed(String),
}

/// Estado compartilhado entre o daemon (comandos e estado para a UI) e a tarefa
/// que mede. `run` muda a cada início ou cancelamento: uma tarefa antiga para sozinha.
#[derive(Default)]
pub struct Calibration {
    pub phase: Phase,
    pub progress: f32,
    run: u64,
    /// Redução de ruído e passa-altas antes da medição, restaurados ao terminar.
    saved: Option<(bool, bool)>,
}

impl Calibration {
    fn restore(&mut self, p: &Params) {
        if let Some((denoise, highpass)) = self.saved.take() {
            p.denoise.store(denoise, Relaxed);
            p.highpass.store(highpass, Relaxed);
        }
    }

    pub fn cancel(&mut self, p: &Params) {
        self.restore(p);
        self.run += 1;
        self.phase = Phase::Idle;
        self.progress = 0.0;
    }

    /// Aplica a sugestão pronta; vira alteração não salva do perfil.
    pub fn apply(&mut self, p: &Params) -> Result<(), String> {
        let Phase::Done(s) = &self.phase else {
            return Err("Nenhuma calibração pronta para aplicar".into());
        };
        p.gate.store(true, Relaxed);
        p.set_gate_db(s.gate_db);
        p.denoise.store(s.denoise, Relaxed);
        if let Some(m) = s.makeup_db {
            p.set_makeup_db(m);
        }
        self.phase = Phase::Idle;
        Ok(())
    }

    pub fn state(&self, p: &Params) -> Value {
        let (phase, extra) = match &self.phase {
            Phase::Idle => ("idle", json!(null)),
            Phase::Silence => ("silence", json!(null)),
            Phase::Speech => ("speech", json!(null)),
            Phase::Failed(m) => ("failed", json!(m)),
            Phase::Done(s) => (
                "done",
                json!({
                    "gate_db": s.gate_db,
                    "gate_from": p.gate_threshold().round(),
                    "gate_was_on": p.gate.load(Relaxed),
                    "denoise": s.denoise,
                    "denoise_from": self.saved.map_or(p.denoise.load(Relaxed), |(d, _)| d),
                    "makeup_db": s.makeup_db,
                    "makeup_from": p.makeup_db().round(),
                    "noise_db": s.noise_db,
                    "voice_db": s.voice_db,
                    "warning": s.warning,
                }),
            ),
        };
        json!({ "phase": phase, "progress": (self.progress * 100.0).round() / 100.0, "detail": extra })
    }
}

fn db(lin: f32) -> f32 {
    20.0 * lin.max(1e-6).log10()
}

/// Começa uma calibração (cancela a anterior). Precisa do celular conectado.
pub fn start(cal: &Arc<Mutex<Calibration>>, shared: &Arc<Shared>) -> Result<(), String> {
    if shared.status().link != Link::Connected {
        return Err("Conecte o celular para calibrar.".into());
    }
    let p = &shared.params;
    let run = {
        let mut c = cal.lock().unwrap();
        c.cancel(p);
        // Mede com o passa-altas e a redução de ruído ligados: é assim que o gate
        // escuta quando a sugestão liga a redução de ruído.
        c.saved = Some((p.denoise.load(Relaxed), p.highpass.load(Relaxed)));
        p.denoise.store(true, Relaxed);
        p.highpass.store(true, Relaxed);
        c.phase = Phase::Silence;
        c.run
    };
    let (cal, shared) = (cal.clone(), shared.clone());
    tokio::spawn(async move {
        let result = measure(&cal, &shared, run).await;
        let mut c = cal.lock().unwrap();
        if c.run != run {
            return; // cancelada ou substituída
        }
        // O que vale para comparar é o estado de antes da medição.
        let p = &shared.params;
        let before_denoise = c.saved.map_or(p.denoise.load(Relaxed), |(d, _)| d);
        c.restore(p);
        c.progress = 1.0;
        c.phase = match result.and_then(|(silence, speech)| {
            suggest(
                &silence,
                &speech,
                Current {
                    gate_db: p.gate_threshold(),
                    denoise: before_denoise,
                    auto_gain: p.auto_gain.load(Relaxed),
                    makeup_db: p.makeup_db(),
                },
            )
        }) {
            Ok(s) => Phase::Done(s),
            Err(m) => Phase::Failed(m),
        };
    });
    Ok(())
}

async fn measure(
    cal: &Mutex<Calibration>,
    shared: &Shared,
    run: u64,
) -> Result<(Vec<Sample>, Vec<Sample>), String> {
    let p = &shared.params;
    let total = (SILENCE + SPEECH).as_secs_f32();
    let (mut silence, mut speech) = (Vec::new(), Vec::new());
    let mut last_frames = p.frames();
    let mut last_fresh = Duration::ZERO;
    let mut t = Duration::ZERO;
    while t < SILENCE + SPEECH {
        tokio::time::sleep(TICK).await;
        t += TICK;
        {
            let mut c = cal.lock().unwrap();
            if c.run != run {
                return Err(String::new());
            }
            c.progress = t.as_secs_f32() / total;
            if t >= SILENCE && c.phase == Phase::Silence {
                c.phase = Phase::Speech;
            }
        }
        if shared.status().link != Link::Connected {
            return Err("O celular desconectou durante a calibração.".into());
        }
        let frames = p.frames();
        if frames == last_frames {
            if t - last_fresh > STALL {
                return Err("O áudio do celular parou de chegar. Tente de novo.".into());
            }
            continue;
        }
        last_frames = frames;
        last_fresh = t;
        let sample = Sample {
            raw_db: db(p.in_level()),
            gate_db: db(p.gate_in_level()),
        };
        if t < SETTLE {
            continue;
        }
        if t < SILENCE {
            silence.push(sample);
        } else {
            speech.push(sample);
        }
    }
    Ok((silence, speech))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(n: usize, raw: f32, gate: f32) -> Vec<Sample> {
        (0..n)
            .map(|i| {
                // Variação pequena e fixa, como medições reais.
                let wobble = ((i * 7) % 5) as f32 - 2.0;
                Sample {
                    raw_db: raw + wobble,
                    gate_db: gate + wobble,
                }
            })
            .collect()
    }

    /// Fala com pausas: 60% sílabas no nível da voz, 40% pausas no nível do ruído.
    fn talk(n: usize, voice: (f32, f32), noise: (f32, f32)) -> Vec<Sample> {
        let v = samples(n, voice.0, voice.1);
        let s = samples(n, noise.0, noise.1);
        (0..n)
            .map(|i| if i % 5 < 3 { v[i] } else { s[i] })
            .collect()
    }

    const NOW: Current = Current {
        gate_db: -55.0,
        denoise: false,
        auto_gain: true,
        makeup_db: 0.0,
    };

    #[test]
    fn quiet_room_keeps_denoise_off_and_puts_gate_just_above_noise() {
        let silence = samples(240, -70.0, -72.0);
        let speech = talk(400, (-30.0, -31.0), (-70.0, -72.0));
        let s = suggest(&silence, &speech, NOW).unwrap();
        assert!(!s.denoise);
        // Ruído bruto ~-70: limiar 6 dB acima.
        assert_eq!(s.gate_db, -64.0);
        assert_eq!(s.makeup_db, None);
        assert_eq!(s.warning, None);
    }

    #[test]
    fn noisy_room_turns_denoise_on_and_measures_after_it() {
        // Ruído de -45 na entrada cai para -65 depois do RNNoise.
        let silence = samples(240, -45.0, -65.0);
        let speech = talk(400, (-28.0, -30.0), (-45.0, -65.0));
        let s = suggest(&silence, &speech, NOW).unwrap();
        assert!(s.denoise);
        assert_eq!(s.noise_db, -63.0); // percentil 90 do ruído no gate
        assert_eq!(s.gate_db, -57.0);
        assert_eq!(s.voice_db, -30.0);
    }

    #[test]
    fn gate_stays_well_below_voice_and_warns_when_too_close() {
        let silence = samples(240, -40.0, -40.0);
        let speech = talk(400, (-32.0, -32.0), (-40.0, -40.0));
        let s = suggest(
            &silence,
            &speech,
            Current {
                denoise: true,
                ..NOW
            },
        )
        .unwrap();
        // Voz só ~8 dB acima: limiar logo acima do ruído e aviso.
        assert!(s.gate_db <= -35.0, "{}", s.gate_db);
        assert!(s.warning.as_deref().unwrap().contains("acima do ruído"));
    }

    #[test]
    fn manual_gain_brings_voice_to_target_only_when_auto_is_off() {
        let silence = samples(240, -70.0, -72.0);
        let speech = talk(400, (-34.0, -35.0), (-70.0, -72.0));
        let manual = Current {
            auto_gain: false,
            ..NOW
        };
        let s = suggest(&silence, &speech, manual).unwrap();
        // Voz em -34 (bruta, sem redução de ruído) -> alvo -18: +16 dB.
        assert_eq!(s.makeup_db, Some(16.0));
    }

    #[test]
    fn never_turns_off_denoise_that_was_on() {
        let silence = samples(240, -80.0, -80.0);
        let speech = talk(400, (-30.0, -30.0), (-80.0, -80.0));
        let s = suggest(
            &silence,
            &speech,
            Current {
                denoise: true,
                ..NOW
            },
        )
        .unwrap();
        assert!(s.denoise);
    }

    #[test]
    fn silence_while_reading_or_too_little_audio_is_an_error() {
        let silence = samples(240, -60.0, -62.0);
        let quiet = samples(400, -60.0, -62.0);
        assert!(
            suggest(&silence, &quiet, NOW)
                .unwrap_err()
                .contains("Não ouvi")
        );
        assert!(
            suggest(&silence[..10], &quiet, NOW)
                .unwrap_err()
                .contains("pouco áudio")
        );
    }

    #[test]
    fn apply_and_cancel_change_only_what_they_should() {
        let p = Params::default();
        let mut c = Calibration::default();
        assert!(c.apply(&p).is_err());
        c.phase = Phase::Done(Suggestion {
            gate_db: -50.0,
            denoise: true,
            makeup_db: Some(12.0),
            noise_db: -60.0,
            voice_db: -30.0,
            warning: None,
        });
        p.gate.store(false, Relaxed);
        p.denoise.store(false, Relaxed);
        c.apply(&p).unwrap();
        assert!(p.gate.load(Relaxed) && p.denoise.load(Relaxed));
        assert_eq!((p.gate_threshold(), p.makeup_db()), (-50.0, 12.0));
        assert_eq!(c.phase, Phase::Idle);
        // Cancelar no meio devolve a redução de ruído e o passa-altas de antes.
        p.denoise.store(false, Relaxed);
        p.highpass.store(false, Relaxed);
        c.saved = Some((false, false));
        p.denoise.store(true, Relaxed);
        p.highpass.store(true, Relaxed);
        c.cancel(&p);
        assert!(!p.denoise.load(Relaxed) && !p.highpass.load(Relaxed));
    }
}
