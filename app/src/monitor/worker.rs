use super::{
    Block, Control, Inner, Settlement,
    gain::GainRamp,
    routing::{RestoreReport, RouteSession, Routing},
};
use crate::output::DieWithParent;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering::SeqCst, mpsc},
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
    sync::watch,
};
pub(super) fn spawn(
    core: Arc<Inner>,
    queue: mpsc::Receiver<Block>,
    commands: watch::Receiver<Control>,
    ack: watch::Sender<Settlement>,
    executable: PathBuf,
    routing: Routing,
    names: Arc<dyn Fn(u64) -> String + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("micbridge-monitor".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime de retorno");
            rt.block_on(run(core, queue, commands, ack, executable, routing, names));
        })
        .expect("thread de retorno");
}
async fn run(
    core: Arc<Inner>,
    queue: mpsc::Receiver<Block>,
    mut commands: watch::Receiver<Control>,
    ack: watch::Sender<Settlement>,
    executable: PathBuf,
    routing: Routing,
    names: Arc<dyn Fn(u64) -> String + Send + Sync>,
) {
    let initial = routing.recover().await;
    restored(&core, &initial);
    let mut pending = None;
    loop {
        let command = match pending.take() {
            Some(c) => c,
            None => {
                if commands.changed().await.is_err() {
                    break;
                }
                commands.borrow_and_update().clone()
            }
        };
        let operation = match &command {
            Control::Stop { operation } | Control::Open { operation, .. } => *operation,
        };
        let report = routing.recover().await;
        restored(&core, &report);
        let error = if report.pending > 0 {
            Some(
                report
                    .error
                    .clone()
                    .unwrap_or_else(|| "Restauração de conexões pendente".into()),
            )
        } else {
            None
        };
        ack.send_replace(Settlement {
            operation,
            error: error.clone(),
        });
        let Control::Open {
            id,
            generation,
            target,
            lease,
            ..
        } = command
        else {
            continue;
        };
        if let Some(error) = error {
            fail(&core, id, error);
            continue;
        }
        while queue.try_recv().is_ok() {}
        if !lease.load(SeqCst) || core.gate.load(SeqCst) != id << 1 {
            close_gate(&core, id);
            continue;
        }
        let Some(serial) = target
            .serial
            .as_deref()
            .filter(|s| s.parse::<u64>().is_ok())
        else {
            fail(
                &core,
                id,
                "A saída não possui identificação estável. Atualize a lista e escolha outra saída."
                    .into(),
            );
            continue;
        };
        let name = names(id);
        let properties=serde_json::json!({"node.dont-fallback":true,"node.dont-reconnect":true,"node.name":name}).to_string();
        let mut child = match Command::new(&executable)
            .args([
                "--playback",
                "--raw",
                "--target",
                serial,
                "--properties",
                &properties,
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
                "-",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .die_with_parent()
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                fail(&core, id, format!("Não foi possível abrir o retorno: {e}"));
                continue;
            }
        };
        let mut stdin = child.stdin.take().expect("stdin solicitado");
        if routing.requires_graph() {
            let prime =
                tokio::time::timeout(Duration::from_millis(250), stdin.write_all(&[0u8; 480 * 4]))
                    .await;
            if !matches!(prime, Ok(Ok(()))) {
                let _ = child.kill().await;
                let _ = child.wait().await;
                fail(
                    &core,
                    id,
                    "Não foi possível preparar o fluxo de retorno".into(),
                );
                continue;
            }
        }
        let mut route = routing.start(id, target, name, lease.clone());
        let opening = tokio::select! {biased;
            changed=commands.changed()=>Some(next_command(changed,&mut commands)),
            _=cancelled(&core,id,&lease,false)=>Some(End::Stopped),
            result=child.wait()=>Some(End::Error(format!("O reprodutor encerrou durante a preparação: {result:?}"))),
            result=route.ready()=>result.err().map(End::Error),
        };
        let end = if let Some(end) = opening {
            end
        } else if super::Monitor::publish(&core, id, &lease) {
            stream(
                &core,
                &queue,
                &mut commands,
                &mut child,
                id,
                generation,
                &lease,
                &mut route,
                stdin,
            )
            .await
        } else {
            End::Stopped
        };
        close_gate(&core, id);
        let _ = child.kill().await;
        let _ = child.wait().await;
        if let End::Error(error) = &end {
            fail(&core, id, error.clone());
        }
        {
            let mut state = core.status.lock().unwrap();
            if state.id == id
                && state.phase != super::MonitorPhase::Disabled
                && core.failed.load(SeqCst) != id
            {
                state.phase = super::MonitorPhase::Restoring;
            }
        }
        let report = route.stop_and_restore().await;
        restored(&core, &report);
        match end {
            End::Command(c) => pending = Some(c),
            End::Closed => break,
            End::Stopped | End::Error(_) => {}
        }
    }
    let report = routing.recover().await;
    restored(&core, &report);
}
fn restored(core: &Inner, report: &RestoreReport) {
    let mut state = core.status.lock().unwrap();
    state.restore_report = Some(report.clone());
    state.restore_pending = report.pending > 0;
    if report.pending > 0 {
        state.phase = super::MonitorPhase::Failed;
        state.error = report.error.clone();
    } else if state.phase == super::MonitorPhase::Restoring {
        state.phase = super::MonitorPhase::Disabled;
        state.error = None;
    }
}
enum End {
    Command(Control),
    Closed,
    Stopped,
    Error(String),
}
async fn cancelled(core: &Inner, id: u64, lease: &std::sync::atomic::AtomicBool, active: bool) {
    loop {
        let notification = core.cancel.notified();
        if core.gate.load(SeqCst) != (id << 1 | u64::from(active)) || !lease.load(SeqCst) {
            return;
        }
        notification.await;
    }
}
fn next_command(
    result: Result<(), watch::error::RecvError>,
    commands: &mut watch::Receiver<Control>,
) -> End {
    if result.is_err() {
        End::Closed
    } else {
        End::Command(commands.borrow_and_update().clone())
    }
}
async fn stream(
    core: &Inner,
    queue: &mpsc::Receiver<Block>,
    commands: &mut watch::Receiver<Control>,
    child: &mut Child,
    id: u64,
    generation: u64,
    lease: &std::sync::atomic::AtomicBool,
    route: &mut RouteSession,
    mut stdin: tokio::process::ChildStdin,
) -> End {
    let mut interval = tokio::time::interval(Duration::from_millis(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut gain = GainRamp::new(f32::from_bits(core.volume.load(SeqCst)));
    loop {
        tokio::select! {biased;
            changed=commands.changed()=>return next_command(changed,commands),
            _=cancelled(core,id,lease,true)=>return End::Stopped,
            error=route.failure()=>return End::Error(error),
            result=child.wait()=>return End::Error(format!("O reprodutor do retorno encerrou: {result:?}")),
            _=interval.tick()=>{}
        }
        let Ok(mut block) = queue.try_recv() else {
            continue;
        };
        if block.id != id
            || block.generation != generation
            || !super::Monitor::valid(core, &block)
            || !lease.load(SeqCst)
        {
            continue;
        }
        gain.set_percent(f32::from_bits(core.volume.load(SeqCst)));
        gain.apply(&mut block.samples);
        let bytes: Vec<u8> = block.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let result = tokio::select! {biased;
            changed=commands.changed()=>return next_command(changed,commands),
            _=cancelled(core,id,lease,true)=>return End::Stopped,
            error=route.failure()=>return End::Error(error),
            result=tokio::time::timeout(Duration::from_millis(250),stdin.write_all(&bytes))=>result,
        };
        if !matches!(result, Ok(Ok(()))) {
            return End::Error("Falha ou espera excessiva ao escrever no retorno".into());
        }
    }
}
fn close_gate(core: &Inner, id: u64) {
    let mut gate = core.gate.load(SeqCst);
    while gate != 0 && gate >> 1 == id {
        match core.gate.compare_exchange(gate, 0, SeqCst, SeqCst) {
            Ok(_) => break,
            Err(current) => gate = current,
        }
    }
}
pub(super) fn fail(core: &Inner, id: u64, message: String) {
    let mut s = core.status.lock().unwrap();
    if s.id != id || s.phase == super::MonitorPhase::Disabled {
        return;
    }
    close_gate(core, id);
    core.failed.fetch_max(id, SeqCst);
    s.error = Some(message);
}
