//! Aparência é uma transação: prévia até Aplicar, fechamento restaura o salvo.
use crate::theme::{Appearance, Palette, Source, ThemeController};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const SOURCES: [Source; 4] = [Source::System, Source::Light, Source::Dark, Source::Custom];
pub fn window(
    parent: &adw::ApplicationWindow,
    theme: Rc<ThemeController>,
) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::builder()
        .title("Aparência")
        .content_width(560)
        .content_height(640)
        .search_enabled(false)
        .build();
    let page = adw::PreferencesPage::new();
    dialog.add(&page);
    let group = adw::PreferencesGroup::builder()
        .title("Tema e integração")
        .description("Veja as alterações na janela e aplique para salvar.")
        .build();
    page.add(&group);
    let model = gtk::StringList::new(&["Seguir o sistema", "Claro", "Escuro", "Personalizado"]);
    let source = adw::ComboRow::builder()
        .title("Tema")
        .subtitle("Acompanha a aparência do seu ambiente")
        .model(&model)
        .build();
    group.add(&source);
    let scale = crate::widgets::slider(60.0, 100.0, 1.0);
    scale.set_digits(0);
    scale.set_width_request(160);
    scale.set_valign(gtk::Align::Center);
    scale.set_tooltip_text(Some("100% é opaco; apenas os fundos ficam translúcidos."));
    let opacity = adw::ActionRow::builder()
        .title("Opacidade do fundo")
        .subtitle("Textos e medidores permanecem opacos")
        .build();
    opacity.add_suffix(&scale);
    group.add(&opacity);
    let note=gtk::Label::builder().label("Desfoque depende do compositor. Alto contraste preserva as cores do sistema e o fundo opaco.").xalign(0.0).wrap(true).css_classes(["caption","dim-label"]).build();
    group.add(&note);
    let sample = gtk::DrawingArea::builder()
        .content_height(30)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let controller = theme.clone();
    sample.set_draw_func(move |_, cr, w, h| {
        let p = controller.effective_palette();
        for (i, c) in [&p.background, &p.surface, &p.foreground, &p.accent]
            .iter()
            .enumerate()
        {
            if let Ok([r, g, b]) = crate::theme::model::rgb(c) {
                cr.set_source_rgb(r, g, b);
                cr.rectangle(i as f64 * w as f64 / 4.0, 0.0, w as f64 / 4.0, h as f64);
                let _ = cr.fill();
            }
        }
    });
    group.add(&sample);
    let files = adw::PreferencesGroup::builder()
        .title("Temas personalizados")
        .description(
            "Importe ou exporte uma paleta MicBridge (.json). Cores e contraste são validados.",
        )
        .build();
    page.add(&files);
    let file_buttons = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(2)
        .column_spacing(6)
        .row_spacing(6)
        .homogeneous(true)
        .build();
    let import = gtk::Button::with_label("Importar tema…");
    let export = gtk::Button::with_label("Exportar tema…");
    file_buttons.insert(&import, -1);
    file_buttons.insert(&export, -1);
    files.add(&file_buttons);
    let restore = gtk::Button::with_label("Restaurar padrão");
    restore.set_halign(gtk::Align::Start);
    restore.set_margin_top(12);
    files.add(&restore);
    let message = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .margin_top(12)
        .build();
    files.add(&message);
    let footer = adw::PreferencesGroup::new();
    page.add(&footer);
    let actions = gtk::Box::builder()
        .spacing(12)
        .halign(gtk::Align::End)
        .build();
    let cancel = gtk::Button::with_label("Cancelar");
    let apply = gtk::Button::with_label("Aplicar");
    apply.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&apply);
    footer.add(&actions);
    let updating = Rc::new(Cell::new(false));
    let applied = Rc::new(Cell::new(false));
    let busy = Rc::new(Cell::new(false));
    let error = Rc::new(RefCell::new(None::<String>));
    let sync = {
        let row = source.downgrade();
        let scale = scale.downgrade();
        let message = message.downgrade();
        let sample = sample.downgrade();
        let c = theme.clone();
        let guard = updating.clone();
        let error = error.clone();
        move || {
            let prefs = c.current();
            guard.set(true);
            if let Some(row) = row.upgrade() {
                row.set_selected(
                    SOURCES.iter().position(|s| *s == prefs.source).unwrap_or(0) as u32
                );
            }
            if let Some(scale) = scale.upgrade() {
                scale.set_value(prefs.opacity as f64);
                scale.set_sensitive(!adw::StyleManager::default().is_high_contrast());
            }
            if let Some(message) = message.upgrade() {
                message.set_label(&error.borrow().clone().unwrap_or_else(|| c.status()));
            }
            if let Some(sample) = sample.upgrade() {
                sample.queue_draw();
            }
            guard.set(false);
        }
    };
    let c = theme.clone();
    let guard = updating.clone();
    let e = error.clone();
    let refresh = sync.clone();
    source.connect_selected_notify(move |row| {
        if guard.get() {
            return;
        }
        let Some(source) = SOURCES.get(row.selected() as usize) else {
            return;
        };
        let mut p = c.current();
        p.source = *source;
        *e.borrow_mut() = c.preview(p).err();
        refresh();
    });
    let c = theme.clone();
    let guard = updating.clone();
    let e = error.clone();
    let refresh = sync.clone();
    scale.connect_value_changed(move |scale| {
        if guard.get() {
            return;
        }
        let mut p = c.current();
        p.opacity = scale.value().round() as u8;
        *e.borrow_mut() = c.preview(p).err();
        refresh();
    });
    let c = theme.clone();
    let e = error.clone();
    let refresh = sync.clone();
    restore.connect_clicked(move |_| {
        *e.borrow_mut() = c.preview(Appearance::default()).err();
        refresh();
    });
    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(d) = weak.upgrade() {
            d.close();
        }
    });
    let c = theme.clone();
    let committed = applied.clone();
    dialog.connect_closed(move |_| {
        if !committed.get() {
            c.cancel();
        }
    });
    let weak = dialog.downgrade();
    let c = theme.clone();
    let b = busy.clone();
    let committed = applied.clone();
    let e = error.clone();
    let refresh = sync.clone();
    apply.connect_clicked(move |_| {
        if b.replace(true) {
            return;
        }
        let Some(d) = weak.upgrade() else {
            return;
        };
        d.set_can_close(false);
        d.set_sensitive(false);
        let c = c.clone();
        let b = b.clone();
        let committed = committed.clone();
        let e = e.clone();
        let refresh = refresh.clone();
        glib::spawn_future_local(async move {
            let result = c.commit().await;
            b.set(false);
            d.set_sensitive(true);
            d.set_can_close(true);
            match result {
                Ok(()) => {
                    committed.set(true);
                    d.close();
                }
                Err(error) => {
                    *e.borrow_mut() = Some(format!("Não foi possível salvar: {error}"));
                    refresh();
                }
            }
        });
    });
    for (button, saving) in [(import, false), (export, true)] {
        let parent = parent.downgrade();
        let weak = dialog.downgrade();
        let c = theme.clone();
        let b = busy.clone();
        let e = error.clone();
        let refresh = sync.clone();
        button.connect_clicked(move |_| {
            if b.replace(true) {
                return;
            }
            let (Some(parent), Some(d)) = (parent.upgrade(), weak.upgrade()) else {
                return;
            };
            d.set_can_close(false);
            d.set_sensitive(false);
            let c = c.clone();
            let b = b.clone();
            let e = e.clone();
            let refresh = refresh.clone();
            glib::spawn_future_local(async move {
                let result = if saving {
                    export_theme(&parent, &c).await
                } else {
                    import_theme(&parent, &c).await
                };
                b.set(false);
                d.set_sensitive(true);
                d.set_can_close(true);
                *e.borrow_mut() = result.err();
                refresh();
            });
        });
    }
    let weak = dialog.downgrade();
    let refresh = sync.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        if weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        refresh();
        glib::ControlFlow::Continue
    });
    sync();
    dialog
}
async fn import_theme(
    parent: &adw::ApplicationWindow,
    c: &Rc<ThemeController>,
) -> Result<(), String> {
    let filter = gtk::FileFilter::new();
    filter.add_pattern("*.json");
    filter.set_name(Some("Paletas MicBridge (.json)"));
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let chooser = gtk::FileDialog::builder()
        .title("Importar tema MicBridge")
        .filters(&filters)
        .default_filter(&filter)
        .build();
    let file = match chooser.open_future(Some(parent)).await {
        Ok(f) => f,
        Err(e) if dismissed(&e) => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let path = file.path().ok_or("Escolha um arquivo local de tema")?;
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ =
            tx.send_blocking(crate::theme::read_limited(&path).and_then(|s| Palette::parse(&s)));
    });
    let palette = rx.recv().await.map_err(|e| e.to_string())??;
    let mut prefs = c.current();
    prefs.custom = Some(palette);
    prefs.source = Source::Custom;
    c.preview(prefs)
}
async fn export_theme(
    parent: &adw::ApplicationWindow,
    c: &Rc<ThemeController>,
) -> Result<(), String> {
    let palette = c.effective_palette();
    palette.validate()?;
    let bytes = serde_json::to_vec_pretty(&palette).map_err(|e| e.to_string())?;
    let chooser = gtk::FileDialog::builder()
        .title("Exportar tema MicBridge")
        .initial_name("micbridge-theme.json")
        .build();
    let file = match chooser.save_future(Some(parent)).await {
        Ok(f) => f,
        Err(e) if dismissed(&e) => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    file.replace_contents_future(
        bytes,
        None,
        false,
        gtk::gio::FileCreateFlags::REPLACE_DESTINATION,
    )
    .await
    .map_err(|(_, e)| e.to_string())?;
    Ok(())
}
fn dismissed(e: &glib::Error) -> bool {
    e.matches(gtk::DialogError::Dismissed) || e.matches(gtk::DialogError::Cancelled)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Appearance, Source};
    use std::{
        cell::Cell,
        time::{Duration, Instant},
    };
    fn find<T: IsA<gtk::Widget> + Clone>(
        w: &impl IsA<gtk::Widget>,
        yes: &impl Fn(&T) -> bool,
    ) -> Option<T> {
        let w = w.as_ref();
        if let Some(t) = w.downcast_ref::<T>() {
            if yes(t) {
                return Some(t.clone());
            }
        }
        let mut ch = w.first_child();
        while let Some(w) = ch {
            if let Some(t) = find(&w, yes) {
                return Some(t);
            }
            ch = w.next_sibling();
        }
        None
    }
    fn until(mut yes: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !yes() {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            assert!(Instant::now() < end, "GTK timeout");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn appearance_dialog_previews_cancels_restores_and_persists() {
        adw::init().unwrap();
        let f = crate::audio_test::Fixture::new("exit 0");
        let path = f.dir.join("settings.json");
        let controller = ThemeController::new(Appearance::default(), path.clone(), vec![]);
        let parent = adw::ApplicationWindow::builder()
            .default_width(700)
            .default_height(700)
            .build();
        parent.add_css_class("mb-window");
        parent.set_content(Some(&gtk::Label::new(Some("Prévia da aparência"))));
        parent.present();
        let dialog = window(&parent, controller.clone());
        dialog.present(Some(&parent));
        let source = find::<adw::ComboRow>(&dialog, &|_| true).expect("theme source control");
        let opacity = find::<gtk::Scale>(&dialog, &|_| true).unwrap();
        let choices = source.model().unwrap();
        let labels: Vec<_> = (0..choices.n_items())
            .map(|i| {
                choices
                    .item(i)
                    .unwrap()
                    .downcast::<gtk::StringObject>()
                    .unwrap()
                    .string()
                    .to_string()
            })
            .collect();
        assert_eq!(
            labels,
            vec!["Seguir o sistema", "Claro", "Escuro", "Personalizado"]
        );
        source.set_selected(2);
        opacity.set_value(85.0);
        assert_eq!(controller.current().source, Source::Dark);
        assert_eq!(controller.current().opacity, 85);
        assert!(adw::StyleManager::default().is_dark());
        let closed = Rc::new(Cell::new(false));
        let c = closed.clone();
        dialog.connect_closed(move |_| c.set(true));
        find::<gtk::Button>(&dialog, &|b| b.label().as_deref() == Some("Cancelar"))
            .unwrap()
            .emit_clicked();
        until(|| closed.get());
        assert_eq!(controller.current(), Appearance::default());
        assert!(!path.exists());
        let dialog = window(&parent, controller.clone());
        dialog.present(Some(&parent));
        find::<adw::ComboRow>(&dialog, &|_| true)
            .unwrap()
            .set_selected(1);
        find::<gtk::Scale>(&dialog, &|_| true)
            .unwrap()
            .set_value(80.0);
        let closed = Rc::new(Cell::new(false));
        let c = closed.clone();
        dialog.connect_closed(move |_| c.set(true));
        find::<gtk::Button>(&dialog, &|b| b.label().as_deref() == Some("Aplicar"))
            .unwrap()
            .emit_clicked();
        until(|| closed.get());
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["appearance"]["source"], "light");
        assert_eq!(saved["appearance"]["opacity"], 80);
        let dialog = window(&parent, controller.clone());
        dialog.present(Some(&parent));
        find::<gtk::Button>(&dialog, &|b| {
            b.label().as_deref() == Some("Restaurar padrão")
        })
        .unwrap()
        .emit_clicked();
        assert_eq!(controller.current(), Appearance::default());
        let closed = Rc::new(Cell::new(false));
        let c = closed.clone();
        dialog.connect_closed(move |_| c.set(true));
        dialog.close();
        until(|| closed.get());
        assert_eq!(controller.current().opacity, 80);
        parent.destroy();
    }
}
