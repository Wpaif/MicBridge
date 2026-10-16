use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDevice {
    pub id: u32,
    pub serial: Option<String>,
    pub name: String,
    pub label: String,
}
impl OutputDevice {
    pub fn same_instance(&self, other: &Self) -> bool {
        self.id == other.id && self.name == other.name && self.serial == other.serial
    }
}
pub fn parse(json: &str) -> Result<Vec<OutputDevice>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Lista de saídas inválida: {e}"))?;
    let nodes = value.as_array().ok_or("Lista de saídas não é um array")?;
    let mut devices = Vec::new();
    for node in nodes {
        if node["type"] != "PipeWire:Interface:Node" {
            continue;
        }
        let p = &node["info"]["props"];
        let (Some(id), Some(name)) = (
            node["id"].as_u64().and_then(|n| u32::try_from(n).ok()),
            p["node.name"].as_str(),
        ) else {
            continue;
        };
        if p["media.class"] != "Audio/Sink"
            || name.trim().is_empty()
            || name == crate::output::SINK
            || name.starts_with("micbridge_monitor")
        {
            continue;
        }
        let serial = p["object.serial"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| p["object.serial"].as_u64().map(|n| n.to_string()));
        let label = p["node.description"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(name)
            .to_owned();
        devices.push(OutputDevice {
            id,
            serial,
            name: name.into(),
            label,
        });
    }
    Ok(devices)
}
pub async fn query() -> Result<Vec<OutputDevice>, String> {
    query_command("pw-dump".into(), vec![]).await
}
#[derive(Clone, Debug, Default)]
pub struct DeviceSnapshot {
    pub query_id: u64,
    pub devices: Vec<OutputDevice>,
    pub busy: bool,
    pub error: Option<String>,
}
pub(crate) type DeviceQuery =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<OutputDevice>, String>> + Send + Sync>;
struct CatalogInner {
    state: Arc<Mutex<DeviceSnapshot>>,
    tx: tokio::sync::watch::Sender<u64>,
    query: DeviceQuery,
}
#[derive(Clone)]
pub struct DeviceCatalog {
    inner: Arc<CatalogInner>,
}
impl DeviceCatalog {
    pub fn new() -> Self {
        Self::with_query(Arc::new(|| Box::pin(query())))
    }
    pub(crate) fn with_query(query: DeviceQuery) -> Self {
        let (tx, mut rx) = tokio::sync::watch::channel(0u64);
        let state = Arc::new(Mutex::new(DeviceSnapshot::default()));
        let worker_state = state.clone();
        let worker_query = query.clone();
        std::thread::Builder::new().name("micbridge-devices".into()).spawn(move|| {
            let rt=tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime de dispositivos");
            rt.block_on(async move {
                let mut pending=None;
                loop {
                    let id=match pending.take() {Some(id)=>id,None=>{if rx.changed().await.is_err(){break;}*rx.borrow_and_update()}};
                    let result=tokio::select! {biased;
                        changed=rx.changed()=>{if changed.is_err(){break;}pending=Some(*rx.borrow_and_update());continue;}
                        result=worker_query()=>result,
                    };
                    let mut s=worker_state.lock().unwrap();
                    if s.query_id!=id {continue;}
                    s.busy=false;
                    match result {Ok(devices)=>{s.devices=devices;s.error=None;},Err(error)=>{s.error=Some(error);}}
                }
            });
        }).expect("thread de dispositivos");
        Self {
            inner: Arc::new(CatalogInner { state, tx, query }),
        }
    }
    pub fn refresh(&self) -> u64 {
        let mut s = self.inner.state.lock().unwrap();
        s.query_id += 1;
        s.busy = true;
        s.error = None;
        self.inner.tx.send_replace(s.query_id);
        s.query_id
    }
    pub fn snapshot(&self) -> DeviceSnapshot {
        self.inner.state.lock().unwrap().clone()
    }
    pub async fn query_now(&self) -> Result<Vec<OutputDevice>, String> {
        (self.inner.query)().await
    }
}
async fn query_command(
    executable: std::path::PathBuf,
    args: Vec<std::ffi::OsString>,
) -> Result<Vec<OutputDevice>, String> {
    use crate::output::DieWithParent;
    use tokio::io::AsyncReadExt;
    let mut child = tokio::process::Command::new(executable)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .die_with_parent()
        .spawn()
        .map_err(|e| format!("Não foi possível consultar saídas: {e}"))?;
    let stdout = child.stdout.take().expect("stdout solicitado");
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut bytes = Vec::new();
        stdout
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Lista de dispositivos excedeu 8 MiB".into());
        }
        let status = child.wait().await.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Consulta de saídas falhou: {status}"));
        }
        parse(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
    })
    .await
    .unwrap_or_else(|_| Err("Consulta de saídas excedeu 2 segundos".into()));
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn node(id: u32, name: &str, label: &str, class: &str) -> serde_json::Value {
        json!({"id":id,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":name,"node.description":label,"media.class":class,"object.serial":id+100}}})
    }
    pub(crate) fn output(id: u32) -> OutputDevice {
        OutputDevice {
            id,
            serial: Some((100 + id).to_string()),
            name: format!("alsa_output.{id}"),
            label: "Fone".into(),
        }
    }
    #[test]
    fn parse_filters_sinks_and_preserves_distinct_identities() {
        let nodes = json!([
            node(1, "alsa.a", "Fone", "Audio/Sink"),
            node(2, "alsa.b", "Fone", "Audio/Sink"),
            node(3, "effects", "Efeitos", "Audio/Sink"),
            node(4, "vdo_input", "MicBridge", "Audio/Sink"),
            node(5, "micbridge_monitor_1", "Retorno", "Audio/Sink"),
            node(6, "mic", "Entrada", "Audio/Source")
        ]);
        let d = parse(&nodes.to_string()).unwrap();
        assert_eq!(
            d.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            ["alsa.a", "alsa.b", "effects"]
        );
        assert_eq!(d[0].serial.as_deref(), Some("101"));
        assert!(!d[0].same_instance(&d[1]));
        assert!(d[0].same_instance(&d[0]));
    }
    #[test]
    fn parse_handles_missing_fields_and_invalid_json() {
        for data in ["bad", "{}", "null"] {
            assert!(parse(data).is_err());
        }
        let mut valid = node(1, "sink", "", "Audio/Sink");
        valid["info"]["props"]
            .as_object_mut()
            .unwrap()
            .remove("node.description");
        valid["info"]["props"]["object.serial"] = json!("101");
        let d=parse(&json!([valid,{"id":2},{"id":3,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink"}}}]).to_string()).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].label, "sink");
        assert_eq!(d[0].serial.as_deref(), Some("101"));
        let mut changed = d[0].clone();
        changed.serial = Some("102".into());
        assert!(!changed.same_instance(&d[0]));
    }
    #[tokio::test]
    async fn query_checks_success_exit_timeout_and_output_limit() {
        let run = |script: &str| query_command("/bin/sh".into(), vec!["-c".into(), script.into()]);
        assert!(run("printf '[]'").await.is_ok());
        assert!(run("exit 3").await.is_err());
        let start = std::time::Instant::now();
        assert!(run("exec sleep 10").await.is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(4));
        assert!(run("head -c 8388609 /dev/zero").await.is_err());
    }
    #[test]
    fn stale_device_query_does_not_overwrite_newer_result() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        let calls = Arc::new(AtomicUsize::new(0));
        let started = calls.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let rx = Arc::new(Mutex::new(Some(rx)));
        let query: DeviceQuery = Arc::new(move || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            let rx = rx.lock().unwrap().take();
            Box::pin(async move {
                if n == 0 {
                    let _ = rx.unwrap().await;
                }
                Ok(vec![output(n as u32)])
            })
        });
        let c = DeviceCatalog::with_query(query);
        let first = c.refresh();
        crate::audio_test::wait(|| started.load(Ordering::SeqCst) > 0);
        let second = c.refresh();
        assert!(second > first);
        let end = Instant::now() + Duration::from_secs(2);
        while c.snapshot().busy && Instant::now() < end {
            std::thread::yield_now();
        }
        let _ = tx.send(());
        let s = c.snapshot();
        assert_eq!(s.query_id, second);
        assert_eq!(s.devices[0].id, 1);
    }
}
