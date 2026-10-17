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
    // Valida sob o mesmo lock da escrita: uma biblioteca incompatível ou danificada
    // deve continuar no disco, inclusive se mudou depois da abertura da interface.
    if key == "voice_profiles" {
        if let Some(current) = settings.get(key) {
            let library: crate::profiles::Library =
                serde_json::from_value(current.clone()).map_err(std::io::Error::other)?;
            library.validate().map_err(std::io::Error::other)?;
        }
    }
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
#[cfg(test)]
fn save_preset_to(path: &Path, preset: Preset) -> std::io::Result<()> {
    update(path, "preset", serde_json::json!(preset.key()))
}
pub(crate) fn load_profiles_from(path: &Path) -> Result<crate::profiles::Library, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(crate::profiles::Library::new(Preset::Chamada));
        }
        Err(e) => return Err(e.to_string()),
    };
    let mut text = String::new();
    file.take(1_048_577)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 1_048_576 {
        return Err("Arquivo de preferências muito grande".into());
    }
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if let Some(saved) = value.get("voice_profiles") {
        let library: crate::profiles::Library =
            serde_json::from_value(saved.clone()).map_err(|e| e.to_string())?;
        library.validate()?;
        Ok(library)
    } else {
        Ok(crate::profiles::Library::new(
            value
                .get("preset")
                .and_then(|p| p.as_str())
                .and_then(Preset::from_key)
                .unwrap_or(Preset::Chamada),
        ))
    }
}
pub(crate) fn save_profiles_to(
    path: &Path,
    library: &crate::profiles::Library,
) -> Result<(), String> {
    library.validate()?;
    update(
        path,
        "voice_profiles",
        serde_json::to_value(library).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn load_appearance() -> crate::theme::Appearance {
    load_appearance_from(&path())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_profiles_restore_selection_and_preserve_appearance_and_other_settings() {
        let file = temp_file("voice-library");
        save_preset_to(&file, Preset::Podcast).unwrap();
        let mut library = load_profiles_from(&file).unwrap();
        assert_eq!(library.selected, "podcast");
        let config = library.config().unwrap();
        library.create("Meu podcast", config).unwrap();
        let appearance = crate::theme::Appearance {
            opacity: 80,
            ..Default::default()
        };
        save_appearance_to(&file, &appearance).unwrap();
        save_profiles_to(&file, &library).unwrap();
        assert_eq!(load_profiles_from(&file).unwrap(), library);
        assert_eq!(load_appearance_from(&file), appearance);
        save_appearance_to(&file, &crate::theme::Appearance::default()).unwrap();
        assert_eq!(load_profiles_from(&file).unwrap(), library);
        let mut invalid = serde_json::to_value(&library).unwrap();
        invalid["selected"] = "missing".into();
        update(&file, "voice_profiles", invalid).unwrap();
        let before = std::fs::read(&file).unwrap();
        assert!(load_profiles_from(&file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), before);
        assert!(
            save_profiles_to(&file, &library).is_err(),
            "biblioteca rejeitada não pode ser substituída silenciosamente"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before);
        std::fs::write(&file, "broken json").unwrap();
        assert!(save_profiles_to(&file, &library).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "broken json");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

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
