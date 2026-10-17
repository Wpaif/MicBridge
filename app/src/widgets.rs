//! Widgets próprios do MicBridge (estilo em `data/style.css`, regras em `DESIGN.md`).

use crate::state::Link;
use adw::prelude::*;

pub fn slider(min: f64, max: f64, step: f64) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    // Desativa só o tratamento da roda no GtkRange. O evento continua até o
    // ScrolledWindow, e os controladores de arraste/teclado permanecem intactos.
    let controllers = scale.observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(scroll) = controllers
            .item(index)
            .and_then(|c| c.downcast::<gtk::EventControllerScroll>().ok())
        {
            scroll.set_propagation_phase(gtk::PropagationPhase::None);
        }
    }
    scale
}

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
    #[ignore = "requer GDK_BACKEND=x11, sessão gráfica e libXtst para entrada real"]
    fn slider_wheel_scrolls_parent_without_changing_value_and_keeps_keyboard_and_drag() {
        gtk::init().unwrap();
        assert!(
            gtk::gdk::Display::default()
                .unwrap()
                .type_()
                .name()
                .contains("X11"),
            "execute com GDK_BACKEND=x11"
        );
        let title = format!("MicBridge Slider Scroll Test {}", std::process::id());
        let scale = slider(0.0, 100.0, 1.0);
        scale.set_value(50.0);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_start(24)
            .margin_end(24)
            .margin_top(24)
            .build();
        content.append(&scale);
        content.append(&gtk::Box::builder().height_request(1400).build());
        let scroll = gtk::ScrolledWindow::builder().child(&content).build();
        let win = gtk::Window::builder()
            .title(&title)
            .default_width(480)
            .default_height(320)
            .child(&scroll)
            .build();
        win.present();
        pump();
        let xid = unsafe {
            // Símbolo opcional apenas neste teste X11; não adiciona dependências
            // de link X11/XTest à aplicação nem aos testes comuns.
            let library = libc::dlopen(c"libgtk-4.so.1".as_ptr(), libc::RTLD_LAZY);
            assert!(!library.is_null());
            let symbol = libc::dlsym(library, c"gdk_x11_surface_get_xid".as_ptr());
            assert!(!symbol.is_null());
            let get_xid: unsafe extern "C" fn(*mut gtk::gdk::ffi::GdkSurface) -> libc::c_ulong =
                std::mem::transmute(symbol);
            let id = get_xid(win.surface().unwrap().as_ptr());
            libc::dlclose(library);
            id
        };
        assert!(scale.grab_focus());
        let send = |action: &str| {
            let bounds = scale.compute_bounds(&win).unwrap();
            let mut process = std::process::Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/gtk_pointer_input.py"
                ))
                .arg(xid.to_string())
                .arg(((bounds.x() + bounds.width() / 2.0) as i32).to_string())
                .arg(((bounds.y() + bounds.height() / 2.0) as i32).to_string())
                .arg(action)
                .arg((bounds.width() as i32 / 5).to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while process.try_wait().unwrap().is_none() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                if std::time::Instant::now() >= deadline {
                    let _ = process.kill();
                    let _ = process.wait();
                    panic!("tempo esgotado ao enviar entrada X11");
                }
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
            let result = process.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            pump();
        };
        send("wheel");
        assert_eq!(
            scale.value(),
            50.0,
            "roda não deve alterar o ajuste mesmo com foco"
        );
        assert!(
            scroll.vadjustment().value() > 0.0,
            "a roda deve continuar rolando o painel"
        );
        scroll.vadjustment().set_value(0.0);
        pump();
        scale.grab_focus();
        send("key");
        assert_eq!(
            scale.value(),
            51.0,
            "setas devem continuar ajustando o valor"
        );
        send("drag");
        assert!(
            scale.value() > 60.0,
            "arraste deve continuar ajustando o valor"
        );
        win.destroy();
    }
    fn pump() {
        let end = std::time::Instant::now() + std::time::Duration::from_millis(180);
        while std::time::Instant::now() < end {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
    }
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
