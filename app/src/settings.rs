//! Preferências que persistem entre execuções, em `~/.local/share/micbridge/settings.json`.

use crate::dsp::Preset;
use crate::state::data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    /// Chave do perfil de voz (`Preset::key`).
    #[serde(default)]
    preset: Option<String>,
}

pub(crate) fn path() -> PathBuf {
    data_dir().join("settings.json")
}

/// Perfil de voz da última execução, se houver um salvo e válido.
pub fn load_preset() -> Option<Preset> {
    load_preset_from(&path())
}

/// Guarda o perfil escolhido. Falhar ao salvar não deve atrapalhar o uso, então só avisa.
pub fn save_preset(preset: Preset) {
    if let Err(e) = save_preset_to(&path(), preset) {
        tracing::warn!("não foi possível salvar o perfil de voz: {e}");
    }
}

fn load_preset_from(path: &Path) -> Option<Preset> {
    let text = std::fs::read_to_string(path).ok()?;
    let settings: Settings = serde_json::from_str(&text).ok()?;
    Preset::from_key(settings.preset.as_deref()?)
}

fn update(path: &Path, key: &str, value: serde_json::Value) -> std::io::Result<()> {
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _write = WRITE.lock().unwrap();
    let mut settings = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text)
            .map_err(std::io::Error::other)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => return Err(e),
    };
    settings.insert(key.into(), value);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", crate::state::random_hex(8)));
    let result = (|| {
        std::fs::write(&tmp, serde_json::to_vec_pretty(&settings)?)?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result
}
fn save_preset_to(path: &Path, preset: Preset) -> std::io::Result<()> {
    update(path, "preset", serde_json::json!(preset.key()))
}
pub fn load_appearance() -> crate::theme::Appearance {
    load_appearance_from(&path())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("micbridge-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn guarda_e_recupera_cada_perfil() {
        let file = temp_file("roundtrip");
        for preset in Preset::ALL {
            save_preset_to(&file, preset).unwrap();
            assert_eq!(load_preset_from(&file), Some(preset));
        }
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn sem_arquivo_ou_com_lixo_volta_ao_padrao() {
        let file = temp_file("invalido");
        assert_eq!(load_preset_from(&file), None);

        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        for lixo in [
            "",
            "não é json",
            "{}",
            r#"{"preset":"inexistente"}"#,
            r#"{"preset":7}"#,
        ] {
            std::fs::write(&file, lixo).unwrap();
            assert_eq!(load_preset_from(&file), None, "{lixo:?}");
        }
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn appearance_and_voice_preset_never_overwrite_each_other() {
        let file = temp_file("appearance");
        save_preset_to(&file, Preset::Chamada).unwrap();
        let appearance = crate::theme::Appearance {
            source: crate::theme::Source::Dark,
            opacity: 85,
            custom: None,
        };
        save_appearance_to(&file, &appearance).unwrap();
        assert_eq!(load_preset_from(&file), Some(Preset::Chamada));
        save_preset_to(&file, Preset::Bruto).unwrap();
        assert_eq!(load_appearance_from(&file), appearance);
        std::fs::write(
            &file,
            r#"{"preset":"chamada","appearance":{"opacity":0},"future":42}"#,
        )
        .unwrap();
        assert_eq!(
            load_appearance_from(&file),
            crate::theme::Appearance::default()
        );
        save_appearance_to(&file, &appearance).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&file).unwrap())
                .unwrap()["future"],
            42
        );
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
    #[test]
    fn chaves_sao_unicas_e_reversiveis() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_key(preset.key()), Some(preset));
        }
        assert_eq!(Preset::from_key("Chamada"), None); // a chave não é o nome exibido
    }
}

fn load_appearance_from(path: &Path) -> crate::theme::Appearance {
    let value = (|| {
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let appearance: crate::theme::Appearance =
            serde_json::from_value(value.get("appearance")?.clone()).ok()?;
        appearance.validate().ok()?;
        Some(appearance)
    })();
    value.unwrap_or_default()
}
pub(crate) fn save_appearance_to(
    path: &Path,
    appearance: &crate::theme::Appearance,
) -> std::io::Result<()> {
    appearance.validate().map_err(std::io::Error::other)?;
    update(path, "appearance", serde_json::to_value(appearance)?)
}
