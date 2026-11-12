//! Controles e apresentação do retorno no fone.
use crate::monitor::{
    MonitorPhase, MonitorStatus,
    devices::{DeviceSnapshot, OutputDevice},
};
use std::time::Duration;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choices {
    pub(crate) devices: Vec<Option<OutputDevice>>,
    pub(crate) labels: Vec<String>,
    pub(crate) selected: u32,
    pub(crate) available: bool,
}
impl Choices {
    pub(crate) fn new(snapshot: &DeviceSnapshot, selected: Option<&OutputDevice>) -> Self {
        let mut c = Self {
            devices: vec![None],
            labels: vec![crate::i18n::t!("daemon.monitor.choose_short").into()],
            selected: 0,
            available: false,
        };
        for device in &snapshot.devices {
            let duplicate = snapshot
                .devices
                .iter()
                .filter(|d| d.label == device.label)
                .count()
                > 1;
            c.labels.push(if duplicate {
                format!("{} · #{}", device.label, device.id)
            } else {
                device.label.clone()
            });
            c.devices.push(Some(device.clone()));
            if selected.is_some_and(|d| d.same_instance(device)) {
                c.selected = (c.devices.len() - 1) as u32;
                c.available = true;
            }
        }
        if let Some(selected) = selected.filter(|_| !c.available) {
            c.devices.push(Some(selected.clone()));
            c.labels.push(crate::i18n::t!(
                "daemon.monitor.device_unavailable",
                device = selected.label
            ));
            c.selected = (c.devices.len() - 1) as u32;
        }
        c
    }
}
pub(crate) struct Presentation {
    pub(crate) active: bool,
    pub(crate) can_toggle: bool,
    pub(crate) can_restore: bool,
    pub(crate) text: String,
}
pub(crate) fn presentation(
    status: &MonitorStatus,
    connected: bool,
    available: bool,
) -> Presentation {
    let active = matches!(status.phase, MonitorPhase::Opening | MonitorPhase::Active);
    let text = match status.phase {
        MonitorPhase::Disabled => crate::i18n::t!("daemon.monitor.off").into(),
        MonitorPhase::Restoring => crate::i18n::t!("daemon.monitor.restoring").into(),
        MonitorPhase::Opening => crate::i18n::t!("daemon.monitor.preparing").into(),
        MonitorPhase::Active => crate::i18n::t!(
            "daemon.monitor.active",
            device = status
                .target
                .as_ref()
                .map(|d| d.label.as_str())
                .unwrap_or(crate::i18n::t!("daemon.monitor.selected_output"))
        ),
        MonitorPhase::Failed => status
            .error
            .clone()
            .unwrap_or_else(|| crate::i18n::t!("daemon.monitor.failed").into()),
    };
    Presentation {
        active,
        can_restore: status.restore_pending && status.phase != MonitorPhase::Restoring,
        can_toggle: !status.restore_pending
            && !matches!(
                status.phase,
                MonitorPhase::Opening | MonitorPhase::Restoring
            )
            && (active || (connected && available)),
        text,
    }
}
// Keep the user's pending intent stable across intermediate worker cleanup states.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingAction {
    Starting(u64),
    Stopping,
    Recovering,
}
impl PendingAction {
    pub(crate) fn active(self) -> bool {
        matches!(self, Self::Starting(_))
    }
    pub(crate) fn text(self) -> &'static str {
        if self.active() {
            crate::i18n::t!("daemon.monitor.preparing")
        } else {
            crate::i18n::t!("daemon.monitor.restoring")
        }
    }
}
#[derive(Default)]
pub(crate) struct RefreshSchedule {
    pub(crate) last: Option<Duration>,
}
impl RefreshSchedule {
    pub(crate) fn due(&mut self, now: Duration, active: bool, busy: bool) -> bool {
        if busy {
            return false;
        }
        if self.last.is_none()
            || (active
                && now.saturating_sub(self.last.unwrap_or_default()) >= Duration::from_secs(2))
        {
            self.last = Some(now);
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn device(id: u32) -> OutputDevice {
        OutputDevice {
            id,
            serial: Some(id.to_string()),
            name: format!("sink.{id}"),
            label: "Fone".into(),
        }
    }
    fn snapshot(devices: Vec<OutputDevice>) -> DeviceSnapshot {
        DeviceSnapshot {
            query_id: 1,
            devices,
            busy: false,
            error: None,
        }
    }
    fn status(phase: MonitorPhase) -> MonitorStatus {
        MonitorStatus {
            id: 1,
            phase,
            target: Some(device(1)),
            generation: Some(1),
            volume_percent: 25.0,
            error: None,
            restore_report: None,
            restore_pending: false,
        }
    }
    #[test]
    fn exclusive_presentation_waits_for_routing_and_offers_retry_only_after_failure() {
        let opening = presentation(&status(MonitorPhase::Opening), true, true);
        assert_eq!(opening.text, "Preparando saída exclusiva…");
        assert!(opening.active);
        assert!(!opening.can_toggle);
        let active = presentation(&status(MonitorPhase::Active), true, true);
        assert!(active.text.starts_with("Retorno exclusivo em "));
        let restoring = presentation(&status(MonitorPhase::Restoring), true, true);
        assert_eq!(restoring.text, "Restaurando conexões…");
        assert!(!restoring.active);
        assert!(!restoring.can_toggle);
        assert!(!restoring.can_restore);
        let mut failed = status(MonitorPhase::Failed);
        failed.restore_pending = true;
        failed.error = Some("Falha ao restaurar".into());
        let pending = presentation(&failed, false, false);
        assert!(pending.can_restore);
        assert!(!pending.can_toggle);
        assert_eq!(pending.text, "Falha ao restaurar");
    }
    #[test]
    fn choices_preserve_identity_and_disambiguate_labels_without_auto_selection() {
        let d = vec![device(1), device(2)];
        let s = snapshot(d.clone());
        let empty = Choices::new(&s, None);
        assert_eq!(empty.selected, 0);
        assert!(!empty.available);
        let selected = Choices::new(&s, Some(&d[1]));
        assert_eq!(selected.selected, 2);
        assert!(selected.available);
        assert_ne!(selected.labels[1], selected.labels[2]);
        let reversed = Choices::new(&snapshot(vec![d[1].clone(), d[0].clone()]), Some(&d[1]));
        assert_eq!(reversed.selected, 1);
        let removed = Choices::new(&snapshot(vec![d[0].clone()]), Some(&d[1]));
        assert!(!removed.available);
        assert!(removed.labels[removed.selected as usize].contains("indisponível"));
        assert_eq!(
            removed.devices[removed.selected as usize],
            Some(d[1].clone())
        );
        let mut replaced = d[1].clone();
        replaced.serial = Some("new".into());
        assert!(!Choices::new(&snapshot(vec![replaced]), Some(&d[1])).available);
    }
    #[test]
    fn presentation_enforces_disconnected_selection_and_local_failure_states() {
        let off = presentation(&status(MonitorPhase::Disabled), true, true);
        assert!(!off.active);
        assert!(off.can_toggle);
        assert!(!presentation(&status(MonitorPhase::Disabled), false, true).can_toggle);
        assert!(!presentation(&status(MonitorPhase::Disabled), true, false).can_toggle);
        for p in [MonitorPhase::Opening, MonitorPhase::Active] {
            let view = presentation(&status(p), true, true);
            assert!(view.active);
            assert_eq!(view.can_toggle, p == MonitorPhase::Active);
            assert!(!view.text.is_empty());
        }
        let mut failed = status(MonitorPhase::Failed);
        failed.error = Some("Falha local".into());
        assert!(
            presentation(&failed, true, true)
                .text
                .contains("Falha local")
        );
    }
    #[test]
    fn refresh_occurs_initially_then_every_two_seconds_only_while_active_and_not_busy() {
        let mut r = RefreshSchedule::default();
        assert!(r.due(Duration::ZERO, false, false));
        assert!(!r.due(Duration::from_secs(1), true, false));
        assert!(!r.due(Duration::from_secs(2), false, false));
        assert!(!r.due(Duration::from_secs(2), true, true));
        assert!(r.due(Duration::from_secs(2), true, false));
        assert!(!r.due(Duration::from_secs(3), true, false));
        assert!(r.due(Duration::from_secs(4), true, false));
    }
}
