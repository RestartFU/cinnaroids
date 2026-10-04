use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub cps: u32,
    pub toggle_key: String,
    pub minimize_on_enable: bool,
    pub dark_mode: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            cps: 12,
            toggle_key: "F8".into(),
            minimize_on_enable: false,
            dark_mode: true,
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
        assert!(!serde_json::to_string(&settings).unwrap().contains("purple"));
    }
    #[test]
    fn preferences_round_trip_without_enabled_state() {
        let original = Settings {
            cps: 24,
            toggle_key: "F6".into(),
            minimize_on_enable: true,
            dark_mode: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(!json.contains("enabled"));
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.cps, 24);
        assert_eq!(restored.toggle_key, "F6");
        assert!(restored.minimize_on_enable);
        assert!(!restored.dark_mode);
    }
}
