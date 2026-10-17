//! Edição explícita dos perfis; disco fora do thread GTK e do produtor de áudio.
use crate::{
    dsp::Preset,
    profiles::{Config, Library},
    settings,
    state::Shared,
};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Clone)]
pub struct Controls {
    /// denoise, highpass, gate, deesser, presence, compressor, auto_gain.
    pub switches: [adw::SwitchRow; 7],
    /// gate, threshold, ratio, makeup.
    pub scales: [gtk::Scale; 4],
}
impl Controls {
    fn sync(&self, config: &Config) {
        for (row, value) in self.switches.iter().zip([
            config.denoise,
            config.highpass,
            config.gate,
            config.deesser,
            config.presence,
            config.compressor,
            config.auto_gain,
        ]) {
            row.set_active(value);
        }
        for (scale, value) in self.scales.iter().zip([
            config.gate_db,
            config.threshold_db,
            config.ratio,
            config.makeup_db,
        ]) {
            scale.set_value(value as f64);
        }
    }
    fn sensitive(&self, enabled: bool) {
        for row in &self.switches {
            row.set_sensitive(enabled);
        }
        for scale in &self.scales {
            scale.set_sensitive(enabled);
        }
    }
}
struct Ui {
    shared: Arc<Shared>,
    controls: Controls,
    library: RefCell<Library>,
    error: RefCell<Option<String>>,
    busy: Cell<bool>,
    read_only: bool,
    syncing: Cell<bool>,
    root: glib::WeakRef<gtk::Box>,
    selector: adw::ComboRow,
    names: gtk::StringList,
    message: gtk::Label,
    new: gtk::Button,
    save: gtk::Button,
    rename: gtk::Button,
    delete: gtk::Button,
}
pub fn panel(shared: Arc<Shared>, controls: Controls) -> gtk::Box {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    let names = gtk::StringList::new(&[]);
    let selector = adw::ComboRow::builder()
        .name("audio-profile")
        .title("Perfil")
        .model(&names)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.append(&selector);
    root.append(&list);
    let actions = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(4)
        .column_spacing(6)
        .row_spacing(6)
        .homogeneous(true)
        .build();
    let button = |label, name| gtk::Button::builder().label(label).name(name).build();
    let new = button("Salvar como novo…", "profile-new");
    let save = button("Salvar alterações", "profile-save");
    let rename = button("Renomear…", "profile-rename");
    let delete = button("Excluir…", "profile-delete");
    save.add_css_class("suggested-action");
    delete.add_css_class("destructive-action");
    for button in [&new, &save, &rename, &delete] {
        actions.insert(button, -1);
    }
    root.append(&actions);
    let message = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["caption"])
        .build();
    root.append(&message);
    let (library, error) = match settings::load_profiles_from(&settings::path()) {
        Ok(library) => (library, None),
        Err(error) => (
            Library::new(settings::load_preset().unwrap_or(Preset::Chamada)),
            Some(format!(
                "Não foi possível carregar os perfis: {error}. O arquivo foi preservado. Corrija as preferências e reabra o aplicativo para salvar perfis."
            )),
        ),
    };
    let config = library.config().expect("biblioteca validada");
    config.apply(&shared.params);
    controls.sync(&config);
    let ui = Rc::new(Ui {
        shared,
        controls,
        library: RefCell::new(library),
        read_only: error.is_some(),
        error: RefCell::new(error),
        busy: Cell::new(false),
        syncing: Cell::new(false),
        root: root.downgrade(),
        selector,
        names,
        message,
        new,
        save,
        rename,
        delete,
    });
    for row in &ui.controls.switches {
        let weak = Rc::downgrade(&ui);
        row.connect_active_notify(move |_| {
            if let Some(ui) = weak.upgrade() {
                if !ui.syncing.get() {
                    ui.refresh();
                }
            }
        });
    }
    for scale in &ui.controls.scales {
        let weak = Rc::downgrade(&ui);
        scale.connect_value_changed(move |_| {
            if let Some(ui) = weak.upgrade() {
                if !ui.syncing.get() {
                    ui.refresh();
                }
            }
        });
    }
    let weak = Rc::downgrade(&ui);
    ui.selector.connect_selected_notify(move |row| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.syncing.get() || ui.busy.get() {
            return;
        }
        let index = row.selected() as usize;
        let target = ui.ids().get(index).cloned();
        ui.refresh();
        if let Some(target) = target {
            if target != ui.library.borrow().selected {
                let ui = ui.clone();
                glib::spawn_future_local(async move {
                    ui.select(target).await;
                });
            }
        }
    });
    for (button, rename) in [(&ui.new, false), (&ui.rename, true)] {
        let weak = Rc::downgrade(&ui);
        button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.name_dialog(rename);
            }
        });
    }
    let weak = Rc::downgrade(&ui);
    ui.save.connect_clicked(move |_| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.busy.replace(true) {
            return;
        }
        let mut candidate = ui.library.borrow().clone();
        let id = candidate.selected.clone();
        if let Err(error) = candidate.update(&id, Config::capture(&ui.shared.params)) {
            ui.finish(Some(error));
            return;
        }
        ui.refresh();
        glib::spawn_future_local(async move {
            let _ = ui.persist(candidate, false).await;
        });
    });
    let weak = Rc::downgrade(&ui);
    ui.delete.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            glib::spawn_future_local(async move {
                ui.delete().await;
            });
        }
    });
    ui.refresh();
    // O root mantém o controlador vivo; o controlador só guarda WeakRef do root.
    root.connect_destroy(move |_| {
        let _keep_alive = &ui;
    });
    root
}
impl Ui {
    fn ids(&self) -> Vec<String> {
        Preset::ALL
            .iter()
            .map(|p| p.key().into())
            .chain(self.library.borrow().custom.iter().map(|p| p.id.clone()))
            .collect()
    }
    fn dirty(&self) -> bool {
        self.library
            .borrow()
            .config()
            .is_ok_and(|c| c != Config::capture(&self.shared.params))
    }
    fn refresh(&self) {
        self.syncing.set(true);
        let library = self.library.borrow();
        let labels: Vec<&str> = Preset::ALL
            .iter()
            .map(|p| p.name())
            .chain(library.custom.iter().map(|p| p.name.as_str()))
            .collect();
        if self.names.n_items() as usize != labels.len()
            || labels
                .iter()
                .enumerate()
                .any(|(i, s)| self.names.string(i as u32).as_deref() != Some(*s))
        {
            self.names.splice(0, self.names.n_items(), &labels);
        }
        let index = self
            .ids()
            .iter()
            .position(|id| *id == library.selected)
            .unwrap_or(1);
        self.selector.set_selected(index as u32);
        let factory = Preset::from_key(&library.selected);
        self.selector.set_subtitle(
            factory
                .map(|p| p.description())
                .unwrap_or("Perfil personalizado"),
        );
        let enabled = !self.busy.get() && !self.read_only;
        self.selector.set_sensitive(enabled);
        self.new.set_sensitive(enabled && library.custom.len() < 32);
        self.save
            .set_sensitive(enabled && factory.is_none() && self.dirty());
        self.rename.set_sensitive(enabled && factory.is_none());
        self.delete.set_sensitive(enabled && factory.is_none());
        self.controls.sensitive(!self.busy.get());
        let text = self.error.borrow().clone().unwrap_or_else(|| {
            if self.busy.get() {
                "Aguarde…".into()
            } else if self.dirty() {
                "Alterações não salvas".into()
            } else if factory.is_some() {
                "Perfil de fábrica · salve uma cópia para personalizar".into()
            } else {
                "Perfil salvo".into()
            }
        });
        self.message.set_label(&text);
        self.syncing.set(false);
    }
    fn finish(&self, error: Option<String>) {
        self.busy.set(false);
        *self.error.borrow_mut() = error;
        self.refresh();
    }
    async fn persist(&self, candidate: Library, apply: bool) -> Result<(), String> {
        let path = settings::path();
        let saved = candidate.clone();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(settings::save_profiles_to(&path, &saved));
        });
        let result = rx.recv().await.map_err(|e| e.to_string()).and_then(|r| r);
        match result {
            Ok(()) => {
                if apply {
                    self.syncing.set(true);
                    let config = candidate.config().expect("biblioteca validada");
                    config.apply(&self.shared.params);
                    self.controls.sync(&config);
                    self.syncing.set(false);
                }
                *self.library.borrow_mut() = candidate;
                self.finish(None);
                Ok(())
            }
            Err(error) => {
                let error = format!("Não foi possível salvar: {error}");
                self.finish(Some(error.clone()));
                Err(error)
            }
        }
    }
    async fn select(self: Rc<Self>, target: String) {
        if self.busy.replace(true) {
            return;
        }
        let Some(parent) = self.root.upgrade() else {
            self.finish(None);
            return;
        };
        let dirty = self.dirty();
        self.refresh();
        if dirty && !confirm(&parent, "Descartar alterações não salvas?", "O perfil salvo será preservado. Os ajustes atuais serão descartados ao trocar de perfil.", "Descartar").await {
            self.finish(None); return;
        }
        let mut candidate = self.library.borrow().clone();
        if let Err(error) = candidate.select(&target) {
            self.finish(Some(error));
            return;
        }
        let _ = self.persist(candidate, true).await;
    }
    async fn delete(self: Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        let Some(parent) = self.root.upgrade() else {
            self.finish(None);
            return;
        };
        let mut candidate = self.library.borrow().clone();
        let id = candidate.selected.clone();
        self.refresh();
        if !confirm(
            &parent,
            "Excluir perfil personalizado?",
            "O perfil e seus ajustes não salvos serão removidos. O perfil Chamada será ativado.",
            "Excluir",
        )
        .await
        {
            self.finish(None);
            return;
        }
        if let Err(error) = candidate.delete(&id) {
            self.finish(Some(error));
            return;
        }
        let _ = self.persist(candidate, true).await;
    }
    fn name_dialog(self: Rc<Self>, rename: bool) {
        if self.busy.replace(true) {
            return;
        }
        let Some(parent) = self.root.upgrade() else {
            self.finish(None);
            return;
        };
        self.refresh();
        let dialog = adw::Dialog::builder()
            .title(if rename {
                "Renomear perfil"
            } else {
                "Salvar como novo perfil"
            })
            .content_width(420)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        let entry = gtk::Entry::builder()
            .name("profile-name")
            .placeholder_text("Nome do perfil")
            .max_length(60)
            .activates_default(true)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label("Nome do perfil")]);
        if rename {
            if let Some(p) = self
                .library
                .borrow()
                .custom
                .iter()
                .find(|p| p.id == self.library.borrow().selected)
            {
                entry.set_text(&p.name);
            }
        }
        let error = gtk::Label::builder().wrap(true).xalign(0.0).build();
        let actions = gtk::Box::builder()
            .spacing(12)
            .halign(gtk::Align::End)
            .build();
        let cancel = gtk::Button::with_label("Cancelar");
        let save = gtk::Button::builder()
            .name("profile-confirm")
            .label("Salvar")
            .css_classes(["suggested-action"])
            .build();
        actions.append(&cancel);
        actions.append(&save);
        content.append(&entry);
        content.append(&error);
        content.append(&actions);
        dialog.set_child(Some(&content));
        dialog.set_default_widget(Some(&save));
        let validate = {
            let ui = Rc::downgrade(&self);
            let save = save.downgrade();
            let error = error.downgrade();
            move |entry: &gtk::Entry| {
                let Some(ui) = ui.upgrade() else { return };
                let library = ui.library.borrow();
                let result =
                    library.name(&entry.text(), rename.then_some(library.selected.as_str()));
                if let Some(save) = save.upgrade() {
                    save.set_sensitive(result.is_ok());
                }
                if let Some(error) = error.upgrade() {
                    error.set_label(&result.err().unwrap_or_default());
                }
            }
        };
        validate(&entry);
        entry.connect_changed(validate);
        let weak = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
        });
        let weak = Rc::downgrade(&self);
        dialog.connect_closed(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.finish(None);
            }
        });
        let ui = self.clone();
        let weak_dialog = dialog.downgrade();
        let weak_entry = entry.downgrade();
        let weak_error = error.downgrade();
        save.connect_clicked(move |_| {
            let (Some(dialog), Some(entry), Some(error)) = (
                weak_dialog.upgrade(),
                weak_entry.upgrade(),
                weak_error.upgrade(),
            ) else {
                return;
            };
            let mut candidate = ui.library.borrow().clone();
            let id = candidate.selected.clone();
            let result = if rename {
                candidate.rename(&id, &entry.text())
            } else {
                candidate.create(&entry.text(), Config::capture(&ui.shared.params))
            };
            if let Err(e) = result {
                error.set_label(&e);
                return;
            }
            ui.busy.set(true);
            ui.refresh();
            dialog.set_can_close(false);
            dialog.set_sensitive(false);
            let ui = ui.clone();
            glib::spawn_future_local(async move {
                let result = ui.persist(candidate, false).await;
                dialog.set_sensitive(true);
                dialog.set_can_close(true);
                match result {
                    Ok(()) => {
                        dialog.close();
                    }
                    Err(e) => {
                        error.set_label(&e);
                    }
                }
            });
        });
        dialog.present(Some(&parent));
        entry.grab_focus();
    }
}
async fn confirm(parent: &gtk::Box, heading: &str, body: &str, action: &str) -> bool {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_responses(&[("cancel", "Cancelar"), ("confirm", action)]);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.choose_future(Some(parent)).await == "confirm"
}
