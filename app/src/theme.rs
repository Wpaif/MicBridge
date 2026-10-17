//! Aparência local: cores nativas, paletas validadas e fundos translúcidos.
pub mod model;
use adw::prelude::*;
pub use model::{Appearance, Mode, Palette, Source};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
const CSS: &str = include_str!("../data/style.css");

pub struct ThemeController {
    saved: RefCell<Appearance>,
    draft: RefCell<Appearance>,
    provider: gtk::CssProvider,
    base: gtk::CssProvider,
    omarchy: RefCell<Option<Palette>>,
    message: RefCell<String>,
    paths: Vec<PathBuf>,
    settings: PathBuf,
    reading: Cell<bool>,
    omarchy_session: bool,
}
// Session markers, not the presence of stale theme files, select Omarchy integration.
fn is_omarchy_session(desktop: &str, path: Option<&str>) -> bool {
    let desktops: Vec<_> = desktop
        .split(':')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    if desktops
        .iter()
        .any(|s| matches!(s.as_str(), "gnome" | "kde" | "plasma"))
    {
        return false;
    }
    desktops.iter().any(|s| s == "omarchy")
        || (desktops.iter().any(|s| s == "hyprland") && path.is_some_and(|s| !s.trim().is_empty()))
}

pub fn install() -> Rc<ThemeController> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    ThemeController::new(
        crate::settings::load_appearance(),
        crate::settings::path(),
        vec![
            state.join("omarchy/current/theme/colors.toml"),
            config.join("omarchy/current/theme/colors.toml"),
        ],
    )
}
impl ThemeController {
    pub fn new(initial: Appearance, settings: PathBuf, paths: Vec<PathBuf>) -> Rc<Self> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let omarchy_path = std::env::var("OMARCHY_PATH").ok();
        Self::new_for_session(
            initial,
            settings,
            paths,
            is_omarchy_session(&desktop, omarchy_path.as_deref()),
        )
    }
    fn new_for_session(
        initial: Appearance,
        settings: PathBuf,
        paths: Vec<PathBuf>,
        omarchy_session: bool,
    ) -> Rc<Self> {
        let base = gtk::CssProvider::new();
        base.load_from_string(CSS);
        let provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &base,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
            );
        }
        let this = Rc::new(Self {
            saved: RefCell::new(initial.clone()),
            draft: RefCell::new(initial),
            provider,
            base,
            omarchy: RefCell::new(None),
            message: RefCell::new(String::new()),
            paths,
            settings,
            reading: Cell::new(false),
            omarchy_session,
        });
        this.apply();
        let weak = Rc::downgrade(&this);
        adw::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.apply();
            }
        });
        let weak = Rc::downgrade(&this);
        adw::StyleManager::default().connect_high_contrast_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.apply();
            }
        });
        let weak = Rc::downgrade(&this);
        glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            this.refresh_omarchy();
            glib::ControlFlow::Continue
        });
        this.refresh_omarchy();
        this
    }
    pub fn current(&self) -> Appearance {
        self.draft.borrow().clone()
    }
    pub fn status(&self) -> String {
        if adw::StyleManager::default().is_high_contrast() {
            "Alto contraste ativo: cores do sistema e fundo opaco".into()
        } else if self.follows_omarchy() {
            let message = self.message.borrow().clone();
            if message.is_empty() && self.omarchy.borrow().is_some() {
                "Seguindo o tema do Omarchy".into()
            } else {
                message
            }
        } else {
            String::new()
        }
    }
    fn follows_omarchy(&self) -> bool {
        self.current().source == Source::System && self.omarchy_session
    }
    pub fn preview(self: &Rc<Self>, appearance: Appearance) -> Result<(), String> {
        appearance.validate()?;
        *self.draft.borrow_mut() = appearance;
        self.apply();
        self.refresh_omarchy();
        Ok(())
    }
    pub fn cancel(&self) {
        *self.draft.borrow_mut() = self.saved.borrow().clone();
        self.apply();
    }
    pub async fn commit(self: &Rc<Self>) -> Result<(), String> {
        let draft = self.current();
        let saved = draft.clone();
        let path = self.settings.clone();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(
                crate::settings::save_appearance_to(&path, &saved).map_err(|e| e.to_string()),
            );
        });
        rx.recv().await.map_err(|e| e.to_string())??;
        *self.saved.borrow_mut() = draft;
        Ok(())
    }
    #[allow(deprecated)] // GTK has no replacement for resolving named background colors for export.
    pub fn effective_palette(&self) -> Palette {
        match if adw::StyleManager::default().is_high_contrast() {
            Source::System
        } else {
            self.current().source
        } {
            Source::Custom => {
                if let Some(p) = self.current().custom {
                    return p;
                }
            }
            Source::System
                if self.omarchy_session && !adw::StyleManager::default().is_high_contrast() =>
            {
                if let Some(p) = self.omarchy.borrow().clone() {
                    return p;
                }
            }
            _ => {}
        }
        let style = gtk::Label::new(None).style_context();
        let color = |name: &str, fallback: &str| {
            style
                .lookup_color(name)
                .map(|c| {
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        (c.red() * 255.0).round() as u8,
                        (c.green() * 255.0).round() as u8,
                        (c.blue() * 255.0).round() as u8
                    )
                })
                .unwrap_or_else(|| fallback.into())
        };
        let dark = adw::StyleManager::default().is_dark();
        let background = color("window_bg_color", if dark { "#242424" } else { "#fafafa" });
        let surface = style
            .lookup_color("card_bg_color")
            .map(|c| {
                let bg = model::rgb(&background).expect("native RGB color");
                let channels = [c.red(), c.green(), c.blue()];
                let rgb: [u8; 3] = std::array::from_fn(|i| {
                    ((channels[i] as f64 * c.alpha() as f64 + bg[i] * (1.0 - c.alpha() as f64))
                        * 255.0)
                        .round() as u8
                });
                format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
            })
            .unwrap_or_else(|| background.clone());
        Palette {
            version: 1,
            name: "Sistema".into(),
            mode: if dark { Mode::Dark } else { Mode::Light },
            surface,
            background,
            foreground: color("window_fg_color", if dark { "#ffffff" } else { "#202020" }),
            accent: color("accent_bg_color", "#3584e4"),
        }
    }
    fn apply(&self) {
        let prefs = self.current();
        let manager = adw::StyleManager::default();
        let palette = match prefs.source {
            Source::Custom => prefs.custom.clone(),
            Source::System if self.omarchy_session => self.omarchy.borrow().clone(),
            _ => None,
        };
        let mode = palette.as_ref().map(|p| p.mode);
        manager.set_color_scheme(match prefs.source {
            Source::Light => adw::ColorScheme::ForceLight,
            Source::Dark => adw::ColorScheme::ForceDark,
            _ => match mode {
                Some(Mode::Light) => adw::ColorScheme::ForceLight,
                Some(Mode::Dark) => adw::ColorScheme::ForceDark,
                None => adw::ColorScheme::Default,
            },
        });
        let high = manager.is_high_contrast();
        let opacity = if high {
            1.0
        } else {
            prefs.opacity as f64 / 100.0
        };
        let mut css = format!(
            "window.mb-window {{ background: transparent; }} window.mb-window toolbarview {{ background: alpha(var(--window-bg-color), {opacity}); }}"
        );
        if !high {
            if let Some(p) = palette {
                let fg = if model::contrast(&p.accent, "#ffffff").unwrap_or(0.0) >= 4.5 {
                    "#ffffff"
                } else {
                    "#000000"
                };
                css += &format!(
                    "window.mb-window {{ --window-bg-color: {}; --window-fg-color: {}; --view-bg-color: {}; --view-fg-color: {}; --card-bg-color: {}; --card-fg-color: {}; --headerbar-bg-color: {}; --headerbar-fg-color: {}; --dialog-bg-color: {}; --dialog-fg-color: {}; --popover-bg-color: {}; --popover-fg-color: {}; --accent-bg-color: {}; --accent-fg-color: {}; }}",
                    p.background,
                    p.foreground,
                    p.background,
                    p.foreground,
                    p.surface,
                    p.foreground,
                    p.surface,
                    p.foreground,
                    p.surface,
                    p.foreground,
                    p.surface,
                    p.foreground,
                    p.accent,
                    fg
                );
            }
        }
        self.provider.load_from_string(&css);
    }
    fn refresh_omarchy(self: &Rc<Self>) {
        if !self.follows_omarchy() || self.reading.replace(true) {
            return;
        }
        let paths = self.paths.clone();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(read_omarchy(&paths));
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = rx.recv().await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            this.reading.set(false);
            match result {
                Ok(Ok(p)) => {
                    let changed = this.omarchy.borrow().as_ref() != Some(&p);
                    *this.omarchy.borrow_mut() = Some(p);
                    this.message.borrow_mut().clear();
                    if changed {
                        this.apply();
                    }
                }
                Ok(Err(e)) => {
                    *this.message.borrow_mut() = if this.omarchy.borrow().is_some() {
                        format!("{e} Mantendo a última paleta válida.")
                    } else {
                        e
                    }
                }
                Err(e) => *this.message.borrow_mut() = e.to_string(),
            }
        });
    }
}
impl Drop for ThemeController {
    fn drop(&mut self) {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.provider);
            gtk::style_context_remove_provider_for_display(&display, &self.base);
        }
    }
}
pub(crate) fn read_limited(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Escolha um arquivo regular de tema".into());
    }
    let mut text = String::new();
    file.take(65537)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 65536 {
        return Err("O tema excede 64 KiB".into());
    }
    Ok(text)
}
fn read_omarchy(paths: &[PathBuf]) -> Result<Palette, String> {
    for path in paths {
        if path.exists() {
            return Palette::omarchy(&read_limited(path)?);
        }
    }
    Err("Paleta Omarchy não encontrada. Usando a aparência do sistema.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_active_omarchy_without_hijacking_gnome_kde_or_generic_hyprland() {
        for (desktop, path, expected) in [
            ("Hyprland", Some("/usr/share/omarchy"), true),
            ("omarchy:Hyprland", None, true),
            ("GNOME", Some("/usr/share/omarchy"), false),
            ("KDE", Some("/usr/share/omarchy"), false),
            ("GNOME:Hyprland", Some("/usr/share/omarchy"), false),
            ("Hyprland", None, false),
            ("Hyprland", Some(""), false),
            ("", None, false),
        ] {
            assert_eq!(
                is_omarchy_session(desktop, path),
                expected,
                "desktop={desktop}"
            );
        }
    }
    #[test]
    fn palette_files_are_bounded_and_legacy_path_is_supported() {
        let f = crate::audio_test::Fixture::new("exit 0");
        let file = f.dir.join("colors.toml");
        std::fs::write(&file, "bg='#14161b'\nfg='#eeeeee'\ncolor4='#8aa2ff'\n").unwrap();
        assert_eq!(
            read_omarchy(&[f.dir.join("missing"), file.clone()])
                .unwrap()
                .background,
            "#14161b"
        );
        std::fs::write(&file, "x".repeat(65537)).unwrap();
        assert!(read_limited(&file).is_err());
        assert!(read_limited(&f.dir).is_err());
    }
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn omarchy_live_reload_keeps_last_valid_palette_and_invalid_preview_is_atomic() {
        adw::init().unwrap();
        let f = crate::audio_test::Fixture::new("exit 0");
        let file = f.dir.join("colors.toml");
        std::fs::write(&file, "bg='#14161b'\nfg='#eeeeee'\naccent='#8aa2ff'\n").unwrap();
        let c = ThemeController::new_for_session(
            Appearance {
                source: Source::System,
                ..Default::default()
            },
            f.dir.join("settings.json"),
            vec![file.clone()],
            true,
        );
        fn until(mut yes: impl FnMut() -> bool) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !yes() {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        until(|| c.effective_palette().name == "Omarchy");
        std::fs::write(&file, "bg='#14161b'\nfg='#eeeeee'\naccent='#ffcc88'\n").unwrap();
        until(|| c.effective_palette().accent == "#ffcc88");
        let valid = c.current();
        assert!(
            c.preview(Appearance {
                opacity: 0,
                ..valid.clone()
            })
            .is_err()
        );
        assert_eq!(c.current(), valid);
        std::fs::write(&file, "invalid toml").unwrap();
        until(|| c.status().contains("inválida"));
        assert_eq!(c.effective_palette().accent, "#ffcc88");
        c.preview(Appearance {
            source: Source::Light,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            adw::StyleManager::default().color_scheme(),
            adw::ColorScheme::ForceLight
        );
        assert_eq!(c.effective_palette().name, "Sistema");
        c.preview(Appearance::default()).unwrap();
        assert_eq!(c.effective_palette().name, "Omarchy");
        drop(c);
        std::fs::write(&file, "bg='#14161b'\nfg='#eeeeee'\naccent='#8aa2ff'\n").unwrap();
        let native = ThemeController::new_for_session(
            Appearance::default(),
            f.dir.join("native.json"),
            vec![file],
            false,
        );
        let end = std::time::Instant::now() + std::time::Duration::from_millis(250);
        until(|| std::time::Instant::now() >= end);
        assert_eq!(native.effective_palette().name, "Sistema");
        assert!(native.status().is_empty());
    }
}

#[cfg(test)]
mod rendering_tests {
    use super::*;
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn background_transparency_preserves_opaque_controls() {
        adw::init().unwrap();
        let f = crate::audio_test::Fixture::new("exit 0");
        let _controller = ThemeController::new(
            Appearance {
                source: Source::Dark,
                opacity: 60,
                ..Default::default()
            },
            f.dir.join("settings.json"),
            vec![],
        );
        let win = adw::ApplicationWindow::builder()
            .default_width(300)
            .default_height(200)
            .build();
        win.add_css_class("mb-window");
        let view = adw::ToolbarView::new();
        let label = gtk::Label::new(Some("Texto nítido"));
        view.set_content(Some(&label));
        win.set_content(Some(&view));
        win.present();
        let end = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < end {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let snapshot = gtk::Snapshot::new();
        win.snapshot_child(&win.child().unwrap(), &snapshot);
        let texture = win
            .native()
            .unwrap()
            .renderer()
            .unwrap()
            .render_texture(&snapshot.to_node().unwrap(), None);
        let stride = texture.width() as usize * 4;
        let mut bytes = vec![0; stride * texture.height() as usize];
        texture.download(&mut bytes, stride);
        let alpha = bytes[stride * 30 + 30 * 4 + 3];
        if adw::StyleManager::default().is_high_contrast() {
            assert_eq!(alpha, 255);
        } else {
            assert!((150..=156).contains(&alpha), "background alpha={alpha}");
        }
        assert_eq!(label.opacity(), 1.0);
        assert_eq!(label.color().alpha(), 1.0);
        win.destroy();
    }
}

#[cfg(test)]
mod export_rendering_tests {
    use super::*;
    #[test]
    #[ignore = "requer sessão gráfica GTK"]
    fn exported_native_surface_matches_the_rendered_card_in_light_and_dark() {
        adw::init().unwrap();
        let f = crate::audio_test::Fixture::new("exit 0");
        let c = ThemeController::new(Appearance::default(), f.dir.join("settings.json"), vec![]);
        let win = adw::ApplicationWindow::builder()
            .default_width(300)
            .default_height(200)
            .build();
        win.add_css_class("mb-window");
        let view = adw::ToolbarView::new();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("mb-hero");
        view.set_content(Some(&card));
        win.set_content(Some(&view));
        win.present();
        for source in [Source::Light, Source::Dark] {
            c.preview(Appearance {
                source,
                ..Default::default()
            })
            .unwrap();
            let end = std::time::Instant::now() + std::time::Duration::from_millis(250);
            while std::time::Instant::now() < end {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
            let snap = gtk::Snapshot::new();
            win.snapshot_child(&win.child().unwrap(), &snap);
            let texture = win
                .native()
                .unwrap()
                .renderer()
                .unwrap()
                .render_texture(&snap.to_node().unwrap(), None);
            let mut download = gtk::gdk::TextureDownloader::new(&texture);
            download.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
            let (bytes, stride) = download.download_bytes();
            let palette = c.effective_palette();
            let reimported = Palette::parse(&serde_json::to_string(&palette).unwrap()).unwrap();
            let expected = model::rgb(&reimported.surface)
                .unwrap()
                .map(|x| (x * 255.0).round() as u8);
            let pixel = &bytes[stride * 50 + 50 * 4..stride * 50 + 50 * 4 + 3];
            assert!(
                pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
                "{source:?} exported surface {expected:?} differs from rendered {pixel:?}"
            );
        }
        win.destroy();
    }
}
