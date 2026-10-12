//! Widgets próprios do MicBridge (estilo em `data/style.css`, regras em `DESIGN.md`).

use crate::state::Link;
use adw::prelude::*;
use std::rc::Rc;

/// Acima disso o medidor fica amarelo, e depois vermelho (perto de estourar).
const METER_HOT: f64 = 0.70;
const METER_CLIP: f64 = 0.90;

/// Card de status em destaque: bolinha, estado, detalhe e medidores de entrada/saída.
pub struct Hero {
    pub root: gtk::Box,
    dot: gtk::Box,
    title: gtk::Label,
    detail: gtk::Label,
    in_bar: gtk::LevelBar,
    out_bar: gtk::LevelBar,
}

pub fn hero() -> Hero {
    let dot = gtk::Box::builder()
        .valign(gtk::Align::Center)
        .css_classes(["mb-dot", "off"])
        .build();
    let title = gtk::Label::builder()
        .label("Desconectado")
        .xalign(0.0)
        .css_classes(["mb-hero-title"])
        .build();
    let detail = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["mb-hero-detail"])
        .build();

    let texts = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    texts.append(&title);
    texts.append(&detail);

    let head = gtk::Box::builder().spacing(18).build();
    head.append(&dot);
    head.append(&texts);

    let (in_row, in_bar) = meter("ENTRADA");
    let (out_row, out_bar) = meter("SAÍDA TRATADA");

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .css_classes(["mb-hero"])
        .build();
    root.append(&head);
    root.append(&in_row);
    root.append(&out_row);

    Hero {
        root,
        dot,
        title,
        detail,
        in_bar,
        out_bar,
    }
}

impl Hero {
    /// Atualiza texto e cor da bolinha. `speaking` faz a bolinha verde pulsar.
    pub fn set_state(&self, link: Link, label: &str, detail: &str, speaking: bool) {
        self.title.set_label(label);
        self.detail.set_label(detail);
        for c in ["ok", "warn", "err", "off", "speaking"] {
            self.dot.remove_css_class(c);
        }
        self.dot.add_css_class(match link {
            Link::Connected => "ok",
            Link::Connecting | Link::Reconnecting => "warn",
            Link::Error => "err",
            Link::Disconnected => "off",
        });
        if speaking && link == Link::Connected {
            self.dot.add_css_class("speaking");
        }
    }

    /// Níveis normalizados (0..1) de entrada e saída.
    pub fn set_levels(&self, input: f64, output: f64) {
        set_level(&self.in_bar, input);
        set_level(&self.out_bar, output);
    }
}

fn set_level(bar: &gtk::LevelBar, value: f64) {
    bar.set_value(value);
    bar.remove_css_class("hot");
    bar.remove_css_class("clip");
    if value >= METER_CLIP {
        bar.add_css_class("clip");
    } else if value >= METER_HOT {
        bar.add_css_class("hot");
    }
}

fn meter(label: &str) -> (gtk::Box, gtk::LevelBar) {
    let bar = gtk::LevelBar::for_interval(0.0, 1.0);
    // Sem os níveis padrão do GTK: a cor segue as classes `hot`/`clip` acima.
    for name in ["low", "high", "full"] {
        bar.remove_offset_value(Some(name));
    }
    bar.add_css_class("mb-meter");
    bar.set_hexpand(true);
    bar.set_valign(gtk::Align::Center);

    let text = gtk::Label::builder()
        .label(label)
        .xalign(0.0)
        .width_request(112)
        .css_classes(["mb-meter-label"])
        .build();

    let row = gtk::Box::builder().spacing(12).build();
    row.append(&text);
    row.append(&bar);
    (row, bar)
}

/// Controle segmentado (um botão por opção). Chama `on_select` com o índice, inclusive
/// uma vez para a opção inicial.
pub fn segmented(
    labels: &[&str],
    selected: usize,
    on_select: impl Fn(usize) + 'static,
) -> gtk::Box {
    let on_select = Rc::new(on_select);
    let root = gtk::Box::builder()
        .spacing(2)
        .homogeneous(true)
        .css_classes(["mb-segmented"])
        .build();

    let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let button = gtk::ToggleButton::builder()
            .label(*label)
            .hexpand(true)
            .build();
        if let Some(first) = buttons.first() {
            button.set_group(Some(first));
        }
        let cb = on_select.clone();
        button.connect_toggled(move |b| {
            if b.is_active() {
                cb(i);
            }
        });
        root.append(&button);
        buttons.push(button);
    }
    if let Some(b) = buttons.get(selected) {
        b.set_active(true);
    }
    root
}
