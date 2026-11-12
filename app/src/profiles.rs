//! Perfis locais de tratamento; estado de sessão não pertence a um perfil.
use crate::dsp::{Params, Preset};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub denoise: bool,
    pub highpass: bool,
    pub gate: bool,
    pub deesser: bool,
    pub presence: bool,
    pub compressor: bool,
    pub auto_gain: bool,
    pub gate_db: f32,
    pub threshold_db: f32,
    pub ratio: f32,
    pub makeup_db: f32,
    /// Perfis salvos antes do equalizador não têm estes campos: carregam desligado e plano.
    #[serde(default)]
    pub eq: bool,
    /// Ganho das bandas em dB: [graves, médios, agudos].
    #[serde(default)]
    pub eq_db: [f32; 3],
    /// Força da redução de ruído (%); perfis antigos carregam com 100.
    #[serde(default = "full_strength")]
    pub denoise_strength: f32,
}
fn full_strength() -> f32 {
    100.0
}
impl Config {
    pub fn capture(p: &Params) -> Self {
        Self {
            denoise: p.denoise.load(Relaxed),
            highpass: p.highpass.load(Relaxed),
            gate: p.gate.load(Relaxed),
            deesser: p.deesser.load(Relaxed),
            presence: p.presence.load(Relaxed),
            compressor: p.compressor.load(Relaxed),
            auto_gain: p.auto_gain.load(Relaxed),
            gate_db: p.gate_threshold(),
            threshold_db: p.comp_threshold(),
            ratio: p.comp_ratio(),
            makeup_db: p.makeup_db(),
            eq: p.eq.load(Relaxed),
            eq_db: p.eq_db(),
            denoise_strength: p.denoise_strength(),
        }
    }
    pub fn apply(&self, p: &Params) {
        p.denoise.store(self.denoise, Relaxed);
        p.highpass.store(self.highpass, Relaxed);
        p.gate.store(self.gate, Relaxed);
        p.deesser.store(self.deesser, Relaxed);
        p.presence.store(self.presence, Relaxed);
        p.compressor.store(self.compressor, Relaxed);
        p.auto_gain.store(self.auto_gain, Relaxed);
        p.set_gate_db(self.gate_db);
        p.set_comp_threshold_db(self.threshold_db);
        p.set_comp_ratio(self.ratio);
        p.set_makeup_db(self.makeup_db);
        p.set_denoise_strength(self.denoise_strength);
        p.eq.store(self.eq, Relaxed);
        for (band, db) in self.eq_db.into_iter().enumerate() {
            p.set_eq_db(band, db);
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        for (value, min, max) in [
            (self.gate_db, -80.0, -20.0),
            (self.threshold_db, -50.0, 0.0),
            (self.ratio, 1.0, 10.0),
            (self.makeup_db, 0.0, 24.0),
            (self.eq_db[0], -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
            (self.eq_db[1], -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
            (self.eq_db[2], -crate::dsp::EQ_MAX_DB, crate::dsp::EQ_MAX_DB),
            (self.denoise_strength, 0.0, 100.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(crate::i18n::t!("daemon.profile.out_of_range").into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub config: Config,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    version: u32,
    pub selected: String,
    pub custom: Vec<Custom>,
}
impl Library {
    pub fn new(preset: Preset) -> Self {
        Self {
            version: 1,
            selected: preset.key().into(),
            custom: Vec::new(),
        }
    }
    pub fn config(&self) -> Result<Config, String> {
        if let Some(preset) = Preset::from_key(&self.selected) {
            let p = Params::default();
            p.apply_preset(preset);
            return Ok(Config::capture(&p));
        }
        self.custom
            .iter()
            .find(|p| p.id == self.selected)
            .map(|p| p.config.clone())
            .ok_or_else(|| crate::i18n::t!("daemon.profile.not_found").into())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.custom.len() > 32 {
            return Err(crate::i18n::t!("daemon.profile.incompatible").into());
        }
        let mut ids = std::collections::HashSet::new();
        for profile in &self.custom {
            if !profile.id.starts_with("custom-")
                || profile.id.len() > 80
                || !ids.insert(&profile.id)
            {
                return Err(crate::i18n::t!("daemon.profile.invalid_id").into());
            }
            self.name(&profile.name, Some(&profile.id))?;
            profile.config.validate()?;
        }
        self.config()?;
        Ok(())
    }
    pub(crate) fn name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
            return Err(crate::i18n::t!("daemon.profile.name_rules").into());
        }
        let key = name.to_lowercase();
        // Vale o nome de fábrica em qualquer idioma: trocar o idioma não cria conflito.
        if Preset::ALL
            .iter()
            .any(|p| crate::i18n::desktop_all(p.name_key()).any(|n| n.to_lowercase() == key))
            || self
                .custom
                .iter()
                .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == key)
        {
            return Err(crate::i18n::t!("daemon.profile.name_taken").into());
        }
        Ok(name.into())
    }
    pub fn create(&mut self, name: &str, config: Config) -> Result<(), String> {
        let name = self.name(name, None)?;
        config.validate()?;
        if self.custom.len() >= 32 {
            return Err(crate::i18n::t!("daemon.profile.limit").into());
        }
        let id = format!("custom-{}", crate::state::random_hex(16));
        self.custom.push(Custom {
            id: id.clone(),
            name,
            config,
        });
        self.selected = id;
        Ok(())
    }
    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = self.name(name, Some(id))?;
        self.edit(id)?.name = name;
        Ok(())
    }
    pub fn update(&mut self, id: &str, config: Config) -> Result<(), String> {
        config.validate()?;
        self.edit(id)?.config = config;
        Ok(())
    }
    fn edit(&mut self, id: &str) -> Result<&mut Custom, String> {
        self.custom
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| crate::i18n::t!("daemon.profile.factory_protected").into())
    }
    pub fn select(&mut self, id: &str) -> Result<(), String> {
        if Preset::from_key(id).is_none() && !self.custom.iter().any(|p| p.id == id) {
            return Err(crate::i18n::t!("daemon.profile.not_found").into());
        }
        self.selected = id.into();
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.edit(id)?;
        self.custom.retain(|p| p.id != id);
        if self.selected == id {
            self.selected = Preset::Chamada.key().into();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_saved_before_new_fields_load_with_safe_defaults() {
        // Perfil gravado antes do EQ e da força da redução de ruído.
        let old = r#"{"denoise":true,"highpass":true,"gate":true,"deesser":true,
            "presence":true,"compressor":true,"auto_gain":true,"gate_db":-50.0,
            "threshold_db":-20.0,"ratio":4.0,"makeup_db":0.0}"#;
        let c: Config = serde_json::from_str(old).unwrap();
        assert!(!c.eq);
        assert_eq!(c.eq_db, [0.0; 3]);
        assert_eq!(c.denoise_strength, 100.0);
        assert!(c.validate().is_ok());
    }
    use crate::dsp::{Params, Preset};
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn snapshots_restore_every_audio_control_without_touching_mute_or_meters() {
        let params = Params::default();
        for preset in Preset::ALL {
            params.apply_preset(preset);
            params.set_gate_db(-63.0);
            params.set_comp_threshold_db(-31.0);
            params.set_comp_ratio(3.5);
            params.set_denoise_strength(55.0);
            params.set_makeup_db(13.0);
            let snapshot = Config::capture(&params);
            params.apply_preset(Preset::Natural);
            params.muted.store(true, Relaxed);
            params.in_level.store(0.125f32.to_bits(), Relaxed);
            snapshot.apply(&params);
            assert_eq!(Config::capture(&params), snapshot);
            assert!(params.muted.load(Relaxed));
            assert_eq!(params.in_level(), 0.125);
        }
    }

    #[test]
    fn custom_profiles_create_rename_update_select_and_delete_without_changing_factory() {
        let mut library = Library::new(Preset::Podcast);
        let factory = library.config().unwrap();
        let params = Params::default();
        params.apply_preset(Preset::Streaming);
        let config = Config::capture(&params);
        library.create("  Minha voz  ", config.clone()).unwrap();
        let id = library.selected.clone();
        assert_eq!(library.custom[0].name, "Minha voz");
        assert_eq!(library.config().unwrap(), config);
        library.rename(&id, "Reunião").unwrap();
        library.update(&id, factory.clone()).unwrap();
        library.select("podcast").unwrap();
        assert_eq!(library.config().unwrap(), factory);
        assert!(library.update("podcast", config.clone()).is_err());
        assert!(library.rename("podcast", "Outro").is_err());
        assert!(library.delete("podcast").is_err());
        library.select(&id).unwrap();
        assert_eq!(library.config().unwrap(), factory);
        library.delete(&id).unwrap();
        assert_eq!(library.selected, "chamada");
        assert!(library.custom.is_empty());
        assert!(library.select(&id).is_err());
    }

    #[test]
    fn invalid_names_values_and_versions_are_rejected_without_mutation() {
        let mut library = Library::new(Preset::Chamada);
        let config = library.config().unwrap();
        for name in ["", "  ", "NATURAL", "voz\nruim", &"a".repeat(61)] {
            assert!(library.create(name, config.clone()).is_err(), "{name:?}");
        }
        library.create("Minha voz", config.clone()).unwrap();
        let before = library.clone();
        assert!(library.create("minha VOZ", config.clone()).is_err());
        assert!(
            library
                .rename(&library.selected.clone(), "Podcast")
                .is_err()
        );
        let mut invalid = config.clone();
        invalid.gate_db = f32::NAN;
        assert!(library.update(&library.selected.clone(), invalid).is_err());
        assert_eq!(library, before);
        let mut value = serde_json::to_value(&library).unwrap();
        value["version"] = 999.into();
        assert!(
            serde_json::from_value::<Library>(value)
                .unwrap()
                .validate()
                .is_err()
        );
        for (field, bad) in [
            ("gate_db", -100.0),
            ("threshold_db", 1.0),
            ("ratio", 0.0),
            ("makeup_db", 25.0),
        ] {
            let mut value = serde_json::to_value(&config).unwrap();
            value[field] = bad.into();
            assert!(
                serde_json::from_value::<Config>(value)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
}
