//! Captura local de áudio original e tratado.
pub mod playback;
pub mod wav;
use std::sync::Arc;
pub type RecordingId = u64;
pub type Generation = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeState {
    Empty,
    Recording,
    Ready,
    Incomplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    Original,
    Treated,
}
#[derive(Debug)]
pub struct Take {
    pub id: RecordingId,
    pub original: Vec<f32>,
    pub treated: Vec<f32>,
}
#[derive(Clone, Debug)]
pub struct RecordingStatus {
    pub id: RecordingId,
    pub state: TakeState,
    pub samples: usize,
    pub message: Option<String>,
}
use std::sync::{
    Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
    mpsc,
};
use std::time::Duration;
const MAX_SAMPLES: usize = 1_440_000;
const MAX_BLOCK: usize = 5760;
struct Pair {
    id: RecordingId,
    original: Vec<f32>,
    treated: Vec<f32>,
}
struct Data {
    status: RecordingStatus,
    original: Vec<f32>,
    treated: Vec<f32>,
    take: Option<Arc<Take>>,
}
struct Inner {
    tx: mpsc::SyncSender<Pair>,
    active: AtomicU64,
    generation: AtomicU64,
    next: AtomicU64,
    failed: AtomicU64,
    producers: AtomicUsize,
    data: Mutex<Data>,
}
#[derive(Clone)]
pub struct Recorder {
    inner: Arc<Inner>,
}
impl Recorder {
    fn with_channel(capacity: usize) -> (Self, mpsc::Receiver<Pair>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        let inner = Arc::new(Inner {
            tx,
            active: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            next: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            producers: AtomicUsize::new(0),
            data: Mutex::new(Data {
                status: RecordingStatus {
                    id: 0,
                    state: TakeState::Empty,
                    samples: 0,
                    message: None,
                },
                original: Vec::new(),
                treated: Vec::new(),
                take: None,
            }),
        });
        (Self { inner }, rx)
    }
    pub fn new() -> Self {
        let (r, rx) = Self::with_channel(64);
        let weak = Arc::downgrade(&r.inner);
        std::thread::Builder::new()
            .name("micbridge-recording".into())
            .spawn(move || {
                loop {
                    let msg = rx.recv_timeout(Duration::from_millis(5));
                    let Some(inner) = weak.upgrade() else { break };
                    if let Ok(pair) = msg {
                        consume(&inner, pair);
                    }
                    // Finalize only after all accepted blocks have been consumed.
                    while let Ok(pair) = rx.try_recv() {
                        consume(&inner, pair);
                    }
                    if inner.active.load(SeqCst) == 0 && inner.producers.load(SeqCst) == 0 {
                        while let Ok(pair) = rx.try_recv() {
                            consume(&inner, pair);
                        }
                        finalize(&inner);
                    }
                }
            })
            .expect("thread de gravação");
        r
    }
    pub fn start(&self, generation: Generation) -> RecordingId {
        let mut d = self.inner.data.lock().unwrap();
        self.inner.active.store(0, SeqCst);
        let id = self.inner.next.fetch_add(1, SeqCst) + 1;
        d.status = RecordingStatus {
            id,
            state: TakeState::Recording,
            samples: 0,
            message: None,
        };
        d.original = Vec::new();
        d.treated = Vec::new();
        d.take = None;
        self.inner.generation.store(generation, SeqCst);
        self.inner.active.store(id, SeqCst);
        id
    }
    pub fn active_id(&self, generation: Generation) -> Option<RecordingId> {
        let id = self.inner.active.load(SeqCst);
        (id != 0
            && self.inner.generation.load(SeqCst) == generation
            && self.inner.active.load(SeqCst) == id)
            .then_some(id)
    }
    pub fn push(&self, id: RecordingId, generation: Generation, original: &[f32], treated: &[f32]) {
        self.inner.producers.fetch_add(1, SeqCst);
        if self.active_id(generation) == Some(id) {
            if original.len() != treated.len() || original.len() > MAX_BLOCK {
                self.fail(id);
            } else if !original.is_empty() {
                let pair = Pair {
                    id,
                    original: original.to_vec(),
                    treated: treated.to_vec(),
                };
                if self.inner.tx.try_send(pair).is_err() {
                    self.fail(id);
                }
            }
        }
        self.inner.producers.fetch_sub(1, SeqCst);
    }
    fn fail(&self, id: RecordingId) {
        self.inner.failed.fetch_max(id, SeqCst);
        let _ = self.inner.active.compare_exchange(id, 0, SeqCst, SeqCst);
    }
    pub fn stop(&self, id: RecordingId) {
        let _ = self.inner.active.compare_exchange(id, 0, SeqCst, SeqCst);
    }
    pub fn disconnect(&self, generation: Generation) {
        if let Some(id) = self.active_id(generation) {
            self.stop(id);
        }
    }
    pub fn status(&self) -> RecordingStatus {
        self.inner.data.lock().unwrap().status.clone()
    }
    pub fn take(&self, id: RecordingId) -> Option<Arc<Take>> {
        self.inner
            .data
            .lock()
            .unwrap()
            .take
            .as_ref()
            .filter(|t| t.id == id)
            .cloned()
    }
}
fn consume(inner: &Inner, pair: Pair) {
    let mut d = inner.data.lock().unwrap();
    if d.status.id != pair.id || d.status.state != TakeState::Recording {
        return;
    }
    let n = pair.original.len().min(MAX_SAMPLES - d.original.len());
    d.original.extend_from_slice(&pair.original[..n]);
    d.treated.extend_from_slice(&pair.treated[..n]);
    d.status.samples = d.original.len();
    if d.status.samples == MAX_SAMPLES {
        let _ = inner.active.compare_exchange(pair.id, 0, SeqCst, SeqCst);
    }
}
fn finalize(inner: &Inner) {
    let mut d = inner.data.lock().unwrap();
    if inner.active.load(SeqCst) != 0
        || inner.producers.load(SeqCst) != 0
        || d.status.state != TakeState::Recording
    {
        return;
    }
    let failed = inner.failed.load(SeqCst) == d.status.id;
    d.status.state = if failed {
        TakeState::Incomplete
    } else if d.original.is_empty() {
        TakeState::Empty
    } else {
        TakeState::Ready
    };
    if failed {
        d.status.message = Some(
            "Teste incompleto: a captura não acompanhou o áudio ou recebeu blocos incompatíveis."
                .into(),
        );
    }
    if !d.original.is_empty() {
        d.take = Some(Arc::new(Take {
            id: d.status.id,
            original: std::mem::take(&mut d.original),
            treated: std::mem::take(&mut d.treated),
        }));
    }
}

pub fn process_block(
    processor: &mut crate::dsp::Processor,
    pcm: &mut [f32],
    params: &crate::dsp::Params,
    recorder: &Recorder,
    generation: Generation,
) {
    let capture = recorder.active_id(generation).map(|id| (id, pcm.to_vec()));
    for frame in pcm.chunks_exact_mut(crate::dsp::FRAME) {
        processor.process(frame, params);
    }
    if let Some((id, original)) = capture {
        recorder.push(id, generation, &original, pcm);
    }
}
pub struct CaptureSession {
    recorder: Recorder,
    generation: Generation,
}
impl CaptureSession {
    pub fn new(recorder: Recorder, generation: Generation) -> Self {
        Self {
            recorder,
            generation,
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.recorder.disconnect(self.generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn finish(r: &Recorder, id: u64) -> Option<Arc<Take>> {
        r.stop(id);
        let deadline = Instant::now() + Duration::from_secs(2);
        while r.status().state == TakeState::Recording && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_ne!(r.status().state, TakeState::Recording);
        r.take(id)
    }
    #[test]
    fn preserves_paired_samples_and_pending_prefix_on_stop() {
        let r = Recorder::new();
        let id = r.start(7);
        r.push(id, 7, &[0.1, 0.2], &[0.3, 0.4]);
        r.push(id, 7, &[0.5], &[0.6]);
        let t = finish(&r, id).expect("captured samples");
        assert_eq!(t.original, [0.1, 0.2, 0.5]);
        assert_eq!(t.treated, [0.3, 0.4, 0.6]);
        assert_eq!(r.status().samples, 3);
        assert_eq!(r.status().state, TakeState::Ready);
    }
    #[test]
    fn empty_stop_is_idempotent_and_has_no_take() {
        let r = Recorder::new();
        let id = r.start(1);
        assert!(finish(&r, id).is_none());
        r.stop(id);
        assert_eq!(r.status().state, TakeState::Empty);
    }
    #[test]
    fn replacement_rejects_old_ids_generations_and_late_stop() {
        let r = Recorder::new();
        let old = r.start(1);
        r.push(old, 1, &[1.0], &[2.0]);
        let id = r.start(2);
        assert_ne!(old, id);
        r.stop(old);
        r.disconnect(1);
        r.push(old, 1, &[3.0], &[4.0]);
        r.push(id, 1, &[3.0], &[4.0]);
        assert_eq!(r.active_id(2), Some(id));
        r.push(id, 2, &[0.1], &[0.2]);
        let t = finish(&r, id).unwrap();
        assert_eq!(t.original, [0.1]);
        assert_eq!(t.treated, [0.2]);
    }
    #[test]
    fn disconnect_finishes_without_another_packet() {
        let r = Recorder::new();
        let id = r.start(3);
        r.push(id, 3, &[0.1], &[0.2]);
        r.disconnect(3);
        assert!(finish(&r, id).is_some());
        assert!(r.active_id(3).is_none());
    }
    #[test]
    fn mismatched_lengths_report_incomplete_without_unpaired_samples() {
        let r = Recorder::new();
        let id = r.start(1);
        r.push(id, 1, &[0.1], &[]);
        assert!(finish(&r, id).is_none());
        assert_eq!(r.status().state, TakeState::Incomplete);
        assert!(r.status().message.is_some());
    }
    #[test]
    fn real_dsp_records_both_sides_without_changing_output() {
        for n in [480, 960, 1080] {
            let p = crate::dsp::Params::default();
            p.apply_preset(crate::dsp::Preset::Bruto);
            p.set_makeup_db(6.0);
            let original = vec![0.01; n];
            let mut live = original.clone();
            let mut reference = original.clone();
            let off = Recorder::new();
            process_block(
                &mut crate::dsp::Processor::new(),
                &mut reference,
                &p,
                &off,
                1,
            );
            let r = Recorder::new();
            let id = r.start(1);
            process_block(&mut crate::dsp::Processor::new(), &mut live, &p, &r, 1);
            let take = finish(&r, id).expect("original e tratado capturados");
            assert_eq!(take.original, original);
            assert_eq!(take.treated, live);
            assert_eq!(live, reference);
            assert!((live[0] - 0.019952623).abs() < 1e-7);
            if n == 1080 {
                assert_eq!(&live[960..], &[0.01; 120]);
            }
        }
    }
    #[test]
    fn pump_exit_guard_finishes_current_capture_on_error_or_track_end() {
        let r = Recorder::new();
        let id = r.start(1);
        r.push(id, 1, &[0.1], &[0.2]);
        {
            let _guard = CaptureSession::new(r.clone(), 1);
        }
        assert!(r.active_id(1).is_none());
        assert!(finish(&r, id).is_some());
    }
    #[test]
    fn stale_block_cannot_be_relabelled_as_new_recording() {
        let r = Recorder::new();
        let old = r.start(1);
        let sampled_id = r.active_id(1).unwrap();
        let original = [0.1];
        let new = r.start(1);
        assert_ne!(old, new);
        r.push(sampled_id, 1, &original, &[0.2]);
        assert!(finish(&r, new).is_none());
    }
    #[test]
    fn full_queue_never_waits_and_preserves_accepted_prefix() {
        let (r, rx) = Recorder::with_channel(1);
        let id = r.start(1);
        r.push(id, 1, &[0.1], &[0.2]);
        let before = Instant::now();
        r.push(id, 1, &[0.3], &[0.4]);
        assert!(before.elapsed() < Duration::from_millis(100));
        assert!(r.active_id(1).is_none());
        for pair in rx.try_iter() {
            consume(&r.inner, pair);
        }
        finalize(&r.inner);
        let t = r.take(id).unwrap();
        assert_eq!(t.original, [0.1]);
        assert_eq!(t.treated, [0.2]);
        assert_eq!(r.status().state, TakeState::Incomplete);
        assert!(r.status().message.is_some());
    }
    #[test]
    fn caps_at_thirty_seconds_inside_last_block() {
        let r = Recorder::new();
        let id = r.start(1);
        // Wait for each block to be consumed, so this exercises the duration limit rather than queue overflow.
        let mut count = 0;
        while count < 1_439_999 {
            let n = (1_439_999 - count).min(5760);
            r.push(id, 1, &vec![0.1; n], &vec![0.2; n]);
            count += n;
            let until = Instant::now() + Duration::from_secs(2);
            while r.status().samples < count && Instant::now() < until {
                std::thread::yield_now();
            }
            assert_eq!(r.status().samples, count);
        }
        r.push(id, 1, &[0.3; 480], &[0.4; 480]);
        let t = finish(&r, id).unwrap();
        assert_eq!(t.original.len(), 1_440_000);
        assert_eq!(t.treated.len(), 1_440_000);
        assert_eq!(t.original[1_439_999], 0.3);
        assert_eq!(t.treated[1_439_999], 0.4);
    }
}
