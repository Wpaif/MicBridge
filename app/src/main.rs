mod app_source;
#[cfg(test)]
mod audio_test;
mod calibration;
mod clip_view;
mod dsp;
#[cfg(test)]
mod dsp_bench;
#[cfg(test)]
mod dsp_quality;
mod effect_view;
mod ipc;
mod jitter;
mod local_audio;
mod monitor;
mod monitor_view;
mod output;
mod profiles;
mod recording;
mod recording_view;
mod server;
mod settings;
mod state;
mod tls;
mod tray;
mod tray_icon;
mod web;

use state::Shared;
use std::net::UdpSocket;
use std::process::ExitCode;
use std::sync::Arc;

const APP_ID: &str = "dev.wilian.MicBridge";
const WEB_PORT: u16 = 3443;
/// Nome mDNS anunciado via avahi (já consta no certificado).
pub const MDNS_HOST: &str = tls::MDNS_HOST;

/// IP da LAN (a conexão UDP não envia nada, só descobre a rota).
pub fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("1.1.1.1:80").and_then(|_| s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    // `--quickshell` vinha de quando havia também a interface GTK; segue aceito
    // (e ignorado) para autostarts e atalhos antigos continuarem funcionando.
    let hidden = std::env::args().any(|a| a == "--hidden");
    run_daemon(!hidden)
}

/// Motor: servidor web, microfone virtual, bandeja e socket da interface Quickshell.
fn run_daemon(show: bool) -> ExitCode {
    // Instância única: se o socket responde, só pede a janela.
    if ipc::ask_running_instance(if show { "show" } else { "ping" }) {
        return ExitCode::SUCCESS;
    }
    let shared = Arc::new(Shared::new());
    let daemon = ipc::Daemon::new(shared.clone());
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    rt.spawn(output::supervise_virtual_mic(shared.clone()));
    rt.spawn(output::supervise_default_source(shared.clone()));
    rt.spawn(app_source::supervise(shared.extra.clone()));
    rt.spawn(output::supervise_mdns());
    let s = shared.clone();
    rt.spawn(async move {
        let ip = local_ip().parse().ok();
        if let Err(e) = server::serve(s, ip, WEB_PORT).await {
            tracing::error!("servidor: {e:#}");
        }
    });
    let (tray_tx, tray_rx) = async_channel::unbounded();
    rt.spawn(tray::spawn(tray_tx, shared.clone()));
    let d = daemon.clone();
    rt.spawn(async move {
        while let Ok(ev) = tray_rx.recv().await {
            match ev {
                tray::TrayEvent::Show => d.show_ui(),
                tray::TrayEvent::Quit => d.request_quit(),
            }
        }
    });
    let d = daemon.clone();
    rt.spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            return;
        };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        d.request_quit();
    });
    let d = daemon.clone();
    let code = rt.block_on(async move {
        let serving = tokio::spawn(d.clone().serve());
        if show {
            d.show_ui();
        }
        tokio::select! {
            _ = d.quit.notified() => ExitCode::SUCCESS,
            r = serving => {
                tracing::error!("socket da interface: {r:?}");
                ExitCode::FAILURE
            }
        }
    });
    shared.stop_audio_tools();
    if let Err(e) = rt.block_on(shared.local_audio.shutdown_and_wait()) {
        tracing::warn!("restauração pendente: {e}");
    }
    let _ = std::fs::remove_file(ipc::socket_path());
    drop(rt);
    code
}

/// Mesmo local que o instalador e o desinstalador usam.
pub(crate) fn autostart_path() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_default()
        .join("autostart/dev.wilian.MicBridge.desktop")
}

/// Liga/desliga a inicialização com a sessão (já abre escondido na bandeja).
pub(crate) fn write_autostart(path: &std::path::Path, on: bool) -> std::io::Result<()> {
    if !on {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or("micbridge".into());
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=MicBridge\nExec={exe} --hidden\nIcon={APP_ID}\nX-GNOME-Autostart-enabled=true\n"
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, entry)
}
