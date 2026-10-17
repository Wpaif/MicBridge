use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    #[serde(alias = "omarchy")]
    System,
    Light,
    Dark,
    Custom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Light,
    Dark,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub version: u8,
    pub name: String,
    pub mode: Mode,
    pub background: String,
    pub surface: String,
    pub foreground: String,
    pub accent: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub source: Source,
    pub opacity: u8,
    pub custom: Option<Palette>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            source: Source::System,
            opacity: 100,
            custom: None,
        }
    }
}
impl Appearance {
    pub fn validate(&self) -> Result<(), String> {
        if !(60..=100).contains(&self.opacity) {
            return Err("A opacidade deve ficar entre 60% e 100%".into());
        }
        if let Some(p) = &self.custom {
            p.validate()?;
        }
        if self.source == Source::Custom && self.custom.is_none() {
            return Err("Importe um tema personalizado primeiro".into());
        }
        Ok(())
    }
}
pub fn rgb(hex: &str) -> Result<[f64; 3], String> {
    if hex.len() != 7
        || !hex.starts_with('#')
        || !hex.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err("Use cores hexadecimais no formato #RRGGBB".into());
    }
    Ok([1, 3, 5].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0))
}
fn luminance(c: &str) -> Result<f64, String> {
    let c = rgb(c)?.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    Ok(c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722)
}
pub fn contrast(a: &str, b: &str) -> Result<f64, String> {
    let a = luminance(a)?;
    let b = luminance(b)?;
    Ok((a.max(b) + 0.05) / (a.min(b) + 0.05))
}
impl Palette {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 65536 {
            return Err("O tema excede 64 KiB".into());
        }
        let palette: Self =
            serde_json::from_str(text).map_err(|e| format!("Tema JSON inválido: {e}"))?;
        palette.validate()?;
        Ok(palette)
    }
    pub fn omarchy(text: &str) -> Result<Self, String> {
        if text.len() > 65536 {
            return Err("A paleta Omarchy excede 64 KiB".into());
        }
        let t: toml::Table = text
            .parse()
            .map_err(|e| format!("Paleta Omarchy inválida: {e}"))?;
        let get = |keys: &[&str]| {
            keys.iter()
                .find_map(|k| t.get(*k)?.as_str())
                .map(String::from)
        };
        let background = get(&["background", "bg", "color0"]).ok_or("Omarchy sem cor de fundo")?;
        let mode = match get(&["mode", "theme_type"]).as_deref() {
            Some("light") => Mode::Light,
            Some("dark") => Mode::Dark,
            None => {
                if luminance(&background)? > 0.5 {
                    Mode::Light
                } else {
                    Mode::Dark
                }
            }
            _ => return Err("Modo Omarchy inválido".into()),
        };
        let palette = Self {
            version: 1,
            name: "Omarchy".into(),
            mode,
            surface: get(&["lighter_background", "lighter_bg"])
                .unwrap_or_else(|| background.clone()),
            background,
            foreground: get(&["foreground", "fg", "color7"]).ok_or("Omarchy sem cor de texto")?,
            accent: get(&["accent", "blue", "color4"]).ok_or("Omarchy sem destaque")?,
        };
        palette.validate()?;
        Ok(palette)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Versão de tema não suportada".into());
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Nome de tema inválido (até 80 caracteres)".into());
        }
        for c in [
            &self.background,
            &self.surface,
            &self.foreground,
            &self.accent,
        ] {
            rgb(c)?;
        }
        if contrast(&self.background, &self.foreground)? < 4.5
            || contrast(&self.surface, &self.foreground)? < 4.5
        {
            return Err("O texto do tema precisa de contraste mínimo de 4,5:1".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub fn palette() -> Palette {
        Palette {
            version: 1,
            name: "Estúdio".into(),
            mode: Mode::Dark,
            background: "#14161b".into(),
            surface: "#20232a".into(),
            foreground: "#eeeeee".into(),
            accent: "#8aa2ff".into(),
        }
    }
    #[test]
    fn legacy_omarchy_preferences_migrate_to_system_without_losing_options() {
        let prefs: Appearance =
            serde_json::from_str(r#"{"source":"omarchy","opacity":85,"custom":null}"#).unwrap();
        assert_eq!(prefs.source, Source::System);
        assert_eq!(prefs.opacity, 85);
        assert_eq!(serde_json::to_value(&prefs).unwrap()["source"], "system");
    }
    #[test]
    fn import_export_preserves_valid_light_and_dark_palettes() {
        for mut p in [palette(), palette()] {
            p.validate().unwrap();
            assert_eq!(
                Palette::parse(&serde_json::to_string(&p).unwrap()).unwrap(),
                p
            );
            p.mode = Mode::Light;
            p.background = "#ffffff".into();
            p.surface = "#eeeeee".into();
            p.foreground = "#171717".into();
            assert_eq!(
                Palette::parse(&serde_json::to_string(&p).unwrap()).unwrap(),
                p
            );
        }
    }
    #[test]
    fn rejects_bad_versions_css_injection_low_contrast_and_large_imports() {
        for change in 0..5 {
            let mut p = palette();
            match change {
                0 => p.version = 2,
                1 => p.background = "red; opacity:0".into(),
                2 => p.foreground = p.surface.clone(),
                3 => p.name = "x".repeat(81),
                _ => p.accent = "url(file:///tmp/foo)".into(),
            }
            assert!(Palette::parse(&serde_json::to_string(&p).unwrap()).is_err());
        }
        assert!(Palette::parse(&" ".repeat(65537)).is_err());
    }
    #[test]
    fn opacity_and_custom_source_are_validated() {
        Appearance::default().validate().unwrap();
        for opacity in [60, 85, 100] {
            Appearance {
                opacity,
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
        for opacity in [0, 59, 101, 255] {
            assert!(
                Appearance {
                    opacity,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Appearance {
                source: Source::Custom,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        Appearance {
            source: Source::Custom,
            custom: Some(palette()),
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
    #[test]
    fn omarchy_reads_semantic_and_legacy_palettes_without_executing_css() {
        let p=Palette::omarchy("mode = 'dark'\nbackground = '#14161b'\nlighter_background = '#20232a'\nforeground = '#eeeeee'\naccent = '#8aa2ff'\n").unwrap();
        assert_eq!(p.background, "#14161b");
        assert_eq!(p.surface, "#20232a");
        let old = Palette::omarchy("bg='#14161b'\nfg='#eeeeee'\ncolor4='#8aa2ff'\n").unwrap();
        assert_eq!(old.mode, Mode::Dark);
        assert!(
            Palette::omarchy(
                "background='$(touch /tmp/unwanted)'\nforeground='#ffffff'\naccent='#aabbcc'"
            )
            .is_err()
        );
    }
}
