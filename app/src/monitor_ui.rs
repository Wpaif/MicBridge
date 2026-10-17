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
    text: String,
}
fn presentation(status: &MonitorStatus, connected: bool, available: bool) -> Presentation {
    let active = matches!(status.phase, MonitorPhase::Opening | MonitorPhase::Active);
    let text = match status.phase {
        MonitorPhase::Disabled => "Retorno desligado".into(),
        MonitorPhase::Restoring => String::new(),
        MonitorPhase::Opening => "Preparando retorno…".into(),
        MonitorPhase::Active => format!(
            "Retorno ativo em {}",
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
        can_toggle: active || (connected && available),
        text,
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
        until(|| shared.monitor.status().phase == MonitorPhase::Active);
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
            assert!(view.can_toggle);
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
        .tooltip_text("Ouvir voz tratada no fone")
        .build();
    let switch_label = gtk::Label::builder()
        .label("_Ouvir voz tratada")
        .use_underline(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    switch_label.set_mnemonic_widget(Some(&switch));
    let switch_row = gtk::Box::builder().spacing(12).build();
    switch_row.append(&switch_label);
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
    let hint=gtk::Label::builder().label("Use fones para evitar que o som volte ao microfone. O retorno tem atraso de monitoramento.").xalign(0.0).wrap(true).css_classes(["caption","dim-label"]).build();
    for widget in [
        title.upcast_ref::<gtk::Widget>(),
        label.upcast_ref(),
        device_row.upcast_ref(),
        switch_row.upcast_ref(),
        volume_label.upcast_ref(),
        volume_row.upcast_ref(),
        status.upcast_ref(),
        hint.upcast_ref(),
    ] {
        root.append(widget);
    }
    let updating = Rc::new(Cell::new(false));
    let choices = Rc::new(RefCell::new(Choices::new(&DeviceSnapshot::default(), None)));
    let error = Rc::new(RefCell::new(None::<String>));
    let schedule = Rc::new(RefCell::new(RefreshSchedule::default()));
    let origin = Instant::now();
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
    switch.connect_active_notify(move |s| {
        if guard.get() {
            return;
        }
        if s.is_active() {
            let result = sh
                .monitor_output()
                .ok_or_else(|| "Escolha uma saída para o retorno".to_string())
                .and_then(|target| sh.start_monitor(target));
            *e.borrow_mut() = result.err();
        } else {
            sh.stop_monitor();
            e.borrow_mut().take();
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
        refresh.set_sensitive(!snapshot.busy);
        outputs.set_sensitive(current.devices.len() > 1);
        let available = current.available && snapshot.error.is_none() && !snapshot.busy;
        let view = presentation(&m, shared.status().link == Link::Connected, available);
        switch.set_sensitive(view.can_toggle);
        switch.set_active(view.active);
        volume.set_value(m.volume_percent as f64);
        volume_text.set_label(&format!("{:.0}%", m.volume_percent));
        let message = error
            .borrow()
            .clone()
            .or_else(|| m.error.clone())
            .or_else(|| snapshot.error.clone())
            .unwrap_or(view.text);
        status.set_label(&message);
        updating.set(false);
        glib::ControlFlow::Continue
    };
    update();
    glib::timeout_add_local(Duration::from_millis(100), update);
    root
}
