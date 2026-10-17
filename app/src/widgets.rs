//! Widgets próprios do MicBridge (estilo em `data/style.css`, regras em `DESIGN.md`).

use crate::state::Link;
use adw::prelude::*;

/// Acima disso o medidor fica amarelo, e depois vermelho (perto de estourar).
const METER_HOT: f64 = 0.70;
const METER_CLIP: f64 = 0.90;

/// Card de status em destaque: bolinha, estado, detalhe e medidores de entrada/saída.
pub struct Hero {
    pub root: gtk::Box,
    pub controls: gtk::Box,
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
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.0)
        .css_classes(["mb-hero-title"])
        .build();
    let detail = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .lines(1)
        .ellipsize(gtk::pango::EllipsizeMode::End)
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

    let (in_row, in_bar, in_value) = meter("Entrada");
    let (out_row, out_bar, out_value) = meter("Saída tratada");

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
        controls: head,
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
        self.detail.set_tooltip_text(Some(detail));
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
    let ticks = gtk::Grid::builder().column_homogeneous(true).build();
    for (i, text) in ["−60", "−48", "−36", "−24", "−12", "0 dBFS"]
        .iter()
        .enumerate()
    {
        // Interior labels are centered at 2/10,4/10,6/10,8/10 of the bar.
        // End labels share two columns too, so narrow windows can still shrink.
        ticks.attach(
            &gtk::Label::builder()
                .label(*text)
                .xalign(if i == 0 {
                    0.0
                } else if i == 5 {
                    1.0
                } else {
                    0.5
                })
                .hexpand(true)
                .css_classes(["mb-meter-tick"])
                .build(),
            if i == 0 {
                0
            } else if i == 5 {
                8
            } else {
                i as i32 * 2 - 1
            },
            0,
            2,
            1,
        );
    }
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    row.append(&head);
    row.append(&bar);
    row.append(&ticks);
    (row, bar, value)
}

#[cfg(test)]
mod meter_tests {
    use super::*;
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn meter_ticks_match_the_linear_dbfs_scale_at_each_width() {
        adw::init().unwrap();
        let (row, bar, _) = meter("Entrada");
        let win = gtk::Window::builder()
            .default_width(500)
            .default_height(120)
            .child(&row)
            .build();
        win.present();
        fn until(mut yes: impl FnMut() -> bool) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !yes() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
        }
        for width in [500, 300] {
            win.set_default_size(width, 120);
            until(|| win.width() == width && bar.is_mapped());
            let scale = bar.compute_bounds(&win).unwrap();
            let ticks = row.last_child().unwrap();
            let mut child = ticks.first_child();
            let mut i = 0;
            while let Some(label) = child {
                if i > 0 && i < 5 {
                    let rect = label.compute_bounds(&win).unwrap();
                    let center = rect.x() + rect.width() / 2.0;
                    assert!(
                        (center - (scale.x() + scale.width() * i as f32 / 5.0)).abs() <= 1.0,
                        "tick{i} misplaced: {center}"
                    );
                }
                child = label.next_sibling();
                i += 1;
            }
            assert_eq!(i, 6);
        }
        win.destroy();
    }
}
