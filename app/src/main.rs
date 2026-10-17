#[cfg(test)]
mod audio_test;
mod dsp;
mod effect_ui;
mod local_audio;
mod monitor;
mod monitor_ui;
mod output;
mod recording;
mod recording_ui;
mod server;
mod settings;
mod state;
mod theme;
mod theme_ui;
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
    // Só debug: MICBRIDGE_APP_ID permite rodar ao lado do app instalado (ver skill libadwaita-design).
    #[cfg(debug_assertions)]
    let app_id = std::env::var("MICBRIDGE_APP_ID").unwrap_or_else(|_| APP_ID.into());
    #[cfg(not(debug_assertions))]
    let app_id = APP_ID;
    let app = adw::Application::builder().application_id(app_id).build();
    if let Err(e) = app.register(None::<&gtk::gio::Cancellable>) {
        tracing::warn!("registro do GApplication: {e}");
    } else if app.is_remote() {
        app.activate();
        return glib::ExitCode::SUCCESS;
    }

    let shared = Arc::new(Shared::new());
    let shutdown_shared = shared.clone();
    app.connect_shutdown(move |_| shutdown_shared.stop_audio_tools());
    let final_shared = shared.clone();

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
        let quit_shared = shared.clone();
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
                    tray::TrayEvent::Quit => {
                        quit_shared.stop_audio_tools();
                        if let Err(e) = quit_shared.local_audio.shutdown_and_wait().await {
                            tracing::warn!("restauração pendente ao sair: {e}");
                        }
                        app.quit();
                    }
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
    final_shared.stop_audio_tools();
    if let Err(e) = rt.block_on(final_shared.local_audio.shutdown_and_wait()) {
        tracing::warn!("restauração pendente: {e}");
    }
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
    scale.set_width_request(140);
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

    let appearance = theme::install();

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
            "Servidor · nome fixo"
        } else {
            "Servidor · IP local"
        })
        .subtitle(&url)
        .title_lines(2)
        .subtitle_lines(2)
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
        .subtitle_lines(2)
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
        .title_lines(2)
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
        .title("Certificado · SHA-256")
        .subtitle_lines(2)
        .subtitle("aguardando o servidor…")
        .subtitle_selectable(true)
        .build();
    let fp_copy = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copiar impressão digital completa")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    fp_copy.connect_clicked(|button| {
        if let Some(fp) = tls::fingerprint() {
            button.clipboard().set_text(&fp);
        }
    });
    fp_row.add_suffix(&fp_copy);
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

    let cleanup = adw::PreferencesGroup::builder()
        .title("01 · Limpeza")
        .build();
    let tone = adw::PreferencesGroup::builder()
        .title("02 · Timbre")
        .build();
    let dynamics = adw::PreferencesGroup::builder()
        .title("03 · Dinâmica")
        .build();
    let gain = adw::PreferencesGroup::builder()
        .title("04 · Volume")
        .build();

    let denoise = switch_row("Redução de ruído", true, p(|x| &x.denoise));
    let highpass = switch_row("Filtro de graves", true, p(|x| &x.highpass));
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
    let (thr_row, thr_scale) = scale_row("Limiar (dB)", (-50.0, 0.0), 1.0, -20.0, move |v| {
        sh.params.set_comp_threshold_db(v)
    });
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

    cleanup.add(&denoise);
    cleanup.add(&highpass);
    cleanup.add(&gate);
    cleanup.add(&gate_row);
    tone.add(&deesser);
    tone.add(&presence);
    dynamics.add(&comp);
    dynamics.add(&thr_row);
    dynamics.add(&ratio_row);
    gain.add(&auto_gain);
    gain.add(&makeup_row);

    let mut effect_indicators = effect_ui::Indicators::new(
        &shared.params,
        shared.generation(),
        [&gate, &comp, &auto_gain],
    );

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
            settings::save_preset(preset);
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
    // Perfil inicial: o da última execução ou, na primeira vez, "Chamada" (índice 1), o mais
    // seguro para uso geral.
    let initial = settings::load_preset()
        .and_then(|saved| dsp::Preset::ALL.iter().position(|p| *p == saved))
        .unwrap_or(1);
    let preset_names: Vec<&str> = dsp::Preset::ALL.iter().map(|p| p.name()).collect();
    let preset_bar = widgets::segmented(&preset_names, initial, sync);
    #[cfg(debug_assertions)]
    if std::env::var("MICBRIDGE_DEMO").is_ok() {
        shared.close_pairing();
    }
    fx.add(&preset_bar);
    fx.add(&preset_desc);

    let modules = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(2)
        .column_spacing(18)
        .row_spacing(18)
        .homogeneous(true)
        .valign(gtk::Align::Start)
        .build();
    for group in [&cleanup, &tone, &dynamics, &gain] {
        group.set_width_request(310);
        group.set_hexpand(true);
        group.set_valign(gtk::Align::Start);
        modules.insert(group, -1);
    }

    let studio = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .hexpand(true)
        .css_classes(["mb-studio"])
        .build();
    studio.append(&fx);
    studio.append(&modules);
    let note = gtk::Label::builder()
        .label("48 kHz · Mono → Estéreo · Limitador de segurança ativo")
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();
    studio.append(&note);

    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .width_request(310)
        .css_classes(["mb-sidebar"])
        .build();
    sidebar.append(&qr_group);
    sidebar.append(&conn);
    let sidebar_clamp = adw::Clamp::builder()
        .maximum_size(340)
        .tightening_threshold(340)
        .hexpand(false)
        .valign(gtk::Align::Start)
        .child(&sidebar)
        .build();

    let workspace = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(30)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .valign(gtk::Align::Start)
        .build();
    workspace.append(&studio);
    workspace.append(&sidebar_clamp);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&workspace)
        .build();
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("MicBridge", "Estúdio de voz")));
    view.add_top_bar(&header);
    let live_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["toolbar", "mb-live-header"])
        .build();
    live_header.append(&state_group);
    let mute_button = gtk::ToggleButton::builder()
        .name("microphone-mute")
        .label("Silenciar microfone")
        .tooltip_text("Silencia a saída para outros programas. Retorno no fone e gravação de teste continuam ativos.")
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .build();
    let mute_shared = shared.clone();
    mute_button.connect_toggled(move |button| {
        let muted = button.is_active();
        mute_shared.params.muted.store(muted, Relaxed);
        button.set_label(if muted {
            "Microfone silenciado"
        } else {
            "Silenciar microfone"
        });
        if muted {
            button.add_css_class("destructive-action");
        } else {
            button.remove_css_class("destructive-action");
        }
    });
    hero.controls.append(&mute_button);
    view.add_top_bar(&live_header);
    view.set_content(Some(&scroll));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("MicBridge")
        .default_width(1200)
        .default_height(800)
        .width_request(420)
        .height_request(400)
        .content(&view)
        .hide_on_close(true)
        .build();
    let appearance_button = gtk::Button::builder()
        .icon_name("applications-graphics-symbolic")
        .tooltip_text("Aparência")
        .build();
    header.pack_end(&appearance_button);
    let parent = win.downgrade();
    let preferences = std::rc::Rc::new(std::cell::RefCell::new(glib::WeakRef::<
        adw::PreferencesDialog,
    >::new()));
    appearance_button.connect_clicked(move |_| {
        let Some(parent) = parent.upgrade() else {
            return;
        };
        if let Some(window) = preferences.borrow().upgrade() {
            window.present(Some(&parent));
            return;
        }
        let window = theme_ui::window(&parent, appearance.clone());
        preferences.borrow().set(Some(&window));
        window.present(Some(&parent));
    });
    // Em telas estreitas os mesmos widgets passam para uma única coluna.
    let compact = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 1100px").expect("breakpoint válido"),
    );
    let narrow = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 600px").expect("breakpoint válido"),
    );
    for breakpoint in [&compact, &narrow] {
        breakpoint.add_setter(
            &workspace,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        breakpoint.add_setter(&sidebar, "width-request", Some(&(-1i32).to_value()));
        breakpoint.add_setter(&sidebar_clamp, "maximum-size", Some(&2000i32.to_value()));
        breakpoint.add_setter(
            &sidebar_clamp,
            "tightening-threshold",
            Some(&2000i32.to_value()),
        );
        breakpoint.add_setter(&modules, "max-children-per-line", Some(&1u32.to_value()));
        breakpoint.add_setter(&workspace, "margin-start", Some(&12i32.to_value()));
        breakpoint.add_setter(&workspace, "margin-end", Some(&12i32.to_value()));
    }
    narrow.add_setter(
        &hero.meters,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    narrow.add_setter(&hero.meters, "spacing", Some(&6i32.to_value()));
    win.add_breakpoint(compact.clone());
    win.add_breakpoint(narrow.clone());

    let recording_panel = recording_ui::panel(shared.clone(), &win);
    let monitor_panel = monitor_ui::panel(shared.clone());
    let local_panels = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(18)
        .homogeneous(true)
        .build();
    for panel in [&recording_panel, &monitor_panel] {
        panel.set_width_request(310);
        panel.set_hexpand(true);
        panel.set_valign(gtk::Align::Start);
        local_panels.append(panel);
    }
    for breakpoint in [&compact, &narrow] {
        breakpoint.add_setter(
            &local_panels,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        breakpoint.add_setter(&local_panels, "homogeneous", Some(&false.to_value()));
    }
    studio.insert_child_after(&local_panels, Some(&fx));

    // add_css_class, e não css_classes(): este último apaga `csd`/`background`, que dão à
    // janela os cantos arredondados e a sombra.
    win.add_css_class("mb-window");

    let (qr_win, qr_url) = (win.clone(), pair_url.clone());
    qr_btn.connect_clicked(move |_| show_qr(&qr_win, &qr_url));

    let indicators_started = std::time::Instant::now();
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
        effect_indicators.update(
            &shared.params,
            shared.generation(),
            st.link == state::Link::Connected,
            indicators_started.elapsed(),
        );
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
            Some(Err(e)) => eprintln!("falha ao salvar a captura: {e}"),
            None => eprintln!("falha ao capturar a janela (sem nó de renderização)"),
        }
        app.quit();
    });
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn mute_control_is_fixed_visible_and_preserves_connection_and_preset() {
        adw::init().unwrap();
        let f = audio_test::Fixture::new("exit 0");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", f.dir.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", f.dir.join("config"));
        }
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.MuteTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        let shared = Arc::new(Shared::new());
        build_ui(&app, shared.clone(), true);
        let win = app.windows()[0].clone();
        until(|| win.is_mapped());
        let button = find::<gtk::ToggleButton>(&win, &|b| b.widget_name() == "microphone-mute")
            .expect("mute disponível no topo fixo");
        assert!(
            button
                .ancestor(gtk::ScrolledWindow::static_type())
                .is_none()
        );
        assert!(!button.is_active());
        let generation = shared.generation();
        let threshold = shared.params.gate_threshold();
        button.emit_clicked();
        assert!(shared.params.muted.load(Relaxed));
        assert!(button.has_css_class("destructive-action"));
        assert_eq!(button.label().as_deref(), Some("Microfone silenciado"));
        assert_eq!(shared.generation(), generation);
        assert_eq!(shared.params.gate_threshold(), threshold);
        button.emit_clicked();
        assert!(!shared.params.muted.load(Relaxed));
        assert!(!button.has_css_class("destructive-action"));
        assert_eq!(button.label().as_deref(), Some("Silenciar microfone"));
        for (width, height) in [(420, 400), (1200, 800)] {
            win.set_default_size(width, height);
            until(|| win.width() == width && button.is_mapped());
            let bounds = button.compute_bounds(&win).unwrap();
            assert!(bounds.x() >= 0.0 && bounds.x() + bounds.width() <= win.width() as f32);
            let scroll = find::<gtk::ScrolledWindow>(&win, &|_| true).unwrap();
            assert!(scroll.height() >= 100);
        }
        win.destroy();
    }
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
        let mut child = w.first_child();
        while let Some(w) = child {
            if let Some(t) = find(&w, yes) {
                return Some(t);
            }
            child = w.next_sibling();
        }
        None
    }
    fn until(mut yes: impl FnMut() -> bool) {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while !yes() {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            assert!(std::time::Instant::now() < end, "GTK timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn fixed_monitoring_header_survives_scroll_and_narrow_window() {
        adw::init().unwrap();
        let f = audio_test::Fixture::new("exit 0");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", f.dir.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", f.dir.join("config"));
        }
        let app = adw::Application::builder()
            .application_id("dev.wilian.MicBridge.AppearanceLayoutTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        let source =
            std::env::var("MICBRIDGE_TEST_APPEARANCE_SOURCE").unwrap_or_else(|_| "system".into());
        let appearance = theme::Appearance {
            source: match source.as_str() {
                "dark" => theme::Source::Dark,
                "light" => theme::Source::Light,
                "custom" => theme::Source::Custom,
                _ => theme::Source::System,
            },
            custom: Some(
                theme::Palette::parse(include_str!("../data/themes/graphite.json")).unwrap(),
            ),
            ..Default::default()
        };
        settings::save_appearance_to(&settings::path(), &appearance).unwrap();
        unsafe {
            std::env::set_var("MICBRIDGE_DEMO", "voice");
        }
        build_ui(&app, Arc::new(Shared::new()), true);
        let win = app.windows()[0]
            .clone()
            .downcast::<adw::ApplicationWindow>()
            .unwrap();
        until(|| win.is_mapped());
        if std::env::var_os("ADW_DEBUG_HIGH_CONTRAST").is_some() {
            assert!(adw::StyleManager::default().is_high_contrast());
        }
        let hero = find::<gtk::Box>(&win, &|b| b.has_css_class("mb-live-header")).unwrap();
        assert!(
            hero.ancestor(gtk::ScrolledWindow::static_type()).is_none(),
            "meters must live outside scrolled content"
        );
        let scroll = find::<gtk::ScrolledWindow>(&win, &|_| true).unwrap();
        for (w, h) in [(1200, 800), (420, 400)] {
            win.set_default_size(w, h);
            until(|| win.width() == w);
            let before = hero.compute_bounds(&win).unwrap();
            let adjustment = scroll.vadjustment();
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            let end = std::time::Instant::now() + Duration::from_millis(180);
            until(|| std::time::Instant::now() >= end);
            let after = hero.compute_bounds(&win).unwrap();
            assert!((before.y() - after.y()).abs() < 1.0);
            assert!(
                scroll.height() >= 100,
                "fixed header leaves useful workspace"
            );
            assert!(after.width() <= win.width() as f32);
            if std::env::var_os("MICBRIDGE_TEST_SNAPSHOTS").is_some() {
                let snapshot = gtk::Snapshot::new();
                win.snapshot_child(&win.child().unwrap(), &snapshot);
                let node = snapshot.to_node().expect("rendered window");
                let texture = win
                    .native()
                    .unwrap()
                    .renderer()
                    .unwrap()
                    .render_texture(&node, None);
                texture
                    .save_to_png(format!("/tmp/micbridge-appearance-{source}-{w}.png"))
                    .unwrap();
            }
            adjustment.set_value(0.0);
            if std::env::var_os("MICBRIDGE_TEST_SNAPSHOTS").is_some() {
                let end = std::time::Instant::now() + Duration::from_millis(180);
                until(|| std::time::Instant::now() >= end);
                let snapshot = gtk::Snapshot::new();
                win.snapshot_child(&win.child().unwrap(), &snapshot);
                let texture = win
                    .native()
                    .unwrap()
                    .renderer()
                    .unwrap()
                    .render_texture(&snapshot.to_node().unwrap(), None);
                texture
                    .save_to_png(format!("/tmp/micbridge-appearance-{source}-{w}-top.png"))
                    .unwrap();
            }
        }
        win.destroy();
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn appearance_uses_system_style_instead_of_forcing_dark() {
        adw::init().unwrap();
        let f = audio_test::Fixture::new("exit 0");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", f.dir.join("data"));
        }
        theme::install();
        assert_eq!(
            adw::StyleManager::default().color_scheme(),
            adw::ColorScheme::Default
        );
    }
}
