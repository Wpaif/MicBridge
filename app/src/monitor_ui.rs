//! Controles e apresentação do retorno no fone.
use crate::monitor::{
    MonitorPhase, MonitorStatus,
    devices::{DeviceSnapshot, OutputDevice},
};
use std::time::Duration;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choices {
    devices: Vec<Option<OutputDevice>>,
    labels: Vec<String>,
    selected: u32,
    available: bool,
}
impl Choices {
    fn new(snapshot: &DeviceSnapshot, selected: Option<&OutputDevice>) -> Self {
        let mut c = Self {
            devices: vec![None],
            labels: vec!["Escolha uma saída".into()],
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
            c.labels.push(format!("{} · indisponível", selected.label));
            c.selected = (c.devices.len() - 1) as u32;
        }
        c
    }
}
struct Presentation {
    active: bool,
    can_toggle: bool,
    can_restore: bool,
    text: String,
}
fn presentation(status: &MonitorStatus, connected: bool, available: bool) -> Presentation {
    let active = matches!(status.phase, MonitorPhase::Opening | MonitorPhase::Active);
    let text = match status.phase {
        MonitorPhase::Disabled => "Retorno desligado".into(),
        MonitorPhase::Restoring => "Restaurando conexões…".into(),
        MonitorPhase::Opening => "Preparando saída exclusiva…".into(),
        MonitorPhase::Active => format!(
            "Retorno exclusivo em {}",
            status
                .target
                .as_ref()
                .map(|d| d.label.as_str())
                .unwrap_or("saída selecionada")
        ),
        MonitorPhase::Failed => status
            .error
            .clone()
            .unwrap_or_else(|| "Falha local no retorno".into()),
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
enum PendingAction {
    Starting(u64),
    Stopping,
    Recovering,
}
impl PendingAction {
    fn active(self) -> bool {
        matches!(self, Self::Starting(_))
    }
    fn text(self) -> &'static str {
        if self.active() {
            "Preparando saída exclusiva…"
        } else {
            "Restaurando conexões…"
        }
    }
}
#[derive(Default)]
struct RefreshSchedule {
    last: Option<Duration>,
}
impl RefreshSchedule {
    fn due(&mut self, now: Duration, active: bool, busy: bool) -> bool {
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
    #[ignore = "requer sessão gráfica GTK"]
    fn exclusive_panel_prepares_restores_retries_and_releases_during_query() {
        use crate::{
            audio_test::{Fixture, SLEEP},
            monitor::{
                Monitor,
                routing::{Routing, controller::tests::Fake, graph::tests::target},
            },
            recording::playback::Playback,
        };
        use std::sync::atomic::Ordering::SeqCst;
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.ExclusivePanelTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        fn until(mut yes: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(4);
            while !yes() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                assert!(Instant::now() < deadline, "GTK update timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        fn find<T: IsA<gtk::Widget> + Clone>(
            w: &impl IsA<gtk::Widget>,
            predicate: &impl Fn(&T) -> bool,
        ) -> Option<T> {
            let w = w.as_ref();
            if let Some(t) = w.downcast_ref::<T>() {
                if predicate(t) {
                    return Some(t.clone());
                }
            }
            let mut c = w.first_child();
            while let Some(w) = c {
                if let Some(t) = find::<T>(&w, predicate) {
                    return Some(t);
                }
                c = w.next_sibling();
            }
            None
        }
        let f = Fixture::new(SLEEP);
        let b = Fake::new();
        let r = Routing::new(Arc::new(b.clone()), f.dir.join("journal.json"));
        let shared = Arc::new(Shared::with_audio_services(
            Monitor::with_test_routing(f.executable.clone(), r),
            Playback::new(),
            crate::monitor::devices::DeviceCatalog::with_query(Arc::new(|| {
                Box::pin(async { Ok(vec![target()]) })
            })),
        ));
        shared.set_link(Link::Connected);
        let parent = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(520)
            .default_height(600)
            .build();
        let root = panel(shared.clone());
        parent.set_content(Some(&root));
        parent.present();
        until(|| parent.is_realized());
        assert!(
            find::<gtk::Label>(&root, &|l| l.text()
                == "Durante o ajuste, outros aplicativos ficam desconectados desta saída")
            .is_some()
        );
        let dropdown = find::<gtk::DropDown>(&root, &|_| true).unwrap();
        let switch = find::<gtk::Switch>(&root, &|_| true).unwrap();
        until(|| dropdown.model().is_some_and(|m| m.n_items() == 2));
        dropdown.set_selected(1);
        until(|| switch.is_sensitive());
        let (opening_tx, opening_rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(opening_rx);
        switch.set_active(true);
        let requested_id = shared.monitor.status().id;
        assert!(
            !switch.is_sensitive(),
            "lock immediately, before the polling timer"
        );
        assert!(!dropdown.is_sensitive());
        let progress = find::<gtk::Spinner>(&root, &|_| true).expect("visible progress indicator");
        assert!(progress.is_spinning());
        assert!(
            find::<gtk::Label>(&root, &|l| l.text() == "Preparando saída exclusiva…").is_some()
        );
        switch.set_active(false);
        switch.set_active(true);
        assert_eq!(shared.monitor.status().id, requested_id, "duplicate intent");
        let responsive = Rc::new(Cell::new(false));
        let flag = responsive.clone();
        glib::idle_add_local_once(move || flag.set(true));
        until(|| responsive.get() && b.entered.load(SeqCst));
        let end = Instant::now() + Duration::from_millis(250);
        until(|| {
            assert!(!switch.is_sensitive());
            assert!(
                switch.is_active(),
                "opening must not flicker off during cleanup"
            );
            Instant::now() >= end
        });
        opening_tx.send(()).unwrap();
        until(|| shared.monitor.status().phase == MonitorPhase::Active && switch.is_sensitive());
        assert!(!progress.is_spinning());
        until(|| {
            find::<gtk::Label>(&root, &|l| l.text().starts_with("Retorno exclusivo em ")).is_some()
        });
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        b.fail_restore.store(true, SeqCst);
        switch.set_active(false);
        assert!(!switch.is_sensitive(), "stop must lock immediately");
        assert!(progress.is_spinning());
        switch.set_active(true);
        assert!(
            !switch.is_active(),
            "ignore reactivation until restoration completes"
        );
        assert_eq!(shared.monitor.status().id, requested_id);
        until(|| find::<gtk::Label>(&root, &|l| l.text() == "Restaurando conexões…").is_some());
        let _ = tx.send(());
        let retry = find::<gtk::Button>(&root, &|b| {
            b.label().as_deref() == Some("Tentar restaurar conexões")
        })
        .unwrap();
        until(|| retry.is_visible());
        assert!(!switch.is_sensitive());
        b.fail_restore.store(false, SeqCst);
        retry.emit_clicked();
        until(|| !shared.monitor.status().restore_pending && switch.is_sensitive());
        assert!(!retry.is_visible());
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(cancel_rx);
        b.entered.store(false, SeqCst);
        switch.set_active(true);
        until(|| b.entered.load(SeqCst));
        // Another app action may cancel opening while the phone remains connected.
        shared.stop_monitor();
        cancel_tx.send(()).unwrap();
        until(|| shared.monitor.status().phase == MonitorPhase::Disabled && switch.is_sensitive());
        assert!(!switch.is_active());
        assert!(!progress.is_spinning());
        let (tx, rx) = tokio::sync::oneshot::channel();
        *b.pause.lock().unwrap() = Some(rx);
        b.entered.store(false, SeqCst);
        switch.set_active(true);
        until(|| b.entered.load(SeqCst));
        let weak = root.downgrade();
        parent.set_content(None::<&gtk::Widget>);
        drop(dropdown);
        drop(switch);
        drop(retry);
        drop(progress);
        drop(root);
        parent.destroy();
        drop(parent);
        until(|| weak.upgrade().is_none());
        let stopped = shared.local_audio.shutdown_and_wait();
        let _ = tx.send(());
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(stopped)
            .unwrap();
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn panel_controls_monitor_and_releases_window_during_pending_query() {
        use crate::{
            audio_test::{Fixture, SLEEP, alive},
            monitor::{
                Monitor,
                devices::{DeviceCatalog, DeviceQuery},
            },
            recording::playback::Playback,
            state::{Link, Shared},
        };
        use std::{
            sync::{Arc, Mutex},
            time::{Duration, Instant},
        };
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.MonitorPanelTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        fn until(mut condition: impl FnMut() -> bool) {
            let end = Instant::now() + Duration::from_secs(3);
            while !condition() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                assert!(Instant::now() < end, "GTK condition timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        fn find<T: IsA<gtk::Widget> + Clone>(widget: &impl IsA<gtk::Widget>) -> Option<T> {
            let widget = widget.as_ref();
            if let Some(w) = widget.downcast_ref::<T>() {
                return Some(w.clone());
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                if let Some(found) = find::<T>(&w) {
                    return Some(found);
                }
                child = w.next_sibling();
            }
            None
        }
        let f = Fixture::new(SLEEP);
        let catalog =
            DeviceCatalog::with_query(Arc::new(|| Box::pin(async { Ok(vec![device(1)]) })));
        let shared = Arc::new(Shared::with_audio_services(
            Monitor::with_executable(f.executable.clone()),
            Playback::new(),
            catalog,
        ));
        shared.set_link(Link::Connected);
        let parent = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(560)
            .default_height(500)
            .build();
        let root = panel(shared.clone());
        parent.set_content(Some(&root));
        parent.present();
        until(|| parent.is_realized());
        let outputs = find::<gtk::DropDown>(&root).unwrap();
        let switch = find::<gtk::Switch>(&root).unwrap();
        let volume = find::<gtk::Scale>(&root).unwrap();
        assert!(!switch.is_active());
        assert_eq!(volume.value(), 25.0);
        until(|| outputs.model().is_some_and(|m| m.n_items() == 2));
        outputs.set_selected(1);
        until(|| switch.is_sensitive());
        volume.set_value(50.0);
        assert_eq!(shared.monitor.status().volume_percent, 50.0);
        switch.set_active(true);
        until(|| shared.monitor.status().phase == MonitorPhase::Active && switch.is_sensitive());
        until(|| f.pid().is_some());
        let pid = f.pid().unwrap();
        switch.set_active(false);
        until(|| !alive(pid));
        assert!(shared.monitor.active_id(shared.generation()).is_none());
        shared.stop_audio_tools();
        parent.set_content(None::<&gtk::Widget>);
        drop(root);
        parent.destroy();
        drop(parent);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let pending = Arc::new(Mutex::new(Some(rx)));
        let query: DeviceQuery = Arc::new(move || {
            let pending = pending.lock().unwrap().take();
            Box::pin(async move {
                if let Some(r) = pending {
                    let _ = r.await;
                }
                Ok(vec![device(1)])
            })
        });
        let shared = Arc::new(Shared::with_audio_services(
            Monitor::new(),
            Playback::new(),
            DeviceCatalog::with_query(query),
        ));
        let parent = adw::ApplicationWindow::builder().application(&app).build();
        let root = panel(shared.clone());
        parent.set_content(Some(&root));
        parent.present();
        until(|| parent.is_realized());
        let weak_root = root.downgrade();
        let weak_window = parent.downgrade();
        parent.set_content(None::<&gtk::Widget>);
        drop(root);
        parent.destroy();
        drop(parent);
        until(|| weak_root.upgrade().is_none() && weak_window.upgrade().is_none());
        let _ = tx.send(());
        until(|| !shared.devices.snapshot().busy);
        shared.stop_audio_tools();
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

use crate::state::{Link, Shared};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Instant,
};
pub fn panel(shared: Arc<Shared>) -> gtk::Box {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .css_classes(["mb-hero", "mb-monitor"])
        .build();
    let title = gtk::Label::builder()
        .label("RETORNO NO FONE")
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let label = gtk::Label::builder()
        .label("Saída de áudio")
        .xalign(0.0)
        .build();
    let outputs = gtk::DropDown::from_strings(&["Escolha uma saída"]);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            item.set_child(Some(
                &gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(28)
                    .build(),
            ));
        }
    });
    factory.connect_bind(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            if let (Some(value), Some(label)) = (
                item.item().and_downcast::<gtk::StringObject>(),
                item.child().and_downcast::<gtk::Label>(),
            ) {
                label.set_label(&value.string());
            }
        }
    });
    outputs.set_factory(Some(&factory));
    outputs.set_hexpand(true);
    outputs.set_width_request(140);
    outputs.set_tooltip_text(Some(
        "Escolha explicitamente o dispositivo de saída do retorno",
    ));
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Atualizar saídas")
        .build();
    let device_row = gtk::Box::builder().spacing(6).build();
    device_row.append(&outputs);
    device_row.append(&refresh);
    let switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .tooltip_text("Ouvir voz tratada e desconectar outros aplicativos desta saída")
        .build();
    let switch_label = gtk::Label::builder()
        .label("_Ajuste em tempo real · retorno exclusivo")
        .wrap(true)
        .use_underline(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    switch_label.set_mnemonic_widget(Some(&switch));
    let switch_row = gtk::Box::builder().spacing(12).build();
    let progress = gtk::Spinner::builder()
        .valign(gtk::Align::Center)
        .tooltip_text("Aguarde a conclusão da operação de áudio")
        .build();
    switch_row.append(&switch_label);
    switch_row.append(&progress);
    switch_row.append(&switch);
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.set_digits(0);
    volume.set_value(25.0);
    volume.set_hexpand(true);
    volume.set_draw_value(false);
    volume.set_tooltip_text(Some(
        "Volume exclusivo do retorno; não altera chamadas ou gravações",
    ));
    let volume_text = gtk::Label::builder()
        .label("25%")
        .width_chars(5)
        .xalign(1.0)
        .build();
    let volume_label = gtk::Label::builder()
        .label("Volume do retorno")
        .xalign(0.0)
        .build();
    let volume_row = gtk::Box::builder().spacing(12).build();
    volume_row.append(&volume);
    volume_row.append(&volume_text);
    let status = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let exclusive_hint = gtk::Label::builder()
        .label("Durante o ajuste, outros aplicativos ficam desconectados desta saída")
        .xalign(0.0)
        .wrap(true)
        .css_classes(["caption", "dim-label"])
        .build();
    let retry = gtk::Button::with_label("Tentar restaurar conexões");
    retry.set_visible(false);
    retry.set_halign(gtk::Align::Start);
    let hint=gtk::Label::builder().label("Use fones para evitar que o som volte ao microfone. O retorno tem atraso de monitoramento.").xalign(0.0).wrap(true).css_classes(["caption","dim-label"]).build();
    for widget in [
        title.upcast_ref::<gtk::Widget>(),
        label.upcast_ref(),
        device_row.upcast_ref(),
        switch_row.upcast_ref(),
        exclusive_hint.upcast_ref(),
        volume_label.upcast_ref(),
        volume_row.upcast_ref(),
        status.upcast_ref(),
        retry.upcast_ref(),
        hint.upcast_ref(),
    ] {
        root.append(widget);
    }
    let updating = Rc::new(Cell::new(false));
    let choices = Rc::new(RefCell::new(Choices::new(&DeviceSnapshot::default(), None)));
    let error = Rc::new(RefCell::new(None::<String>));
    let schedule = Rc::new(RefCell::new(RefreshSchedule::default()));
    let origin = Instant::now();
    let pending = Rc::new(Cell::new(None::<PendingAction>));
    // Weak widget references prevent callbacks from retaining their own controls.
    let lock_controls = {
        let switch = switch.downgrade();
        let outputs = outputs.downgrade();
        let refresh = refresh.downgrade();
        let retry = retry.downgrade();
        let progress = progress.downgrade();
        let status = status.downgrade();
        let guard = updating.clone();
        move |action: PendingAction| {
            guard.set(true);
            if let Some(s) = switch.upgrade() {
                s.set_sensitive(false);
                s.set_active(action.active());
            }
            if let Some(w) = outputs.upgrade() {
                w.set_sensitive(false);
            }
            if let Some(w) = refresh.upgrade() {
                w.set_sensitive(false);
            }
            if let Some(w) = retry.upgrade() {
                w.set_sensitive(false);
            }
            if let Some(w) = progress.upgrade() {
                w.start();
            }
            if let Some(w) = status.upgrade() {
                w.set_label(action.text());
            }
            guard.set(false);
        }
    };
    let sh = shared.clone();
    let busy = pending.clone();
    let lock = lock_controls.clone();
    retry.connect_clicked(move |button| {
        if busy.get().is_some() || !button.is_sensitive() {
            return;
        }
        busy.set(Some(PendingAction::Recovering));
        lock(PendingAction::Recovering);
        let done = sh.monitor.retry_restore();
        let busy = busy.clone();
        glib::spawn_future_local(async move {
            let _ = done.await;
            if busy.get() == Some(PendingAction::Recovering) {
                busy.set(None);
            }
        });
    });
    let sh = shared.clone();
    let guard = updating.clone();
    let c = choices.clone();
    let e = error.clone();
    outputs.connect_selected_notify(move |dropdown| {
        if guard.get() {
            return;
        }
        let target = c
            .borrow()
            .devices
            .get(dropdown.selected() as usize)
            .cloned()
            .flatten();
        sh.select_monitor_output(target);
        e.borrow_mut().take();
    });
    let sh = shared.clone();
    let guard = updating.clone();
    let e = error.clone();
    let busy = pending.clone();
    let lock = lock_controls.clone();
    switch.connect_active_notify(move |s| {
        if guard.get() {
            return;
        }
        if let Some(action) = busy.get() {
            lock(action);
            return;
        }
        // Disable synchronously: no second click can enter before the next timer tick.
        let active = s.is_active();
        let action = if active {
            PendingAction::Starting(0)
        } else {
            PendingAction::Stopping
        };
        busy.set(Some(action));
        lock(action);
        e.borrow_mut().take();
        if active {
            let result = sh
                .monitor_output()
                .ok_or_else(|| "Escolha uma saída para o retorno".to_string())
                .and_then(|target| sh.start_monitor(target));
            match result {
                Ok(id) => busy.set(Some(PendingAction::Starting(id))),
                Err(error) => {
                    busy.set(None);
                    *e.borrow_mut() = Some(error);
                    guard.set(true);
                    s.set_active(false);
                    guard.set(false);
                }
            }
        } else {
            sh.stop_monitor();
            let done = sh.monitor.stop_and_wait();
            let busy = busy.clone();
            glib::spawn_future_local(async move {
                let _ = done.await;
                if busy.get() == Some(PendingAction::Stopping) {
                    busy.set(None);
                }
            });
        }
    });
    let sh = shared.clone();
    let guard = updating.clone();
    volume.connect_value_changed(move |scale| {
        if !guard.get() {
            sh.monitor.set_volume(scale.value() as f32);
        }
    });
    let sh = shared.clone();
    let sc = schedule.clone();
    let e = error.clone();
    refresh.connect_clicked(move |_| {
        if !sh.devices.snapshot().busy {
            sc.borrow_mut().last = Some(origin.elapsed());
            e.borrow_mut().take();
            sh.devices.refresh();
        }
    });
    let weak = root.downgrade();
    let update = move || {
        if weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        let m = shared.monitor.status();
        let connected = shared.status().link == Link::Connected;
        if let Some(PendingAction::Starting(id)) = pending.get() {
            let settled = matches!(m.phase, MonitorPhase::Active | MonitorPhase::Failed);
            let cancelled = !shared.local_audio.monitor_requested(id);
            if settled || cancelled {
                pending.set(None);
            }
        }
        let action = pending.get();
        let transitioning =
            action.is_some() || matches!(m.phase, MonitorPhase::Opening | MonitorPhase::Restoring);
        let snapshot = shared.devices.snapshot();
        if schedule.borrow_mut().due(
            origin.elapsed(),
            m.phase == MonitorPhase::Active,
            snapshot.busy,
        ) {
            shared.devices.refresh();
        }
        let selected = shared.monitor_output();
        let current = Choices::new(&snapshot, selected.as_ref());
        updating.set(true);
        if *choices.borrow() != current {
            let labels: Vec<&str> = current.labels.iter().map(String::as_str).collect();
            let model = gtk::StringList::new(&labels);
            outputs.set_model(Some(&model));
            outputs.set_selected(current.selected);
            *choices.borrow_mut() = current.clone();
        }
        refresh.set_sensitive(!snapshot.busy && !transitioning);
        outputs.set_sensitive(current.devices.len() > 1 && !transitioning);
        let available = current.available && snapshot.error.is_none() && !snapshot.busy;
        let view = presentation(&m, connected, available);
        switch.set_sensitive(view.can_toggle && !transitioning);
        progress.set_spinning(transitioning);
        retry.set_visible(view.can_restore);
        retry.set_sensitive(view.can_restore && !transitioning);
        let active = action.map(PendingAction::active).unwrap_or(view.active);
        if switch.is_active() != active {
            switch.set_active(active);
        }
        volume.set_value(m.volume_percent as f64);
        volume_text.set_label(&format!("{:.0}%", m.volume_percent));
        let message = if let Some(action) = action {
            action.text().to_string()
        } else if m.phase == MonitorPhase::Restoring {
            view.text.clone()
        } else {
            error
                .borrow()
                .clone()
                .or_else(|| m.error.clone())
                .or_else(|| snapshot.error.clone())
                .unwrap_or(view.text)
        };
        status.set_label(&message);
        updating.set(false);
        glib::ControlFlow::Continue
    };
    update();
    glib::timeout_add_local(Duration::from_millis(100), update);
    root
}
