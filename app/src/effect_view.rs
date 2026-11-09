use crate::dsp::effects::{EffectReading, EffectSnapshot, GateState};
use std::time::Duration;

#[derive(Debug, PartialEq)]
struct Presentation {
    gate: String,
    compressor: String,
    auto_gain: String,
}
fn presentation(snapshot: EffectSnapshot, enabled: [bool; 3], fresh: bool) -> Presentation {
    let status = |on: bool, value: Option<String>| {
        if !on {
            "Desligado".into()
        } else if !fresh {
            "Sem áudio".into()
        } else {
            value.unwrap_or_else(|| "Sem áudio".into())
        }
    };
    let gate = match snapshot.gate {
        GateState::Disabled => None,
        GateState::Open => Some("Aberto".into()),
        GateState::Holding => Some("Em espera".into()),
        GateState::Attenuating => Some("Atenuando".into()),
    };
    Presentation {
        gate: status(enabled[0], gate),
        compressor: status(
            enabled[1],
            snapshot
                .compression_db
                .filter(|v| v.is_finite())
                .map(|v| format!("Redução: {v:.1} dB").replace('.', ",")),
        ),
        auto_gain: status(
            enabled[2],
            snapshot
                .auto_gain_db
                .filter(|v| v.is_finite())
                .map(|v| format!("Ajuste: {v:+.1} dB").replace('.', ",")),
        ),
    }
}
struct Activity {
    generation: u64,
    frames: u32,
    last_audio: Option<Duration>,
}
impl Activity {
    fn new(generation: u64, frames: u32) -> Self {
        Self {
            generation,
            frames,
            last_audio: None,
        }
    }
    fn observe_effects(
        &mut self,
        reading: EffectReading,
        generation: u64,
        connected: bool,
        now: Duration,
    ) -> bool {
        self.observe(
            generation,
            reading.frames,
            connected && reading.generation == generation,
            now,
        )
    }
    fn observe(&mut self, generation: u64, frames: u32, connected: bool, now: Duration) -> bool {
        if !connected || self.generation != generation {
            *self = Self::new(generation, frames);
            return false;
        }
        if self.frames != frames {
            self.last_audio = Some(now);
        }
        self.frames = frames;
        self.last_audio
            .is_some_and(|last| now.saturating_sub(last) < Duration::from_millis(300))
    }
}

use crate::dsp::Params;
/// Textos de atuação de gate, compressor e volume automático para a interface.
pub struct Readout {
    activity: Activity,
}
impl Readout {
    pub fn new(params: &Params, generation: u64) -> Self {
        Self {
            activity: Activity::new(generation, params.effect_reading().frames),
        }
    }
    pub fn read(
        &mut self,
        params: &Params,
        generation: u64,
        connected: bool,
        now: Duration,
    ) -> [String; 3] {
        use std::sync::atomic::Ordering::Relaxed;
        let reading = params.effect_reading();
        let fresh = self
            .activity
            .observe_effects(reading, generation, connected, now);
        let flags = [&params.gate, &params.compressor, &params.auto_gain].map(|v| v.load(Relaxed));
        let value = presentation(reading.levels, flags, fresh);
        [value.gate, value.compressor, value.auto_gain]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> EffectSnapshot {
        EffectSnapshot {
            gate: GateState::Holding,
            compression_db: Some(4.25),
            auto_gain_db: Some(-2.0),
        }
    }
    #[test]
    fn delayed_old_generation_cannot_refresh_or_overwrite_new_effects() {
        use crate::dsp::{FRAME, Params, Preset, Processor};
        let p = Params::default();
        p.apply_preset(Preset::Bruto);
        p.begin_effects(1);
        p.auto_gain
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut old = Processor::for_generation(&p, 1);
        let mut a = Activity::new(1, 0);
        old.process(&mut [0.1; FRAME], &p);
        assert!(a.observe_effects(p.effect_reading(), 1, true, Duration::from_millis(10)));
        p.begin_effects(2);
        assert!(!a.observe_effects(p.effect_reading(), 2, true, Duration::from_millis(20)));
        let mut late = Processor::for_generation(&p, 1);
        late.process(&mut [0.8; FRAME], &p);
        old.process(&mut [0.2; FRAME], &p);
        assert!(
            !a.observe_effects(p.effect_reading(), 2, true, Duration::from_millis(30)),
            "old packet appeared as fresh generation-2 audio"
        );
        let mut current = Processor::for_generation(&p, 2);
        current.process(&mut [0.3; FRAME], &p);
        assert!(a.observe_effects(p.effect_reading(), 2, true, Duration::from_millis(40)));
        let expected = p.effects();
        old.process(&mut [0.4; FRAME], &p);
        assert_eq!(
            p.effects(),
            expected,
            "old producer overwrote current levels"
        );
        assert!(!a.observe_effects(p.effect_reading(), 2, true, Duration::from_millis(340)));
    }
    #[test]
    fn presentation_distinguishes_disabled_stale_and_measured_effects() {
        let value = presentation(sample(), [true; 3], true);
        assert_eq!(value.gate, "Em espera");
        assert_eq!(value.compressor, "Redução: 4,2 dB");
        assert_eq!(value.auto_gain, "Ajuste: -2,0 dB");
        let disabled = presentation(sample(), [false; 3], true);
        assert_eq!(
            disabled,
            Presentation {
                gate: "Desligado".into(),
                compressor: "Desligado".into(),
                auto_gain: "Desligado".into()
            }
        );
        let stale = presentation(sample(), [true; 3], false);
        assert_eq!(
            stale,
            Presentation {
                gate: "Sem áudio".into(),
                compressor: "Sem áudio".into(),
                auto_gain: "Sem áudio".into()
            }
        );
        for (gate, label) in [
            (GateState::Open, "Aberto"),
            (GateState::Attenuating, "Atenuando"),
            (GateState::Disabled, "Sem áudio"),
        ] {
            let s = EffectSnapshot {
                gate,
                compression_db: Some(0.0),
                auto_gain_db: Some(3.0),
            };
            let p = presentation(s, [true; 3], true);
            assert_eq!(p.gate, label);
            assert_eq!(p.compressor, "Redução: 0,0 dB");
            assert_eq!(p.auto_gain, "Ajuste: +3,0 dB");
        }
        let p = presentation(EffectSnapshot::default(), [true; 3], true);
        assert_eq!(p.compressor, "Sem áudio");
        assert_eq!(p.auto_gain, "Sem áudio");
    }
    #[test]
    fn freshness_expires_and_reconnection_requires_new_frames() {
        let ms = Duration::from_millis;
        let mut a = Activity::new(1, 10);
        assert!(
            !a.observe(1, 10, true, ms(0)),
            "old metrics must not appear at startup"
        );
        assert!(a.observe(1, 11, true, ms(50)));
        assert!(a.observe(1, 11, true, ms(349)));
        assert!(!a.observe(1, 11, true, ms(350)));
        assert!(!a.observe(1, 12, false, ms(400)));
        assert!(!a.observe(1, 12, true, ms(450)));
        assert!(a.observe(1, 13, true, ms(500)));
        assert!(!a.observe(2, 14, true, ms(550)));
        assert!(a.observe(2, 15, true, ms(600)));
        let mut wrap = Activity::new(1, u32::MAX);
        assert!(wrap.observe(1, 0, true, ms(0)));
    }
}
