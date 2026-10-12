//! Tema visual do app: tema escuro fixo e o CSS do MicBridge (`data/style.css`).

const CSS: &str = include_str!("../data/style.css");

/// Aplica o tema escuro e o CSS próprio a todas as janelas. Chamar uma única vez.
pub fn install() {
    // O visual (vidro translúcido, brilhos) foi desenhado para o escuro.
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
