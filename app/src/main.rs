mod dsp;
mod output;
mod server;
mod state;
mod theme;
mod tls;
mod tray;
mod tray_icon;
mod web;
mod widgets;

use adw::prelude::*;
use state::Shared;
use std::net::UdpSocket;
use std::sync::{Arc, atomic::Ordering::Relaxed};
use std::time::Duration;

const APP_ID: &str = "dev.wilian.MicBridge";
const WEB_PORT: u16 = 3443;
/// Nome mDNS anunciado via avahi (já consta no certificado).
pub const MDNS_HOST: &str = tls::MDNS_HOST;

/// IP da LAN (a conexão UDP não envia nada, só descobre a rota).
pub fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("1.1.1.1:80").and_then(|_| s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();
    let hidden = std::env::args().any(|a| a == "--hidden");

    // Instância única: registra no D-Bus antes de subir servidores e bandeja.
    // Se já houver uma instância rodando, apenas pede que ela mostre a janela.
    let app = adw::Application::builder().application_id(APP_ID).build();
    if let Err(e) = app.register(None::<&gtk::gio::Cancellable>) {
        tracing::warn!("registro do GApplication: {e}");
    } else if app.is_remote() {
        app.activate();
        return glib::ExitCode::SUCCESS;
    }

    let shared = Arc::new(Shared::new());

    // Runtime tokio (WebRTC + HTTP) em thread própria; GTK fica na principal.
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let s = shared.clone();
    // Supervisor: cria o microfone virtual e o recria se o node do PipeWire sumir.
    rt.spawn(output::supervise_virtual_mic(shared.clone()));
    rt.spawn(output::supervise_mdns());
    rt.spawn(async move {
        let ip = local_ip().parse().ok();
        if let Err(e) = server::serve(s, ip, WEB_PORT).await {
            tracing::error!("servidor: {e:#}");
        }
    });

    let (tray_tx, tray_rx) = async_channel::unbounded();
    // SIGTERM/SIGINT encerram de forma limpa (os filhos morrem via drop).
    let sig_tx = tray_tx.clone();
    rt.spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            return;
        };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        let _ = sig_tx.send(tray::TrayEvent::Quit).await;
    });
    rt.spawn(tray::spawn(tray_tx, shared.clone()));

    // Mantém o app vivo com a janela escondida (fechar = minimizar para a bandeja).
    std::mem::forget(app.hold());
    {
        let app = app.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = tray_rx.recv().await {
                match ev {
                    tray::TrayEvent::Show => {
                        if let Some(w) = app
                            .active_window()
                            .or_else(|| app.windows().into_iter().next())
                        {
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
            build_ui(app, shared.clone(), !hidden);
        }
    });
    // O GApplication não deve interpretar nossas flags (--hidden).
    let code = app.run_with_args::<&str>(&[]);
    drop(rt);
    code
}

fn scale_row(
    title: &str,
    range: (f64, f64),
    step: f64,
    value: f64,
    on_change: impl Fn(f32) + 'static,
) -> (adw::ActionRow, gtk::Scale) {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, range.0, range.1, step);
    // Sem isso o GTK arredonda para inteiro e desfaz valores como razão 2,5.
    let digits = if step < 1.0 { 1 } else { 0 };
    scale.set_digits(digits);
    scale.set_round_digits(digits);
    scale.set_value(value);
    scale.set_width_request(200);
    scale.set_draw_value(true);
    scale.set_valign(gtk::Align::Center);
    scale.connect_value_changed(move |s| on_change(s.value() as f32));
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&scale);
    (row, scale)
}

fn switch_row(title: &str, active: bool, on_change: impl Fn(bool) + 'static) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .active(active)
        .build();
    row.connect_active_notify(move |r| on_change(r.is_active()));
    row
}

fn qr_area(url: &str, size: i32) -> Option<gtk::DrawingArea> {
    use qrcode::{Color, QrCode};

    let code = QrCode::new(url.as_bytes()).ok()?;
    let n = code.width();
    let colors = code.into_colors();

    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .width_request(size)
        .height_request(size)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .hexpand(false)
        .vexpand(false)
        .build();

    area.set_draw_func(move |_, cr, w, h| {
        const QUIET: f64 = 4.0;

        let modules = n as f64 + 2.0 * QUIET;
        let side = w.min(h) as f64;
        let cell = (side / modules).floor();

        if cell < 1.0 {
            return;
        }

        let qr_size = modules * cell;
        let offset_x = (w as f64 - qr_size) / 2.0;
        let offset_y = (h as f64 - qr_size) / 2.0;

        // Fundo branco.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();

        cr.set_antialias(gtk::cairo::Antialias::None);
        cr.set_source_rgb(0.0, 0.0, 0.0);

        for (i, color) in colors.iter().enumerate() {
            if *color == Color::Dark {
                let x = i % n;
                let y = i / n;

                cr.rectangle(
                    offset_x + (x as f64 + QUIET) * cell,
                    offset_y + (y as f64 + QUIET) * cell,
                    cell,
                    cell,
                );
            }
        }

        let _ = cr.fill();
    });

    Some(area)
}

fn show_qr(parent: &adw::ApplicationWindow, url: &str) {
    let Some(area) = qr_area(url, 300) else {
        return;
    };
    let dialog = adw::Dialog::builder()
        .title("Abrir no celular")
        .follows_content_size(true)
        .build();

    let close_btn = gtk::Button::builder()
        .label("Fechar")
        .halign(gtk::Align::Center)
        .build();
    close_btn.add_css_class("pill");
    close_btn.add_css_class("suggested-action");
    let d = dialog.clone();
    close_btn.connect_clicked(move |_| {
        d.close();
    });

    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    body.append(&area);
    body.append(&close_btn);

    // Com HeaderBar o diálogo ganha o botão de fechar (X), além do Esc.
    let view = adw::ToolbarView::new();
    view.set_top_bar_style(adw::ToolbarStyle::Flat);
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&body));
    dialog.set_child(Some(&view));
    dialog.set_default_widget(Some(&close_btn));
    dialog.present(Some(parent));
}

fn autostart_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".config/autostart/dev.wilian.MicBridge.desktop")
}

/// Liga/desliga a inicialização com a sessão (já abre escondido na bandeja).
fn set_autostart(on: bool) {
    let path = autostart_path();
    if on {
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or("micbridge".into());
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=MicBridge\nExec={exe} --hidden\nIcon={APP_ID}\nX-GNOME-Autostart-enabled=true\n"
        );
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, entry);
    } else {
        let _ = std::fs::remove_file(&path);
    }
}

fn build_ui(app: &adw::Application, shared: Arc<Shared>, show: bool) {
    let p = |f: fn(&dsp::Params) -> &std::sync::atomic::AtomicBool| {
        let sh = shared.clone();
        move |v: bool| f(&sh.params).store(v, Relaxed)
    };

    theme::install();

    // Estado da conexão em destaque: conectado, desconectado, reconectando ou erro.
    let state_group = adw::PreferencesGroup::new();
    let hero = widgets::hero();
    state_group.add(&hero.root);

    // Conexão
    let conn = adw::PreferencesGroup::builder().title("Conexão").build();
    // Com o avahi ativo usa o nome estável; senão, o IP (muda com o DHCP).
    let mdns = output::avahi_active();
    let host = if mdns {
        MDNS_HOST.to_string()
    } else {
        local_ip()
    };
    let url = format!("https://{host}:{WEB_PORT}");
    // Link de pareamento: só quem escaneia o QR (ou copia o link) entra.
    let pair_url = format!("{url}/?t={}", shared.pair_token);
    tracing::debug!("link de pareamento: {pair_url}");

    // QR em destaque enquanto a janela de pareamento estiver aberta (primeira utilização).
    let qr_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .halign(gtk::Align::Center)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    if let Some(area) = qr_area(&pair_url, 220) {
        let qr_card = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();

        qr_card.add_css_class("mb-qr");
        qr_card.set_overflow(gtk::Overflow::Hidden);

        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();

        frame.append(&area);
        qr_card.append(&frame);
        qr_box.append(&qr_card);
    }
    qr_box.append(
        &gtk::Label::builder()
            .label(
                "Escaneie com a câmera do celular, na mesma rede.\nSó é preciso na primeira vez.",
            )
            .justify(gtk::Justification::Center)
            .css_classes(["dim-label"])
            .build(),
    );
    let qr_group = adw::PreferencesGroup::new();
    qr_group.add(&qr_box);

    let url_row = adw::ActionRow::builder()
        .title(if mdns {
            "Endereço do servidor (nome fixo)"
        } else {
            "Endereço do servidor (IP, pode mudar)"
        })
        .subtitle(&url)
        .subtitle_selectable(true)
        .build();
    let qr_btn = gtk::Button::builder()
        .icon_name("view-barcode-symbolic")
        .tooltip_text("Mostrar QR code")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let copy_btn = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copiar link de pareamento")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let link = pair_url.clone();
    copy_btn.connect_clicked(move |b| b.clipboard().set_text(&link));
    url_row.add_suffix(&copy_btn);
    url_row.add_suffix(&qr_btn);
    conn.add(&url_row);

    // Janela de pareamento: novos celulares só entram enquanto estiver aberta.
    let pair_row = adw::SwitchRow::builder()
        .title("Aceitar novo celular")
        .active(shared.pairing_remaining() > 0)
        .build();
    {
        let sh = shared.clone();
        pair_row.connect_active_notify(move |r| {
            let open = sh.pairing_remaining() > 0;
            if r.is_active() && !open {
                sh.open_pairing();
            } else if !r.is_active() && open {
                sh.close_pairing();
            }
        });
    }
    conn.add(&pair_row);

    // Celulares pareados (persistem entre execuções).
    let devices_row = adw::ActionRow::builder()
        .title("Celulares pareados")
        .build();
    let forget_btn = gtk::Button::builder()
        .label("Esquecer todos")
        .valign(gtk::Align::Center)
        .css_classes(["destructive-action"])
        .build();
    {
        let sh = shared.clone();
        forget_btn.connect_clicked(move |_| {
            sh.forget_all();
            sh.open_pairing(); // sem celulares, volta a mostrar o QR
        });
    }
    devices_row.add_suffix(&forget_btn);
    conn.add(&devices_row);

    conn.add(&switch_row(
        "Iniciar com o sistema",
        autostart_path().exists(),
        set_autostart,
    ));

    // Impressão digital do certificado: confira no celular ao aceitar o aviso.
    let fp_row = adw::ActionRow::builder()
        .title("Impressão digital do certificado (SHA-256)")
        .subtitle("aguardando o servidor…")
        .subtitle_selectable(true)
        .build();
    conn.add(&fp_row);

    // Tratamento: um preset resolve para a maioria; o resto fica em "Avançado".
    let fx = adw::PreferencesGroup::builder()
        .title("Perfil de voz")
        .build();

    let preset_desc = gtk::Label::builder()
        .xalign(0.0)
        .margin_top(2)
        .margin_bottom(10)
        .margin_start(4)
        .css_classes(["mb-profile-desc"])
        .build();

    let adv = adw::ExpanderRow::builder()
        .title("Ajustes avançados")
        .subtitle("Opcional: só mexa se quiser refinar o som")
        .build();

    let denoise = switch_row("Cancelamento de ruído (RNNoise)", true, p(|x| &x.denoise));
    let highpass = switch_row("Filtro de graves (tira zumbido)", true, p(|x| &x.highpass));
    let gate = switch_row("Noise gate", true, p(|x| &x.gate));
    let sh = shared.clone();
    let (gate_row, gate_scale) = scale_row(
        "Limiar do gate (dB)",
        (-80.0, -20.0),
        1.0,
        -55.0,
        move |v| sh.params.set_gate_db(v),
    );
    let deesser = switch_row("Suavizar chiados (\"s\")", true, p(|x| &x.deesser));
    let presence = switch_row("Realçar clareza da voz", true, p(|x| &x.presence));
    let comp = switch_row("Compressor", true, p(|x| &x.compressor));
    let sh = shared.clone();
    let (thr_row, thr_scale) = scale_row(
        "Limiar do compressor (dB)",
        (-50.0, 0.0),
        1.0,
        -20.0,
        move |v| sh.params.set_comp_threshold_db(v),
    );
    let sh = shared.clone();
    let (ratio_row, ratio_scale) = scale_row("Razão", (1.0, 10.0), 0.5, 4.0, move |v| {
        sh.params.set_comp_ratio(v)
    });
    let auto_gain = switch_row("Volume automático", true, p(|x| &x.auto_gain));
    let sh = shared.clone();
    let (makeup_row, makeup_scale) =
        scale_row("Ganho manual (dB)", (0.0, 24.0), 1.0, 0.0, move |v| {
            sh.params.set_makeup_db(v)
        });
    // O ganho manual só vale com o volume automático desligado.
    auto_gain
        .bind_property("active", &makeup_row, "sensitive")
        .invert_boolean()
        .sync_create()
        .build();

    for row in [
        &denoise, &highpass, &gate, &deesser, &presence, &comp, &auto_gain,
    ] {
        adv.add_row(row);
    }
    for row in [&gate_row, &thr_row, &ratio_row, &makeup_row] {
        adv.add_row(row);
    }
    // Aplica o preset e espelha os valores nos controles avançados.
    let sync = {
        let sh = shared.clone();
        let preset_desc = preset_desc.clone();
        move |index: usize| {
            let preset = dsp::Preset::ALL
                .get(index)
                .copied()
                .unwrap_or(dsp::Preset::Chamada);
            let prm = &sh.params;
            prm.apply_preset(preset);
            preset_desc.set_label(preset.description());

            let flag = |f: fn(&dsp::Params) -> &std::sync::atomic::AtomicBool| f(prm).load(Relaxed);
            denoise.set_active(flag(|x| &x.denoise));
            highpass.set_active(flag(|x| &x.highpass));
            gate.set_active(flag(|x| &x.gate));
            deesser.set_active(flag(|x| &x.deesser));
            presence.set_active(flag(|x| &x.presence));
            comp.set_active(flag(|x| &x.compressor));
            auto_gain.set_active(flag(|x| &x.auto_gain));
            gate_scale.set_value(prm.gate_threshold() as f64);
            thr_scale.set_value(prm.comp_threshold() as f64);
            ratio_scale.set_value(prm.comp_ratio() as f64);
            makeup_scale.set_value(prm.makeup_db() as f64);
        }
    };
    // Perfil inicial: "Chamada" (índice 1), o mais seguro para uso geral.
    let preset_names: Vec<&str> = dsp::Preset::ALL.iter().map(|p| p.name()).collect();
    let preset_bar = widgets::segmented(&preset_names, 1, sync);
    // O seletor e a descrição entram antes dos ajustes avançados.
    // Só debug: MICBRIDGE_DEMO=<estado> mostra os ajustes avançados e fecha o pareamento.
    #[cfg(debug_assertions)]
    if std::env::var("MICBRIDGE_DEMO").is_ok() {
        adv.set_expanded(true);
        shared.close_pairing();
    }
    fx.add(&preset_bar);
    fx.add(&preset_desc);
    // Em grupo à parte: o libadwaita põe as linhas de lista antes dos demais widgets.
    let adv_group = adw::PreferencesGroup::new();
    adv_group.add(&adv);

    let page = adw::PreferencesPage::new();
    page.add(&state_group);
    page.add(&qr_group);
    page.add(&fx);
    page.add(&adv_group);
    page.add(&conn);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&page));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("MicBridge")
        .css_classes(["mb-window"])
        .default_width(560)
        .default_height(780)
        .content(&view)
        .hide_on_close(true)
        .build();

    let (qr_win, qr_url) = (win.clone(), pair_url.clone());
    qr_btn.connect_clicked(move |_| show_qr(&qr_win, &qr_url));

    // Atualiza medidores e status (20 Hz).
    let mut fp_loaded = false;
    let mut last_frames = shared.params.frames();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        // Escala dB -> 0..1 (-60..0 dB).
        let norm =
            |rms: f32| (((20.0 * (rms + 1e-9).log10()) + 60.0) / 60.0).clamp(0.0, 1.0) as f64;
        let left = shared.pairing_remaining();
        if pair_row.is_active() != (left > 0) {
            pair_row.set_active(left > 0);
        }
        qr_group.set_visible(left > 0);
        pair_row.set_subtitle(&if left > 0 {
            format!("Aberto: fecha em {}:{:02}", left / 60, left % 60)
        } else {
            "Fechado: só celulares já pareados reconectam".to_string()
        });
        let n = shared.device_count();
        devices_row.set_subtitle(&match n {
            0 => "Nenhum".to_string(),
            1 => "1 celular".to_string(),
            n => format!("{n} celulares"),
        });
        forget_btn.set_sensitive(n > 0);
        if !fp_loaded {
            if let Some(fp) = tls::fingerprint() {
                fp_row.set_subtitle(&fp);
                fp_loaded = true;
            }
        }
        // Estado da conexão, bem visível. A bolinha pulsa enquanto há voz chegando.
        let st = shared.status();
        let frames = shared.params.frames();
        let voice = shared.params.out_level() > 0.005 && frames != last_frames;
        last_frames = frames;
        hero.set_state(st.link, st.label, &st.detail, voice);
        hero.set_levels(
            norm(shared.params.in_level()),
            norm(shared.params.out_level()),
        );
        // Só debug: força um estado para conferir o visual (conectado, voz, conectando, erro).
        #[cfg(debug_assertions)]
        if let Ok(demo) = std::env::var("MICBRIDGE_DEMO") {
            let ok = state::Link::Connected;
            let (link, label, detail, speaking, input, output) = match demo.as_str() {
                "voice" => (
                    ok,
                    "Conectado",
                    "Recebendo áudio do celular",
                    true,
                    0.62,
                    0.78,
                ),
                "hot" => (
                    ok,
                    "Conectado",
                    "Recebendo áudio do celular",
                    true,
                    0.84,
                    0.95,
                ),
                "warn" => (
                    state::Link::Reconnecting,
                    "Reconectando",
                    "Conexão perdida. Aguardando o celular voltar…",
                    false,
                    0.0,
                    0.0,
                ),
                "error" => (
                    state::Link::Error,
                    "Erro",
                    "Microfone virtual indisponível: recriando…",
                    false,
                    0.0,
                    0.0,
                ),
                _ => (
                    ok,
                    "Conectado",
                    "Recebendo áudio do celular",
                    false,
                    0.2,
                    0.3,
                ),
            };
            hero.set_state(link, label, detail, speaking);
            hero.set_levels(input, output);
        }
        glib::ControlFlow::Continue
    });

    gtk::Window::set_default_icon_name(APP_ID);
    if show {
        win.present();
    }
    #[cfg(debug_assertions)]
    snapshot_and_quit(&win, app);
}

/// Só em build de debug: com `MICBRIDGE_SNAPSHOT=/caminho.png`, salva uma captura da
/// janela depois de 2 s e encerra. Serve para conferir o visual (ver a skill libadwaita-design).
#[cfg(debug_assertions)]
fn snapshot_and_quit(win: &adw::ApplicationWindow, app: &adw::Application) {
    let Ok(path) = std::env::var("MICBRIDGE_SNAPSHOT") else {
        return;
    };
    let (win, app) = (win.clone(), app.clone());
    // MICBRIDGE_SNAPSHOT_SIZE=LARGURAxALTURA captura a janela maior (ver a página inteira).
    if let Some((w, h)) = std::env::var("MICBRIDGE_SNAPSHOT_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').map(|(w, h)| (w.parse(), h.parse())))
        .and_then(|(w, h)| Some((w.ok()?, h.ok()?)))
    {
        win.set_default_size(w, h);
    }
    glib::timeout_add_local_once(Duration::from_secs(2), move || {
        let paintable = gtk::WidgetPaintable::new(Some(&win));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, win.width() as f64, win.height() as f64);
        let texture = snapshot
            .to_node()
            .zip(win.native().and_then(|n| n.renderer()))
            .map(|(node, renderer)| renderer.render_texture(&node, None));
        match texture.map(|t| t.save_to_png(&path)) {
            Some(Ok(())) => eprintln!("captura salva em {path}"),
            _ => eprintln!("falha ao capturar a janela"),
        }
        app.quit();
    });
}
