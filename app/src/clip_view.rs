//! Aviso de saturação: transforma os contadores do DSP em "CLIP" aceso por
//! alguns segundos, para dar tempo de ver o aviso depois de um pico curto.

use crate::dsp::Params;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// Quanto tempo o aviso fica aceso depois do último frame saturado.
pub const HOLD: Duration = Duration::from_secs(3);

/// Lembra a última vez que um contador mudou.
#[derive(Default)]
struct Hold {
    count: Option<u32>,
    last: Option<Duration>,
}

impl Hold {
    fn observe(&mut self, count: u32, now: Duration) -> bool {
        // A primeira leitura só registra o valor: contagem antiga não é um pico novo.
        if self.count.is_some_and(|c| c != count) {
            self.last = Some(now);
        }
        self.count = Some(count);
        self.last.is_some_and(|t| now.saturating_sub(t) < HOLD)
    }
}

/// Estado do aviso de entrada (celular) e de saída (limitador).
#[derive(Default)]
pub struct Clip {
    input: Hold,
    output: Hold,
}

impl Clip {
    pub fn read(&mut self, p: &Params, now: Duration) -> [bool; 2] {
        [
            self.input.observe(p.in_clip_frames.load(Relaxed), now),
            self.output.observe(p.out_limit_frames.load(Relaxed), now),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{FRAME, Processor};

    fn s(secs: f32) -> Duration {
        Duration::from_secs_f32(secs)
    }

    #[test]
    fn hold_lights_on_change_and_turns_off_after_hold() {
        let mut h = Hold::default();
        assert!(!h.observe(5, s(0.0)), "contagem antiga não acende");
        assert!(!h.observe(5, s(1.0)));
        assert!(h.observe(6, s(1.5)));
        assert!(h.observe(6, s(4.0)));
        assert!(!h.observe(6, s(4.6)));
    }

    #[test]
    fn hot_input_and_heavy_limiting_light_both_warnings() {
        let p = Params::default();
        p.apply_preset(crate::dsp::Preset::Bruto);
        p.set_makeup_db(24.0);
        let mut proc = Processor::new();
        let mut clip = Clip::default();
        assert_eq!(clip.read(&p, s(0.0)), [false, false]);

        // Senoide a 0 dBFS: o celular saturou e o ganho alto esmaga a saída.
        for n in 0..50 {
            let mut frame: Vec<f32> = (0..FRAME)
                .map(|i| ((n * FRAME + i) as f32 * 0.05).sin())
                .collect();
            proc.process(&mut frame, &p);
        }
        assert_eq!(clip.read(&p, s(0.5)), [true, true]);
        assert_eq!(clip.read(&p, s(5.0)), [false, false]);
    }

    #[test]
    fn normal_speech_level_lights_nothing() {
        let p = Params::default();
        p.apply_preset(crate::dsp::Preset::Bruto);
        p.set_makeup_db(0.0);
        let mut proc = Processor::new();
        let mut clip = Clip::default();
        clip.read(&p, s(0.0));
        for n in 0..50 {
            let mut frame: Vec<f32> = (0..FRAME)
                .map(|i| 0.1 * ((n * FRAME + i) as f32 * 0.05).sin())
                .collect();
            proc.process(&mut frame, &p);
        }
        assert_eq!(clip.read(&p, s(0.5)), [false, false]);
    }
}
