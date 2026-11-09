//! Bancada do DSP: processa um áudio gravado (f32 mono 48 kHz, cru) com várias
//! configurações, para comparar original e tratado com a mesma entrada.
//!
//!   MICBRIDGE_BENCH_IN=entrada.f32 MICBRIDGE_BENCH_OUT=pasta \
//!     cargo test dsp_bench -- --ignored --nocapture
use crate::dsp::{FRAME, Params, Preset, Processor};
use std::sync::atomic::Ordering::Relaxed;

fn configs() -> Vec<(&'static str, Params)> {
    let mut out = Vec::new();
    let bruto = Params::default();
    bruto.apply_preset(Preset::Bruto);
    bruto.set_makeup_db(0.0);
    out.push(("0-bruto", bruto));
    for preset in [
        Preset::Natural,
        Preset::Chamada,
        Preset::Podcast,
        Preset::Streaming,
    ] {
        let p = Params::default();
        p.apply_preset(preset);
        out.push((preset.key(), p));
    }
    // Chamada sem compressor: se sair igual à Chamada, o compressor não atuou.
    let nocomp = Params::default();
    nocomp.apply_preset(Preset::Chamada);
    nocomp.compressor.store(false, Relaxed);
    out.push(("chamada-sem-comp", nocomp));
    // Chamada com EQ "voz encorpada e menos chiado": graves +4, agudos -3.
    let eq = Params::default();
    eq.apply_preset(Preset::Chamada);
    eq.eq.store(true, Relaxed);
    eq.set_eq_db(0, 4.0);
    eq.set_eq_db(2, -3.0);
    out.push(("chamada+eq", eq));
    out
}

#[test]
#[ignore = "bancada manual: precisa de MICBRIDGE_BENCH_IN"]
fn dsp_bench() {
    let input = std::env::var("MICBRIDGE_BENCH_IN").expect("MICBRIDGE_BENCH_IN");
    let dir = std::path::PathBuf::from(
        std::env::var("MICBRIDGE_BENCH_OUT").expect("MICBRIDGE_BENCH_OUT"),
    );
    let bytes = std::fs::read(input).unwrap();
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    for (name, params) in configs() {
        let mut proc = Processor::new();
        let mut buf = samples.clone();
        for frame in buf.chunks_exact_mut(FRAME) {
            proc.process(frame, &params);
        }
        let raw: Vec<u8> = buf.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(dir.join(format!("{name}.f32")), raw).unwrap();
        println!("{name}: {} amostras", buf.len());
    }
}
