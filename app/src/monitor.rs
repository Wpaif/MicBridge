//! Retorno local da voz tratada.
pub mod devices;

pub mod gain;
use devices::OutputDevice;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::SeqCst},
    mpsc,
};
pub type MonitorId = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorPhase {
    Disabled,
    Opening,
    Active,
    Failed,
}
#[derive(Clone, Debug)]
pub struct MonitorStatus {
    pub id: u64,
    pub phase: MonitorPhase,
    pub target: Option<OutputDevice>,
    pub generation: Option<u64>,
    pub volume_percent: f32,
    pub error: Option<String>,
}
struct Block {
    id: u64,
    generation: u64,
    samples: Vec<f32>,
}
use std::sync::{
    Mutex,
    atomic::{AtomicU32, AtomicU64},
};
struct Inner {
    tx: mpsc::SyncSender<Block>,
    gate: AtomicU64,
    generation: AtomicU64,
    failed: AtomicU64,
    volume: AtomicU32,
    status: Mutex<MonitorStatus>,
    cancel: tokio::sync::Notify,
}
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
}
impl Monitor {
    pub fn new() -> Self {
        Self::with_channel().0
    }
    fn with_channel() -> (Self, mpsc::Receiver<Block>) {
        let (tx, rx) = mpsc::sync_channel(6);
        (
            Self {
                inner: Arc::new(Inner {
                    tx,
                    gate: AtomicU64::new(0),
                    generation: AtomicU64::new(0),
                    failed: AtomicU64::new(0),
                    volume: AtomicU32::new(25f32.to_bits()),
                    status: Mutex::new(MonitorStatus {
                        id: 0,
                        phase: MonitorPhase::Disabled,
                        target: None,
                        generation: None,
                        volume_percent: 25.0,
                        error: None,
                    }),
                    cancel: tokio::sync::Notify::new(),
                }),
            },
            rx,
        )
    }
    fn reserve(&self, id: u64, generation: u64, target: OutputDevice) {
        let mut status = self.inner.status.lock().unwrap();
        self.inner.gate.store(0, SeqCst);
        self.inner.generation.store(generation, SeqCst);
        *status = MonitorStatus {
            id,
            phase: MonitorPhase::Opening,
            target: Some(target),
            generation: Some(generation),
            volume_percent: self.volume(),
            error: None,
        };
        self.inner.gate.store(id << 1, SeqCst);
    }
    fn publish(&self, id: u64, lease: &AtomicBool) -> bool {
        lease.load(SeqCst)
            && self
                .inner
                .gate
                .compare_exchange(id << 1, id << 1 | 1, SeqCst, SeqCst)
                .is_ok()
    }
    pub fn status(&self) -> MonitorStatus {
        let mut s = self.inner.status.lock().unwrap().clone();
        s.volume_percent = self.volume();
        let gate = self.inner.gate.load(SeqCst);
        if s.phase != MonitorPhase::Disabled {
            if self.inner.failed.load(SeqCst) == s.id {
                s.phase = MonitorPhase::Failed;
                if s.error.is_none() {
                    s.error=Some("Retorno interrompido: não acompanhou o áudio ou recebeu um bloco incompatível.".into());
                }
            } else if gate == s.id << 1 | 1 {
                s.phase = MonitorPhase::Active;
            } else if gate == 0 {
                s.phase = MonitorPhase::Disabled;
            }
        }
        s
    }
    fn volume(&self) -> f32 {
        f32::from_bits(self.inner.volume.load(SeqCst))
    }
    pub fn set_volume(&self, percent: f32) {
        if percent.is_finite() {
            self.inner
                .volume
                .store(percent.clamp(0.0, 100.0).to_bits(), SeqCst);
        }
    }
    pub fn active_id(&self, generation: u64) -> Option<u64> {
        let gate = self.inner.gate.load(SeqCst);
        (gate & 1 == 1
            && self.inner.generation.load(SeqCst) == generation
            && self.inner.gate.load(SeqCst) == gate)
            .then_some(gate >> 1)
    }
    pub fn push(&self, id: u64, generation: u64, samples: &[f32]) {
        if self.active_id(generation) != Some(id) {
            return;
        }
        if samples.len() > 480 {
            self.fail(id);
            return;
        }
        if samples.is_empty() {
            return;
        }
        if self
            .inner
            .tx
            .try_send(Block {
                id,
                generation,
                samples: samples.to_vec(),
            })
            .is_err()
        {
            self.fail(id);
        }
    }
    fn fail(&self, id: u64) {
        if self
            .inner
            .gate
            .compare_exchange(id << 1 | 1, 0, SeqCst, SeqCst)
            .is_ok()
        {
            self.inner.failed.fetch_max(id, SeqCst);
            self.inner.cancel.notify_one();
        }
    }
    pub fn disable(&self) {
        self.inner.gate.store(0, SeqCst);
        self.inner.cancel.notify_one();
        let mut s = self.inner.status.lock().unwrap();
        s.phase = MonitorPhase::Disabled;
        s.error = None;
    }
    fn valid(&self, block: &Block) -> bool {
        self.active_id(block.generation) == Some(block.id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> OutputDevice {
        OutputDevice {
            id: 1,
            serial: Some("101".into()),
            name: "sink".into(),
            label: "Fone".into(),
        }
    }
    #[test]
    fn bounded_queue_stops_monitor_without_waiting_for_consumer() {
        let (m, rx) = Monitor::with_channel();
        m.reserve(1, 7, target());
        assert!(m.publish(1, &AtomicBool::new(true)));
        let (tx, done) = mpsc::channel();
        let producer = m.clone();
        let thread = std::thread::spawn(move || {
            for _ in 0..7 {
                producer.push(1, 7, &[0.1; 480]);
            }
            let _ = tx.send(());
        });
        assert!(
            done.recv_timeout(std::time::Duration::from_secs(1)).is_ok(),
            "producer waited for consumer"
        );
        thread.join().unwrap();
        assert_eq!(m.status().phase, MonitorPhase::Failed);
        assert!(m.status().error.is_some());
        assert!(m.active_id(7).is_none());
        assert_eq!(rx.try_iter().count(), 6);
    }
    #[test]
    fn stale_sessions_and_generations_cannot_queue_or_play() {
        let (m, rx) = Monitor::with_channel();
        m.reserve(1, 1, target());
        m.publish(1, &AtomicBool::new(true));
        m.push(1, 1, &[0.1]);
        let old = rx.try_recv().unwrap();
        m.disable();
        assert!(!m.valid(&old));
        m.reserve(2, 2, target());
        assert!(m.publish(2, &AtomicBool::new(true)));
        m.push(1, 1, &[0.2]);
        m.push(2, 1, &[0.3]);
        m.push(2, 2, &[0.4]);
        let blocks: Vec<_> = rx.try_iter().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].samples, [0.4]);
        assert!(!m.valid(&old));
        m.disable();
        assert!(!m.valid(&blocks[0]));
    }
    #[test]
    fn cancelled_open_does_not_become_active_and_volume_is_session_local() {
        let (m, _) = Monitor::with_channel();
        assert_eq!(m.status().volume_percent, 25.0);
        m.set_volume(75.0);
        m.set_volume(f32::NAN);
        m.reserve(1, 1, target());
        m.disable();
        assert!(!m.publish(1, &AtomicBool::new(true)));
        m.reserve(2, 1, target());
        assert!(!m.publish(2, &AtomicBool::new(false)));
        assert_eq!(m.status().volume_percent, 75.0);
        assert_eq!(Monitor::new().status().phase, MonitorPhase::Disabled);
        assert_eq!(Monitor::new().status().volume_percent, 25.0);
    }
}
