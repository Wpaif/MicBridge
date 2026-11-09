//! Seção "Fonte extra" da janela GTK: escolhe um app (ex.: aba do navegador) cujo som
//! vai junto com a voz para o microfone virtual.
use crate::state::Shared;
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const OFF: &str = "Desligada";

pub fn group(shared: Arc<Shared>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Fonte extra")
        .description("Som de um app (ex.: aba do navegador) junto com a voz na call. Você continua ouvindo no fone.")
        .build();

    let list = gtk::StringList::new(&[OFF]);
    let combo = adw::ComboRow::builder().title("App").model(&list).build();
    // Ids na mesma ordem da lista; a posição 0 é "Desligada".
    let ids: Rc<RefCell<Vec<Option<u32>>>> = Rc::new(RefCell::new(vec![None]));
    // Evita reenviar a escolha quando é o próprio código que atualiza o combo.
    let syncing = Rc::new(Cell::new(false));

    let (sh, ids_c, sync_c) = (shared.clone(), ids.clone(), syncing.clone());
    combo.connect_selected_notify(move |c| {
        if sync_c.get() {
            return;
        }
        let id = ids_c.borrow().get(c.selected() as usize).copied().flatten();
        if let Err(e) = sh.extra.select(id) {
            tracing::warn!("fonte extra: {e}");
        }
    });

    let sh = shared.clone();
    let (volume_row, volume) = crate::scale_row(
        "Volume (%)",
        (0.0, 100.0),
        1.0,
        f64::from(shared.extra.snapshot().volume),
        move |v| sh.extra.set_volume(v),
    );

    group.add(&combo);
    group.add(&volume_row);

    let mut last: Vec<(Option<u32>, String)> = Vec::new();
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let snap = shared.extra.snapshot();
        let entries: Vec<(Option<u32>, String)> = std::iter::once((None, OFF.to_owned()))
            .chain(snap.streams.iter().map(|s| {
                let mut label = s.label();
                if s.call {
                    label.push_str("  ⚠ app de chamada");
                }
                (Some(s.id), label)
            }))
            .collect();
        syncing.set(true);
        if entries != last {
            let labels: Vec<&str> = entries.iter().map(|e| e.1.as_str()).collect();
            list.splice(0, list.n_items(), &labels);
            *ids.borrow_mut() = entries.iter().map(|e| e.0).collect();
            last = entries;
        }
        let pos = ids
            .borrow()
            .iter()
            .position(|id| *id == snap.selected && snap.selected.is_some())
            .unwrap_or(0) as u32;
        if combo.selected() != pos {
            combo.set_selected(pos);
        }
        syncing.set(false);

        let call = snap
            .streams
            .iter()
            .any(|s| Some(s.id) == snap.selected && s.call);
        combo.set_subtitle(&if let Some(e) = &snap.error {
            e.clone()
        } else if call {
            "Cuidado: isso devolve o som da call para ela mesma (eco)".into()
        } else if snap.linked {
            "No microfone virtual".into()
        } else if snap.streams.is_empty() {
            "Nenhum app tocando som agora".into()
        } else {
            String::new()
        });
        volume_row.set_sensitive(snap.selected.is_some());
        if (volume.value() as f32 - snap.volume).abs() >= 1.0 {
            volume.set_value(f64::from(snap.volume));
        }
        glib::ControlFlow::Continue
    });
    group
}
