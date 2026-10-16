use super::{Block, Control, Inner, gain::GainRamp};
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
    mut commands: watch::Receiver<Control>,
    ack: watch::Sender<u64>,
    executable: PathBuf,
) {
    std::thread::Builder::new()
        .name("micbridge-monitor".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime de retorno");
            rt.block_on(async move {
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
                    ack.send_replace(operation);
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
                    // Remove old queue entries before enabling the producer for this session.
                    while queue.try_recv().is_ok() {}
                    if !lease.load(SeqCst) || core.gate.load(SeqCst) != id << 1 {
                        close_gate(&core, id);
                        continue;
                    }
                    let mut child = match Command::new(&executable)
                        .args([
                            "--playback",
                            "--raw",
                            "--target",
                            &target.name,
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
                        Ok(child) => child,
                        Err(e) => {
                            fail(&core, id, format!("Não foi possível abrir o retorno: {e}"));
                            continue;
                        }
                    };
                    if !lease.load(SeqCst)
                        || core
                            .gate
                            .compare_exchange(id << 1, id << 1 | 1, SeqCst, SeqCst)
                            .is_err()
                    {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        close_gate(&core, id);
                        continue;
                    }
                    let end = stream(
                        &core,
                        &queue,
                        &mut commands,
                        &mut child,
                        id,
                        generation,
                        &lease,
                    )
                    .await;
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    match end {
                        End::Command(c) => pending = Some(c),
                        End::Closed => break,
                        End::Stopped => {}
                        End::Error(e) => fail(&core, id, e),
                    }
                }
            });
        })
        .expect("thread de retorno");
}
enum End {
    Command(Control),
    Closed,
    Stopped,
    Error(String),
}
async fn cancelled(core: &Inner, id: u64, lease: &std::sync::atomic::AtomicBool) {
    loop {
        let notification = core.cancel.notified();
        if core.gate.load(SeqCst) != id << 1 | 1 || !lease.load(SeqCst) {
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
) -> End {
    let mut stdin = child.stdin.take().expect("stdin solicitado");
    let mut interval = tokio::time::interval(Duration::from_millis(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut gain = GainRamp::new(f32::from_bits(core.volume.load(SeqCst)));
    loop {
        tokio::select! {biased;
            changed=commands.changed()=>return next_command(changed,commands),
            _=cancelled(core,id,lease)=>return End::Stopped,
            result=child.wait()=>return End::Error(format!("O reprodutor do retorno encerrou: {result:?}")),
            _=interval.tick()=>{}
        }
        let Ok(mut block) = queue.try_recv() else {
            continue;
        };
        if block.id != id
            || block.generation != generation
            || core.gate.load(SeqCst) != id << 1 | 1
            || !lease.load(SeqCst)
        {
            continue;
        }
        gain.set_percent(f32::from_bits(core.volume.load(SeqCst)));
        gain.apply(&mut block.samples);
        let bytes: Vec<u8> = block.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let result = tokio::select! {biased;
            changed=commands.changed()=>return next_command(changed,commands),
            _=cancelled(core,id,lease)=>return End::Stopped,
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
