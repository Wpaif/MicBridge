use super::graph::{Graph, SavedLink, SavedState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
#[async_trait::async_trait]
pub trait RoutingBackend: Send + Sync {
    fn watch(&self) -> Result<Option<Changes>, String> {
        Ok(None)
    }
    async fn snapshot(&self) -> Result<Graph, String>;
    async fn connect(&self, link: &SavedLink) -> Result<(), String>;
    async fn disconnect(&self, link: &SavedLink) -> Result<(), String>;
    async fn disconnect_current(&self, link: &SavedLink, lease: &AtomicBool) -> Result<(), String> {
        if !lease.load(SeqCst) {
            return Err("Retorno cancelado".into());
        }
        self.disconnect(link).await
    }
}
pub struct Changes {
    child: tokio::process::Child,
    out: tokio::process::ChildStdout,
}
impl Changes {
    pub async fn next(&mut self) -> Result<(), String> {
        use tokio::io::AsyncReadExt;
        let mut bytes = [0u8; 4096];
        match self.out.read(&mut bytes).await {
            Ok(0) => Err("Observação das conexões PipeWire encerrou".into()),
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub async fn stop(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}
pub struct RealBackend {
    dump: PathBuf,
    link: PathBuf,
}
impl Default for RealBackend {
    fn default() -> Self {
        Self {
            dump: "pw-dump".into(),
            link: "pw-link".into(),
        }
    }
}
#[async_trait::async_trait]
impl RoutingBackend for RealBackend {
    fn watch(&self) -> Result<Option<Changes>, String> {
        use crate::output::DieWithParent;
        let mut child = tokio::process::Command::new(&self.link)
            .args(["-l", "-I", "-m"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .die_with_parent()
            .spawn()
            .map_err(|e| e.to_string())?;
        let out = child.stdout.take().expect("stdout solicitado");
        Ok(Some(Changes { child, out }))
    }

    async fn snapshot(&self) -> Result<Graph, String> {
        Graph::parse(&run(&self.dump, &[]).await?)
    }
    async fn connect(&self, link: &SavedLink) -> Result<(), String> {
        self.change(link, true, None).await
    }
    async fn disconnect(&self, link: &SavedLink) -> Result<(), String> {
        self.change(link, false, None).await
    }
    async fn disconnect_current(&self, link: &SavedLink, lease: &AtomicBool) -> Result<(), String> {
        self.change(link, false, Some(lease)).await
    }
}
impl RealBackend {
    async fn change(
        &self,
        link: &SavedLink,
        connect: bool,
        lease: Option<&AtomicBool>,
    ) -> Result<(), String> {
        let before = self.snapshot().await?.validate_saved(link);
        if before == SavedState::Unavailable {
            return Err("Conexão removida ou substituída antes da operação".into());
        }
        let expected = if connect {
            SavedState::Present
        } else {
            SavedState::Missing
        };
        if before == expected {
            return Ok(());
        }
        let mut args = vec![];
        if connect {
            args.extend([
                "-p".into(),
                serde_json::json!({"link.passive":link.passive}).to_string(),
            ]);
        } else {
            args.push("-d".into());
        }
        args.extend([
            link.output.port.id.to_string(),
            link.input.port.id.to_string(),
        ]);
        if lease.is_some_and(|lease| !lease.load(SeqCst)) {
            return Err("Retorno cancelado".into());
        }
        run(&self.link, &args).await?;
        if self.snapshot().await?.validate_saved(link) != expected {
            return Err("PipeWire não confirmou a alteração da conexão".into());
        }
        Ok(())
    }
}
async fn run(exe: &std::path::Path, args: &[String]) -> Result<String, String> {
    let exe = exe.to_owned();
    let args = args.to_vec();
    let (mut tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        use crate::output::DieWithParent;
        use tokio::io::AsyncReadExt;
        let mut child = match tokio::process::Command::new(&exe)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .die_with_parent()
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Err(format!("{}: {e}", exe.display())));
                return;
            }
        };
        let out = child.stdout.take().expect("stdout solicitado");
        let result = tokio::select! {
            _=tx.closed()=>Err("Operação PipeWire cancelada".into()),
            result=tokio::time::timeout(std::time::Duration::from_secs(2),async {
                let mut bytes=vec![];out.take(8*1024*1024+1).read_to_end(&mut bytes).await.map_err(|e|e.to_string())?;
                if bytes.len()>8*1024*1024 {return Err("Grafo excedeu 8 MiB".into())}
                let status=child.wait().await.map_err(|e|e.to_string())?;
                if !status.success(){return Err(format!("{} encerrou com {status}",exe.display()))}
                String::from_utf8(bytes).map_err(|e|e.to_string())
            })=>result.unwrap_or_else(|_|Err("Operação PipeWire excedeu 2 segundos".into())),
        };
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let _ = tx.send(result);
    });
    rx.await
        .map_err(|_| "Executor de comandos encerrou".to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio_test::{Fixture, SLEEP, alive, wait},
        monitor::routing::graph::tests::{fixture, graph, target},
    };
    #[tokio::test]
    async fn watch_reports_eof_and_reaps() {
        let f = Fixture::new(&format!("printf 'change\\n'; {SLEEP}"));
        let backend = RealBackend {
            dump: f.executable.clone(),
            link: f.executable.clone(),
        };
        let mut events = backend.watch().unwrap().unwrap();
        events.next().await.unwrap();
        wait(|| f.pid().is_some());
        let pid = f.pid().unwrap();
        events.stop().await;
        assert!(!alive(pid));
        let f = Fixture::new("exit 0");
        let backend = RealBackend {
            dump: f.executable.clone(),
            link: f.executable.clone(),
        };
        let mut events = backend.watch().unwrap().unwrap();
        assert!(events.next().await.is_err());
        events.stop().await;
    }
    #[tokio::test]
    async fn cancelled_backend_validation_does_not_launch_unlink() {
        let f = Fixture::new(
            "touch '@DIR@/entered'; while [ ! -f '@DIR@/release' ]; do sleep 0.01; done; cat '@DIR@/graph.json'",
        );
        std::fs::write(f.dir.join("graph.json"), fixture().to_string()).unwrap();
        let b = RealBackend {
            dump: f.executable.clone(),
            link: f.dir.join("must-not-run"),
        };
        let link = graph()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap()
            .remove(0);
        let lease = std::sync::Arc::new(AtomicBool::new(true));
        let task_lease = lease.clone();
        let task = tokio::spawn(async move { b.disconnect_current(&link, &task_lease).await });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !f.dir.join("entered").exists() {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        lease.store(false, SeqCst);
        std::fs::write(f.dir.join("release"), "").unwrap();
        assert_eq!(task.await.unwrap().unwrap_err(), "Retorno cancelado");
    }
    #[tokio::test]
    async fn commands_revalidate_and_confirm_endpoints() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new("cat '@DIR@/graph.json'");
        let linker = f.dir.join("link");
        std::fs::write(&linker,format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/args'\ncp '{}/after.json' '{}/graph.json'\n",f.dir.display(),f.dir.display(),f.dir.display())).unwrap();
        std::fs::set_permissions(&linker, std::fs::Permissions::from_mode(0o700)).unwrap();
        let b = RealBackend {
            dump: f.executable.clone(),
            link: linker,
        };
        let e = graph()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap()
            .remove(0);
        let present = fixture();
        let mut absent = present.clone();
        absent.as_array_mut().unwrap().retain(|n| n["id"] != 1002);
        std::fs::write(f.dir.join("graph.json"), present.to_string()).unwrap();
        std::fs::write(f.dir.join("after.json"), absent.to_string()).unwrap();
        b.disconnect(&e).await.unwrap();
        assert_eq!(
            b.snapshot().await.unwrap().validate_saved(&e),
            super::super::graph::SavedState::Missing
        );
        assert_eq!(
            std::fs::read_to_string(f.dir.join("args"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            ["-d", "31", "11"]
        );
        std::fs::write(f.dir.join("after.json"), present.to_string()).unwrap();
        b.connect(&e).await.unwrap();
        assert_eq!(
            b.snapshot().await.unwrap().validate_saved(&e),
            super::super::graph::SavedState::Present
        );
        std::fs::remove_file(f.dir.join("args")).unwrap();
        let mut reused = present.clone();
        for n in reused.as_array_mut().unwrap() {
            if n["id"] == 31 {
                n["info"]["props"]["object.serial"] = serde_json::json!(9999);
            }
        }
        std::fs::write(f.dir.join("graph.json"), reused.to_string()).unwrap();
        assert!(b.disconnect(&e).await.is_err());
        assert!(!f.dir.join("args").exists());
        std::fs::write(f.dir.join("graph.json"), present.to_string()).unwrap();
        assert!(
            b.disconnect(&e).await.is_err(),
            "exit 0 must not confirm unchanged graph"
        );
    }
    #[tokio::test]
    async fn backend_reaps_timeout_exit_and_oversized_snapshot() {
        let f = Fixture::new(SLEEP);
        assert!(run(&f.executable, &[]).await.is_err());
        wait(|| f.pid().is_some_and(|p| !alive(p)));
        let f = Fixture::new("exit 8");
        assert!(run(&f.executable, &[]).await.is_err());
        let f = Fixture::new("head -c 8388609 /dev/zero");
        assert!(run(&f.executable, &[]).await.is_err());
        let f = Fixture::new(SLEEP);
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(150),
                run(&f.executable, &[])
            )
            .await
            .is_err()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !f.pid().is_some_and(|p| !alive(p)) {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
}
