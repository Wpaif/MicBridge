//! Coordenação das saídas locais, sem esperar na thread GTK ou no pump.
use crate::{
    monitor::{
        Monitor,
        devices::{DeviceCatalog, OutputDevice},
    },
    recording::{Take, Version, playback::Playback},
};
use std::sync::{Arc, atomic::AtomicBool};
use std::sync::{Mutex, atomic::Ordering::SeqCst};
use tokio::sync::watch;
#[derive(Clone)]
enum Intent {
    Stop,
    Monitor {
        id: u64,
        generation: u64,
        target: OutputDevice,
        lease: Arc<AtomicBool>,
    },
    Replay {
        take: Arc<Take>,
        version: Version,
        lease: Arc<AtomicBool>,
    },
}
struct Coordinator {
    tx: watch::Sender<Intent>,
    next: u64,
    lease: Option<(u64, Arc<AtomicBool>)>,
}
struct Inner {
    control: Mutex<Coordinator>,
    monitor: Monitor,
    playback: Playback,
    commit: Arc<Mutex<()>>,
}
#[derive(Clone)]
pub struct LocalAudio {
    inner: Arc<Inner>,
}
impl LocalAudio {
    pub fn new(monitor: Monitor, playback: Playback, devices: DeviceCatalog) -> Self {
        let (tx, rx) = watch::channel(Intent::Stop);
        let commit = Arc::new(Mutex::new(()));
        let worker_commit = commit.clone();
        let worker_monitor = monitor.clone();
        let worker_playback = playback.clone();
        std::thread::Builder::new()
            .name("micbridge-local-audio".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime de saídas locais");
                rt.block_on(run(
                    worker_monitor,
                    worker_playback,
                    devices,
                    rx,
                    worker_commit,
                ));
            })
            .expect("thread de saídas locais");
        Self {
            inner: Arc::new(Inner {
                control: Mutex::new(Coordinator {
                    tx,
                    next: 0,
                    lease: None,
                }),
                monitor,
                playback,
                commit,
            }),
        }
    }
    /// Distinguish a pending opening from an intent cancelled by another app action.
    pub(crate) fn monitor_requested(&self, id: u64) -> bool {
        let control = self.inner.control.lock().unwrap();
        let intent = control.tx.borrow();
        matches!(&*intent, Intent::Monitor { id: requested, lease, .. }
            if *requested == id && lease.load(SeqCst))
    }
    pub fn request_monitor(
        &self,
        generation: u64,
        target: OutputDevice,
        lease: Arc<AtomicBool>,
    ) -> u64 {
        let _commit = self.inner.commit.lock().unwrap();
        let mut c = self.inner.control.lock().unwrap();
        if let Some((_, old)) = c.lease.take() {
            old.store(false, SeqCst);
        }
        cancel_replay(&c);
        self.inner.monitor.disable();
        self.inner.playback.stop();
        c.next += 1;
        let id = c.next;
        c.lease = Some((generation, lease.clone()));
        self.inner.monitor.prepare(id, generation, target.clone());
        c.tx.send_replace(Intent::Monitor {
            id,
            generation,
            target,
            lease,
        });
        id
    }
    pub fn request_playback(&self, take: Arc<Take>, version: Version) {
        let _commit = self.inner.commit.lock().unwrap();
        let mut c = self.inner.control.lock().unwrap();
        if let Some((_, lease)) = c.lease.take() {
            lease.store(false, SeqCst);
        }
        cancel_replay(&c);
        self.inner.monitor.disable();
        self.inner.playback.stop();
        c.tx.send_replace(Intent::Replay {
            take,
            version,
            lease: Arc::new(AtomicBool::new(true)),
        });
    }
    fn stop_monitor_locked(&self, c: &mut Coordinator) {
        if let Some((_, lease)) = c.lease.take() {
            lease.store(false, SeqCst);
        }
        self.inner.monitor.disable();
        let is_monitor = matches!(*c.tx.borrow(), Intent::Monitor { .. });
        if is_monitor {
            c.tx.send_replace(Intent::Stop);
        }
    }
    pub fn stop_monitor(&self) {
        let _commit = self.inner.commit.lock().unwrap();
        let mut c = self.inner.control.lock().unwrap();
        self.stop_monitor_locked(&mut c);
    }
    pub fn stop_playback(&self) {
        let _commit = self.inner.commit.lock().unwrap();
        let c = self.inner.control.lock().unwrap();
        self.inner.playback.stop();
        cancel_replay(&c);
        let is_replay = matches!(*c.tx.borrow(), Intent::Replay { .. });
        if is_replay {
            c.tx.send_replace(Intent::Stop);
        }
    }
    pub fn disconnect(&self, generation: u64) {
        let _commit = self.inner.commit.lock().unwrap();
        let mut c = self.inner.control.lock().unwrap();
        if c.lease.as_ref().is_some_and(|(g, _)| *g == generation) {
            self.stop_monitor_locked(&mut c);
        }
    }
    pub fn shutdown_and_wait(
        &self,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
        self.shutdown();
        let monitor = self.inner.monitor.stop_and_wait();
        let playback = self.inner.playback.stop_and_wait();
        async move { tokio::try_join!(monitor, playback).map(|_| ()) }
    }
    pub fn shutdown(&self) {
        let _commit = self.inner.commit.lock().unwrap();
        let mut c = self.inner.control.lock().unwrap();
        if let Some((_, lease)) = c.lease.take() {
            lease.store(false, SeqCst);
        }
        cancel_replay(&c);
        self.inner.monitor.disable();
        self.inner.playback.stop();
        c.tx.send_replace(Intent::Stop);
    }
}
async fn run(
    monitor: Monitor,
    playback: Playback,
    devices: DeviceCatalog,
    mut commands: watch::Receiver<Intent>,
    commit: Arc<Mutex<()>>,
) {
    let mut pending = None;
    loop {
        let intent = match pending.take() {
            Some(c) => c,
            None => {
                if commands.changed().await.is_err() {
                    break;
                }
                commands.borrow_and_update().clone()
            }
        };
        let settled = tokio::select! {biased;
            changed=commands.changed()=>{if changed.is_err(){break;}pending=Some(commands.borrow_and_update().clone());continue;}
            result=async{tokio::try_join!(monitor.stop_and_wait(),playback.stop_and_wait())}=>result,
        };
        match intent {
            Intent::Stop => {}
            Intent::Replay {
                take,
                version,
                lease,
            } => {
                if settled.is_ok() {
                    commit_replay(&playback, take, version, &lease, &commit);
                }
            }
            Intent::Monitor {
                id,
                generation,
                target,
                lease,
            } => {
                if !lease.load(SeqCst) {
                    continue;
                }
                monitor.prepare(id, generation, target.clone());
                if let Err(e) = settled {
                    monitor.report_failure(id, e);
                    continue;
                }
                let mut first = true;
                loop {
                    if !first {
                        tokio::select! {biased;
                            changed=commands.changed()=>{if changed.is_err(){break;}pending=Some(commands.borrow_and_update().clone());break;}
                            _=tokio::time::sleep(std::time::Duration::from_secs(2))=>{},
                        }
                        let current = monitor.status();
                        if !lease.load(SeqCst)
                            || current.id != id
                            || current.generation != Some(generation)
                            || !matches!(
                                current.phase,
                                crate::monitor::MonitorPhase::Opening
                                    | crate::monitor::MonitorPhase::Active
                            )
                        {
                            break;
                        }
                    }
                    let result = tokio::select! {biased;
                        changed=commands.changed()=>{if changed.is_err(){break;}pending=Some(commands.borrow_and_update().clone());break;}
                        result=devices.query_now()=>result,
                    };
                    if !lease.load(SeqCst) {
                        break;
                    }
                    match result {
                        Ok(outputs) if outputs.iter().any(|d| d.same_instance(&target)) => {
                            if first {
                                monitor.open(id, generation, target.clone(), lease.clone());
                            }
                        }
                        Ok(_) => {
                            devices.refresh();
                            lease.store(false, SeqCst);
                            monitor.report_failure(id,"A saída selecionada foi removida ou substituída. Escolha o dispositivo e ative novamente.".into());
                            break;
                        }
                        Err(e) => {
                            devices.refresh();
                            lease.store(false, SeqCst);
                            monitor.report_failure(id, e);
                            break;
                        }
                    }
                    first = false;
                }
                if commands.has_changed().is_err() {
                    break;
                }
            }
        }
    }
    let _ = tokio::join!(monitor.stop_and_wait(), playback.stop_and_wait());
}
fn cancel_replay(c: &Coordinator) {
    if let Intent::Replay { lease, .. } = &*c.tx.borrow() {
        lease.store(false, SeqCst);
    }
}
fn commit_replay(
    playback: &Playback,
    take: Arc<Take>,
    version: Version,
    lease: &AtomicBool,
    commit: &Mutex<()>,
) {
    let _guard = commit.lock().unwrap();
    if lease.load(SeqCst) {
        playback.play(take, version);
    }
}
pub struct MonitorSession {
    audio: LocalAudio,
    generation: u64,
}
impl MonitorSession {
    pub fn new(audio: LocalAudio, generation: u64) -> Self {
        Self { audio, generation }
    }
}
impl Drop for MonitorSession {
    fn drop(&mut self) {
        self.audio.disconnect(self.generation);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio_test::{Fixture, SLEEP, alive, wait},
        monitor::MonitorPhase,
        monitor::devices::DeviceQuery,
        state::{Link, Shared},
    };
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    pub(crate) fn output() -> OutputDevice {
        OutputDevice {
            id: 1,
            serial: Some("101".into()),
            name: "headphones".into(),
            label: "Fone".into(),
        }
    }
    fn services(m: &Fixture, p: &Fixture, q: DeviceQuery) -> Shared {
        let s = Shared::with_audio_services(
            Monitor::with_executable(m.executable.clone()),
            Playback::with_command(p.executable.clone(), vec![]),
            DeviceCatalog::with_query(q),
        );
        s.devices.refresh();
        wait(|| !s.devices.snapshot().busy);
        s
    }
    fn constant() -> DeviceQuery {
        Arc::new(|| Box::pin(async { Ok(vec![output()]) }))
    }
    fn take() -> Arc<Take> {
        Arc::new(Take {
            id: 1,
            original: vec![0.1],
            treated: vec![0.2],
        })
    }
    #[test]
    fn cancelled_replay_at_publication_boundary_cannot_restart() {
        for cancellation in 0..3 {
            let p = Fixture::new(SLEEP);
            let playback = Playback::with_command(p.executable.clone(), vec![]);
            let lease = Arc::new(AtomicBool::new(true));
            let commit = Arc::new(Mutex::new(()));
            let (tx, _rx) = watch::channel(Intent::Replay {
                take: take(),
                version: Version::Original,
                lease: lease.clone(),
            });
            // Hold the actor exactly at its final publication boundary, after ACKs.
            let audio = LocalAudio {
                inner: Arc::new(Inner {
                    control: Mutex::new(Coordinator {
                        tx,
                        next: 0,
                        lease: None,
                    }),
                    monitor: Monitor::new(),
                    playback: playback.clone(),
                    commit: commit.clone(),
                }),
            };
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(playback.stop_and_wait()).unwrap();
            match cancellation {
                0 => audio.stop_playback(), // Also used when starting capture.
                1 => audio.shutdown(),
                _ => {
                    audio.request_monitor(1, output(), Arc::new(AtomicBool::new(true)));
                }
            }
            let stopped = playback.status().operation;
            commit_replay(&playback, take(), Version::Original, &lease, &commit);
            assert_eq!(
                playback.status().operation,
                stopped,
                "obsolete replay replaced cancellation"
            );
            assert!(!playback.status().playing);
            assert!(p.pid().is_none());
            audio.shutdown();
        }
    }

    #[test]
    fn backend_rejects_disconnected_unselected_and_removed_output() {
        let m = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let s = services(&m, &p, constant());
        assert!(s.start_monitor(output()).is_err());
        s.set_link(Link::Connected);
        assert!(s.start_monitor(output()).is_err());
        s.select_monitor_output(Some(output()));
        let id = s.start_monitor(output()).unwrap();
        wait(|| s.monitor.active_id(s.generation()) == Some(id));
        s.stop_monitor();
        wait(|| !m.pid().is_some_and(alive));
        let mut missing = output();
        missing.id = 2;
        s.select_monitor_output(Some(missing.clone()));
        assert!(s.start_monitor(missing).is_err());
    }
    #[test]
    fn switching_modes_reaps_previous_process_before_starting_new_one() {
        let m = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let s = services(&m, &p, constant());
        s.set_link(Link::Connected);
        s.select_monitor_output(Some(output()));
        s.start_monitor(output()).unwrap();
        wait(|| m.pid().is_some());
        let mpid = m.pid().unwrap();
        s.play_recording(take(), Version::Original);
        wait(|| p.pid().is_some());
        assert!(!alive(mpid));
        let ppid = p.pid().unwrap();
        s.start_monitor(output()).unwrap();
        wait(|| m.pid().is_some_and(|pid| pid != mpid));
        assert!(!alive(ppid));
        // Recording stops A/B only; live return remains active.
        let id = s.monitor.status().id;
        wait(|| s.monitor.active_id(s.generation()) == Some(id));
        s.start_recording();
        assert_eq!(s.monitor.active_id(s.generation()), Some(id));
        s.stop_audio_tools();
        wait(|| !m.pid().is_some_and(alive));
    }
    #[test]
    fn stale_validation_after_disconnect_or_selection_change_cannot_activate() {
        for disconnect in [false, true] {
            let m = Fixture::new(SLEEP);
            let p = Fixture::new(SLEEP);
            let calls = Arc::new(AtomicUsize::new(0));
            let c = calls.clone();
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
            let q: DeviceQuery = Arc::new(move || {
                let n = c.fetch_add(1, SeqCst);
                let receiver = if n == 1 {
                    rx.lock().unwrap().take()
                } else {
                    None
                };
                Box::pin(async move {
                    if let Some(r) = receiver {
                        let _ = r.await;
                    }
                    Ok(vec![output()])
                })
            });
            let s = services(&m, &p, q);
            s.set_link(Link::Connected);
            s.select_monitor_output(Some(output()));
            s.start_monitor(output()).unwrap();
            wait(|| calls.load(SeqCst) >= 2);
            if disconnect {
                s.set_link(Link::Disconnected);
            } else {
                s.select_monitor_output(None);
            }
            let _ = tx.send(());
            std::thread::sleep(std::time::Duration::from_millis(30));
            assert!(s.monitor.active_id(s.generation()).is_none());
            assert_ne!(s.monitor.status().phase, MonitorPhase::Active);
            assert!(m.pid().is_none());
        }
    }
    #[test]
    fn fresh_validation_rejects_removed_or_replaced_device_without_connection_error() {
        for replaced in [false, true] {
            let m = Fixture::new(SLEEP);
            let p = Fixture::new(SLEEP);
            let calls = AtomicUsize::new(0);
            let q: DeviceQuery = Arc::new(move || {
                let n = calls.fetch_add(1, SeqCst);
                Box::pin(async move {
                    if n == 0 {
                        Ok(vec![output()])
                    } else if replaced {
                        let mut d = output();
                        d.serial = Some("102".into());
                        Ok(vec![d])
                    } else {
                        Ok(vec![])
                    }
                })
            });
            let s = services(&m, &p, q);
            s.set_link(Link::Connected);
            s.select_monitor_output(Some(output()));
            s.start_monitor(output()).unwrap();
            wait(|| s.monitor.status().phase == MonitorPhase::Failed);
            assert_eq!(s.status().link, Link::Connected);
            assert!(m.pid().is_none());
        }
    }
    #[test]
    fn periodic_validation_stops_removed_device_and_never_falls_back() {
        let m = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let calls = AtomicUsize::new(0);
        let q: DeviceQuery = Arc::new(move || {
            let n = calls.fetch_add(1, SeqCst);
            Box::pin(async move { Ok(if n < 2 { vec![output()] } else { vec![] }) })
        });
        let s = services(&m, &p, q);
        s.set_link(Link::Connected);
        s.select_monitor_output(Some(output()));
        s.start_monitor(output()).unwrap();
        wait(|| m.pid().is_some());
        let pid = m.pid().unwrap();
        wait(|| s.monitor.status().phase == MonitorPhase::Failed);
        wait(|| !alive(pid));
        assert_eq!(s.status().link, Link::Connected);
        assert_eq!(s.monitor_output(), Some(output()));
        wait(|| !s.devices.snapshot().busy && s.devices.snapshot().devices.is_empty());
    }
    #[test]
    fn disconnect_and_pump_exit_do_not_restart_after_reconnection() {
        let m = Fixture::new(SLEEP);
        let p = Fixture::new(SLEEP);
        let s = services(&m, &p, constant());
        let generation = s.next_generation();
        s.set_link(Link::Connected);
        s.select_monitor_output(Some(output()));
        s.start_monitor(output()).unwrap();
        wait(|| m.pid().is_some());
        let pid = m.pid().unwrap();
        drop(MonitorSession::new(s.local_audio.clone(), generation));
        assert!(s.monitor.active_id(generation).is_none());
        wait(|| !alive(pid));
        s.set_link(Link::Disconnected);
        s.next_generation();
        s.set_link(Link::Connected);
        assert!(s.monitor.active_id(s.generation()).is_none());
    }
}
