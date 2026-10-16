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
    pub meters: gtk::Box,
    dot: gtk::Box,
    title: gtk::Label,
    detail: gtk::Label,
    in_bar: gtk::LevelBar,
    out_bar: gtk::LevelBar,
    in_value: gtk::Label,
    out_value: gtk::Label,
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

    let (in_row, in_bar, in_value) = meter("ENTRADA");
    let (out_row, out_bar, out_value) = meter("SAÍDA TRATADA");

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .css_classes(["mb-hero"])
        .build();
    let meters = gtk::Box::builder().spacing(24).homogeneous(true).build();
    in_row.set_hexpand(true);
    out_row.set_hexpand(true);
    meters.append(&in_row);
    meters.append(&out_row);
    root.append(&head);
    root.append(&meters);

    Hero {
        root,
        meters,
        dot,
        title,
        detail,
        in_bar,
        out_bar,
        in_value,
        out_value,
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
        for (label, value) in [(&self.in_value, input), (&self.out_value, output)] {
            label.set_label(&if value <= 0.0 {
                "−∞ dBFS".to_string()
            } else {
                format!("{:.1} dBFS", value * 60.0 - 60.0)
            });
        }
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

fn meter(label: &str) -> (gtk::Box, gtk::LevelBar, gtk::Label) {
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

    let value = gtk::Label::builder()
        .label("−∞ dBFS")
        .xalign(1.0)
        .hexpand(true)
        .css_classes(["mb-meter-value"])
        .build();
    let head = gtk::Box::builder().spacing(12).build();
    head.append(&text);
    head.append(&value);
    let ticks = gtk::DrawingArea::builder().content_height(14).build();
    ticks.set_draw_func(|_, cr, width, _| {
        cr.set_source_rgba(0.85, 0.88, 0.93, 0.55);
        cr.select_font_face(
            "monospace",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Normal,
        );
        cr.set_font_size(10.0);
        for (i, text) in ["−60", "−48", "−36", "−24", "−12", "0 dBFS"]
            .iter()
            .enumerate()
        {
            let extent = cr.text_extents(text).map(|e| e.width()).unwrap_or(0.0);
            let x = (width as f64 * i as f64 / 5.0 - extent / 2.0)
                .clamp(0.0, (width as f64 - extent).max(0.0));
            cr.move_to(x, 11.0);
            let _ = cr.show_text(text);
        }
    });
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    row.append(&head);
    row.append(&bar);
    row.append(&ticks);
    (row, bar, value)
}

/// Controle segmentado (um botão por opção). Chama `on_select` com o índice, inclusive
/// uma vez para a opção inicial.
pub fn segmented(
    labels: &[&str],
    selected: usize,
    on_select: impl Fn(usize) + 'static,
) -> gtk::FlowBox {
    let on_select = Rc::new(on_select);
    let root = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(5)
        .column_spacing(2)
        .row_spacing(2)
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
        root.insert(&button, -1);
        buttons.push(button);
    }
    if let Some(b) = buttons.get(selected) {
        b.set_active(true);
    }
    root
}
