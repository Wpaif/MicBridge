//! Retorno local da voz tratada.
pub mod devices;

pub mod gain;
mod worker;
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
enum Control {
    Stop {
        operation: u64,
    },
    Open {
        operation: u64,
        id: u64,
        generation: u64,
        target: OutputDevice,
        lease: Arc<AtomicBool>,
    },
}
struct ControlHandle {
    tx: tokio::sync::watch::Sender<Control>,
    next: AtomicU64,
    settled: tokio::sync::watch::Receiver<u64>,
    transaction: Mutex<()>,
}
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
    control: Arc<ControlHandle>,
}
impl Monitor {
    pub(crate) fn with_executable(executable: std::path::PathBuf) -> Self {
        let (mut m, rx) = Self::with_channel();
        let (ack, settled) = tokio::sync::watch::channel(0u64);
        Arc::get_mut(&mut m.control).expect("controle novo").settled = settled;
        worker::spawn(
            m.inner.clone(),
            rx,
            m.control.tx.subscribe(),
            ack,
            executable,
        );
        m
    }
    pub fn open(&self, id: u64, generation: u64, target: OutputDevice, lease: Arc<AtomicBool>) {
        let _transaction = self.control.transaction.lock().unwrap();
        if !lease.load(SeqCst) {
            return;
        }
        self.reserve(id, generation, target.clone());
        let operation = self.control.next.fetch_add(1, SeqCst) + 1;
        self.control.tx.send_replace(Control::Open {
            operation,
            id,
            generation,
            target,
            lease,
        });
    }
    pub fn stop_and_wait(
        &self,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
        let _transaction = self.control.transaction.lock().unwrap();
        self.disable();
        let operation = self.control.next.fetch_add(1, SeqCst) + 1;
        let mut settled = self.control.settled.clone();
        self.control.tx.send_replace(Control::Stop { operation });
        async move {
            loop {
                if *settled.borrow_and_update() >= operation {
                    return Ok(());
                }
                settled.changed().await.map_err(|_| {
                    "Worker de retorno encerrou antes de confirmar parada".to_string()
                })?;
            }
        }
    }
    pub fn new() -> Self {
        Self::with_executable("pw-cat".into())
    }
    fn with_channel() -> (Self, mpsc::Receiver<Block>) {
        let (tx, rx) = mpsc::sync_channel(6);
        let (commands, _) = tokio::sync::watch::channel(Control::Stop { operation: 0 });
        let (_, settled) = tokio::sync::watch::channel(0u64);
        (
            Self {
                control: Arc::new(ControlHandle {
                    tx: commands,
                    next: AtomicU64::new(0),
                    settled,
                    transaction: Mutex::new(()),
                }),
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
    pub(crate) fn prepare(&self, id: u64, generation: u64, target: OutputDevice) {
        self.reserve(id, generation, target);
        self.inner.cancel.notify_one();
    }
    pub(crate) fn report_failure(&self, id: u64, error: String) {
        worker::fail(&self.inner, id, error);
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
pub fn submit_processed(monitor: &Monitor, generation: u64, pcm: &[f32]) {
    let Some(id) = monitor.active_id(generation) else {
        return;
    };
    for block in pcm.chunks(480) {
        monitor.push(id, generation, block);
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
    fn processed_audio_is_unchanged_with_monitoring_and_large_packet_failure_is_local() {
        for n in [480, 960, 5760, 120] {
            let params = crate::dsp::Params::default();
            params.apply_preset(crate::dsp::Preset::Bruto);
            let original = vec![0.01; n];
            let mut pcm = original.clone();
            let mut reference = original.clone();
            let rec = crate::recording::Recorder::new();
            let take_id = rec.start(7);
            let (m, queue) = Monitor::with_channel();
            m.reserve(1, 7, target());
            m.publish(1, &AtomicBool::new(true));
            crate::recording::process_block(
                &mut crate::dsp::Processor::new(),
                &mut pcm,
                &params,
                &rec,
                7,
            );
            submit_processed(&m, 7, &pcm);
            crate::recording::process_block(
                &mut crate::dsp::Processor::new(),
                &mut reference,
                &params,
                &crate::recording::Recorder::new(),
                7,
            );
            assert_eq!(pcm, reference);
            rec.stop(take_id);
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while rec.status().state == crate::recording::TakeState::Recording
                && std::time::Instant::now() < end
            {
                std::thread::yield_now();
            }
            let take = rec.take(take_id).unwrap();
            assert_eq!(take.original, original);
            assert_eq!(take.treated, pcm);
            let blocks: Vec<_> = queue.try_iter().collect();
            if n == 5760 {
                assert_eq!(m.status().phase, MonitorPhase::Failed);
                assert_eq!(blocks.len(), 6);
            } else {
                assert_eq!(
                    blocks
                        .iter()
                        .flat_map(|b| b.samples.clone())
                        .collect::<Vec<_>>(),
                    pcm
                );
            }
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
#[cfg(test)]
mod worker_tests {
    use super::*;
    use crate::audio_test::{Fixture, SLEEP, alive, wait};
    use std::time::Duration;
    fn target() -> OutputDevice {
        OutputDevice {
            id: 1,
            serial: Some("11".into()),
            name: "headphones.target".into(),
            label: "Fone".into(),
        }
    }
    #[tokio::test]
    async fn monitor_worker_sends_explicit_target_and_scaled_pcm() {
        let f = Fixture::new("printf '%s\\n' \"$@\" > '@DIR@/args'; cat > '@DIR@/pcm'");
        let m = Monitor::with_executable(f.executable.clone());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase == MonitorPhase::Active);
        m.push(1, 7, &[0.4, 0.8]);
        wait(|| std::fs::metadata(f.dir.join("pcm")).is_ok_and(|m| m.len() == 8));
        m.stop_and_wait().await.unwrap();
        assert_eq!(
            std::fs::read(f.dir.join("pcm")).unwrap(),
            [0.1f32.to_le_bytes(), 0.2f32.to_le_bytes()].concat()
        );
        assert_eq!(
            std::fs::read_to_string(f.dir.join("args"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "--playback",
                "--raw",
                "--target",
                "headphones.target",
                "--rate",
                "48000",
                "--channels",
                "1",
                "--format",
                "f32",
                "--latency",
                "20ms",
                "--media-role",
                "Music",
                "-"
            ]
        );
    }
    #[tokio::test]
    async fn coalesced_stop_acknowledges_child_reaping() {
        let f = Fixture::new(SLEEP);
        let m = Monitor::with_executable(f.executable.clone());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| f.pid().is_some());
        let old = f.pid().unwrap();
        let stopped = m.stop_and_wait();
        m.open(2, 8, target(), Arc::new(AtomicBool::new(true)));
        tokio::time::timeout(Duration::from_secs(2), stopped)
            .await
            .unwrap()
            .unwrap();
        assert!(!alive(old));
        m.stop_and_wait().await.unwrap();
    }
    #[tokio::test]
    async fn monitor_stop_ack_reaps_blocked_child_and_cancels_pending_audio() {
        let f = Fixture::new(SLEEP);
        let m = Monitor::with_executable(f.executable.clone());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| f.pid().is_some());
        let pid = f.pid().unwrap();
        for _ in 0..6 {
            m.push(1, 7, &[0.2; 480]);
        }
        tokio::time::timeout(Duration::from_secs(2), m.stop_and_wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!alive(pid));
        assert!(m.active_id(7).is_none());
    }
    #[tokio::test]
    async fn monitor_errors_do_not_reactivate_or_get_overwritten_by_late_sessions() {
        for script in ["exit 7", "exec 0<&-; exit 8"] {
            let f = Fixture::new(script);
            let m = Monitor::with_executable(f.executable.clone());
            m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
            wait(|| m.status().phase == MonitorPhase::Failed);
            assert!(m.status().error.is_some());
            assert!(m.active_id(7).is_none());
            m.stop_and_wait().await.unwrap();
        }
        let m = Monitor::with_executable("/nonexistent/micbridge-player".into());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase == MonitorPhase::Failed);
        m.stop_and_wait().await.unwrap();
        let f = Fixture::new("cat > '@DIR@/pcm'");
        let m = Monitor::with_executable(f.executable.clone());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(false)));
        m.stop_and_wait().await.unwrap();
        assert!(!f.dir.join("pcm").exists());
        m.open(2, 8, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.active_id(8) == Some(2));
        assert_eq!(m.status().id, 2);
        assert!(m.status().error.is_none());
        m.stop_and_wait().await.unwrap();
    }
}
