//! Saída para o PipeWire. Reaproveita o esquema do `vdo_audio`:
//! sink `vdo_input` -> fonte virtual `Microfone_VDO`.

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};

pub const SINK: &str = "vdo_input";

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
pub fn spawn_web() -> Result<Child> {
    let home = std::env::var("MICBRIDGE_HOME")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/..").into());
    Command::new(format!("{home}/scripts/web-https.sh"))
        .env("MICBRIDGE_PARENT", std::process::id().to_string())
        .kill_on_drop(true)
        .die_with_parent()
        .spawn()
        .context("falha ao iniciar o servidor web (scripts/web-https.sh)")
}
