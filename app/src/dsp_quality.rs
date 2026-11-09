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
    voice_seeded(seconds, 7)
}

/// Mesma voz com outra semente: outra entonação e outro ritmo (vozes ao fundo).
fn voice_seeded(seconds: f32, seed: u64) -> (Vec<f32>, Vec<bool>) {
    let n = (seconds * SAMPLE_RATE) as usize / FRAME * FRAME;
    let mut out = vec![0.0; n];
    let mut rng = Lcg(seed);
    let mut t = 0usize;
    let mut phase = 0.0_f32;
    while t < n {
        // Frase de ~1,5 s com sílabas, depois pausa de ~0,6 s.
        let phrase_end = (t + (1.5 * SAMPLE_RATE) as usize).min(n);
        while t < phrase_end {
            let len = ((0.15 + 0.15 * (rng.next() * 0.5 + 0.5)) * SAMPLE_RATE) as usize;
            let f0 = 150.0 + 30.0 * rng.next();
            let end = (t + len).min(phrase_end);
            for (i, o) in out.iter_mut().enumerate().take(end).skip(t) {
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
                *o = s * env;
            }
            t = end + (0.04 * SAMPLE_RATE) as usize;
        }
        t += (0.6 * SAMPLE_RATE) as usize;
    }
    let mask = out
        .as_chunks::<FRAME>()
        .0
        .iter()
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

/// Ruídos de teste: as "cores" (inclinação do espectro, de grave a agudo) e três
/// ruídos comuns em chamadas que dá para sintetizar.
#[derive(Debug, Clone, Copy)]
enum Noise {
    /// Espectro plano.
    White,
    /// -3 dB/oitava: o mais parecido com ruído de ambiente em geral.
    Pink,
    /// -6 dB/oitava: ronco grave (ar-condicionado, trânsito distante).
    Brown,
    /// +3 dB/oitava.
    Blue,
    /// +6 dB/oitava: chiado agudo (sibilância de ventilador, hiss).
    Violet,
    /// Zumbido elétrico de 60 Hz (rede no Brasil) com harmônicos.
    Hum,
    /// Cliques de teclado: rajadas curtas e irregulares.
    Keyboard,
    /// Vozes ao fundo: quatro "vozes" sobrepostas.
    Babble,
}

const NOISES: [Noise; 8] = [
    Noise::White,
    Noise::Pink,
    Noise::Brown,
    Noise::Blue,
    Noise::Violet,
    Noise::Hum,
    Noise::Keyboard,
    Noise::Babble,
];

/// Gera `len` amostras do ruído, com RMS 1 (quem usa ajusta o nível).
fn noise(kind: Noise, len: usize) -> Vec<f32> {
    let mut rng = Lcg(42);
    let mut x: Vec<f32> = match kind {
        Noise::White => (0..len).map(|_| rng.next()).collect(),
        Noise::Pink => pink(len),
        Noise::Brown => {
            // Integral do branco, com vazamento para não derivar sem limite.
            let mut y = 0.0_f32;
            (0..len)
                .map(|_| {
                    y = 0.998 * y + rng.next();
                    y
                })
                .collect()
        }
        // Derivar sobe +6 dB/oitava: rosa vira azul, branco vira violeta.
        Noise::Blue => diff(&pink(len)),
        Noise::Violet => diff(&(0..len).map(|_| rng.next()).collect::<Vec<_>>()),
        Noise::Hum => (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE;
                (1..=6)
                    .map(|h| (2.0 * PI * 60.0 * h as f32 * t).sin() / h as f32)
                    .sum()
            })
            .collect(),
        Noise::Keyboard => {
            let mut out = vec![0.0; len];
            let mut i = 0usize;
            while i < len {
                // ~7 teclas por segundo, com intervalo irregular.
                i += ((0.08 + 0.10 * (rng.next() * 0.5 + 0.5)) * SAMPLE_RATE) as usize;
                let click = (0.006 * SAMPLE_RATE) as usize;
                for (k, o) in out.iter_mut().skip(i).take(click).enumerate() {
                    *o = rng.next() * (-(k as f32) / (0.0012 * SAMPLE_RATE)).exp();
                }
            }
            out
        }
        Noise::Babble => {
            let mut out = vec![0.0; len];
            // As pausas da voz sintética caem sempre no mesmo lugar; desloca
            // cada uma para o fundo não silenciar junto com a voz principal.
            for (seed, shift) in [(11, 0.3), (23, 0.8), (37, 1.2), (51, 1.7)] {
                let (v, _) = voice_seeded(len as f32 / SAMPLE_RATE + 2.0, seed);
                let skip = (shift * SAMPLE_RATE) as usize;
                out.iter_mut().zip(&v[skip..]).for_each(|(o, s)| *o += s);
            }
            out
        }
    };
    let r = rms(&x);
    x.iter_mut().for_each(|s| *s /= r);
    x
}

fn diff(x: &[f32]) -> Vec<f32> {
    let mut prev = 0.0;
    x.iter()
        .map(|&s| {
            let d = s - prev;
            prev = s;
            d
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
        .as_chunks::<FRAME>()
        .0
        .iter()
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
    for frame in out.as_chunks_mut::<FRAME>().0.iter_mut() {
        proc.process(frame, p);
    }
    out
}

/// Nível em dBFS das partes com fala e das pausas, ignorando o primeiro segundo
/// (ganho automático e makeup ainda se acomodando) e as bordas das frases.
fn levels(x: &[f32], mask: &[bool]) -> (f32, f32) {
    let skip = (SAMPLE_RATE as usize / FRAME).max(1);
    let (mut sp, mut pa) = (Vec::new(), Vec::new());
    for (i, f) in x.as_chunks::<FRAME>().0.iter().enumerate().skip(skip) {
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
    voice_with(Noise::Pink)
}

/// Voz a ~-30 dBFS com o ruído escolhido 10 dB abaixo.
fn voice_with(kind: Noise) -> (Vec<f32>, Vec<bool>, Vec<f32>) {
    let (mut v, mask) = voice(12.0);
    scale_to(&mut v, &mask, -30.0);
    let g = 10f32.powf(-40.0 / 20.0);
    let mix = v
        .iter()
        .zip(noise(kind, v.len()))
        .map(|(a, b)| a + b * g)
        .collect();
    (v, mask, mix)
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
    for frame in with.as_chunks_mut::<FRAME>().0.iter_mut() {
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

/// Energia abaixo de ~250 Hz e acima de ~3 kHz em relação ao total, em dB, por
/// filtros de um polo: grosseiro, mas suficiente para ver para onde o timbre foi.
fn tilt(x: &[f32]) -> (f32, f32) {
    let (mut lp, mut hp_prev_in, mut hp) = (0.0_f32, 0.0_f32, 0.0_f32);
    let a_lo = 1.0 - (-2.0 * PI * 250.0 / SAMPLE_RATE).exp();
    let a_hi = (-2.0 * PI * 3000.0 / SAMPLE_RATE).exp();
    let (mut lo_e, mut hi_e, mut all) = (0.0_f64, 0.0_f64, 0.0_f64);
    for &s in x {
        lp += a_lo * (s - lp);
        hp = a_hi * (hp + s - hp_prev_in);
        hp_prev_in = s;
        lo_e += (lp * lp) as f64;
        hi_e += (hp * hp) as f64;
        all += (s * s) as f64;
    }
    (
        10.0 * (lo_e / all).log10() as f32,
        10.0 * (hi_e / all).log10() as f32,
    )
}

#[test]
fn eq_presets_move_the_voice_the_way_their_names_say() {
    // Ruído rosa cobre o espectro todo; a voz sintética quase não tem energia
    // acima de 4 kHz e esconderia a banda de agudos (6 kHz).
    let mut v = pink(6 * SAMPLE_RATE as usize);
    let g = 10f32.powf(-30.0 / 20.0) / rms(&v);
    v.iter_mut().for_each(|s| *s *= g);
    // Só o EQ, para medir o atalho e não o resto da cadeia.
    let run = |id: &str| {
        let p = preset(Preset::Bruto);
        p.set_makeup_db(0.0);
        p.apply_eq_preset(id).unwrap();
        tilt(&process(&p, &v))
    };
    let (lo0, hi0) = run("flat");
    for id in ["clear", "warm", "dry"] {
        let (lo, hi) = run(id);
        println!(
            "{id}: graves {:+.1} dB, agudos {:+.1} dB",
            lo - lo0,
            hi - hi0
        );
    }
    let (lo, hi) = run("clear");
    assert!(
        hi - hi0 > 2.0 && lo < lo0,
        "Clarear: agudos {hi0:.1}→{hi:.1}, graves {lo0:.1}→{lo:.1}"
    );
    let (lo, hi) = run("warm");
    assert!(
        lo - lo0 > 1.0 && hi < hi0,
        "Encorpar: graves {lo0:.1}→{lo:.1}, agudos {hi0:.1}→{hi:.1}"
    );
    let (lo, hi) = run("dry");
    assert!(
        lo < lo0 - 1.5 && hi > hi0,
        "Secar: graves {lo0:.1}→{lo:.1}, agudos {hi0:.1}→{hi:.1}"
    );
}

#[test]
#[ignore = "exploração: imprime a separação por ruído e perfil"]
fn explore_noise_separation() {
    for kind in NOISES {
        let (_, mask, mix) = voice_with(kind);
        let (s0, p0) = levels(&mix, &mask);
        let row: Vec<String> = SPEAKING
            .iter()
            .map(|&pr| {
                let (s, p) = levels(&process(&preset(pr), &mix), &mask);
                format!("{pr:?} {:+.1}", (s - p) - (s0 - p0))
            })
            .collect();
        println!("{kind:?}: antes {:.1} dB | {}", s0 - p0, row.join(" | "));
    }
}

#[test]
fn treatment_separates_voice_from_each_kind_of_noise() {
    // Ganho mínimo de separação (dB) por ruído, ~5 dB abaixo do pior perfil medido.
    // Vozes ao fundo são fala para o RNNoise e o gate: o tratamento não as separa
    // (medido -0,4 a -4,6 dB, o ganho automático levanta o fundo). O limite só
    // impede que isso piore.
    let min = |kind| match kind {
        Noise::White | Noise::Pink => 15.0,
        Noise::Brown | Noise::Blue | Noise::Hum => 30.0,
        Noise::Violet => 55.0,
        Noise::Keyboard => 6.0,
        Noise::Babble => -10.0,
    };
    for kind in NOISES {
        let (_, mask, mix) = voice_with(kind);
        let (s0, p0) = levels(&mix, &mask);
        for pr in SPEAKING {
            let (s, p) = levels(&process(&preset(pr), &mix), &mask);
            let gain = (s - p) - (s0 - p0);
            assert!(gain >= min(kind), "{kind:?}/{pr:?}: {gain:+.1} dB");
        }
    }
}

/// Lê o WAV que o MicBridge exporta (PCM 16 bits, mono, 48 kHz).
fn read_wav(path: &str) -> Vec<f32> {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert_eq!(&b[..4], b"RIFF", "{path}: não é WAV");
    let u16at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    assert_eq!(
        (u16at(22), u32at(24), u16at(34)),
        (1, SAMPLE_RATE as u32, 16),
        "{path}: use o WAV original exportado pelo MicBridge (mono, 48 kHz, 16 bits)"
    );
    // Procura o bloco "data" em vez de supor o cabeçalho de 44 bytes.
    let mut i = 12;
    while &b[i..i + 4] != b"data" {
        i += 8 + u32at(i + 4) as usize;
    }
    let end = (i + 8 + u32at(i + 4) as usize).min(b.len());
    b[i + 8..end]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|x| i16::from_le_bytes(*x) as f32 / 32768.0)
        .collect()
}

/// Máscara de fala de uma voz real: frames a menos de 30 dB do mais alto.
fn speech_mask(x: &[f32]) -> Vec<bool> {
    let frames: Vec<f32> = x
        .as_chunks::<FRAME>()
        .0
        .iter()
        .map(|f| db(rms(f)))
        .collect();
    let top = frames.iter().copied().fold(f32::MIN, f32::max);
    frames.iter().map(|&d| d > top - 30.0).collect()
}

/// Mesmas medições da voz sintética, com uma gravação real (só local: a voz de
/// quem grava não vai para o repositório). Grave um teste de voz em silêncio,
/// exporte os WAVs e rode:
///
/// MICBRIDGE_VOICE=voz-original.wav cargo test --release dsp_quality::real_voice \
///   -- --ignored --nocapture
#[test]
#[ignore = "precisa de MICBRIDGE_VOICE apontando para uma gravação real"]
fn real_voice_report() {
    let path = std::env::var("MICBRIDGE_VOICE").expect("defina MICBRIDGE_VOICE");
    let mut v = read_wav(&path);
    v.truncate(v.len() / FRAME * FRAME);
    let mask = speech_mask(&v);
    let (speech, room) = levels(&v, &mask);
    let fala = mask.iter().filter(|m| **m).count() as f32 / mask.len() as f32;
    println!(
        "gravação: {:.1} s, fala {:.0}%, voz {speech:.1} dBFS, ambiente {room:.1} dBFS",
        v.len() as f32 / SAMPLE_RATE,
        fala * 100.0
    );

    println!("\nseparação ganha pelo tratamento (ruído 10 dB abaixo da voz):");
    let g = 10f32.powf((speech - 10.0) / 20.0);
    for kind in NOISES {
        let mix: Vec<f32> = v
            .iter()
            .zip(noise(kind, v.len()))
            .map(|(a, b)| a + b * g)
            .collect();
        let (s0, p0) = levels(&mix, &mask);
        let row: Vec<String> = SPEAKING
            .iter()
            .map(|&pr| {
                let (s, p) = levels(&process(&preset(pr), &mix), &mask);
                format!("{pr:?} {:+.1}", (s - p) - (s0 - p0))
            })
            .collect();
        println!("  {kind:?}: {}", row.join(" | "));
    }

    println!("\natalhos do EQ na sua voz (graves / agudos, em relação ao Plano):");
    let run = |id: &str| {
        let p = preset(Preset::Bruto);
        p.set_makeup_db(0.0);
        p.apply_eq_preset(id).unwrap();
        tilt(&process(&p, &v))
    };
    let (lo0, hi0) = run("flat");
    for (id, label, _) in crate::dsp::EQ_PRESETS.iter().skip(1) {
        let (lo, hi) = run(id);
        println!("  {label}: {:+.1} / {:+.1} dB", lo - lo0, hi - hi0);
    }
}
