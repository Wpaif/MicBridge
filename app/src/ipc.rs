//! Modo daemon para a interface Quickshell: o motor roda sem GTK e conversa com a
//! UI por um socket Unix, uma mensagem JSON por linha (o mesmo formato do flea).
//!
//! UI → daemon: `{"c":"mute","on":true}`, `{"c":"pairing","on":true}`,
//! `{"c":"profile","id":"chamada"}`, `{"c":"effect","name":"denoise","on":false}`,
//! `{"c":"forget"}`, `{"c":"quit"}`.
//! Daemon → UI: `{"t":"qr",...}` quando o link muda e `{"t":"state",...}` a cada
//! mudança de estado (no máximo 10 vezes por segundo, por causa dos medidores).

use crate::dsp::Preset;
use crate::profiles::Library;
use crate::settings;
use crate::state::{Link, Shared};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Socket da instância, privado ao usuário (`$XDG_RUNTIME_DIR/micbridge.sock`).
pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("micbridge.sock")
}

/// Pede à instância em execução que abra a interface. `false` se não houver daemon.
pub fn ask_running_instance(command: &str) -> bool {
    use std::io::Write;
    std::os::unix::net::UnixStream::connect(socket_path())
        .and_then(|mut s| writeln!(s, "{}", json!({ "c": command })))
        .is_ok()
}

/// Pasta da UI QML: `MICBRIDGE_UI_DIR`, a instalada em `~/.local/share/micbridge/ui`
/// ou, em debug, a do checkout.
pub fn ui_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MICBRIDGE_UI_DIR") {
        return dir.into();
    }
    let installed = crate::state::data_dir().join("ui");
    #[cfg(debug_assertions)]
    if !installed.join("shell.qml").exists() {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
    }
    installed
}

/// Perfis e efeitos ficam aqui no daemon (no GTK eles vivem em `profiles_ui`).
pub struct Daemon {
    shared: Arc<Shared>,
    library: Mutex<Library>,
    settings_path: PathBuf,
    /// Um arquivo de perfis inválido é preservado: nada é gravado por cima.
    read_only: bool,
    ui: Mutex<Option<std::process::Child>>,
    pub quit: tokio::sync::Notify,
    quitting: AtomicBool,
    readout: Mutex<crate::effect_ui::Readout>,
    started: std::time::Instant,
}

impl Daemon {
    pub fn new(shared: Arc<Shared>) -> Arc<Self> {
        Self::with_settings(shared, settings::path())
    }

    fn with_settings(shared: Arc<Shared>, settings_path: PathBuf) -> Arc<Self> {
        let (library, read_only) = match settings::load_profiles_from(&settings_path) {
            Ok(library) => (library, false),
            Err(e) => {
                tracing::warn!("perfis não carregados, arquivo preservado: {e}");
                (
                    Library::new(settings::load_preset().unwrap_or(Preset::Chamada)),
                    true,
                )
            }
        };
        library
            .config()
            .expect("biblioteca validada")
            .apply(&shared.params);
        Arc::new(Self {
            library: Mutex::new(library),
            settings_path,
            read_only,
            ui: Mutex::new(None),
            quit: tokio::sync::Notify::new(),
            quitting: AtomicBool::new(false),
            readout: Mutex::new(crate::effect_ui::Readout::new(
                &shared.params,
                shared.generation(),
            )),
            started: std::time::Instant::now(),
            shared,
        })
    }

    /// Abre a janela Quickshell, a não ser que ela já esteja aberta.
    pub fn show_ui(&self) {
        let mut ui = self.ui.lock().unwrap();
        if let Some(child) = ui.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            return;
        }
        let dir = ui_dir();
        match std::process::Command::new("qs")
            .arg("-p")
            .arg(&dir)
            .env("MICBRIDGE_SOCKET", socket_path())
            .spawn()
        {
            Ok(child) => *ui = Some(child),
            Err(e) => tracing::error!("não consegui abrir a interface ({}): {e}", dir.display()),
        }
    }

    pub fn request_quit(&self) {
        if !self.quitting.swap(true, Relaxed) {
            if let Some(mut child) = self.ui.lock().unwrap().take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.quit.notify_one();
        }
    }

    fn handle(&self, line: &str) -> Result<(), String> {
        let msg: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let on = msg["on"].as_bool().unwrap_or(false);
        match msg["c"].as_str().unwrap_or_default() {
            "show" => self.show_ui(),
            "ping" => {}
            "quit" => self.request_quit(),
            "mute" => self.shared.params.muted.store(on, Relaxed),
            "pairing" if on => self.shared.open_pairing(),
            "pairing" => self.shared.close_pairing(),
            "forget" => self.shared.forget_all(),
            "effect" => {
                let p = &self.shared.params;
                let flag = match msg["name"].as_str().unwrap_or_default() {
                    "denoise" => &p.denoise,
                    "highpass" => &p.highpass,
                    "gate" => &p.gate,
                    "deesser" => &p.deesser,
                    "presence" => &p.presence,
                    "compressor" => &p.compressor,
                    "auto_gain" => &p.auto_gain,
                    other => return Err(format!("efeito desconhecido: {other}")),
                };
                flag.store(on, Relaxed);
            }
            "tune" => {
                let p = &self.shared.params;
                let value = msg["value"].as_f64().ok_or("valor ausente")? as f32;
                // Mesmos limites dos sliders GTK e da validação de perfis.
                let (set, min, max): (fn(&crate::dsp::Params, f32), f32, f32) =
                    match msg["name"].as_str().unwrap_or_default() {
                        "gate_db" => (crate::dsp::Params::set_gate_db, -80.0, -20.0),
                        "threshold_db" => (crate::dsp::Params::set_comp_threshold_db, -50.0, 0.0),
                        "ratio" => (crate::dsp::Params::set_comp_ratio, 1.0, 10.0),
                        "makeup_db" => (crate::dsp::Params::set_makeup_db, 0.0, 24.0),
                        other => return Err(format!("ajuste desconhecido: {other}")),
                    };
                if !value.is_finite() {
                    return Err("valor inválido".into());
                }
                set(p, value.clamp(min, max));
            }
            "profile" => {
                let id = msg["id"].as_str().unwrap_or_default();
                let mut library = self.library.lock().unwrap();
                let mut candidate = library.clone();
                candidate.select(id)?;
                if !self.read_only {
                    settings::save_profiles_to(&self.settings_path, &candidate)?;
                }
                // Trocar de perfil preserva o mute, como na interface GTK.
                candidate.config()?.apply(&self.shared.params);
                *library = candidate;
            }
            other => return Err(format!("comando desconhecido: {other}")),
        }
        Ok(())
    }

    fn qr(&self) -> Value {
        let host = if crate::output::avahi_active() {
            crate::MDNS_HOST.to_string()
        } else {
            crate::local_ip()
        };
        let url = format!("https://{host}:{}", crate::WEB_PORT);
        let pair_url = format!("{url}/?t={}", self.shared.pair_token);
        // Cada linha da matriz vira uma string de '0' e '1'; o QML desenha num Canvas.
        let rows: Vec<String> = qrcode::QrCode::new(pair_url.as_bytes())
            .map(|code| {
                let n = code.width();
                code.into_colors()
                    .chunks(n)
                    .map(|row| {
                        row.iter()
                            .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({ "t": "qr", "url": url, "pair_url": pair_url, "rows": rows })
    }

    fn state(&self) -> Value {
        let st = self.shared.status();
        let p = &self.shared.params;
        let [gate, compressor, auto_gain] = self.readout.lock().unwrap().read(
            p,
            self.shared.generation(),
            st.link == Link::Connected,
            self.started.elapsed(),
        );
        let activity = json!({ "gate": gate, "compressor": compressor, "auto_gain": auto_gain });
        let library = self.library.lock().unwrap();
        let mut profiles: Vec<Value> = Preset::ALL
            .iter()
            .map(|p| json!({ "id": p.key(), "name": p.name(), "description": p.description() }))
            .collect();
        profiles.extend(
            library.custom.iter().map(
                |c| json!({ "id": c.id, "name": c.name, "description": "Perfil personalizado" }),
            ),
        );
        json!({
            "t": "state",
            "link": match st.link {
                Link::Disconnected => "disconnected",
                Link::Connecting => "connecting",
                Link::Connected => "connected",
                Link::Reconnecting => "reconnecting",
                Link::Error => "error",
            },
            "label": st.label,
            "detail": st.detail,
            "pairing": self.shared.pairing_remaining(),
            "devices": self.shared.device_count(),
            "muted": p.muted.load(Relaxed),
            "profile": library.selected,
            "profiles": profiles,
            "effects": {
                "denoise": p.denoise.load(Relaxed),
                "highpass": p.highpass.load(Relaxed),
                "gate": p.gate.load(Relaxed),
                "deesser": p.deesser.load(Relaxed),
                "presence": p.presence.load(Relaxed),
                "compressor": p.compressor.load(Relaxed),
                "auto_gain": p.auto_gain.load(Relaxed),
            },
            "tuning": {
                "gate_db": p.gate_threshold(),
                "threshold_db": p.comp_threshold(),
                "ratio": p.comp_ratio(),
                "makeup_db": p.makeup_db(),
            },
            "activity": activity,
            // Níveis arredondados para não reenviar o estado por ruído de ponto flutuante.
            "in": (p.in_level() * 1000.0).round() / 1000.0,
            "out": (p.out_level() * 1000.0).round() / 1000.0,
            "read_only": self.read_only,
        })
    }

    async fn client(self: Arc<Self>, stream: UnixStream) {
        let (read, mut write) = stream.into_split();
        let reader = {
            let daemon = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Err(e) = daemon.handle(&line) {
                        tracing::warn!("comando da interface recusado: {e}");
                    }
                }
            })
        };
        let mut last_qr = String::new();
        let mut last_state = String::new();
        let mut qr = String::new();
        let mut tick = 0u32;
        while !reader.is_finished() {
            // O host (mDNS ou IP) consulta o systemd: basta conferir a cada 5 s.
            if tick.is_multiple_of(50) {
                qr = self.qr().to_string();
            }
            tick = tick.wrapping_add(1);
            let qr = qr.clone();
            let state = self.state().to_string();
            for (current, last) in [(qr, &mut last_qr), (state, &mut last_state)] {
                if current != *last {
                    let line = format!("{current}\n");
                    if write.write_all(line.as_bytes()).await.is_err() {
                        reader.abort();
                        return;
                    }
                    *last = current;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Escuta o socket até o processo terminar. Falha se outra instância já o ocupa.
    pub async fn serve(self: Arc<Self>) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let path = socket_path();
        // Um socket que ninguém atende é sobra de uma execução que caiu.
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "outra instância do MicBridge já está rodando",
            ));
        }
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        loop {
            let (stream, _) = listener.accept().await?;
            tokio::spawn(self.clone().client(stream));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_test::Fixture;

    fn daemon(f: &Fixture) -> Arc<Daemon> {
        Daemon::with_settings(Arc::new(Shared::new()), f.dir.join("settings.json"))
    }

    #[test]
    fn tune_clamps_to_profile_limits_and_rejects_bad_input() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        d.handle(r#"{"c":"tune","name":"ratio","value":7.5}"#)
            .unwrap();
        d.handle(r#"{"c":"tune","name":"gate_db","value":-999}"#)
            .unwrap();
        d.handle(r#"{"c":"tune","name":"makeup_db","value":99}"#)
            .unwrap();
        let p = &d.shared.params;
        assert_eq!(p.comp_ratio(), 7.5);
        assert_eq!(p.gate_threshold(), -80.0);
        assert_eq!(p.makeup_db(), 24.0);
        assert!(d.handle(r#"{"c":"tune","name":"ratio"}"#).is_err());
        assert!(
            d.handle(r#"{"c":"tune","name":"other","value":1}"#)
                .is_err()
        );
        assert_eq!(p.comp_ratio(), 7.5);
    }

    #[test]
    fn effects_and_mute_follow_commands() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        d.handle(r#"{"c":"effect","name":"denoise","on":false}"#)
            .unwrap();
        d.handle(r#"{"c":"mute","on":true}"#).unwrap();
        assert!(!d.shared.params.denoise.load(Relaxed));
        assert!(d.shared.params.muted.load(Relaxed));
        assert!(
            d.handle(r#"{"c":"effect","name":"reverb","on":true}"#)
                .is_err()
        );
        assert!(d.handle("não é json").is_err());
        let state = d.state();
        assert_eq!(state["effects"]["denoise"], false);
        assert_eq!(state["muted"], true);
    }

    #[test]
    fn profile_selection_persists_and_keeps_mute() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        d.handle(r#"{"c":"mute","on":true}"#).unwrap();
        d.handle(r#"{"c":"profile","id":"bruto"}"#).unwrap();
        assert!(!d.shared.params.denoise.load(Relaxed));
        assert!(d.shared.params.muted.load(Relaxed));
        let saved = settings::load_profiles_from(&f.dir.join("settings.json")).unwrap();
        assert_eq!(saved.selected, "bruto");
        assert!(d.handle(r#"{"c":"profile","id":"inexistente"}"#).is_err());
        assert_eq!(d.state()["profile"], "bruto");
    }

    #[test]
    fn damaged_profiles_file_is_preserved() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("settings.json");
        let damaged = r#"{"voice_profiles":{"version":99}}"#;
        std::fs::write(&path, damaged).unwrap();
        let d = daemon(&f);
        assert_eq!(d.state()["read_only"], true);
        d.handle(r#"{"c":"profile","id":"podcast"}"#).unwrap();
        assert_eq!(d.state()["profile"], "podcast");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged);
    }
}
