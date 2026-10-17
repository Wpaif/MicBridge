use super::{
    backend::{Changes, RoutingBackend},
    graph::{Graph, ObjectIdentity, SavedLink, SavedState},
    journal::Journal,
};
use crate::monitor::devices::OutputDevice;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: usize,
    pub existing: usize,
    pub unavailable: usize,
    pub pending: usize,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct RoutingStatus {
    pub busy: bool,
    pub report: RestoreReport,
}
#[derive(Clone, Debug, Default)]
struct SessionState {
    ready: bool,
    error: Option<String>,
    done: Option<RestoreReport>,
}
enum Command {
    Start {
        id: u64,
        target: OutputDevice,
        name: String,
        lease: Arc<AtomicBool>,
        state: watch::Sender<SessionState>,
    },
    Stop {
        id: u64,
        reply: oneshot::Sender<RestoreReport>,
    },
    Recover(oneshot::Sender<RestoreReport>),
}
#[derive(Clone)]
pub struct Routing {
    tx: mpsc::UnboundedSender<Command>,
    status: watch::Receiver<RoutingStatus>,
}
pub struct RouteSession {
    id: u64,
    tx: mpsc::UnboundedSender<Command>,
    state: watch::Receiver<SessionState>,
    lease: Arc<AtomicBool>,
}
impl Routing {
    pub fn new(backend: Arc<dyn RoutingBackend>, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel(RoutingStatus {
            busy: true,
            ..Default::default()
        });
        std::thread::Builder::new()
            .name("micbridge-routing".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime de roteamento");
                rt.block_on(run(
                    Engine {
                        backend,
                        path,
                        journal: None,
                        entries: vec![],
                        loaded: false,
                    },
                    rx,
                    status_tx,
                ));
            })
            .expect("thread de roteamento");
        Self { tx, status }
    }
    pub fn status(&self) -> RoutingStatus {
        self.status.borrow().clone()
    }
    pub fn start(
        &self,
        id: u64,
        target: OutputDevice,
        name: String,
        lease: Arc<AtomicBool>,
    ) -> RouteSession {
        let (state_tx, state) = watch::channel(SessionState::default());
        let _ = self.tx.send(Command::Start {
            id,
            target,
            name,
            lease: lease.clone(),
            state: state_tx,
        });
        RouteSession {
            id,
            tx: self.tx.clone(),
            state,
            lease,
        }
    }
    pub fn recover(&self) -> impl std::future::Future<Output = RestoreReport> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Command::Recover(tx));
        async move {
            rx.await.unwrap_or_else(|_| RestoreReport {
                pending: 1,
                error: Some("Worker de roteamento indisponível".into()),
                ..Default::default()
            })
        }
    }
}
impl RouteSession {
    pub async fn ready(&mut self) -> Result<(), String> {
        loop {
            let state = self.state.borrow_and_update().clone();
            if let Some(e) = state.error {
                return Err(e);
            }
            if !self.lease.load(SeqCst) || state.done.is_some() {
                return Err("Preparação do retorno cancelada".into());
            }
            if state.ready {
                return Ok(());
            }
            self.state
                .changed()
                .await
                .map_err(|_| "Roteamento encerrou".to_string())?;
        }
    }
    pub async fn failure(&mut self) -> String {
        loop {
            let state = self.state.borrow_and_update().clone();
            if let Some(e) = state.error {
                return e;
            }
            if state.done.is_some() {
                return "Retorno encerrado".into();
            }
            if self.state.changed().await.is_err() {
                return "Roteamento encerrou".into();
            }
        }
    }
    pub fn stop_and_restore(
        &self,
    ) -> impl std::future::Future<Output = RestoreReport> + Send + 'static {
        self.lease.store(false, SeqCst);
        let done = self.state.borrow().done.clone();
        let (tx, rx) = oneshot::channel();
        if done.is_none() {
            let _ = self.tx.send(Command::Stop {
                id: self.id,
                reply: tx,
            });
        }
        async move {
            match done {
                Some(report) => report,
                None => rx.await.unwrap_or_else(|_| RestoreReport {
                    pending: 1,
                    error: Some("Restauração não confirmada".into()),
                    ..Default::default()
                }),
            }
        }
    }
}
impl Drop for RouteSession {
    fn drop(&mut self) {
        self.lease.store(false, SeqCst);
        let (reply, _) = oneshot::channel();
        let _ = self.tx.send(Command::Stop { id: self.id, reply });
    }
}
struct Active {
    id: u64,
    target: OutputDevice,
    name: String,
    lease: Arc<AtomicBool>,
    state: watch::Sender<SessionState>,
    stream: Option<ObjectIdentity>,
    changes: Option<Changes>,
}
struct Engine {
    backend: Arc<dyn RoutingBackend>,
    path: PathBuf,
    journal: Option<Journal>,
    entries: Vec<SavedLink>,
    loaded: bool,
}
impl Engine {
    fn load(&mut self) -> Result<(), String> {
        if self.journal.is_none() {
            self.journal = Some(Journal::open(self.path.clone())?);
        }
        if !self.loaded {
            self.entries = self.journal.as_ref().unwrap().load()?;
            self.loaded = true;
        }
        Ok(())
    }
    fn remember(&mut self, e: &SavedLink) -> Result<(), String> {
        if !self
            .entries
            .iter()
            .any(|p| p.core == e.core && p.input == e.input && p.output == e.output)
        {
            let mut next = self.entries.clone();
            next.push(e.clone());
            self.journal.as_ref().unwrap().save(&next)?;
            self.entries = next;
        }
        Ok(())
    }
    async fn restore(&mut self) -> RestoreReport {
        let mut report = RestoreReport::default();
        if let Err(e) = self.load() {
            return RestoreReport {
                pending: 1,
                error: Some(e),
                ..report
            };
        }
        let mut pending = vec![];
        for e in &self.entries {
            let result = match self.backend.snapshot().await {
                Ok(g) => match g.validate_saved(e) {
                    SavedState::Unavailable => {
                        report.unavailable += 1;
                        Ok(())
                    }
                    SavedState::Present => {
                        report.existing += 1;
                        Ok(())
                    }
                    SavedState::Missing => match self.backend.connect(e).await {
                        Ok(()) => match self.backend.snapshot().await {
                            Ok(g) if g.validate_saved(e) == SavedState::Present => {
                                report.restored += 1;
                                Ok(())
                            }
                            Ok(_) => Err("Restauração não confirmada pelo grafo".into()),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    },
                },
                Err(e) => Err(e),
            };
            if let Err(error) = result {
                pending.push(e.clone());
                report.error = Some(error);
            }
        }
        let saved = if pending.is_empty() {
            self.journal.as_ref().unwrap().clear()
        } else {
            self.journal.as_ref().unwrap().save(&pending)
        };
        match saved {
            Ok(()) => self.entries = pending,
            Err(e) => {
                report.error = Some(e);
            }
        }
        report.pending = self.entries.len();
        if report.error.is_some() && report.pending == 0 {
            report.pending = 1;
        }
        if report.pending == 0 {
            self.journal = None;
            self.loaded = false;
        }
        report
    }
    async fn reconcile(&mut self, a: &mut Active, preparing: bool) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut graph = loop {
            if !a.lease.load(SeqCst) {
                return Err("Retorno cancelado".into());
            }
            let g = self.backend.snapshot().await?;
            if !g.target_matches(&a.target) {
                return Err("Saída removida ou substituída".into());
            }
            if g.return_links(&a.target, &a.name).is_ok() {
                break g;
            }
            if !preparing || Instant::now() >= deadline {
                return Err("Portas do retorno não ficaram disponíveis".into());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        for _ in 0..4 {
            if !a.lease.load(SeqCst) {
                return Err("Retorno cancelado".into());
            }
            let links = graph.return_links(&a.target, &a.name)?;
            if let Some(id) = &a.stream {
                if *id != links[0].output.node {
                    return Err("Fluxo do retorno foi substituído".into());
                }
            } else {
                a.stream = Some(links[0].output.node.clone());
            }
            for e in links {
                if !a.lease.load(SeqCst) {
                    return Err("Retorno cancelado".into());
                }
                if graph.validate_saved(&e) != SavedState::Present {
                    self.backend.connect(&e).await?;
                }
            }
            graph = self.backend.snapshot().await?;
            if graph
                .return_links(&a.target, &a.name)?
                .iter()
                .any(|e| graph.validate_saved(e) != SavedState::Present)
            {
                return Err("Retorno não conectado à saída".into());
            }
            let competitors = graph.competitors(&a.target, &a.name)?;
            if competitors.is_empty() {
                return Ok(());
            }
            for e in competitors {
                if !a.lease.load(SeqCst) {
                    return Err("Retorno cancelado".into());
                }
                self.remember(&e)?;
                if !a.lease.load(SeqCst) {
                    return Err("Retorno cancelado".into());
                }
                if self.backend.snapshot().await?.validate_saved(&e) == SavedState::Present {
                    self.backend.disconnect(&e).await?;
                }
            }
            graph = self.backend.snapshot().await?;
        }
        if graph.competitors(&a.target, &a.name)?.is_empty() {
            Ok(())
        } else {
            Err(
                "Outros aplicativos continuam religando à saída; ajuste exclusivo interrompido"
                    .into(),
            )
        }
    }
}
async fn finish(
    engine: &mut Engine,
    active: &mut Option<Active>,
    status: &watch::Sender<RoutingStatus>,
) -> RestoreReport {
    let previous = status.borrow().report.clone();
    status.send_replace(RoutingStatus {
        busy: true,
        report: previous,
    });
    let mut old = active.take();
    if let Some(changes) = old.as_mut().and_then(|a| a.changes.as_mut()) {
        changes.stop().await;
    }
    let report = engine.restore().await;
    if let Some(a) = old {
        a.state.send_modify(|s| {
            s.done = Some(report.clone());
        });
    }
    status.send_replace(RoutingStatus {
        busy: false,
        report: report.clone(),
    });
    report
}
async fn run(
    mut e: Engine,
    mut rx: mpsc::UnboundedReceiver<Command>,
    status: watch::Sender<RoutingStatus>,
) {
    let mut active: Option<Active> = None;
    let initial = e.restore().await;
    status.send_replace(RoutingStatus {
        busy: false,
        report: initial,
    });
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let has_active = active.is_some();
        let has_changes = active.as_ref().is_some_and(|a| a.changes.is_some());
        let mut event_error = None;
        let command = tokio::select! {biased;
            command=rx.recv()=>match command {Some(c)=>Some(c),None=>break},
            event=async {active.as_mut().unwrap().changes.as_mut().unwrap().next().await},if has_changes=>{
                if let Err(e)=event{event_error=Some(e);}else{tokio::time::sleep(Duration::from_millis(50)).await;}
                None
            },
            _=tick.tick(),if has_active=>None,
        };
        match command {
            Some(Command::Start {
                id,
                target,
                name,
                lease,
                state,
            }) => {
                if active.is_some() {
                    finish(&mut e, &mut active, &status).await;
                }
                if status.borrow().report.pending > 0 {
                    state.send_replace(SessionState {
                        error: Some(
                            "Restaure as conexões pendentes antes de iniciar outro modo".into(),
                        ),
                        done: Some(status.borrow().report.clone()),
                        ..Default::default()
                    });
                    continue;
                }
                if let Err(error) = e.load() {
                    state.send_replace(SessionState {
                        error: Some(error),
                        ..Default::default()
                    });
                    continue;
                }
                let mut a = Active {
                    id,
                    target,
                    name,
                    lease,
                    state,
                    stream: None,
                    changes: None,
                };
                let result = match e.backend.watch() {
                    Ok(changes) => {
                        a.changes = changes;
                        e.reconcile(&mut a, true).await
                    }
                    Err(error) => Err(error),
                };
                if result.is_ok() && a.lease.load(SeqCst) {
                    a.state.send_modify(|s| s.ready = true);
                    active = Some(a);
                } else {
                    a.state.send_modify(|s| {
                        s.error = Some(result.err().unwrap_or_else(|| "Retorno cancelado".into()))
                    });
                    active = Some(a);
                    finish(&mut e, &mut active, &status).await;
                }
            }
            Some(Command::Stop { id, reply }) => {
                let report = if active.as_ref().is_some_and(|a| a.id == id) {
                    finish(&mut e, &mut active, &status).await
                } else {
                    status.borrow().report.clone()
                };
                let _ = reply.send(report);
            }
            Some(Command::Recover(reply)) => {
                let report = if active.is_none() {
                    finish(&mut e, &mut active, &status).await
                } else {
                    status.borrow().report.clone()
                };
                let _ = reply.send(report);
            }
            None => {
                if let Some(a) = active.as_mut() {
                    let result = if let Some(error) = event_error {
                        Err(error)
                    } else {
                        e.reconcile(a, false).await
                    };
                    if let Err(error) = result {
                        a.state.send_modify(|s| s.error = Some(error));
                        finish(&mut e, &mut active, &status).await;
                    }
                }
            }
        }
    }
    finish(&mut e, &mut active, &status).await;
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        audio_test::Fixture,
        monitor::routing::graph::{
            Graph, SavedLink, SavedState,
            tests::{graph, target},
        },
    };
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering::SeqCst},
    };
    #[derive(Clone)]
    pub struct Fake {
        pub graph: Arc<Mutex<Graph>>,
        pub unlink_calls: Arc<AtomicUsize>,
        pub fail_after: Arc<AtomicI32>,
        pub fail_restore: Arc<AtomicBool>,
        pub relink: Arc<AtomicBool>,
        pub pause: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
        pub entered: Arc<AtomicBool>,
        pub journal_path: Option<PathBuf>,
    }
    impl Fake {
        pub fn new() -> Self {
            Self {
                graph: Arc::new(Mutex::new(graph())),
                unlink_calls: Arc::new(AtomicUsize::new(0)),
                fail_after: Arc::new(AtomicI32::new(-1)),
                fail_restore: Arc::new(AtomicBool::new(false)),
                relink: Arc::new(AtomicBool::new(false)),
                pause: Arc::new(Mutex::new(None)),
                entered: Arc::new(AtomicBool::new(false)),
                journal_path: None,
            }
        }
    }
    #[async_trait::async_trait]
    impl RoutingBackend for Fake {
        async fn snapshot(&self) -> Result<Graph, String> {
            let rx = self.pause.lock().unwrap().take();
            if let Some(rx) = rx {
                self.entered.store(true, SeqCst);
                let _ = rx.await;
            }
            Ok(self.graph.lock().unwrap().clone())
        }
        async fn connect(&self, e: &SavedLink) -> Result<(), String> {
            if self.fail_restore.load(SeqCst) {
                return Err("restauração falhou".into());
            }
            self.graph.lock().unwrap().insert(e.clone());
            Ok(())
        }
        async fn disconnect(&self, e: &SavedLink) -> Result<(), String> {
            if let Some(path) = &self.journal_path {
                assert!(
                    std::fs::read_to_string(path)
                        .unwrap()
                        .contains(&e.output.node_name),
                    "unlink before durable intent"
                );
            }
            let n = self.unlink_calls.fetch_add(1, SeqCst) as i32;
            if n == self.fail_after.load(SeqCst) {
                return Err("remoção falhou".into());
            }
            let mut g = self.graph.lock().unwrap();
            g.remove(e);
            if self.relink.load(SeqCst) {
                g.insert(e.clone());
            }
            Ok(())
        }
    }
    async fn ready(s: &mut RouteSession) {
        tokio::time::timeout(std::time::Duration::from_secs(3), s.ready())
            .await
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn activation_links_return_then_journals_unlinks_and_verifies() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("journal.json");
        let mut b = Fake::new();
        b.journal_path = Some(path.clone());
        let expected = b.graph.lock().unwrap().links.clone();
        let r = Routing::new(Arc::new(b.clone()), path);
        let mut s = r.start(
            1,
            target(),
            "micbridge_monitor_test".into(),
            Arc::new(AtomicBool::new(true)),
        );
        ready(&mut s).await;
        assert!(
            b.graph
                .lock()
                .unwrap()
                .competitors(&target(), "micbridge_monitor_test")
                .unwrap()
                .is_empty()
        );
        assert_eq!(b.unlink_calls.load(SeqCst), 4);
        let report = s.stop_and_restore().await;
        assert_eq!(report.restored, 4);
        assert_eq!(report.pending, 0);
        let actual = b.graph.lock().unwrap().links.clone();
        assert_eq!(actual.len(), expected.len());
        for e in expected {
            assert!(actual.contains(&e));
        }
    }
    #[tokio::test]
    async fn new_competitor_is_removed_and_restored_once() {
        let f = Fixture::new("exit 0");
        let b = Fake::new();
        let r = Routing::new(Arc::new(b.clone()), f.dir.join("journal.json"));
        let competitor = b.graph.lock().unwrap().links[2].clone();
        let mut s = r.start(
            1,
            target(),
            "micbridge_monitor_test".into(),
            Arc::new(AtomicBool::new(true)),
        );
        ready(&mut s).await;
        b.graph.lock().unwrap().insert(competitor);
        crate::audio_test::wait(|| b.unlink_calls.load(SeqCst) >= 5);
        assert_eq!(s.stop_and_restore().await.restored, 4);
        b.relink.store(true, SeqCst);
        let mut s = r.start(
            2,
            target(),
            "micbridge_monitor_test".into(),
            Arc::new(AtomicBool::new(true)),
        );
        assert!(s.ready().await.is_err());
        assert_eq!(s.stop_and_restore().await.pending, 0);
    }
    #[tokio::test]
    async fn partial_failure_rolls_back_and_retry_keeps_pending_record() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("journal.json");
        let b = Fake::new();
        b.fail_after.store(1, SeqCst);
        b.fail_restore.store(true, SeqCst);
        let r = Routing::new(Arc::new(b.clone()), path.clone());
        let mut s = r.start(
            1,
            target(),
            "micbridge_monitor_test".into(),
            Arc::new(AtomicBool::new(true)),
        );
        assert!(s.ready().await.is_err());
        let report = s.stop_and_restore().await;
        assert!(report.pending > 0);
        assert!(path.exists());
        b.fail_restore.store(false, SeqCst);
        assert_eq!(r.recover().await.pending, 0);
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn cancelled_or_replaced_route_cannot_unlink_after_cleanup() {
        let f = Fixture::new("exit 0");
        let b = Fake::new();
        let r = Routing::new(Arc::new(b.clone()), f.dir.join("journal.json"));
        r.recover().await;
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        let lease = Arc::new(AtomicBool::new(true));
        let mut s = r.start(1, target(), "micbridge_monitor_test".into(), lease.clone());
        let end = Instant::now() + Duration::from_secs(3);
        while !b.entered.load(SeqCst) {
            assert!(
                Instant::now() < end,
                "snapshot not entered: {:?}",
                s.state.borrow().clone()
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        lease.store(false, SeqCst);
        let stop = s.stop_and_restore();
        tokio::pin!(stop);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut stop)
                .await
                .is_err()
        );
        let _ = tx.send(());
        assert_eq!(stop.await.pending, 0);
        assert_eq!(b.unlink_calls.load(SeqCst), 0);
        assert!(s.ready().await.is_err());
    }
    #[tokio::test]
    async fn recovery_respects_core_and_endpoint_identity() {
        use super::super::journal::Journal;
        let f = Fixture::new("exit 0");
        let path = f.dir.join("journal.json");
        let b = Fake::new();
        let mut entries = b
            .graph
            .lock()
            .unwrap()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap();
        {
            let mut g = b.graph.lock().unwrap();
            for e in &entries {
                g.remove(e);
            }
        }
        entries[0].core.cookie += 1;
        {
            Journal::open(path.clone()).unwrap().save(&entries).unwrap();
        }
        let r = Routing::new(Arc::new(b.clone()), path);
        let report = r.recover().await;
        assert_eq!(report.pending, 0);
        assert_eq!(
            b.graph.lock().unwrap().validate_saved(&entries[1]),
            SavedState::Present
        );
        assert_eq!(b.graph.lock().unwrap().links.len(), 7);
    }
}
