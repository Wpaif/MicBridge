use crate::dsp::effects::{EffectSnapshot, GateState};
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

use crate::dsp::Params;
use adw::prelude::*;
pub struct Indicators {
    rows: [glib::WeakRef<adw::SwitchRow>; 3],
    activity: Activity,
}
impl Indicators {
    pub fn new(params: &Params, generation: u64, rows: [&adw::SwitchRow; 3]) -> Self {
        let mut result = Self {
            rows: rows.map(|r| r.downgrade()),
            activity: Activity::new(generation, params.frames()),
        };
        result.update(params, generation, false, Duration::ZERO);
        result
    }
    pub fn update(&mut self, params: &Params, generation: u64, connected: bool, now: Duration) {
        use std::sync::atomic::Ordering::Relaxed;
        let fresh = self
            .activity
            .observe(generation, params.frames(), connected, now);
        let flags = [&params.gate, &params.compressor, &params.auto_gain].map(|v| v.load(Relaxed));
        let value = presentation(params.effects(), flags, fresh);
        for (row, subtitle) in self
            .rows
            .iter()
            .zip([value.gate, value.compressor, value.auto_gain])
        {
            if let Some(row) = row.upgrade() {
                if row.subtitle().as_deref() != Some(subtitle.as_str()) {
                    row.set_subtitle(&subtitle);
                }
            }
        }
    }
}
#[cfg(test)]
mod graphical_tests {
    use super::*;
    use crate::dsp::{FRAME, Preset, Processor};
    use std::sync::atomic::Ordering::Relaxed;
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn indicators_show_processed_audio_bypass_staleness_and_release_widgets() {
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.EffectTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::builder().application(&app).build();
        let group = adw::PreferencesGroup::new();
        let rows: [adw::SwitchRow; 3] = std::array::from_fn(|_| adw::SwitchRow::new());
        for r in &rows {
            group.add(r);
        }
        parent.set_content(Some(&group));
        parent.present();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !parent.is_realized() {
            glib::MainContext::default().iteration(false);
            assert!(
                std::time::Instant::now() < deadline,
                "window did not realize"
            );
        }
        let p = Params::default();
        p.apply_preset(Preset::Bruto);
        p.gate.store(true, Relaxed);
        p.compressor.store(true, Relaxed);
        p.auto_gain.store(true, Relaxed);
        let mut indicators = Indicators::new(&p, 1, [&rows[0], &rows[1], &rows[2]]);
        assert_eq!(rows[0].subtitle().as_deref(), Some("Sem áudio"));
        let mut processor = Processor::new();
        processor.process(&mut [0.5; FRAME], &p);
        indicators.update(&p, 1, true, Duration::from_millis(50));
        assert_eq!(rows[0].subtitle().as_deref(), Some("Aberto"));
        assert!(rows[1].subtitle().unwrap().starts_with("Redução: "));
        assert!(rows[2].subtitle().unwrap().starts_with("Ajuste: -"));
        p.gate.store(false, Relaxed);
        indicators.update(&p, 1, true, Duration::from_millis(100));
        assert_eq!(rows[0].subtitle().as_deref(), Some("Desligado"));
        indicators.update(&p, 1, true, Duration::from_millis(350));
        assert_eq!(rows[1].subtitle().as_deref(), Some("Sem áudio"));
        let weak = rows[0].downgrade();
        parent.set_content(None::<&gtk::Widget>);
        parent.destroy();
        drop(group);
        drop(rows);
        drop(parent);
        assert!(weak.upgrade().is_none(), "telemetry retained its widget");
        indicators.update(&p, 1, false, Duration::from_millis(400));
    }
}
