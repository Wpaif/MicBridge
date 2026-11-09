//! Modo daemon para a interface Quickshell: o motor roda sem GTK e conversa com a
//! UI por um socket Unix, uma mensagem JSON por linha (o mesmo formato do flea).
//!
//! UI → daemon, um objeto com o comando em `c`:
//! - janela e conexão: `show`, `ping`, `quit`, `pairing`/`mute`/`autostart` (`on`), `forget`;
//! - tratamento: `effect` (`name`, `on`), `tune` (`name`, `value`);
//! - perfis: `profile` (`id`), `profile_new`/`profile_rename` (`name`), `profile_save`,
//!   `profile_delete`;
//! - retorno: `monitor` (`on`), `monitor_output` (`id` do node ou null),
//!   `monitor_volume` (`value`), `monitor_refresh`, `monitor_retry`;
//! - teste de voz: `record` (`on`), `play` (`version`), `stop_play`, `export` (`path`),
//!   `export_confirm` (`on`).
//!
//! Daemon → UI: `{"t":"qr",...}` quando o link muda e `{"t":"state",...}` a cada
//! mudança de estado (no máximo 10 vezes por segundo, por causa dos medidores).

use crate::dsp::Preset;
use crate::monitor::MonitorPhase;
use crate::monitor_ui::{Choices, PendingAction, RefreshSchedule};
use crate::profiles::Library;
use crate::recording::Version;
use crate::recording_ui::{
    Destinations, ExportOutcome, Notice, OperationKey, current_notice, export_selection,
};
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

/// Traz a janela já aberta para frente. No Wayland só o compositor pode fazer
/// isso, então vale para os que aceitam o pedido por linha de comando.
fn focus_window() {
    let window = format!("class:^({})$", crate::APP_ID);
    let attempts: Vec<(&str, Vec<String>)> =
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            // Hyprland com config em Lua (0.55+) só aceita a forma hl.dsp; as versões
            // anteriores, só a antiga. Tenta as duas, como o omarchy-launch-or-focus.
            vec![
                (
                    "hyprctl",
                    vec![
                        "dispatch".into(),
                        format!("hl.dsp.focus({{ window = \"{window}\" }})"),
                    ],
                ),
                (
                    "hyprctl",
                    vec!["dispatch".into(), "focuswindow".into(), window],
                ),
            ]
        } else if std::env::var_os("SWAYSOCK").is_some() {
            vec![(
                "swaymsg",
                vec![format!("[app_id=\"^{}$\"] focus", crate::APP_ID)],
            )]
        } else {
            return;
        };
    for (program, args) in attempts {
        let ok = std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return;
        }
    }
}

/// Perfis e efeitos ficam aqui no daemon (no GTK eles vivem em `profiles_ui`).
pub struct Daemon {
    shared: Arc<Shared>,
    library: Mutex<Library>,
    settings_path: PathBuf,
    autostart_path: PathBuf,
    /// Um arquivo de perfis inválido é preservado: nada é gravado por cima.
    read_only: bool,
    ui: Mutex<Option<std::process::Child>>,
    pub quit: tokio::sync::Notify,
    quitting: AtomicBool,
    readout: Mutex<crate::effect_ui::Readout>,
    started: std::time::Instant,
    monitor: Arc<Mutex<MonitorControl>>,
    recording: Arc<Mutex<RecordingControl>>,
    profile_error: Mutex<Option<String>>,
}

/// Exportação do teste de voz: a confirmação de substituição fica pendente aqui
/// até a UI responder, e o aviso só vale enquanto a operação que o gerou é a atual.
#[derive(Default)]
struct RecordingControl {
    exporting: bool,
    sequence: u64,
    notice: Option<crate::recording_ui::Notice>,
    confirm: Option<crate::recording_ui::Destinations>,
}

/// Mesmo ciclo do painel GTK de retorno: ação em curso, erro da última tentativa
/// e consulta periódica das saídas.
#[derive(Default)]
struct MonitorControl {
    pending: Option<PendingAction>,
    error: Option<String>,
    schedule: RefreshSchedule,
}

impl Daemon {
    pub fn new(shared: Arc<Shared>) -> Arc<Self> {
        Self::with_paths(shared, settings::path(), crate::autostart_path())
    }

    fn with_paths(
        shared: Arc<Shared>,
        settings_path: PathBuf,
        autostart_path: PathBuf,
    ) -> Arc<Self> {
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
            autostart_path,
            read_only,
            ui: Mutex::new(None),
            quit: tokio::sync::Notify::new(),
            quitting: AtomicBool::new(false),
            readout: Mutex::new(crate::effect_ui::Readout::new(
                &shared.params,
                shared.generation(),
            )),
            started: std::time::Instant::now(),
            monitor: Arc::default(),
            recording: Arc::default(),
            profile_error: Mutex::default(),
            shared,
        })
    }

    /// Abre a janela Quickshell, a não ser que ela já esteja aberta.
    pub fn show_ui(&self) {
        let mut ui = self.ui.lock().unwrap();
        if let Some(child) = ui.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            focus_window();
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
            "forget" => {
                self.shared.forget_all();
                // Sem celulares, volta a mostrar o QR, como na interface GTK.
                self.shared.open_pairing();
            }
            "autostart" => crate::write_autostart(&self.autostart_path, on, "--quickshell")
                .map_err(|e| format!("Não foi possível alterar a inicialização: {e}"))?,
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
                    "eq" => &p.eq,
                    other => return Err(format!("efeito desconhecido: {other}")),
                };
                flag.store(on, Relaxed);
            }
            "monitor_output" => {
                // Pelo id do node: a lista pode ter mudado desde que a UI a desenhou.
                let id = msg["id"].as_u64();
                let target = self
                    .shared
                    .devices
                    .snapshot()
                    .devices
                    .into_iter()
                    .chain(self.shared.monitor_output())
                    .find(|d| Some(u64::from(d.id)) == id);
                if id.is_some() && target.is_none() {
                    return Err("Saída não encontrada".into());
                }
                self.shared.select_monitor_output(target);
                self.monitor.lock().unwrap().error = None;
            }
            "monitor" => self.toggle_monitor(on),
            "extra_source" => {
                let id = msg["id"].as_u64().map(|i| i as u32);
                self.shared.extra.select(id)?;
            }
            "extra_volume" => {
                let value = msg["value"].as_f64().ok_or("valor ausente")?;
                self.shared.extra.set_volume(value as f32);
            }
            "monitor_volume" => {
                let value = msg["value"].as_f64().ok_or("valor ausente")?;
                self.shared.monitor.set_volume(value as f32);
            }
            "monitor_refresh" => {
                if !self.shared.devices.snapshot().busy {
                    let mut control = self.monitor.lock().unwrap();
                    control.schedule.last = Some(self.started.elapsed());
                    control.error = None;
                    self.shared.devices.refresh();
                }
            }
            "monitor_retry" => {
                let mut control = self.monitor.lock().unwrap();
                if control.pending.is_some() || !self.shared.monitor.status().restore_pending {
                    return Ok(());
                }
                control.pending = Some(PendingAction::Recovering);
                let done = self.shared.monitor.retry_restore();
                let monitor = self.monitor.clone();
                tokio::spawn(async move {
                    let _ = done.await;
                    let mut control = monitor.lock().unwrap();
                    if control.pending == Some(PendingAction::Recovering) {
                        control.pending = None;
                    }
                });
            }
            "record" if on => {
                self.recording.lock().unwrap().notice = None;
                self.shared.start_recording();
            }
            "record" => self.shared.recorder.stop(self.shared.recorder.status().id),
            "play" => {
                let version = match msg["version"].as_str().unwrap_or_default() {
                    "original" => Version::Original,
                    "treated" => Version::Treated,
                    other => return Err(format!("versão desconhecida: {other}")),
                };
                let take = self.current_take().ok_or("Nenhum teste gravado")?;
                self.recording.lock().unwrap().notice = None;
                self.shared.play_recording(take, version);
            }
            "stop_play" => self.shared.stop_recording_playback(),
            "export" => {
                let path = msg["path"].as_str().ok_or("destino ausente")?;
                self.export(Destinations::new(std::path::Path::new(path)), false)?;
            }
            "export_confirm" => {
                let pending = self.recording.lock().unwrap().confirm.take();
                if let Some(dest) = pending.filter(|_| on) {
                    self.export(dest, true)?;
                } else {
                    self.notify_export("Exportação cancelada.".into());
                }
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
                        // Closures sem captura viram `fn` comum, como os setters acima.
                        "eq_low" => (|p, v| p.set_eq_db(0, v), -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
                        "eq_mid" => (|p, v| p.set_eq_db(1, v), -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
                        "eq_high" => (|p, v| p.set_eq_db(2, v), -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
                        other => return Err(format!("ajuste desconhecido: {other}")),
                    };
                if !value.is_finite() {
                    return Err("valor inválido".into());
                }
                set(p, value.clamp(min, max));
            }
            name if name.starts_with("profile") => {
                // Erros de perfil aparecem na janela, não só no log.
                let result = self.profile_command(name, &msg);
                *self.profile_error.lock().unwrap() = result.clone().err();
                return result;
            }
            other => return Err(format!("comando desconhecido: {other}")),
        }
        Ok(())
    }

    fn profile_command(&self, command: &str, msg: &Value) -> Result<(), String> {
        let name = msg["name"].as_str().unwrap_or_default();
        let mut library = self.library.lock().unwrap();
        if self.read_only {
            return Err(
                "Perfis somente leitura: corrija as preferências e reabra o MicBridge".into(),
            );
        }
        let mut candidate = library.clone();
        let selected = candidate.selected.clone();
        let current = crate::profiles::Config::capture(&self.shared.params);
        // Selecionar e criar aplicam o perfil; salvar, renomear e excluir mantêm os ajustes.
        let apply = match command {
            "profile" => {
                candidate.select(msg["id"].as_str().unwrap_or_default())?;
                true
            }
            "profile_new" => {
                candidate.create(name, current)?;
                true
            }
            "profile_save" => {
                candidate.update(&selected, current)?;
                false
            }
            "profile_rename" => {
                candidate.rename(&selected, name)?;
                false
            }
            "profile_delete" => {
                candidate.delete(&selected)?;
                true
            }
            other => return Err(format!("comando desconhecido: {other}")),
        };
        settings::save_profiles_to(&self.settings_path, &candidate)
            .map_err(|e| format!("Não foi possível salvar: {e}"))?;
        // Aplicar preserva o mute, como na interface GTK.
        if apply {
            candidate.config()?.apply(&self.shared.params);
        }
        *library = candidate;
        Ok(())
    }

    /// Mesmas regras do painel GTK: perfis de fábrica são protegidos e só um
    /// perfil personalizado com alterações pode ser salvo por cima.
    fn profile_state(&self, library: &Library) -> Value {
        let factory = Preset::from_key(&library.selected).is_some();
        let dirty = library
            .config()
            .is_ok_and(|c| c != crate::profiles::Config::capture(&self.shared.params));
        let editable = !self.read_only;
        let message = self.profile_error.lock().unwrap().clone().unwrap_or_else(|| {
            if self.read_only {
                "Não foi possível carregar os perfis. O arquivo foi preservado. Corrija as preferências e reabra o aplicativo para salvar perfis.".into()
            } else if dirty {
                "Alterações não salvas".into()
            } else if factory {
                "Perfil de fábrica · salve uma cópia para personalizar".into()
            } else {
                "Perfil salvo".into()
            }
        });
        json!({
            "dirty": dirty,
            "can_select": editable,
            "can_new": editable && library.custom.len() < 32,
            "can_save": editable && !factory && dirty,
            "can_rename": editable && !factory,
            "can_delete": editable && !factory,
            "message": message,
        })
    }

    fn current_take(&self) -> Option<Arc<crate::recording::Take>> {
        self.shared.recorder.take(self.shared.recorder.status().id)
    }

    fn operation_key(&self, sequence: u64) -> OperationKey {
        OperationKey {
            recording: self.shared.recorder.status().id,
            playback: self.shared.playback.status().operation,
            export: sequence,
        }
    }

    fn notify_export(&self, text: String) {
        let mut control = self.recording.lock().unwrap();
        let key = self.operation_key(control.sequence);
        control.notice = Some(Notice { key, text });
    }

    /// Grava os dois WAVs fora da thread do socket. Arquivos existentes só são
    /// substituídos depois de `export_confirm`, como no diálogo GTK.
    fn export(&self, dest: Destinations, confirmed: bool) -> Result<(), String> {
        let take = self.current_take().ok_or("Nenhum teste gravado")?;
        let mut control = self.recording.lock().unwrap();
        if control.exporting {
            return Ok(());
        }
        if dest.needs_confirmation() && !confirmed {
            control.confirm = Some(dest);
            return Ok(());
        }
        control.exporting = true;
        control.sequence = control.sequence.wrapping_add(1);
        let key = self.operation_key(control.sequence);
        let recording = self.recording.clone();
        tokio::task::spawn_blocking(move || {
            let text = match export_selection(&take, Some(&dest), confirmed) {
                Ok(ExportOutcome::Saved) => "Os dois arquivos WAV foram exportados.".into(),
                Ok(ExportOutcome::Cancelled) => "Exportação cancelada: o destino mudou. Escolha novamente para confirmar a substituição.".into(),
                Err(e) => e,
            };
            let mut control = recording.lock().unwrap();
            control.exporting = false;
            control.notice = Some(Notice { key, text });
        });
        Ok(())
    }

    fn recording_state(&self, connected: bool) -> Value {
        let r = self.shared.recorder.status();
        let pb = self.shared.playback.status();
        let control = self.recording.lock().unwrap();
        let view = crate::recording_ui::presentation(&r, &pb, connected, control.exporting);
        let key = OperationKey {
            recording: r.id,
            playback: pb.operation,
            export: control.sequence,
        };
        let message = crate::recording_ui::message(
            &r,
            &pb,
            &view,
            connected,
            control.exporting,
            current_notice(&control.notice, key),
        );
        let playing = (pb.playing && pb.take_id == Some(r.id))
            .then_some(pb.version)
            .flatten()
            .map(|v| match v {
                Version::Original => "original",
                Version::Treated => "treated",
            });
        json!({
            "caption": crate::recording_ui::caption(&r, &view),
            "progress": (r.samples as f64 / crate::recording::MAX_SAMPLES as f64).min(1.0),
            "can_start": view.can_start,
            "can_stop": view.can_stop,
            "can_listen": view.can_listen,
            "can_export": view.can_export,
            "can_stop_play": pb.playing,
            "playing": playing,
            "message": message,
            "confirm": control.confirm.as_ref().map(|d| json!({
                "original": d.original,
                "treated": d.treated,
            })),
        })
    }

    fn toggle_monitor(&self, on: bool) {
        let mut control = self.monitor.lock().unwrap();
        // Um clique por vez: enquanto prepara ou restaura, o pedido é ignorado.
        if control.pending.is_some() {
            return;
        }
        control.error = None;
        if on {
            let result = self
                .shared
                .monitor_output()
                .ok_or_else(|| "Escolha uma saída para o retorno".to_string())
                .and_then(|target| self.shared.start_monitor(target));
            match result {
                Ok(id) => control.pending = Some(PendingAction::Starting(id)),
                Err(e) => control.error = Some(e),
            }
        } else {
            control.pending = Some(PendingAction::Stopping);
            self.shared.stop_monitor();
            let done = self.shared.monitor.stop_and_wait();
            let monitor = self.monitor.clone();
            tokio::spawn(async move {
                let _ = done.await;
                let mut control = monitor.lock().unwrap();
                if control.pending == Some(PendingAction::Stopping) {
                    control.pending = None;
                }
            });
        }
    }

    /// Espelha o `update` do painel GTK: o que pode ser clicado e qual mensagem mostrar.
    fn monitor_state(&self, connected: bool) -> Value {
        let m = self.shared.monitor.status();
        let mut control = self.monitor.lock().unwrap();
        if let Some(PendingAction::Starting(id)) = control.pending {
            let settled = matches!(m.phase, MonitorPhase::Active | MonitorPhase::Failed);
            if settled || !self.shared.local_audio.monitor_requested(id) {
                control.pending = None;
            }
        }
        let action = control.pending;
        let transitioning =
            action.is_some() || matches!(m.phase, MonitorPhase::Opening | MonitorPhase::Restoring);
        let snapshot = self.shared.devices.snapshot();
        if control.schedule.due(
            self.started.elapsed(),
            m.phase == MonitorPhase::Active,
            snapshot.busy,
        ) {
            self.shared.devices.refresh();
        }
        let choices = Choices::new(&snapshot, self.shared.monitor_output().as_ref());
        let available = choices.available && snapshot.error.is_none() && !snapshot.busy;
        let view = crate::monitor_ui::presentation(&m, connected, available);
        let message = if let Some(action) = action {
            action.text().to_string()
        } else if m.phase == MonitorPhase::Restoring {
            view.text.clone()
        } else {
            control
                .error
                .clone()
                .or_else(|| m.error.clone())
                .or_else(|| snapshot.error.clone())
                .unwrap_or(view.text)
        };
        let outputs: Vec<Value> = choices
            .devices
            .iter()
            .zip(&choices.labels)
            .skip(1)
            .map(|(d, label)| json!({ "id": d.as_ref().map(|d| d.id), "label": label }))
            .collect();
        json!({
            "active": action.map(PendingAction::active).unwrap_or(view.active),
            "can_toggle": view.can_toggle && !transitioning,
            "can_retry": view.can_restore && !transitioning,
            "can_choose": choices.devices.len() > 1 && !transitioning,
            "can_refresh": !snapshot.busy && !transitioning,
            "busy": transitioning,
            "selected": self.shared.monitor_output().map(|d| d.id),
            "outputs": outputs,
            "volume": m.volume_percent.round(),
            "message": message,
        })
    }

    fn extra_state(&self) -> Value {
        let e = self.shared.extra.snapshot();
        let streams: Vec<Value> = e
            .streams
            .iter()
            .map(|s| json!({ "id": s.id, "label": s.label(), "call": s.call }))
            .collect();
        json!({
            "streams": streams,
            "selected": e.selected,
            "linked": e.linked,
            "volume": e.volume.round(),
            "error": e.error,
        })
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
        // Números para os medidores da mesa de efeitos; nulos quando o efeito está
        // desligado ou sem áudio recente (o mesmo critério dos textos acima).
        let live = |text: &str| text != "Desligado" && text != "Sem áudio";
        let levels = p.effect_reading().levels;
        let fx = json!({
            "gate": live(&gate).then(|| match levels.gate {
                crate::dsp::effects::GateState::Open => "open",
                crate::dsp::effects::GateState::Holding => "hold",
                crate::dsp::effects::GateState::Attenuating => "closed",
                crate::dsp::effects::GateState::Disabled => "off",
            }),
            "reduction_db": levels.compression_db.filter(|v| v.is_finite() && live(&compressor))
                .map(|v| (v * 10.0).round() / 10.0),
            "auto_gain_db": levels.auto_gain_db.filter(|v| v.is_finite() && live(&auto_gain))
                .map(|v| (v * 10.0).round() / 10.0),
        });
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
            "autostart": self.autostart_path.exists(),
            "fingerprint": crate::tls::fingerprint(),
            "muted": p.muted.load(Relaxed),
            "profile": library.selected,
            "editor": self.profile_state(&library),
            "profiles": profiles,
            "effects": {
                "denoise": p.denoise.load(Relaxed),
                "highpass": p.highpass.load(Relaxed),
                "gate": p.gate.load(Relaxed),
                "deesser": p.deesser.load(Relaxed),
                "presence": p.presence.load(Relaxed),
                "compressor": p.compressor.load(Relaxed),
                "auto_gain": p.auto_gain.load(Relaxed),
                "eq": p.eq.load(Relaxed),
            },
            "tuning": {
                "gate_db": p.gate_threshold(),
                "threshold_db": p.comp_threshold(),
                "ratio": p.comp_ratio(),
                "makeup_db": p.makeup_db(),
                "eq_low": p.eq_db()[0],
                "eq_mid": p.eq_db()[1],
                "eq_high": p.eq_db()[2],
            },
            "activity": activity,
            "fx": fx,
            "monitor": self.monitor_state(st.link == Link::Connected),
            "recording": self.recording_state(st.link == Link::Connected),
            "extra": self.extra_state(),
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
                        // Quem só manda um comando (`show`) fecha antes de ler:
                        // os comandos já recebidos ainda são executados.
                        let _ = reader.await;
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
        Daemon::with_paths(
            Arc::new(Shared::new()),
            f.dir.join("settings.json"),
            f.dir.join("autostart/micbridge.desktop"),
        )
    }

    #[tokio::test]
    async fn command_from_client_that_closes_at_once_is_not_lost() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        let (mut client, server) = UnixStream::pair().unwrap();
        client
            .write_all(b"{\"c\":\"mute\",\"on\":true}\n")
            .await
            .unwrap();
        drop(client);
        d.clone().client(server).await;
        assert!(d.shared.params.muted.load(Relaxed));
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

    fn with_output(f: &Fixture) -> Arc<Daemon> {
        use crate::monitor::{Monitor, devices::DeviceCatalog};
        use crate::recording::playback::Playback;
        let query: crate::monitor::devices::DeviceQuery =
            Arc::new(|| Box::pin(async { Ok(vec![crate::local_audio::tests::output()]) }));
        let shared = Shared::with_audio_services(
            Monitor::with_executable(f.executable.clone()),
            Playback::with_command(f.executable.clone(), vec![]),
            DeviceCatalog::with_query(query),
        );
        shared.devices.refresh();
        crate::audio_test::wait(|| !shared.devices.snapshot().busy);
        Daemon::with_paths(
            Arc::new(shared),
            f.dir.join("settings.json"),
            f.dir.join("autostart/micbridge.desktop"),
        )
    }

    #[test]
    fn monitor_output_is_chosen_by_node_id_and_requires_connection() {
        let f = Fixture::new(crate::audio_test::SLEEP);
        let d = with_output(&f);
        let mon = d.monitor_state(false);
        assert_eq!(mon["outputs"][0]["label"], "Fone");
        assert_eq!(mon["can_toggle"], false, "sem saída escolhida");
        assert!(d.handle(r#"{"c":"monitor_output","id":999}"#).is_err());
        d.handle(r#"{"c":"monitor_output","id":1}"#).unwrap();
        assert_eq!(d.monitor_state(false)["selected"], 1);
        // Desconectado: o pedido não abre a saída e a mensagem explica por quê.
        d.handle(r#"{"c":"monitor","on":true}"#).unwrap();
        let mon = d.monitor_state(false);
        assert_eq!(mon["active"], false);
        assert_eq!(mon["message"], "Conecte o celular para ativar o retorno");
        // Trocar a saída limpa o erro anterior; null desmarca.
        d.handle(r#"{"c":"monitor_output","id":null}"#).unwrap();
        let mon = d.monitor_state(false);
        assert!(mon["selected"].is_null());
        assert_eq!(mon["message"], "Retorno desligado");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn monitor_starts_and_stops_while_connected() {
        let f = Fixture::new(crate::audio_test::SLEEP);
        let d = with_output(&f);
        d.shared.set_link(Link::Connected);
        d.handle(r#"{"c":"monitor_output","id":1}"#).unwrap();
        d.handle(r#"{"c":"monitor","on":true}"#).unwrap();
        assert_eq!(
            d.monitor_state(true)["active"],
            true,
            "otimista enquanto prepara"
        );
        crate::audio_test::wait(|| d.shared.monitor.status().phase == MonitorPhase::Active);
        let mon = d.monitor_state(true);
        assert_eq!(mon["busy"], false);
        assert_eq!(mon["message"], "Retorno exclusivo em Fone");
        d.handle(r#"{"c":"monitor","on":false}"#).unwrap();
        assert_eq!(d.monitor_state(true)["active"], false);
        crate::audio_test::wait(|| d.monitor_state(true)["busy"] == false);
        assert_eq!(d.shared.monitor.status().phase, MonitorPhase::Disabled);
        d.shared.stop_audio_tools();
    }

    /// Grava um bloco curto pelo protocolo e espera o teste ficar pronto.
    fn record_take(d: &Daemon) {
        d.shared.set_link(Link::Connected);
        d.handle(r#"{"c":"record","on":true}"#).unwrap();
        assert_eq!(d.recording_state(true)["can_stop"], true);
        let mut pcm = [0.01; 480];
        crate::recording::process_block(
            &mut crate::dsp::Processor::new(),
            &mut pcm,
            &d.shared.params,
            &d.shared.recorder,
            d.shared.generation(),
        );
        d.handle(r#"{"c":"record","on":false}"#).unwrap();
        crate::audio_test::wait(|| d.recording_state(true)["can_export"] == true);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn voice_test_exports_and_confirms_before_replacing() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        assert_eq!(d.recording_state(false)["can_start"], false);
        assert!(d.handle(r#"{"c":"play","version":"original"}"#).is_err());
        record_take(&d);
        assert_eq!(d.recording_state(true)["can_listen"], true);

        let path = f.dir.join("teste-original.wav");
        let export = json!({ "c": "export", "path": path }).to_string();
        d.handle(&export).unwrap();
        crate::audio_test::wait(|| {
            d.recording_state(true)["message"] == "Os dois arquivos WAV foram exportados."
        });
        let treated = f.dir.join("teste-tratado.wav");
        assert!(path.exists() && treated.exists());

        // Já existem: nada é escrito até a confirmação.
        std::fs::write(&treated, b"antigo").unwrap();
        d.handle(&export).unwrap();
        let rec = d.recording_state(true);
        assert_eq!(rec["confirm"]["treated"], treated.to_str().unwrap());
        d.handle(r#"{"c":"export_confirm","on":false}"#).unwrap();
        assert!(d.recording_state(true)["confirm"].is_null());
        assert_eq!(d.recording_state(true)["message"], "Exportação cancelada.");
        assert_eq!(std::fs::read(&treated).unwrap(), b"antigo");

        d.handle(&export).unwrap();
        d.handle(r#"{"c":"export_confirm","on":true}"#).unwrap();
        crate::audio_test::wait(|| {
            d.recording_state(true)["message"] == "Os dois arquivos WAV foram exportados."
        });
        assert_ne!(std::fs::read(&treated).unwrap(), b"antigo");
        d.shared.stop_audio_tools();
    }

    #[test]
    fn monitor_volume_is_clamped() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        d.handle(r#"{"c":"monitor_volume","value":140}"#).unwrap();
        assert_eq!(d.monitor_state(false)["volume"], 100.0);
        assert!(d.handle(r#"{"c":"monitor_volume"}"#).is_err());
    }

    #[test]
    fn custom_profiles_are_created_saved_renamed_and_deleted() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        let editor = |d: &Daemon| d.state()["editor"].clone();
        assert_eq!(editor(&d)["can_save"], false, "fábrica é protegida");
        assert_eq!(editor(&d)["can_rename"], false);

        d.handle(r#"{"c":"tune","name":"ratio","value":9}"#)
            .unwrap();
        assert_eq!(editor(&d)["dirty"], true);
        assert_eq!(editor(&d)["message"], "Alterações não salvas");
        d.handle(r#"{"c":"profile_new","name":"  Meu perfil "}"#)
            .unwrap();
        let state = d.state();
        let id = state["profile"].as_str().unwrap().to_string();
        assert!(id.starts_with("custom-"));
        assert_eq!(state["profiles"][5]["name"], "Meu perfil");
        assert_eq!(state["editor"]["message"], "Perfil salvo");

        // Nome repetido: o erro aparece na janela e nada muda.
        assert!(
            d.handle(r#"{"c":"profile_new","name":"meu PERFIL"}"#)
                .is_err()
        );
        assert_eq!(editor(&d)["message"], "Já existe um perfil com esse nome");

        d.handle(r#"{"c":"tune","name":"ratio","value":3}"#)
            .unwrap();
        assert_eq!(editor(&d)["can_save"], true);
        d.handle(r#"{"c":"profile_save"}"#).unwrap();
        assert_eq!(editor(&d)["dirty"], false);
        d.handle(r#"{"c":"profile_rename","name":"Voz baixa"}"#)
            .unwrap();

        let saved = settings::load_profiles_from(&f.dir.join("settings.json")).unwrap();
        assert_eq!(saved.custom[0].name, "Voz baixa");
        assert_eq!(saved.custom[0].config.ratio, 3.0);
        assert_eq!(saved.selected, id);

        d.handle(r#"{"c":"profile_delete"}"#).unwrap();
        let saved = settings::load_profiles_from(&f.dir.join("settings.json")).unwrap();
        assert!(saved.custom.is_empty());
        assert_eq!(saved.selected, "chamada");
        assert!(
            d.handle(r#"{"c":"profile_delete"}"#).is_err(),
            "fábrica não é excluída"
        );
    }

    #[test]
    fn autostart_entry_starts_the_daemon_hidden() {
        let f = Fixture::new("exit 0");
        let d = daemon(&f);
        assert_eq!(d.state()["autostart"], false);
        d.handle(r#"{"c":"autostart","on":true}"#).unwrap();
        let entry = std::fs::read_to_string(&d.autostart_path).unwrap();
        assert!(entry.contains(" --quickshell --hidden\n"), "{entry}");
        assert_eq!(d.state()["autostart"], true);
        d.handle(r#"{"c":"autostart","on":false}"#).unwrap();
        d.handle(r#"{"c":"autostart","on":false}"#).unwrap();
        assert_eq!(d.state()["autostart"], false);
    }

    #[test]
    fn damaged_profiles_file_is_preserved() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("settings.json");
        let damaged = r#"{"voice_profiles":{"version":99}}"#;
        std::fs::write(&path, damaged).unwrap();
        let d = daemon(&f);
        let state = d.state();
        assert_eq!(state["read_only"], true);
        assert_eq!(state["editor"]["can_select"], false);
        assert!(d.handle(r#"{"c":"profile","id":"podcast"}"#).is_err());
        assert!(d.handle(r#"{"c":"profile_new","name":"Meu"}"#).is_err());
        assert_eq!(d.state()["profile"], "chamada");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged);
    }
}
