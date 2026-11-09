//! Testes de qualidade do DSP com sinais sintéticos: travam o comportamento que a
//! bancada (`dsp_bench`) mediu com áudio real. Falham se uma mudança na cadeia fizer
//! um efeito parar de atuar, os perfis ficarem iguais ou o tratamento piorar.
use crate::dsp::{FRAME, Params, Preset, Processor, SAMPLE_RATE};
use std::f32::consts::PI;
use std::sync::atomic::Ordering::Relaxed;

/// Gerador pseudoaleatório fixo: o mesmo sinal em toda execução.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

/// "Voz": vogais harmônicas (f0 ~120-180 Hz, formantes simples) em sílabas de
/// 150-300 ms, com pausas entre frases. Devolve o sinal e a máscara de fala por frame.
fn voice(seconds: f32) -> (Vec<f32>, Vec<bool>) {
    let n = (seconds * SAMPLE_RATE) as usize / FRAME * FRAME;
    let mut out = vec![0.0; n];
    let mut rng = Lcg(7);
    let mut t = 0usize;
    let mut phase = 0.0_f32;
    while t < n {
        // Frase de ~1,5 s com sílabas, depois pausa de ~0,6 s.
        let phrase_end = (t + (1.5 * SAMPLE_RATE) as usize).min(n);
        while t < phrase_end {
            let len = ((0.15 + 0.15 * (rng.next() * 0.5 + 0.5)) * SAMPLE_RATE) as usize;
            let f0 = 150.0 + 30.0 * rng.next();
            let end = (t + len).min(phrase_end);
            for i in t..end {
                let k = (i - t) as f32 / len as f32;
                let env = (PI * k).sin().powf(0.6);
                phase += 2.0 * PI * f0 / SAMPLE_RATE;
                // Harmônicos com queda de -6 dB/oitava e reforço perto de 700 e 1200 Hz.
                let mut s = 0.0;
                for h in 1..=25 {
                    let f = f0 * h as f32;
                    let formant = 1.0
                        + 2.0 * (-((f - 700.0) / 250.0).powi(2)).exp()
                        + 1.5 * (-((f - 1200.0) / 300.0).powi(2)).exp();
                    s += (phase * h as f32).sin() * formant / h as f32;
                }
                out[i] = s * env;
            }
            t = end + (0.04 * SAMPLE_RATE) as usize;
        }
        t += (0.6 * SAMPLE_RATE) as usize;
    }
    let mask = out
        .chunks_exact(FRAME)
        .map(|f| f.iter().map(|s| s * s).sum::<f32>() > 1e-3)
        .collect();
    (out, mask)
}

/// Ruído rosa aproximado (filtro de Paul Kellet sobre ruído branco fixo).
fn pink(len: usize) -> Vec<f32> {
    let mut rng = Lcg(42);
    let (mut b0, mut b1, mut b2) = (0.0_f32, 0.0_f32, 0.0_f32);
    (0..len)
        .map(|_| {
            let w = rng.next();
            b0 = 0.99765 * b0 + w * 0.0990460;
            b1 = 0.96300 * b1 + w * 0.2965164;
            b2 = 0.57000 * b2 + w * 1.0526913;
            b0 + b1 + b2 + w * 0.1848
        })
        .collect()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Escala para que a parte com fala tenha `db_rms` dBFS.
fn scale_to(x: &mut [f32], mask: &[bool], db_rms: f32) {
    let speech: Vec<f32> = x
        .chunks_exact(FRAME)
        .zip(mask)
        .filter(|(_, m)| **m)
        .flat_map(|(f, _)| f.iter().copied())
        .collect();
    let g = 10f32.powf(db_rms / 20.0) / rms(&speech);
    x.iter_mut().for_each(|s| *s *= g);
}

fn process(p: &Params, input: &[f32]) -> Vec<f32> {
    let mut proc = Processor::new();
    let mut out = input.to_vec();
    for frame in out.chunks_exact_mut(FRAME) {
        proc.process(frame, p);
    }
    out
}

/// Nível em dBFS das partes com fala e das pausas, ignorando o primeiro segundo
/// (ganho automático e makeup ainda se acomodando) e as bordas das frases.
fn levels(x: &[f32], mask: &[bool]) -> (f32, f32) {
    let skip = (SAMPLE_RATE as usize / FRAME).max(1);
    let (mut sp, mut pa) = (Vec::new(), Vec::new());
    for (i, f) in x.chunks_exact(FRAME).enumerate().skip(skip) {
        // Pausa "de verdade": sem fala neste frame nem nos 30 vizinhos (300 ms).
        let quiet = (i.saturating_sub(30)..(i + 30).min(mask.len())).all(|j| !mask[j]);
        if mask[i] {
            sp.extend_from_slice(f);
        } else if quiet {
            pa.extend_from_slice(f);
        }
    }
    (db(rms(&sp)), db(rms(&pa)))
}

fn preset(p: Preset) -> Params {
    let params = Params::default();
    params.apply_preset(p);
    params
}

const SPEAKING: [Preset; 4] = [
    Preset::Natural,
    Preset::Chamada,
    Preset::Podcast,
    Preset::Streaming,
];

/// Voz no nível em que o celular costuma mandar (~-30 dBFS) com ruído rosa 10 dB abaixo.
fn phone_voice_with_noise() -> (Vec<f32>, Vec<bool>, Vec<f32>) {
    let (mut v, mask) = voice(12.0);
    scale_to(&mut v, &mask, -30.0);
    let mut noise = pink(v.len());
    let n = rms(&noise);
    noise
        .iter_mut()
        .for_each(|s| *s *= 10f32.powf(-40.0 / 20.0) / n);
    let mix = v.iter().zip(&noise).map(|(a, b)| a + b).collect();
    (v, mask, mix)
}

#[test]
fn treatment_opens_at_least_15_db_between_voice_and_noise() {
    // Medido: +20 a +31 dB conforme o perfil.
    let (_, mask, mix) = phone_voice_with_noise();
    let (s, p) = levels(&mix, &mask);
    let before = s - p;
    for pr in SPEAKING {
        let (s, p) = levels(&process(&preset(pr), &mix), &mask);
        assert!(
            s - p >= before + 15.0,
            "{pr:?}: {before:.1} -> {:.1} dB",
            s - p
        );
    }
}

#[test]
fn voice_lands_near_target_level_and_never_passes_the_ceiling() {
    // Medido: -20 a -21,5 dBFS de fala para entradas de -30 e -20 dBFS.
    let ceiling = 10f32.powf(-1.0 / 20.0) + 1e-6;
    for (input, min, max) in [
        (-30.0, -23.0, -18.0),
        (-20.0, -23.0, -18.0),
        (-42.0, -29.0, -18.0),
    ] {
        let (mut v, mask) = voice(12.0);
        scale_to(&mut v, &mask, input);
        for pr in SPEAKING {
            let (s, _) = levels(&process(&preset(pr), &v), &mask);
            assert!(
                (min..=max).contains(&s),
                "{pr:?} entrada {input}: fala {s:.1} dBFS"
            );
        }
    }
    // Voz muito alta: o limitador segura todo pico em -1 dBFS.
    let (mut v, mask) = voice(6.0);
    scale_to(&mut v, &mask, -6.0);
    for pr in Preset::ALL {
        let out = process(&preset(pr), &v);
        let peak = out.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(peak <= ceiling, "{pr:?}: pico {:.2} dBFS", db(peak));
    }
}

#[test]
fn compressor_acts_on_voice_at_phone_level() {
    // Regressão: com o compressor antes do ganho automático, a voz do celular
    // (~-30 dBFS) nunca chegava ao limiar e ligar o compressor não mudava nada.
    let (v, _, _) = phone_voice_with_noise();
    let on = preset(Preset::Chamada);
    let mut proc = Processor::new();
    let mut max_gr = 0.0_f32;
    let mut with = v.clone();
    for frame in with.chunks_exact_mut(FRAME) {
        proc.process(frame, &on);
        max_gr = max_gr.max(on.effects().compression_db.unwrap_or(0.0));
    }
    assert!(max_gr >= 3.0, "redução máxima {max_gr:.1} dB"); // medido: 6,9 dB
    let off = preset(Preset::Chamada);
    off.compressor.store(false, Relaxed);
    let without = process(&off, &v);
    let diff = with
        .iter()
        .zip(&without)
        .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(
        diff > 0.05,
        "compressor ligado e desligado quase iguais: {diff:.3}"
    );
}

#[test]
fn speaking_presets_sound_different_from_each_other() {
    // Medido: diferença entre 16 e 31 dB abaixo do sinal. Perfis idênticos (como
    // Chamada e Podcast quando o compressor não atuava) dariam diferença zero.
    let (v, _, _) = phone_voice_with_noise();
    let outs: Vec<_> = SPEAKING
        .iter()
        .map(|p| (*p, process(&preset(*p), &v)))
        .collect();
    for (i, (pa, a)) in outs.iter().enumerate() {
        for (pb, b) in &outs[i + 1..] {
            let d: Vec<f32> = a.iter().zip(b).map(|(x, y)| x - y).collect();
            let below = db(rms(a)) - db(rms(&d));
            assert!(
                below < 40.0,
                "{pa:?} e {pb:?} quase iguais: diferença {below:.1} dB abaixo"
            );
        }
    }
}

#[test]
fn normal_voice_never_lights_the_clipping_warning() {
    use std::sync::atomic::Ordering::Relaxed;
    let (input, _, _) = phone_voice_with_noise();
    for pr in SPEAKING {
        let p = preset(pr);
        process(&p, &input);
        let (i, o) = (
            p.in_clip_frames.load(Relaxed),
            p.out_limit_frames.load(Relaxed),
        );
        assert_eq!((i, o), (0, 0), "{pr:?}: entrada {i}, saída {o} frames");
    }
}
