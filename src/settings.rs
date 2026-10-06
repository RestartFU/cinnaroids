use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark_mode: bool,
    // Retain legacy preferences without using them for client discovery.
    pub cinnabar_path: Option<PathBuf>,
}

/// Always watch the standard installation, including before its executable exists.
pub fn installed_client_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .filter(|base| !base.is_empty())
            .map(|base| installed_client_in(Path::new(&base)))
    }
    #[cfg(unix)]
    {
        if let Some(path) = std::env::var_os("CINNABAR_EXECUTABLE").filter(|path| !path.is_empty())
        {
            return Some(PathBuf::from(path));
        }
        #[cfg(target_os = "macos")]
        return Some(PathBuf::from(
            "/Applications/Cinnabar.app/Contents/MacOS/bedrock-client",
        ));
        #[cfg(target_os = "linux")]
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/bin/bedrock-client"));
    }
}

#[cfg(any(windows, test))]
fn installed_client_in(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Programs/Cinnabar/bedrock-client.exe")
}

/// Per-user storage shared by preferences, component installation and registration.
pub fn data_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    return std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "linux")]
    return std::env::var_os("XDG_DATA_HOME")
        .filter(|base| !base.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark_mode: true,
            cinnabar_path: None,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        data_directory().map(|base| base.join("Cinnaroids/settings.json"))
    }

    pub fn load() -> (Self, Option<String>) {
        let Some(base) = data_directory() else {
            return (Self::default(), Some("Settings folder unavailable.".into()));
        };
        Self::load_from(
            &base.join("Cinnaroids/settings.json"),
            &base.join("CinnabarClicker/settings.json"),
        )
    }

    fn load_from(path: &Path, legacy: &Path) -> (Self, Option<String>) {
        let (bytes, migrated) = match fs::read(path) {
            Ok(bytes) => (bytes, false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::read(legacy) {
                Ok(bytes) => (bytes, true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return (Self::default(), None);
                }
                Err(_) => {
                    return (
                        Self::default(),
                        Some("Saved settings could not be read.".into()),
                    );
                }
            },
            Err(_) => {
                return (
                    Self::default(),
                    Some("Saved settings could not be read.".into()),
                );
            }
        };
        let settings = match serde_json::from_slice::<Self>(&bytes) {
            Ok(settings) => settings,
            Err(_) => {
                return (
                    Self::default(),
                    Some("Saved settings could not be read. Defaults restored.".into()),
                );
            }
        };
        let error = if migrated {
            settings.save_to(path).err()
        } else {
            None
        };
        (settings, error)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Settings folder unavailable.")?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path.parent().ok_or("Settings folder unavailable.")?)
            .map_err(|_| "Could not create settings folder.")?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "Could not encode settings.")?;
        let temp = path.with_extension(format!("pending-{}", std::process::id()));
        let result = (|| {
            fs::write(&temp, bytes).map_err(|_| "Could not save settings.")?;
            fs::rename(&temp, path).map_err(|_| "Could not replace saved settings.".to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn old_module_fields_do_not_become_launcher_controls() {
        let settings: Settings = serde_json::from_str(r#"{"cps":18,"toggle_key":"F6","aim_strength":70,"dark_mode":false,"cinnabar_path":"C:/Cinnabar/bedrock-client.exe"}"#).unwrap();
        assert!(!settings.dark_mode);
        assert_eq!(
            settings.cinnabar_path,
            Some(PathBuf::from("C:/Cinnabar/bedrock-client.exe"))
        );
        let values = serde_json::to_value(&settings).unwrap();
        assert_eq!(values.as_object().unwrap().len(), 2);
        assert!(Settings::default().dark_mode);
    }

    #[test]
    fn migration_preserves_legacy_file_and_new_settings_take_precedence() {
        let directory = std::env::temp_dir().join(format!(
            "cinnaroids-settings-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("legacy.json");
        let new = directory.join("new/settings.json");
        let old_bytes =
            br#"{"dark_mode":false,"cinnabar_path":"C:/Custom/bedrock-client.exe","cps":20}"#;
        fs::write(&legacy, old_bytes).unwrap();
        let (settings, error) = Settings::load_from(&new, &legacy);
        assert!(error.is_none());
        assert!(!settings.dark_mode);
        assert!(new.is_file());
        assert_eq!(fs::read(&legacy).unwrap(), old_bytes);
        Settings::default().save_to(&new).unwrap();
        let (settings, error) = Settings::load_from(&new, &legacy);
        assert!(error.is_none());
        assert!(settings.dark_mode);
        assert!(settings.cinnabar_path.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_client_paths_do_not_override_the_standard_installation() {
        let local = Path::new("C:/Users/Test/AppData/Local");
        let expected = local.join("Programs/Cinnabar/bedrock-client.exe");
        for saved in [
            "C:/Custom/Cinnabar/bedrock-client.exe",
            "C:/Apps/Cinnaroids/Cinnabar/bedrock-client.exe",
            "C:/Apps/CinnabarClicker/Cinnabar/bedrock-client.exe",
        ] {
            let settings: Settings = serde_json::from_value(serde_json::json!({
                "dark_mode": false,
                "cinnabar_path": saved,
            }))
            .unwrap();
            assert!(!settings.dark_mode);
            assert_eq!(settings.cinnabar_path.as_deref(), Some(Path::new(saved)));
            assert_eq!(installed_client_in(local), expected);
        }
    }

    #[test]
    fn discovery_watches_the_standard_path_before_it_is_installed() {
        let directory = std::env::temp_dir().join(format!(
            "cinnaroids-discovery-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        let installed = directory.join("Programs/Cinnabar/bedrock-client.exe");
        assert!(!installed.exists());
        assert_eq!(installed_client_in(&directory), installed);
    }
}
