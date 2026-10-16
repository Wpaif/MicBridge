//! Reprodução local exclusiva do teste A/B, separada do microfone virtual.
use super::{Take, Version};
use crate::output::DieWithParent;
use std::{ffi::OsString, path::PathBuf, sync::Arc};
#[derive(Clone, Debug, Default)]
pub struct PlaybackStatus {
    pub operation: u64,
    pub take_id: Option<u64>,
    pub version: Option<Version>,
    pub playing: bool,
    pub error: Option<String>,
}
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering::SeqCst},
};
use tokio::{io::AsyncWriteExt, process::Command, sync::watch};
#[derive(Clone)]
enum Request {
    Stop {
        operation: u64,
    },
    Play {
        operation: u64,
        take: Arc<Take>,
        version: Version,
    },
}
struct Inner {
    tx: watch::Sender<Request>,
    status: Arc<Mutex<PlaybackStatus>>,
    next: AtomicU64,
    settled: watch::Receiver<u64>,
}
#[derive(Clone)]
pub struct Playback {
    inner: Arc<Inner>,
}
impl Playback {
    pub fn new() -> Self {
        Self::with_executable("pw-cat".into())
    }
    fn with_executable(executable: PathBuf) -> Self {
        Self::with_command(
            executable,
            [
                "--playback",
                "--raw",
                "--rate",
                "48000",
                "--channels",
                "1",
                "--format",
                "f32",
                "--media-role",
                "Music",
                "-",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
        )
    }
    pub(crate) fn with_command(executable: PathBuf, args: Vec<OsString>) -> Self {
        let (tx, mut rx) = watch::channel(Request::Stop { operation: 0 });
        let (settled_tx, settled) = watch::channel(0u64);
        let status = Arc::new(Mutex::new(PlaybackStatus::default()));
        let worker_status = status.clone();
        std::thread::Builder::new().name("micbridge-playback".into()).spawn(move || {
            let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime de reprodução");
            rt.block_on(async move {
                let mut pending=None;
                loop {
                    let request=match pending.take() {
                        Some(request)=>request,
                        None=>{if rx.changed().await.is_err() {break;}rx.borrow_and_update().clone()}
                    };
                    let operation=match &request {Request::Stop{operation}|Request::Play{operation,..}=>*operation};
                    // The previous child's kill/wait completed before this command is acknowledged.
                    settled_tx.send_replace(operation);
                    let Request::Play {operation,take,version}=request else {continue};
                    let samples=match version {Version::Original=>&take.original,Version::Treated=>&take.treated};
                    if samples.is_empty() {complete(&worker_status,operation,Some("Teste sem áudio para reproduzir".into()));continue;}
                    let mut child=match Command::new(&executable).args(&args)
                        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                        .kill_on_drop(true).die_with_parent().spawn() {
                        Ok(child)=>child,Err(e)=>{complete(&worker_status,operation,Some(format!("Não foi possível abrir a reprodução: {e}")));continue;}
                    };
                    let mut stdin=child.stdin.take().expect("stdin solicitado");
                    let bytes:Vec<u8>=samples.iter().flat_map(|s|s.to_le_bytes()).collect();
                    let result=tokio::select! {
                        biased;
                        changed=rx.changed()=>{
                            let _=child.kill().await;let _=child.wait().await;
                            if changed.is_err() {break;}
                            pending=Some(rx.borrow_and_update().clone());
                            continue;
                        }
                        result=async {
                            stdin.write_all(&bytes).await?;
                            stdin.shutdown().await?;
                            drop(stdin);
                            let status=child.wait().await?;
                            if status.success() {Ok(())} else {Err(std::io::Error::other(format!("reprodutor encerrou com {status}")))}
                        }=>result,
                    };
                    if result.is_err() {let _=child.kill().await;let _=child.wait().await;}
                    complete(&worker_status,operation,result.err().map(|e|format!("Falha na reprodução: {e}")));
                }
            });
        }).expect("thread de reprodução");
        Self {
            inner: Arc::new(Inner {
                tx,
                status,
                next: AtomicU64::new(0),
                settled,
            }),
        }
    }
    pub fn play(&self, take: Arc<Take>, version: Version) {
        let mut status = self.inner.status.lock().unwrap();
        let operation = self.inner.next.fetch_add(1, SeqCst) + 1;
        *status = PlaybackStatus {
            operation,
            take_id: Some(take.id),
            version: Some(version),
            playing: true,
            error: None,
        };
        self.inner.tx.send_replace(Request::Play {
            operation,
            take,
            version,
        });
    }
    pub fn stop_and_wait(
        &self,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
        let (operation, mut settled) = self.request_stop();
        async move {
            loop {
                if *settled.borrow_and_update() >= operation {
                    return Ok(());
                }
                settled
                    .changed()
                    .await
                    .map_err(|_| "Worker A/B encerrou antes de confirmar a parada".to_string())?;
            }
        }
    }
    fn request_stop(&self) -> (u64, watch::Receiver<u64>) {
        let mut status = self.inner.status.lock().unwrap();
        let operation = self.inner.next.fetch_add(1, SeqCst) + 1;
        *status = PlaybackStatus {
            operation,
            ..PlaybackStatus::default()
        };
        self.inner.tx.send_replace(Request::Stop { operation });
        (operation, self.inner.settled.clone())
    }
    pub fn stop(&self) {
        self.request_stop();
    }
    pub fn status(&self) -> PlaybackStatus {
        self.inner.status.lock().unwrap().clone()
    }
}
fn complete(status: &Mutex<PlaybackStatus>, operation: u64, error: Option<String>) {
    let mut status = status.lock().unwrap();
    if status.operation == operation {
        status.playing = false;
        status.error = error;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "micbridge-playback-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn wait(mut condition: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(3);
        while !condition() {
            assert!(Instant::now() < end, "condition timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn take() -> Arc<Take> {
        Arc::new(Take {
            id: 9,
            original: vec![0.1, 0.2],
            treated: vec![0.3, 0.4],
        })
    }
    fn player(d: &Temp, script: &str) -> Playback {
        Playback::with_command(
            "/bin/sh".into(),
            vec![
                "-c".into(),
                script.into(),
                "playback-test".into(),
                d.0.clone().into_os_string(),
            ],
        )
    }
    fn alive(pid: &str) -> bool {
        std::path::Path::new("/proc").join(pid.trim()).exists()
    }
    #[test]
    fn writes_selected_pcm_and_uses_default_output_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let d = Temp::new();
        let exe = d.0.join("player");
        std::fs::write(
            &exe,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/args'\ncat > '{}/pcm'\n",
                d.0.display(),
                d.0.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let p = Playback::with_executable(exe);
        p.play(take(), Version::Original);
        wait(|| d.0.join("pcm").exists() && !p.status().playing);
        let bytes = std::fs::read(d.0.join("pcm")).unwrap();
        assert_eq!(bytes, [0.1f32.to_le_bytes(), 0.2f32.to_le_bytes()].concat());
        assert_eq!(
            std::fs::read_to_string(d.0.join("args"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "--playback",
                "--raw",
                "--rate",
                "48000",
                "--channels",
                "1",
                "--format",
                "f32",
                "--media-role",
                "Music",
                "-"
            ]
        );
        assert!(p.status().error.is_none());
        p.play(take(), Version::Treated);
        wait(|| !p.status().playing);
        assert_eq!(
            std::fs::read(d.0.join("pcm")).unwrap(),
            [0.3f32.to_le_bytes(), 0.4f32.to_le_bytes()].concat()
        );
    }
    #[test]
    fn reports_spawn_exit_and_pipe_errors() {
        let d = Temp::new();
        let p = Playback::with_command(d.0.join("missing"), vec![]);
        p.play(take(), Version::Original);
        wait(|| p.status().error.is_some());
        assert!(!p.status().playing);
        for script in ["exit 7", "exec 0<&-; exit 8"] {
            let p = player(&d, script);
            p.play(
                Arc::new(Take {
                    id: 1,
                    original: vec![0.1; 1_440_000],
                    treated: vec![],
                }),
                Version::Original,
            );
            wait(|| p.status().error.is_some());
            assert!(!p.status().playing);
        }
    }
    #[test]
    fn replacement_stops_previous_child_before_starting_next_and_ignores_old_completion() {
        let d = Temp::new();
        let p = player(&d, "echo $$ > \"$1/pid\"; exec sleep 30");
        p.play(take(), Version::Original);
        wait(|| {
            std::fs::read_to_string(d.0.join("pid"))
                .is_ok_and(|pid| pid.trim().parse::<u32>().is_ok())
        });
        let old = std::fs::read_to_string(d.0.join("pid")).unwrap();
        let old_op = p.status().operation;
        p.play(
            Arc::new(Take {
                id: 10,
                original: vec![0.1],
                treated: vec![0.2],
            }),
            Version::Treated,
        );
        wait(|| {
            std::fs::read_to_string(d.0.join("pid"))
                .is_ok_and(|pid| pid != old && pid.trim().parse::<u32>().is_ok())
        });
        assert!(!alive(&old));
        let s = p.status();
        assert!(s.operation > old_op);
        assert!(s.playing);
        assert_eq!(s.take_id, Some(10));
        assert_eq!(s.version, Some(Version::Treated));
        assert!(s.error.is_none());
        let new = std::fs::read_to_string(d.0.join("pid")).unwrap();
        p.stop();
        wait(|| !alive(&new));
        assert!(!p.status().playing);
    }
    #[test]
    fn starting_capture_or_shutting_down_stops_playback() {
        for shutdown in [false, true] {
            let d = Temp::new();
            let mut shared = crate::state::Shared::new();
            shared.set_link(crate::state::Link::Connected);
            shared.playback = player(&d, "echo $$ > \"$1/pid\"; exec sleep 30");
            shared.playback.play(take(), Version::Original);
            wait(|| {
                std::fs::read_to_string(d.0.join("pid"))
                    .is_ok_and(|pid| pid.trim().parse::<u32>().is_ok())
            });
            let pid = std::fs::read_to_string(d.0.join("pid")).unwrap();
            if shutdown {
                shared.stop_audio_tools();
            } else {
                shared.start_recording();
            }
            assert!(!shared.playback.status().playing);
            wait(|| !alive(&pid));
        }
    }
    #[tokio::test]
    async fn coalesced_stop_acknowledges_child_reaping() {
        let d = Temp::new();
        let p = player(&d, "echo $$ > \"$1/pid\"; exec sleep 30");
        p.play(take(), Version::Original);
        wait(|| {
            std::fs::read_to_string(d.0.join("pid")).is_ok_and(|p| p.trim().parse::<u32>().is_ok())
        });
        let old = std::fs::read_to_string(d.0.join("pid")).unwrap();
        let stopped = p.stop_and_wait();
        p.play(take(), Version::Treated);
        tokio::time::timeout(Duration::from_secs(2), stopped)
            .await
            .unwrap()
            .unwrap();
        assert!(!alive(&old));
        p.stop_and_wait().await.unwrap();
    }
    #[tokio::test]
    async fn stop_ack_proves_child_reaped_before_it_resolves() {
        let d = Temp::new();
        let p = player(&d, "echo $$ > \"$1/pid\"; exec sleep 30");
        p.play(take(), Version::Original);
        wait(|| {
            std::fs::read_to_string(d.0.join("pid"))
                .is_ok_and(|pid| pid.trim().parse::<u32>().is_ok())
        });
        let pid = std::fs::read_to_string(d.0.join("pid")).unwrap();
        tokio::time::timeout(Duration::from_secs(2), p.stop_and_wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!alive(&pid));
    }
    #[test]
    fn drop_cancels_blocked_write_and_reaps_child() {
        let d = Temp::new();
        let p = player(&d, "echo $$ > \"$1/pid\"; exec sleep 30");
        p.play(
            Arc::new(Take {
                id: 1,
                original: vec![0.1; 1_440_000],
                treated: vec![],
            }),
            Version::Original,
        );
        wait(|| {
            std::fs::read_to_string(d.0.join("pid"))
                .is_ok_and(|pid| pid.trim().parse::<u32>().is_ok())
        });
        let pid = std::fs::read_to_string(d.0.join("pid")).unwrap();
        drop(p);
        wait(|| !alive(&pid));
    }
}
