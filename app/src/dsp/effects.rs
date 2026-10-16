//! Telemetria informativa; não participa dos cálculos de áudio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GateState {
    #[default]
    Disabled,
    Open,
    Holding,
    Attenuating,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EffectSnapshot {
    pub gate: GateState,
    pub compression_db: Option<f32>,
    pub auto_gain_db: Option<f32>,
}

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
/// Uma única leitura atômica, sem mutex no DSP. Ganhos quantizados em 0,01 dB
/// somente para exibição; precisão e amostras do processamento permanecem intactas.
#[derive(Default)]
pub(super) struct EffectMeter(AtomicU64);
impl EffectMeter {
    pub fn publish(&self, value: EffectSnapshot) {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let comp =
            (finite(value.compression_db.unwrap_or(0.0)).clamp(0.0, 327.67) * 100.0).round() as u64;
        let agc = (finite(value.auto_gain_db.unwrap_or(0.0)).clamp(-327.68, 327.67) * 100.0).round()
            as i16;
        let gate = match value.gate {
            GateState::Disabled => 0,
            GateState::Open => 1,
            GateState::Holding => 2,
            GateState::Attenuating => 3,
        };
        self.0.store(
            gate | (comp << 2)
                | ((agc as u16 as u64) << 17)
                | ((value.compression_db.is_some() as u64) << 33)
                | ((value.auto_gain_db.is_some() as u64) << 34),
            Relaxed,
        );
    }
    pub fn snapshot(&self) -> EffectSnapshot {
        let bits = self.0.load(Relaxed);
        EffectSnapshot {
            gate: match bits & 3 {
                1 => GateState::Open,
                2 => GateState::Holding,
                3 => GateState::Attenuating,
                _ => GateState::Disabled,
            },
            compression_db: (bits & (1 << 33) != 0)
                .then_some(((bits >> 2) & 0x7fff) as f32 / 100.0),
            auto_gain_db: (bits & (1 << 34) != 0)
                .then_some(((bits >> 17) as u16 as i16) as f32 / 100.0),
        }
    }
}
