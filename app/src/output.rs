//! Saída para o PipeWire. Reaproveita o esquema do `vdo_audio`:
//! sink `vdo_input` -> fonte virtual `Microfone_VDO`.

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};

pub const SINK: &str = "vdo_input";
const SOURCE: &str = "vdo_mic";

trait DieWithParent {
    fn die_with_parent(&mut self) -> &mut Self;
}

impl DieWithParent for Command {
    /// O kernel mata o filho se o app morrer de qualquer forma (SIGKILL, crash).
    fn die_with_parent(&mut self) -> &mut Self {
        // SAFETY: só chama prctl, que é async-signal-safe.
        unsafe {
            self.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        self
    }
}

/// Cria o microfone virtual (equivalente a `pw-loopback` do `vdo_audio`).
/// O processo morre junto com o app.
pub fn spawn_virtual_mic() -> Result<Child> {
    Command::new("pw-loopback")
        .args([
            "-m",
            "[FL FR]",
            "--capture-props=media.class=Audio/Sink node.name=vdo_input node.description=VDO_Input",
            "--playback-props=media.class=Audio/Source node.name=vdo_mic node.description=Microfone_VDO",
        ])
        .kill_on_drop(true)
        .die_with_parent()
        .spawn()
        .context("falha ao executar pw-loopback (pipewire-utils instalado?)")
}

/// Stream PCM f32 estéreo 48 kHz para o sink via `pw-cat`.
pub struct Sink {
    _child: Child,
    stdin: ChildStdin,
}

impl Sink {
    pub fn open() -> Result<Self> {
        let mut child = Command::new("pw-cat")
            .args([
                "--playback",
                "--raw",
                "--target",
                SINK,
                "--rate",
                "48000",
                "--channels",
                "2",
                "--format",
                "f32",
                "--latency",
                "20ms",
                "--media-role",
                "Communication",
                "-",
            ])
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("falha ao executar pw-cat")?;
        let stdin = child.stdin.take().context("pw-cat sem stdin")?;
        Ok(Self { _child: child, stdin })
    }

    /// Duplica mono em L/R e escreve.
    pub async fn write_mono(&mut self, mono: &[f32]) -> Result<()> {
        let mut bytes = Vec::with_capacity(mono.len() * 8);
        for s in mono {
            let b = s.to_le_bytes();
            bytes.extend_from_slice(&b);
            bytes.extend_from_slice(&b);
        }
        self.stdin.write_all(&bytes).await?;
        Ok(())
    }
}

/// Sobe o Rails (HTTPS na LAN). Morre junto com o app (kill_on_drop).
/// Diretório do projeto: `MICBRIDGE_HOME` ou o local onde foi compilado.
pub fn spawn_web(token: &str) -> Result<Child> {
    let home = std::env::var("MICBRIDGE_HOME")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/..").into());
    Command::new(format!("{home}/scripts/web-https.sh"))
        .env("MICBRIDGE_TOKEN", token)
        .env("MICBRIDGE_PARENT", std::process::id().to_string())
        .kill_on_drop(true)
        .die_with_parent()
        .spawn()
        .context("falha ao iniciar o servidor web (scripts/web-https.sh)")
}

/// Saída que se recupera sozinha: se o `pw-cat` morrer, travar ou o sink sumir,
/// descarta o áudio daquele instante e tenta reabrir (no máximo 1x por segundo).
pub struct ResilientSink {
    sink: Option<Sink>,
    last_try: Option<std::time::Instant>,
}

impl ResilientSink {
    pub fn new() -> Self {
        Self { sink: None, last_try: None }
    }

    pub async fn write_mono(&mut self, mono: &[f32]) {
        if self.sink.is_none() {
            let ready = self
                .last_try
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1));
            if !ready {
                return;
            }
            self.last_try = Some(std::time::Instant::now());
            match Sink::open() {
                Ok(s) => self.sink = Some(s),
                Err(e) => return tracing::warn!("saída de áudio: {e:#}"),
            }
        }
        let sink = self.sink.as_mut().expect("aberto acima");
        // Timeout: se o sink foi recriado, o pw-cat antigo pode parar de consumir o pipe.
        let res = tokio::time::timeout(std::time::Duration::from_millis(250), sink.write_mono(mono)).await;
        if !matches!(res, Ok(Ok(()))) {
            tracing::warn!("saída de áudio falhou; reabrindo");
            self.sink = None; // kill_on_drop encerra o pw-cat
        }
    }
}

/// Lista os nodes do PipeWire e diz se o microfone virtual (sink e fonte) existe.
async fn virtual_mic_exists() -> bool {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        Command::new("pw-cli").args(["ls", "Node"]).kill_on_drop(true).output(),
    )
    .await;
    let Ok(Ok(out)) = out else { return false };
    let text = String::from_utf8_lossy(&out.stdout);
    text.contains(&format!("\"{SINK}\"")) && text.contains(&format!("\"{SOURCE}\""))
}

/// Mantém o microfone virtual vivo: se o node sumir (PipeWire reiniciou, `pw-loopback`
/// morreu), recria sem reiniciar o app. Respeita um `vdo_audio` já rodando por fora.
pub async fn supervise_virtual_mic(shared: std::sync::Arc<crate::state::Shared>) {
    use std::time::Duration;
    let mut child: Option<Child> = None;
    let mut last_ok = true;
    loop {
        if virtual_mic_exists().await {
            if child.as_mut().is_some_and(|c| !matches!(c.try_wait(), Ok(None))) {
                child = None; // o nosso morreu, mas há outro provendo o node
            }
            shared.set_pw_ok(true);
            last_ok = true;
        } else {
            if last_ok {
                tracing::warn!("microfone virtual ausente; recriando");
            }
            last_ok = false;
            shared.set_pw_ok(false);
            drop(child.take()); // kill_on_drop remove um pw-loopback travado
            match spawn_virtual_mic() {
                Ok(c) => child = Some(c),
                Err(e) => tracing::debug!("{e:#}"),
            }
            // Dá até ~3 s para o node aparecer antes de reavaliar.
            for _ in 0..15 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if virtual_mic_exists().await {
                    shared.set_pw_ok(true);
                    last_ok = true;
                    tracing::info!("microfone virtual recriado");
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// `avahi-daemon` está rodando? (necessário para anunciar o nome `micbridge.local`)
pub fn avahi_active() -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "avahi-daemon"])
        .status()
        .is_ok_and(|s| s.success())
}

/// Nome estável na rede local (mDNS): o celular acessa `https://micbridge.local:3443` mesmo que
/// o IP do PC mude, e o cookie de pareamento continua valendo. Reanuncia se o IP mudar.
pub async fn supervise_mdns() {
    use std::time::Duration;
    let mut current: Option<(String, Child)> = None; // (ip anunciado, processo)
    loop {
        let ip = crate::local_ip();
        let alive = current
            .as_mut()
            .is_some_and(|(cur_ip, c)| *cur_ip == ip && matches!(c.try_wait(), Ok(None)));
        if !alive {
            drop(current.take()); // kill_on_drop remove o anúncio antigo
            if avahi_active() && ip != "127.0.0.1" {
                match Command::new("avahi-publish")
                    .args(["-a", "-R", crate::MDNS_HOST, &ip])
                    .kill_on_drop(true)
                    .die_with_parent()
                    .spawn()
                {
                    Ok(c) => {
                        tracing::info!("anunciando {} -> {ip}", crate::MDNS_HOST);
                        current = Some((ip, c));
                    }
                    Err(e) => tracing::warn!("avahi-publish: {e}"),
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
