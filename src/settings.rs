use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AimMode {
    Continuous,
    #[default]
    WhileClicking,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub cps: u32,
    pub toggle_key: String,
    pub minimize_on_enable: bool,
    pub dark_mode: bool,
    pub aim_strength: u32,
    pub aim_mode: AimMode,
    pub cinnabar_path: Option<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            cps: 12,
            toggle_key: "F8".into(),
            minimize_on_enable: false,
            dark_mode: true,
            aim_strength: 35,
            aim_mode: AimMode::WhileClicking,
            cinnabar_path: None,
        }
    }
}
impl Settings {
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join("CinnabarClicker")
                .join("settings.json")
        })
    }
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), Some("Settings folder unavailable.".into()));
        };
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Self>(&bytes) {
                Ok(mut settings) => {
                    settings.cps = settings.cps.clamp(1, 30);
                    settings.aim_strength = settings.aim_strength.min(100);
                    (settings, None)
                }
                Err(_) => (
                    Self::default(),
                    Some("Saved settings could not be read. Defaults restored.".into()),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(_) => (
                Self::default(),
                Some("Saved settings could not be read.".into()),
            ),
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Settings folder unavailable.")?;
        fs::create_dir_all(path.parent().unwrap())
            .map_err(|_| "Could not create settings folder.")?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "Could not encode settings.")?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|_| "Could not save settings.")?;
        fs::rename(&temp, &path).map_err(|_| "Could not replace saved settings.".to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_settings_preserve_preferences_and_ignore_legacy_accent() {
        let settings: Settings = serde_json::from_str(
            r#"{"cps":18,"toggle_key":"VK:65","minimize_on_enable":true,"purple":true}"#,
        )
        .unwrap();
        assert_eq!(settings.cps, 18);
        assert_eq!(settings.toggle_key, "VK:65");
        assert!(settings.minimize_on_enable);
        assert!(settings.dark_mode);
        assert_eq!(settings.aim_strength, 35);
        assert_eq!(settings.aim_mode, AimMode::WhileClicking);
        assert_eq!(settings.cinnabar_path, None);
        assert!(!serde_json::to_string(&settings).unwrap().contains("purple"));
    }
    #[test]
    fn preferences_round_trip_without_enabled_state() {
        let original = Settings {
            cps: 24,
            toggle_key: "F6".into(),
            minimize_on_enable: true,
            dark_mode: false,
            aim_strength: 70,
            aim_mode: AimMode::Continuous,
            cinnabar_path: Some(PathBuf::from("C:/Cinnabar/bedrock-client.exe")),
        };
        let json = serde_json::to_string(&original).unwrap();
        let values: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(values.get("enabled").is_none());
        assert!(values.get("aim_enabled").is_none());
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.cps, 24);
        assert_eq!(restored.toggle_key, "F6");
        assert!(restored.minimize_on_enable);
        assert!(!restored.dark_mode);
        assert_eq!(restored.aim_strength, 70);
        assert_eq!(restored.aim_mode, AimMode::Continuous);
        assert_eq!(restored.cinnabar_path, original.cinnabar_path);
        assert_eq!(values["aim_mode"], "continuous");
    }

    #[test]
    fn aim_preferences_default_to_clicking_and_serialize_with_snake_case() {
        let settings = Settings::default();
        assert_eq!(settings.aim_strength, 35);
        assert_eq!(settings.aim_mode, AimMode::WhileClicking);
        assert_eq!(settings.cinnabar_path, None);
        let values = serde_json::to_value(settings).unwrap();
        assert_eq!(values["aim_mode"], "while_clicking");
        assert!(values.get("aim_enabled").is_none());
    }
}
