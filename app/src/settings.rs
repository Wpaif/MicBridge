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

fn path() -> PathBuf {
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

fn save_preset_to(path: &Path, preset: Preset) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let settings = Settings {
        preset: Some(preset.key().to_string()),
    };
    // Grava num arquivo temporário e renomeia: um crash no meio não deixa o JSON pela metade.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&settings)?)?;
    std::fs::rename(tmp, path)
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
    fn chaves_sao_unicas_e_reversiveis() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_key(preset.key()), Some(preset));
        }
        assert_eq!(Preset::from_key("Chamada"), None); // a chave não é o nome exibido
    }
}
