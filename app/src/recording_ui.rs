//! Painel do teste de voz e decisões de apresentação/exportação.
use crate::recording::{RecordingStatus, Take, TakeState, playback::PlaybackStatus};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Debug)]
struct Presentation {
    can_start: bool,
    can_stop: bool,
    can_listen: bool,
    can_export: bool,
    duration: String,
    warning: Option<String>,
}
fn presentation(
    r: &RecordingStatus,
    _: &PlaybackStatus,
    connected: bool,
    exporting: bool,
) -> Presentation {
    let ready = matches!(r.state, TakeState::Ready | TakeState::Incomplete) && r.samples > 0;
    Presentation {
        can_start: connected && r.state != TakeState::Recording,
        can_stop: r.state == TakeState::Recording,
        can_listen: ready,
        can_export: ready && !exporting,
        duration: format!("{:.1} / 30 s", r.samples as f64 / 48000.0).replace('.', ","),
        warning: r.message.clone(),
    }
}
#[derive(Debug)]
struct Destinations {
    original: PathBuf,
    treated: PathBuf,
}
impl Destinations {
    fn new(path: &Path) -> Self {
        let original = path.with_extension("wav");
        let stem = original.file_stem().unwrap_or_default().to_string_lossy();
        let stem = stem.strip_suffix("-original").unwrap_or(&stem);
        let treated = original.with_file_name(format!("{stem}-tratado.wav"));
        Self { original, treated }
    }
    fn needs_confirmation(&self) -> bool {
        self.original.exists() || self.treated.exists()
    }
}
#[derive(Debug, PartialEq, Eq)]
enum ExportOutcome {
    Cancelled,
    Saved,
}
fn export_selection(
    take: &Take,
    dest: Option<&Destinations>,
    confirmed: bool,
) -> Result<ExportOutcome, String> {
    let Some(dest) = dest else {
        return Ok(ExportOutcome::Cancelled);
    };
    if dest.needs_confirmation() && !confirmed {
        return Ok(ExportOutcome::Cancelled);
    }
    let result = if confirmed {
        crate::recording::wav::export_with_overwrite(take, &dest.original, &dest.treated, true)
    } else {
        crate::recording::wav::export(take, &dest.original, &dest.treated)
    };
    result.map_err(|e| {
        if e.completed.is_empty() {
            format!("Não foi possível exportar: {}", e.source)
        } else {
            format!(
                "Exportação parcial: {} foi salvo; falha no segundo arquivo: {}",
                e.completed[0].display(),
                e.source
            )
        }
    })?;
    Ok(ExportOutcome::Saved)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OperationKey {
    recording: u64,
    playback: u64,
    export: u64,
}
struct Notice {
    key: OperationKey,
    text: String,
}
fn current_notice(notice: &Option<Notice>, key: OperationKey) -> Option<&str> {
    notice
        .as_ref()
        .filter(|n| n.key == key)
        .map(|n| n.text.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn status(state: TakeState, samples: usize) -> RecordingStatus {
        RecordingStatus {
            id: 1,
            state,
            samples,
            message: None,
        }
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn panel_actions_capture_real_dsp_and_update_button_sensitivity() {
        use std::time::{Duration, Instant};
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.PanelTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::builder().application(&app).build();
        let shared = Arc::new(Shared::new());
        shared.set_link(Link::Connected);
        let root = panel(shared.clone(), &parent);
        parent.set_content(Some(&root));
        parent.present();
        fn button(widget: &impl IsA<gtk::Widget>, label: &str) -> Option<gtk::Button> {
            let widget = widget.as_ref();
            if let Some(b) = widget.downcast_ref::<gtk::Button>() {
                if b.label().as_deref() == Some(label) {
                    return Some(b.clone());
                }
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                if let Some(found) = button(&w, label) {
                    return Some(found);
                }
                child = w.next_sibling();
            }
            None
        }
        fn until(mut condition: impl FnMut() -> bool) {
            let end = Instant::now() + Duration::from_secs(3);
            while !condition() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                assert!(Instant::now() < end, "GTK update timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        until(|| parent.is_realized());
        let start = button(&root, "Gravar teste").unwrap();
        let stop = button(&root, "Parar gravação").unwrap();
        let original = button(&root, "Ouvir original").unwrap();
        let export = button(&root, "Exportar WAVs").unwrap();
        assert!(start.is_sensitive());
        assert!(!original.is_sensitive());
        assert!(!export.is_sensitive());
        start.emit_clicked();
        until(|| stop.is_sensitive());
        assert!(!original.is_sensitive());
        let mut pcm = [0.01; 480];
        shared.params.apply_preset(crate::dsp::Preset::Bruto);
        crate::recording::process_block(
            &mut crate::dsp::Processor::new(),
            &mut pcm,
            &shared.params,
            &shared.recorder,
            shared.generation(),
        );
        stop.emit_clicked();
        until(|| original.is_sensitive() && export.is_sensitive());
        let take = shared.recorder.take(shared.recorder.status().id).unwrap();
        assert_eq!(take.original, vec![0.01; 480]);
        assert_eq!(take.treated, pcm);
        assert!(!stop.is_sensitive());
        shared.set_link(Link::Disconnected);
        until(|| !start.is_sensitive());
        shared.stop_audio_tools();
        parent.set_content(None::<&gtk::Widget>);
        parent.destroy();
    }
    #[test]
    fn empty_and_recording_disable_comparison_and_export() {
        let pb = PlaybackStatus::default();
        let empty = presentation(&status(TakeState::Empty, 0), &pb, true, false);
        assert!(empty.can_start);
        assert!(!empty.can_listen);
        assert!(!empty.can_export);
        assert!(!empty.can_stop);
        let active = presentation(&status(TakeState::Recording, 24000), &pb, true, false);
        assert!(active.can_stop);
        assert!(!active.can_listen);
        assert!(!active.can_export);
        assert_eq!(active.duration, "0,5 / 30 s");
        assert!(!presentation(&status(TakeState::Empty, 0), &pb, false, false).can_start);
    }
    #[test]
    fn ready_and_incomplete_allow_comparison_of_existing_audio() {
        let pb = PlaybackStatus::default();
        for state in [TakeState::Ready, TakeState::Incomplete] {
            let mut st = status(state, 48000);
            st.message = (state == TakeState::Incomplete).then(|| "Teste incompleto".into());
            let view = presentation(&st, &pb, true, false);
            assert!(view.can_listen);
            assert!(view.can_export);
            assert_eq!(view.duration, "1,0 / 30 s");
            assert_eq!(view.warning, st.message);
            assert!(!presentation(&st, &pb, true, true).can_export);
        }
    }
    #[test]
    fn cancelled_or_unconfirmed_export_preserves_both_destinations() {
        let d = std::env::temp_dir().join(format!(
            "micbridge-export-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&d).unwrap();
        let t = Take {
            id: 1,
            original: vec![0.1],
            treated: vec![0.2],
        };
        let dest = Destinations::new(&d.join("teste-original.wav"));
        assert_eq!(dest.treated, d.join("teste-tratado.wav"));
        assert_eq!(
            export_selection(&t, None, false).unwrap(),
            ExportOutcome::Cancelled
        );
        assert!(!dest.original.exists());
        assert!(!dest.treated.exists());
        std::fs::write(&dest.treated, b"antigo tratado").unwrap();
        assert!(dest.needs_confirmation());
        assert_eq!(
            export_selection(&t, Some(&dest), false).unwrap(),
            ExportOutcome::Cancelled
        );
        assert!(!dest.original.exists());
        assert_eq!(std::fs::read(&dest.treated).unwrap(), b"antigo tratado");
        std::fs::write(&dest.original, b"antigo original").unwrap();
        assert_eq!(
            export_selection(&t, Some(&dest), false).unwrap(),
            ExportOutcome::Cancelled
        );
        assert_eq!(std::fs::read(&dest.original).unwrap(), b"antigo original");
        assert_eq!(
            export_selection(&t, Some(&dest), true).unwrap(),
            ExportOutcome::Saved
        );
        assert_eq!(std::fs::read(&dest.original).unwrap().len(), 46);
        assert_eq!(std::fs::read(&dest.treated).unwrap().len(), 46);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn late_export_result_cannot_replace_new_capture_or_playback_message() {
        let key = OperationKey {
            recording: 1,
            playback: 2,
            export: 3,
        };
        let notice = Some(Notice {
            key,
            text: "Exportado".into(),
        });
        assert_eq!(current_notice(&notice, key), Some("Exportado"));
        assert!(
            current_notice(
                &notice,
                OperationKey {
                    recording: 2,
                    ..key
                }
            )
            .is_none()
        );
        assert!(current_notice(&notice, OperationKey { playback: 3, ..key }).is_none());
        assert!(current_notice(&notice, OperationKey { export: 4, ..key }).is_none());
    }
    #[test]
    fn export_message_reports_second_file_failure_as_partial() {
        let d = std::env::temp_dir().join(format!(
            "micbridge-export-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&d).unwrap();
        let dest = Destinations {
            original: d.join("a.wav"),
            treated: d.join("missing/b.wav"),
        };
        let t = Take {
            id: 1,
            original: vec![0.1],
            treated: vec![0.2],
        };
        let err = export_selection(&t, Some(&dest), false).unwrap_err();
        assert!(err.contains("parcial"));
        assert!(err.contains("a.wav"));
        assert!(dest.original.exists());
        std::fs::remove_dir_all(d).unwrap();
    }
}

use crate::{
    recording::Version,
    state::{Link, Shared},
};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

pub fn panel(shared: Arc<Shared>, parent: &adw::ApplicationWindow) -> gtk::Box {
    let title = gtk::Label::builder()
        .label("TESTE DE VOZ · COMPARAÇÃO A/B")
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let state = gtk::Label::builder().xalign(0.0).wrap(true).build();
    let message = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let hint=gtk::Label::builder()
        .label("Use fones ao ouvir o teste. Original = áudio recebido antes dos efeitos locais. O RNNoise pode adicionar um pequeno atraso à versão tratada.")
        .xalign(0.0).wrap(true).css_classes(["caption","dim-label"]).build();
    let start = gtk::Button::with_label("Gravar teste");
    start.add_css_class("suggested-action");
    let stop = gtk::Button::with_label("Parar gravação");
    let original = gtk::Button::with_label("Ouvir original");
    let treated = gtk::Button::with_label("Ouvir tratado");
    let stop_play = gtk::Button::with_label("Parar reprodução");
    let export = gtk::Button::with_label("Exportar WAVs");
    let actions = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(3)
        .homogeneous(true)
        .column_spacing(6)
        .row_spacing(6)
        .build();
    for button in [&start, &stop, &original, &treated, &stop_play, &export] {
        actions.insert(button, -1);
    }
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .css_classes(["mb-hero", "mb-recording"])
        .build();
    root.append(&title);
    root.append(&state);
    root.append(&actions);
    root.append(&message);
    root.append(&hint);
    let exporting = Rc::new(Cell::new(false));
    let export_sequence = Rc::new(Cell::new(0u64));
    let notice: Rc<RefCell<Option<Notice>>> = Rc::new(RefCell::new(None));
    let sh = shared.clone();
    let n = notice.clone();
    start.connect_clicked(move |_| {
        n.borrow_mut().take();
        sh.start_recording();
    });
    let sh = shared.clone();
    stop.connect_clicked(move |_| {
        sh.recorder.stop(sh.recorder.status().id);
    });
    for (button, version) in [(&original, Version::Original), (&treated, Version::Treated)] {
        let sh = shared.clone();
        let n = notice.clone();
        button.connect_clicked(move |_| {
            if let Some(take) = sh.recorder.take(sh.recorder.status().id) {
                n.borrow_mut().take();
                sh.playback.play(take, version);
            }
        });
    }
    let sh = shared.clone();
    stop_play.connect_clicked(move |_| sh.playback.stop());
    let sh = shared.clone();
    let busy = exporting.clone();
    let sequence = export_sequence.clone();
    let n = notice.clone();
    let parent = parent.downgrade();
    export.connect_clicked(move |_| {
        if busy.get() {
            return;
        }
        let Some(take) = sh.recorder.take(sh.recorder.status().id) else {
            return;
        };
        let Some(parent) = parent.upgrade() else {
            return;
        };
        busy.set(true);
        sequence.set(sequence.get().wrapping_add(1));
        let key = OperationKey {
            recording: take.id,
            playback: sh.playback.status().operation,
            export: sequence.get(),
        };
        let busy = busy.clone();
        let n = n.clone();
        glib::spawn_future_local(async move {
            let result = export_dialog(&parent, take).await;
            busy.set(false);
            *n.borrow_mut() = Some(Notice { key, text: result });
        });
    });
    // Stop polling when the panel is destroyed; the timer does not own the window.
    let weak = root.downgrade();
    let update = move || {
        if weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        let r = shared.recorder.status();
        let pb = shared.playback.status();
        let connected = shared.status().link == Link::Connected;
        let view = presentation(&r, &pb, connected, exporting.get());
        start.set_sensitive(view.can_start);
        stop.set_sensitive(view.can_stop);
        original.set_sensitive(view.can_listen);
        treated.set_sensitive(view.can_listen);
        export.set_sensitive(view.can_export);
        stop_play.set_sensitive(pb.playing);
        for (button, version) in [(&original, Version::Original), (&treated, Version::Treated)] {
            if pb.playing && pb.take_id == Some(r.id) && pb.version == Some(version) {
                button.add_css_class("suggested-action");
            } else {
                button.remove_css_class("suggested-action");
            }
        }
        let caption = match r.state {
            TakeState::Empty => "Pronto para gravar",
            TakeState::Recording => "Gravando",
            TakeState::Ready => "Teste pronto",
            TakeState::Incomplete => "Teste incompleto",
        };
        state.set_label(&format!("{caption} · {}", view.duration));
        let key = OperationKey {
            recording: r.id,
            playback: pb.operation,
            export: export_sequence.get(),
        };
        let n = notice.borrow();
        let text = if exporting.get() {
            "Exportando o teste selecionado…"
        } else if let Some(text) = current_notice(&n, key) {
            text
        } else if let Some(error) = pb.error.as_deref().filter(|_| pb.take_id == Some(r.id)) {
            error
        } else if let Some(warning) = view.warning.as_deref() {
            warning
        } else if !connected {
            "Conecte o celular para gravar um novo teste."
        } else if pb.playing {
            "Reproduzindo no dispositivo de saída padrão do sistema."
        } else {
            "Até 30 segundos. Nada é salvo até você escolher Exportar WAVs."
        };
        message.set_label(text);
        glib::ControlFlow::Continue
    };
    update();
    glib::timeout_add_local(Duration::from_millis(100), update);
    root
}

async fn export_dialog(parent: &adw::ApplicationWindow, take: Arc<Take>) -> String {
    let dialog = gtk::FileDialog::builder()
        .title("Salvar teste original e tratado")
        .initial_name("teste-original.wav")
        .accept_label("Exportar")
        .build();
    let selected = match dialog.save_future(Some(parent)).await {
        Ok(file) => file,
        Err(e)
            if e.matches(gtk::DialogError::Dismissed) || e.matches(gtk::DialogError::Cancelled) =>
        {
            return "Exportação cancelada.".into();
        }
        Err(e) => return format!("Não foi possível escolher o destino: {e}"),
    };
    let Some(path) = selected.path() else {
        return "Escolha uma pasta local para exportar.".into();
    };
    let destinations = Destinations::new(&path);
    let confirmed = if destinations.needs_confirmation() {
        let confirm=adw::AlertDialog::builder().heading("Substituir arquivos do teste?")
            .body(format!("Os arquivos abaixo serão gravados. Os que já existem serão substituídos.\n\n{}\n{}",destinations.original.display(),destinations.treated.display()))
            .build();
        confirm.add_response("cancel", "Cancelar");
        confirm.add_response("replace", "Substituir");
        confirm.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        confirm.set_default_response(Some("cancel"));
        confirm.set_close_response("cancel");
        if confirm.choose_future(Some(parent)).await != "replace" {
            return "Exportação cancelada.".into();
        }
        true
    } else {
        false
    };
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = export_selection(&take, Some(&destinations), confirmed);
        let _ = tx.send_blocking(result);
    });
    match rx.recv().await {
        Ok(Ok(ExportOutcome::Saved))=>"Os dois arquivos WAV foram exportados.".into(),
        Ok(Ok(ExportOutcome::Cancelled))=>"Exportação cancelada: o destino mudou. Escolha novamente para confirmar a substituição.".into(),
        Ok(Err(e))=>e,
        Err(e)=>format!("A exportação foi interrompida: {e}"),
    }
}
