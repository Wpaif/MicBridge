//! Teste de voz: o que a interface mostra e as decisões de exportação.
use crate::recording::{RecordingStatus, Take, TakeState, playback::PlaybackStatus};
use std::path::{Path, PathBuf};
#[derive(Debug)]
pub(crate) struct Presentation {
    pub(crate) can_start: bool,
    pub(crate) can_stop: bool,
    pub(crate) can_listen: bool,
    pub(crate) can_export: bool,
    pub(crate) duration: String,
    pub(crate) warning: Option<String>,
}
pub(crate) fn presentation(
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
pub(crate) struct Destinations {
    pub(crate) original: PathBuf,
    pub(crate) treated: PathBuf,
}
impl Destinations {
    pub(crate) fn new(path: &Path) -> Self {
        let original = path.with_extension("wav");
        let stem = original.file_stem().unwrap_or_default().to_string_lossy();
        let stem = stem.strip_suffix("-original").unwrap_or(&stem);
        let treated = original.with_file_name(format!("{stem}-tratado.wav"));
        Self { original, treated }
    }
    pub(crate) fn needs_confirmation(&self) -> bool {
        self.original.exists() || self.treated.exists()
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExportOutcome {
    Cancelled,
    Saved,
}
pub(crate) fn export_selection(
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
pub(crate) struct OperationKey {
    pub(crate) recording: u64,
    pub(crate) playback: u64,
    pub(crate) export: u64,
}
pub(crate) struct Notice {
    pub(crate) key: OperationKey,
    pub(crate) text: String,
}
pub(crate) fn current_notice(notice: &Option<Notice>, key: OperationKey) -> Option<&str> {
    notice
        .as_ref()
        .filter(|n| n.key == key)
        .map(|n| n.text.as_str())
}
/// Linha de estado: fase do teste e duração gravada.
pub(crate) fn caption(r: &RecordingStatus, view: &Presentation) -> String {
    let caption = match r.state {
        TakeState::Empty => "Pronto para gravar",
        TakeState::Recording => "Gravando",
        TakeState::Ready => "Teste pronto",
        TakeState::Incomplete => "Teste incompleto",
    };
    format!("{caption} · {}", view.duration)
}
/// Mensagem do painel, da mais urgente para a dica padrão.
pub(crate) fn message<'a>(
    r: &RecordingStatus,
    pb: &'a PlaybackStatus,
    view: &'a Presentation,
    connected: bool,
    exporting: bool,
    notice: Option<&'a str>,
) -> &'a str {
    if exporting {
        "Exportando o teste selecionado…"
    } else if let Some(text) = notice {
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
    }
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
