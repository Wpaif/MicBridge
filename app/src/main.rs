mod dsp;
mod output;
mod server;
mod tray;

use adw::prelude::*;
use std::net::UdpSocket;
use std::sync::{Arc, atomic::Ordering::Relaxed};
use std::time::Duration;

const APP_ID: &str = "dev.wilian.MicBridge";
const CORE_PORT: u16 = 7878;
const WEB_PORT: u16 = 3443;

/// IP da LAN (a conexão UDP não envia nada, só descobre a rota).
pub fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("1.1.1.1:80").and_then(|_| s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();
    let shared = Arc::new(server::Shared {
        params: dsp::Params::default(),
        connected: Default::default(),
    });

    // Runtime tokio (WebRTC + HTTP) em thread própria; GTK fica na principal.
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let s = shared.clone();
    let handle = rt.handle().clone();
    let _virtual_mic = {
        let _g = handle.enter();
        output::spawn_virtual_mic()
            .map_err(|e| tracing::warn!("{e:#}"))
            .ok()
    };
    let _web = {
        let _g = handle.enter();
        output::spawn_web().map_err(|e| tracing::warn!("{e:#}")).ok()
    };
    rt.spawn(async move {
        if let Err(e) = server::serve(s, CORE_PORT).await {
            tracing::error!("servidor: {e:#}");
        }
    });

    let (tray_tx, tray_rx) = async_channel::unbounded();
    // SIGTERM/SIGINT encerram de forma limpa (os filhos morrem via drop).
    let sig_tx = tray_tx.clone();
    rt.spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int)) =
            (signal(SignalKind::terminate()), signal(SignalKind::interrupt()))
        else {
            return;
        };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        let _ = sig_tx.send(tray::TrayEvent::Quit).await;
    });
    rt.spawn(tray::spawn(tray_tx, shared.clone()));

    let app = adw::Application::builder().application_id(APP_ID).build();
    // Mantém o app vivo com a janela escondida (fechar = minimizar para a bandeja).
    std::mem::forget(app.hold());
    {
        let app = app.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = tray_rx.recv().await {
                match ev {
                    tray::TrayEvent::Show => {
                        if let Some(w) = app.active_window().or_else(|| app.windows().into_iter().next()) {
                            w.present();
                        }
                    }
                    tray::TrayEvent::Quit => app.quit(),
                }
            }
        });
    }
    app.connect_activate(move |app| {
        // Segunda execução / clique no atalho: só reexibe a janela.
        if let Some(w) = app.windows().into_iter().next() {
            w.present();
        } else {
            build_ui(app, shared.clone());
        }
    });
    let code = app.run();
    drop(rt);
    code
}

fn scale_row(
    title: &str,
    range: (f64, f64),
    value: f64,
    on_change: impl Fn(f32) + 'static,
) -> adw::ActionRow {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, range.0, range.1, 1.0);
    scale.set_value(value);
    scale.set_width_request(200);
    scale.set_draw_value(true);
    scale.set_valign(gtk::Align::Center);
    scale.connect_value_changed(move |s| on_change(s.value() as f32));
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&scale);
    row
}

fn switch_row(title: &str, active: bool, on_change: impl Fn(bool) + 'static) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder().title(title).active(active).build();
    row.connect_active_notify(move |r| on_change(r.is_active()));
    row
}

fn meter(title: &str) -> (adw::ActionRow, gtk::LevelBar) {
    let bar = gtk::LevelBar::for_interval(0.0, 1.0);
    bar.set_width_request(200);
    bar.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&bar);
    (row, bar)
}

/// Diálogo com o QR code da URL (desenhado via cairo, sem dependência de imagem).
fn show_qr(parent: &adw::ApplicationWindow, url: &str) {
    use qrcode::{Color, QrCode};
    let Ok(code) = QrCode::new(url.as_bytes()) else { return };
    let n = code.width();
    let colors = code.into_colors();
    let area = gtk::DrawingArea::builder().content_width(300).content_height(300).build();
    area.set_draw_func(move |_, cr, w, h| {
        let quiet = 2.0; // margem (módulos)
        let cell = (w.min(h) as f64) / (n as f64 + 2.0 * quiet);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();
        cr.set_source_rgb(0.0, 0.0, 0.0);
        for (i, c) in colors.iter().enumerate() {
            if *c == Color::Dark {
                let (x, y) = ((i % n) as f64, (i / n) as f64);
                cr.rectangle((x + quiet) * cell, (y + quiet) * cell, cell, cell);
            }
        }
        let _ = cr.fill();
    });
    let dialog = adw::Dialog::builder().title("Abrir no celular").child(&area).build();
    dialog.present(Some(parent));
}

fn build_ui(app: &adw::Application, shared: Arc<server::Shared>) {
    let p = |f: fn(&dsp::Params) -> &std::sync::atomic::AtomicBool| {
        let sh = shared.clone();
        move |v: bool| f(&sh.params).store(v, Relaxed)
    };

    // Conexão
    let conn = adw::PreferencesGroup::builder().title("Conexão").build();
    let url = format!("https://{}:{}", local_ip(), WEB_PORT);
    let url_row = adw::ActionRow::builder()
        .title("Abra no celular (mesma rede)")
        .subtitle(&url)
        .subtitle_selectable(true)
        .build();
    let status = gtk::Label::new(Some("Aguardando celular"));
    status.add_css_class("dim-label");
    url_row.add_suffix(&status);
    let qr_btn = gtk::Button::builder()
        .icon_name("view-barcode-symbolic")
        .tooltip_text("Mostrar QR code")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    url_row.add_suffix(&qr_btn);
    conn.add(&url_row);
    let (in_row, in_bar) = meter("Entrada");
    let (out_row, out_bar) = meter("Saída tratada");
    conn.add(&in_row);
    conn.add(&out_row);

    // Tratamento
    let fx = adw::PreferencesGroup::builder().title("Tratamento de áudio").build();
    fx.add(&switch_row("Cancelamento de ruído (RNNoise)", true, p(|x| &x.denoise)));
    fx.add(&switch_row("Noise gate", true, p(|x| &x.gate)));
    let sh = shared.clone();
    fx.add(&scale_row("Limiar do gate (dB)", (-80.0, -20.0), -55.0, move |v| {
        sh.params.set_gate_db(v)
    }));
    fx.add(&switch_row("Compressor", true, p(|x| &x.compressor)));
    let sh = shared.clone();
    fx.add(&scale_row("Limiar do compressor (dB)", (-50.0, 0.0), -20.0, move |v| {
        sh.params.set_comp_threshold_db(v)
    }));
    let sh = shared.clone();
    fx.add(&scale_row("Razão", (1.0, 10.0), 3.0, move |v| sh.params.set_comp_ratio(v)));
    let sh = shared.clone();
    fx.add(&scale_row("Ganho final (dB)", (-12.0, 18.0), 4.0, move |v| {
        sh.params.set_makeup_db(v)
    }));

    let page = adw::PreferencesPage::new();
    page.add(&conn);
    page.add(&fx);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&page));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("MicBridge")
        .default_width(520)
        .default_height(680)
        .content(&view)
        .hide_on_close(true)
        .build();

    let (qr_win, qr_url) = (win.clone(), url.clone());
    qr_btn.connect_clicked(move |_| show_qr(&qr_win, &qr_url));

    // Atualiza medidores e status (20 Hz).
    glib::timeout_add_local(Duration::from_millis(50), move || {
        // Escala dB -> 0..1 (-60..0 dB).
        let norm = |rms: f32| (((20.0 * (rms + 1e-9).log10()) + 60.0) / 60.0).clamp(0.0, 1.0) as f64;
        in_bar.set_value(norm(shared.params.in_level()));
        out_bar.set_value(norm(shared.params.out_level()));
        status.set_text(if shared.connected.load(Relaxed) {
            "Conectado"
        } else {
            "Aguardando celular"
        });
        glib::ControlFlow::Continue
    });

    gtk::Window::set_default_icon_name(APP_ID);
    win.present();
}
