//! Retorno local da voz tratada.
pub mod devices;
pub mod routing;

pub mod gain;
mod worker;
use devices::OutputDevice;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::SeqCst},
    mpsc,
};
pub type MonitorId = u64;
// Opus can deliver 120 ms at once (5760 mono samples at 48 kHz).
// Keep two maximum packets of scheduling headroom, bounded to 240 ms;
// a full queue still stops the return without blocking the main audio pump.
const QUEUE_BLOCKS: usize = 24;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorPhase {
    Disabled,
    Opening,
    Active,
    Restoring,
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
    pub restore_report: Option<routing::RestoreReport>,
    pub restore_pending: bool,
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
#[derive(Clone, Debug, Default)]
struct Settlement {
    operation: u64,
    error: Option<String>,
}
struct ControlHandle {
    tx: tokio::sync::watch::Sender<Control>,
    next: AtomicU64,
    settled: tokio::sync::watch::Receiver<Settlement>,
    transaction: Mutex<()>,
    lease: Mutex<Option<Arc<AtomicBool>>>,
}
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
    control: Arc<ControlHandle>,
    routing: Option<routing::Routing>,
}
impl Monitor {
    #[cfg(test)]
    pub(crate) fn with_test_routing(
        executable: std::path::PathBuf,
        routing: routing::Routing,
    ) -> Self {
        Self::build(
            executable,
            routing,
            Arc::new(|_| "micbridge_monitor_test".into()),
        )
    }
    #[cfg(test)]
    pub(crate) fn with_executable(executable: std::path::PathBuf) -> Self {
        Self::with_services(executable, routing::Routing::bypass())
    }
    pub(crate) fn with_services(executable: std::path::PathBuf, routing: routing::Routing) -> Self {
        Self::build(
            executable,
            routing,
            Arc::new(|id| {
                format!(
                    "micbridge_monitor_{}_{}_{}",
                    std::process::id(),
                    id,
                    crate::state::random_hex(16)
                )
            }),
        )
    }
    fn build(
        executable: std::path::PathBuf,
        routing: routing::Routing,
        names: Arc<dyn Fn(u64) -> String + Send + Sync>,
    ) -> Self {
        let (mut m, rx) = Self::with_channel();
        let (ack, settled) = tokio::sync::watch::channel(Settlement::default());
        Arc::get_mut(&mut m.control).expect("controle novo").settled = settled;
        m.routing = Some(routing.clone());
        worker::spawn(
            m.inner.clone(),
            rx,
            m.control.tx.subscribe(),
            ack,
            executable,
            routing,
            names,
        );
        m
    }
    pub fn open(&self, id: u64, generation: u64, target: OutputDevice, lease: Arc<AtomicBool>) {
        let _transaction = self.control.transaction.lock().unwrap();
        if !lease.load(SeqCst) {
            return;
        }
        if let Some(old) = self.control.lease.lock().unwrap().replace(lease.clone()) {
            if !Arc::ptr_eq(&old, &lease) {
                old.store(false, SeqCst);
            }
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
                let ack = settled.borrow_and_update().clone();
                if ack.operation >= operation {
                    return ack.error.map_or(Ok(()), Err);
                }
                settled.changed().await.map_err(|_| {
                    "Worker de retorno encerrou antes de confirmar parada".to_string()
                })?;
            }
        }
    }
    pub fn new() -> Self {
        #[cfg(test)]
        {
            Self::with_executable("pw-cat".into())
        }
        #[cfg(not(test))]
        {
            Self::with_services(
                "pw-cat".into(),
                routing::Routing::new(
                    Arc::new(routing::backend::RealBackend::default()),
                    crate::state::data_dir().join("monitor-routing.json"),
                ),
            )
        }
    }
    pub fn retry_restore(
        &self,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
        self.stop_and_wait()
    }
    fn with_channel() -> (Self, mpsc::Receiver<Block>) {
        let (tx, rx) = mpsc::sync_channel(QUEUE_BLOCKS);
        let (commands, _) = tokio::sync::watch::channel(Control::Stop { operation: 0 });
        let (_, settled) = tokio::sync::watch::channel(Settlement::default());
        (
            Self {
                control: Arc::new(ControlHandle {
                    tx: commands,
                    next: AtomicU64::new(0),
                    settled,
                    transaction: Mutex::new(()),
                    lease: Mutex::new(None),
                }),
                routing: None,
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
                        restore_report: None,
                        restore_pending: false,
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
            restore_report: None,
            restore_pending: false,
        };
        self.inner.gate.store(id << 1, SeqCst);
    }
    fn publish(inner: &Inner, id: u64, lease: &AtomicBool) -> bool {
        lease.load(SeqCst)
            && inner
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
            } else if gate == 0 && s.phase != MonitorPhase::Restoring {
                s.phase = MonitorPhase::Disabled;
            }
        }
        if let Some(routing) = &self.routing {
            let route = routing.status();
            s.restore_report = Some(route.report.clone());
            s.restore_pending = route.report.pending > 0;
            if route.busy && s.phase != MonitorPhase::Opening {
                s.phase = MonitorPhase::Restoring;
            }
            if s.restore_pending {
                if !route.busy && s.phase != MonitorPhase::Restoring {
                    s.phase = MonitorPhase::Failed;
                }
                s.error = route
                    .report
                    .error
                    .or_else(|| Some("Conexões aguardam restauração".into()));
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
        if let Some(lease) = self.control.lease.lock().unwrap().take() {
            lease.store(false, SeqCst);
        }
        self.inner.gate.store(0, SeqCst);
        self.inner.cancel.notify_one();
        let mut s = self.inner.status.lock().unwrap();
        s.phase = if self.routing.as_ref().is_some_and(|r| r.requires_graph()) {
            MonitorPhase::Restoring
        } else {
            MonitorPhase::Disabled
        };
        s.error = None;
    }
    fn valid(inner: &Inner, block: &Block) -> bool {
        let gate = inner.gate.load(SeqCst);
        gate == block.id << 1 | 1
            && inner.generation.load(SeqCst) == block.generation
            && inner.gate.load(SeqCst) == gate
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
    fn processed_audio_is_unchanged_and_every_valid_opus_packet_reaches_monitor() {
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
            Monitor::publish(&m.inner, 1, &AtomicBool::new(true));
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
            assert_eq!(
                m.status().phase,
                MonitorPhase::Active,
                "valid packet: {n} samples"
            );
            assert_eq!(
                blocks
                    .iter()
                    .flat_map(|b| b.samples.clone())
                    .collect::<Vec<_>>(),
                pcm
            );
        }
    }
    #[test]
    fn two_maximum_opus_packets_survive_consumer_scheduling_delay() {
        // The real decoder accepts 5760 mono samples (120 ms) per RTP packet.
        // Neither packet may depend on the worker running between 10 ms chunks.
        let (m, rx) = Monitor::with_channel();
        m.reserve(1, 7, target());
        assert!(Monitor::publish(&m.inner, 1, &AtomicBool::new(true)));
        let first = vec![0.1; 5760];
        let second = vec![0.2; 5760];
        submit_processed(&m, 7, &first);
        submit_processed(&m, 7, &second);
        assert_eq!(m.status().phase, MonitorPhase::Active);
        let actual: Vec<_> = rx.try_iter().flat_map(|b| b.samples).collect();
        assert_eq!(actual, [first, second].concat());
        // Once consumed, subsequent packets must still be accepted.
        submit_processed(&m, 7, &[0.3; 960]);
        assert_eq!(
            rx.try_iter().flat_map(|b| b.samples).collect::<Vec<_>>(),
            vec![0.3; 960]
        );
    }
    #[test]
    fn bounded_queue_stops_monitor_without_waiting_for_consumer() {
        let (m, rx) = Monitor::with_channel();
        m.reserve(1, 7, target());
        assert!(Monitor::publish(&m.inner, 1, &AtomicBool::new(true)));
        let (tx, done) = mpsc::channel();
        let producer = m.clone();
        let thread = std::thread::spawn(move || {
            for _ in 0..25 {
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
        assert_eq!(rx.try_iter().count(), 24);
    }
    #[test]
    fn stale_sessions_and_generations_cannot_queue_or_play() {
        let (m, rx) = Monitor::with_channel();
        m.reserve(1, 1, target());
        Monitor::publish(&m.inner, 1, &AtomicBool::new(true));
        m.push(1, 1, &[0.1]);
        let old = rx.try_recv().unwrap();
        m.disable();
        assert!(!Monitor::valid(&m.inner, &old));
        m.reserve(2, 2, target());
        assert!(Monitor::publish(&m.inner, 2, &AtomicBool::new(true)));
        m.push(1, 1, &[0.2]);
        m.push(2, 1, &[0.3]);
        m.push(2, 2, &[0.4]);
        let blocks: Vec<_> = rx.try_iter().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].samples, [0.4]);
        assert!(!Monitor::valid(&m.inner, &old));
        m.disable();
        assert!(!Monitor::valid(&m.inner, &blocks[0]));
    }
    #[test]
    fn cancelled_open_does_not_become_active_and_volume_is_session_local() {
        let (m, _) = Monitor::with_channel();
        assert_eq!(m.status().volume_percent, 25.0);
        m.set_volume(75.0);
        m.set_volume(f32::NAN);
        m.reserve(1, 1, target());
        m.disable();
        assert!(!Monitor::publish(&m.inner, 1, &AtomicBool::new(true)));
        m.reserve(2, 1, target());
        assert!(!Monitor::publish(&m.inner, 2, &AtomicBool::new(false)));
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
    #[ignore = "requer fone Bluetooth; altera links temporariamente e reproduz tom baixo"]
    async fn bluetooth_live_monitor_keeps_paced_audio_and_links_to_selected_sink() {
        use crate::{
            monitor::routing::{
                backend::{RealBackend, RoutingBackend},
                graph::SavedState,
            },
            output::DieWithParent,
            recording::playback::Playback,
        };
        use futures::FutureExt;
        use tokio::io::AsyncWriteExt;
        let f = Fixture::new("exit 0");
        let backend = Arc::new(RealBackend::default());
        let target = devices::query()
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name.starts_with("bluez_output."))
            .expect("fone Bluetooth indisponível");
        let serial = target.serial.clone().unwrap();
        let shared = crate::state::Shared::with_audio_services(
            Monitor::with_services(
                "pw-cat".into(),
                routing::Routing::new(
                    backend.clone(),
                    crate::state::data_dir().join("monitor-routing.json"),
                ),
            ),
            Playback::new(),
            devices::DeviceCatalog::new(),
        );
        let mut competitor=tokio::process::Command::new("pw-cat").args(["--playback","--raw","--target",&serial,"--rate","48000","--channels","1","--format","f32","--properties","node.name=micbridge_exclusive_test_competitor node.dont-fallback=true node.dont-reconnect=true","-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).kill_on_drop(true).die_with_parent().spawn().unwrap();
        let mut competing_input = competitor.stdin.take().unwrap();
        competing_input.write_all(&[0; 1920]).await.unwrap();
        let capture_path = f.dir.join("capture.f32");
        let mut capture: Option<tokio::process::Child> = None;
        let result = std::panic::AssertUnwindSafe(async {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let before = backend.snapshot().await.unwrap();
            let originals = before.competitors(&target, "no-such-session").unwrap();
            assert!(
                originals
                    .iter()
                    .any(|l| l.output.node_name == "micbridge_exclusive_test_competitor")
            );
            shared.devices.refresh();
            wait(|| !shared.devices.snapshot().busy);
            shared.set_link(crate::state::Link::Connected);
            shared.select_monitor_output(Some(target.clone()));
            let id = shared.start_monitor(target.clone()).unwrap();
            let generation = shared.generation();
            let deadline = std::time::Instant::now() + Duration::from_secs(7);
            while shared.monitor.active_id(generation) != Some(id) {
                assert_ne!(
                    shared.monitor.status().phase,
                    MonitorPhase::Failed,
                    "{:?}",
                    shared.monitor.status()
                );
                assert!(
                    std::time::Instant::now() < deadline,
                    "{:?}",
                    shared.monitor.status()
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let exclusive = backend.snapshot().await.unwrap();
            let name = exclusive
                .links
                .iter()
                .find(|l| l.input.node.id == target.id)
                .expect("retorno sem link")
                .output
                .node_name
                .clone();
            assert!(name.starts_with("micbridge_monitor_"));
            assert!(exclusive.competitors(&target, &name).unwrap().is_empty());
            capture=Some(tokio::process::Command::new("pw-cat").args(["--record","--raw","--target",&serial,"--rate","48000","--channels","1","--format","f32","--properties","stream.capture.sink=true node.name=micbridge_exclusive_test_capture node.dont-fallback=true node.dont-reconnect=true","-"]).stdout(std::fs::File::create(&capture_path).unwrap()).stderr(std::process::Stdio::null()).kill_on_drop(true).die_with_parent().spawn().unwrap());
            let mut cadence = tokio::time::interval(Duration::from_millis(120));
            cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            for block in 0..25 {
                cadence.tick().await;
                let mut pcm = [0f32; 5760];
                for (i, v) in pcm.iter_mut().enumerate() {
                    *v = 0.04
                        * (2.0 * std::f32::consts::PI * 440.0 * (block * 5760 + i) as f32 / 48000.0)
                            .sin();
                }
                submit_processed(&shared.monitor, generation, &pcm);
                assert_eq!(
                    shared.monitor.status().phase,
                    MonitorPhase::Active,
                    "{:?}",
                    shared.monitor.status()
                );
            }
            cadence.tick().await;
            assert_eq!(shared.monitor.status().phase, MonitorPhase::Active);
            if let Some(c)=capture.as_mut(){let _=c.kill().await;let _=c.wait().await;}
            let bytes = std::fs::read(&capture_path).unwrap();
            let peak = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()).abs())
                .fold(0f32, f32::max);
            assert!(
                peak > 0.001,
                "no non-silent PCM captured at Bluetooth sink, peak={peak}"
            );
            let samples:Vec<f32>=bytes.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect();
            let (re,im)=samples.iter().enumerate().fold((0f64,0f64),|(re,im),(i,v)|{let p=2.0*std::f64::consts::PI*440.0*i as f64/48000.0;(re+*v as f64*p.cos(),im+*v as f64*p.sin())});
            let tone=2.0*re.hypot(im)/samples.len().max(1) as f64;
            assert!(tone>0.005 && peak<0.012,"unexpected captured tone amplitude={tone}, peak={peak}");
            shared.local_audio.shutdown_and_wait().await.unwrap();
            let after = backend.snapshot().await.unwrap();
            let original_count=originals.len();
            for link in originals {
                assert_ne!(
                    after.validate_saved(&link),
                    SavedState::Missing,
                    "original link not restored: {link:?}"
                );
            }
            eprintln!("Bluetooth: {original_count} links originais verificados após restauração; pico {peak:.6}; amplitude 440Hz {tone:.6}");
        })
        .catch_unwind()
        .await;
        let cleanup = shared.local_audio.shutdown_and_wait().await;
        if let Some(c) = capture.as_mut() {
            let _ = c.kill().await;
            let _ = c.wait().await;
        }
        let _ = competitor.kill().await;
        let _ = competitor.wait().await;
        drop(competing_input);
        if let Err(error) = result {
            if let Err(e) = cleanup {
                eprintln!("Restauração pendente no registro: {e}");
            }
            std::panic::resume_unwind(error);
        }
        cleanup.unwrap();
    }
    #[tokio::test]
    async fn missing_identity_fails_without_name_fallback() {
        let f = Fixture::new(SLEEP);
        let m = Monitor::with_executable(f.executable.clone());
        let mut output = target();
        output.serial = None;
        m.open(1, 7, output, Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase != MonitorPhase::Opening);
        assert_eq!(m.status().phase, MonitorPhase::Failed);
        assert!(f.pid().is_none());
        m.stop_and_wait().await.unwrap();
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
        let args = std::fs::read_to_string(f.dir.join("args")).unwrap();
        let args: Vec<&str> = args.lines().collect();
        let props: serde_json::Value = serde_json::from_str(args[5]).unwrap();
        assert_eq!(props["node.dont-fallback"], true);
        assert_eq!(props["node.dont-reconnect"], true);
        let name = props["node.name"].as_str().unwrap();
        assert!(name.starts_with(&format!("micbridge_monitor_{}_1_", std::process::id())));
        assert_eq!(name.rsplit('_').next().unwrap().len(), 32);
        assert_eq!(
            args,
            [
                "--playback",
                "--raw",
                "--target",
                "11",
                "--properties",
                args[5],
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
    async fn maximum_packet_is_written_in_full_by_live_worker() {
        let f = Fixture::new("cat > '@DIR@/pcm'");
        let m = Monitor::with_executable(f.executable.clone());
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase == MonitorPhase::Active);
        submit_processed(&m, 7, &vec![0.4; 5760]);
        assert_eq!(m.status().phase, MonitorPhase::Active);
        wait(|| std::fs::metadata(f.dir.join("pcm")).is_ok_and(|m| m.len() == 23040));
        m.stop_and_wait().await.unwrap();
        let actual = std::fs::read(f.dir.join("pcm")).unwrap();
        assert!(
            actual
                .chunks_exact(4)
                .all(|bytes| bytes == 0.1f32.to_le_bytes())
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

#[cfg(test)]
mod routed_tests {
    use super::*;
    use crate::{
        audio_test::{Fixture, SLEEP, alive, wait},
        monitor::routing::{controller::tests::Fake, graph::tests::target},
    };
    use std::time::Duration;
    fn setup(f: &Fixture, b: &Fake) -> (Monitor, routing::Routing) {
        let r = routing::Routing::new(Arc::new(b.clone()), f.dir.join("journal.json"));
        let m = Monitor::with_test_routing(f.executable.clone(), r.clone());
        (m, r)
    }
    #[tokio::test]
    async fn monitor_active_requires_route_ready_and_pcm_never_waits_for_graph() {
        let f = Fixture::new(SLEEP);
        let b = Fake::new();
        let (m, r) = setup(&f, &b);
        r.recover().await;
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| f.pid().is_some());
        assert_eq!(
            m.status().phase,
            MonitorPhase::Opening,
            "must wait for route confirmation"
        );
        let _ = tx.send(());
        wait(|| m.status().phase == MonitorPhase::Active);
        let (tx, rx) = tokio::sync::oneshot::channel();
        b.entered.store(false, SeqCst);
        *b.pause.lock().unwrap() = Some(rx);
        wait(|| b.entered.load(SeqCst));
        let now = std::time::Instant::now();
        submit_processed(&m, 7, &[0.1; 480]);
        assert!(now.elapsed() < Duration::from_millis(50));
        let stop = m.stop_and_wait();
        assert_eq!(m.status().phase, MonitorPhase::Restoring);
        let _ = tx.send(());
        stop.await.unwrap();
    }
    #[tokio::test]
    async fn stop_ack_includes_reap_restore_and_error() {
        let f = Fixture::new(SLEEP);
        let b = Fake::new();
        let (m, _) = setup(&f, &b);
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase == MonitorPhase::Active);
        wait(|| f.pid().is_some());
        assert!(
            b.graph
                .lock()
                .unwrap()
                .competitors(&target(), "micbridge_monitor_test")
                .unwrap()
                .is_empty()
        );
        let pid = f.pid().unwrap();
        b.fail_restore.store(true, SeqCst);
        assert!(m.stop_and_wait().await.is_err());
        assert!(!alive(pid));
        b.fail_restore.store(false, SeqCst);
        m.stop_and_wait().await.unwrap();
        assert_eq!(
            b.graph
                .lock()
                .unwrap()
                .competitors(&target(), "micbridge_monitor_test")
                .unwrap()
                .len(),
            4
        );
    }
    #[tokio::test]
    async fn pending_restore_blocks_ab_until_retry_succeeds() {
        use crate::{
            recording::{Take, Version, playback::Playback},
            state::{Link, Shared},
        };
        let f = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let b = Fake::new();
        let (m, _) = setup(&f, &b);
        let devices =
            devices::DeviceCatalog::with_query(Arc::new(|| Box::pin(async { Ok(vec![target()]) })));
        let shared = Shared::with_audio_services(
            m,
            Playback::with_command(p.executable.clone(), vec![]),
            devices,
        );
        shared.devices.refresh();
        wait(|| !shared.devices.snapshot().busy);
        shared.set_link(Link::Connected);
        shared.select_monitor_output(Some(target()));
        shared.start_monitor(target()).unwrap();
        wait(|| shared.monitor.status().phase == MonitorPhase::Active);
        b.fail_restore.store(true, SeqCst);
        let take = Arc::new(Take {
            id: 1,
            original: vec![0.1],
            treated: vec![0.2],
        });
        shared.play_recording(take.clone(), Version::Original);
        wait(|| shared.monitor.status().restore_pending);
        assert!(shared.monitor.stop_and_wait().await.is_err());
        assert!(p.pid().is_none());
        b.fail_restore.store(false, SeqCst);
        std::mem::drop(shared.monitor.retry_restore());
        wait(|| !shared.monitor.status().restore_pending);
        shared.play_recording(take, Version::Treated);
        wait(|| p.pid().is_some());
        shared.local_audio.shutdown_and_wait().await.unwrap();
    }
    #[tokio::test]
    async fn cancelled_open_and_failed_journal_never_disconnect_others() {
        let f = Fixture::new(SLEEP);
        let b = Fake::new();
        let (m, r) = setup(&f, &b);
        r.recover().await;
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| b.entered.load(SeqCst));
        let stop = m.stop_and_wait();
        let _ = tx.send(());
        stop.await.unwrap();
        assert!(m.active_id(7).is_none());
        assert_eq!(b.unlink_calls.load(SeqCst), 0);
        b.entered.store(false, SeqCst);
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        m.open(2, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| b.entered.load(SeqCst));
        std::fs::create_dir(f.dir.join("journal.json")).unwrap();
        let _ = tx.send(());
        wait(|| m.status().phase == MonitorPhase::Failed);
        assert_eq!(b.unlink_calls.load(SeqCst), 0);
        assert!(m.stop_and_wait().await.is_err());
        std::fs::remove_dir(f.dir.join("journal.json")).unwrap();
        m.stop_and_wait().await.unwrap();
    }
    #[tokio::test]
    async fn normal_quit_waits_for_restore_without_blocking_gtk() {
        let f = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let b = Fake::new();
        let (m, _) = setup(&f, &b);
        m.open(1, 7, target(), Arc::new(AtomicBool::new(true)));
        wait(|| m.status().phase == MonitorPhase::Active);
        let audio = crate::local_audio::LocalAudio::new(
            m,
            crate::recording::playback::Playback::with_command(p.executable.clone(), vec![]),
            devices::DeviceCatalog::new(),
        );
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        let stopped = audio.shutdown_and_wait();
        tokio::pin!(stopped);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut stopped)
                .await
                .is_err(),
            "quit resolved before restoration"
        );
        let _ = tx.send(());
        stopped.await.unwrap();
    }
}
