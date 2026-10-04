use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark_mode: bool,
    pub cinnabar_path: Option<PathBuf>,
}

/// Prefer installed Cinnabar over managed bundles, preserving an explicit custom client.
pub fn select_client(saved: Option<PathBuf>, bundled: Option<PathBuf>) -> Option<PathBuf> {
    let installed = std::env::var_os("LOCALAPPDATA")
        .filter(|base| !base.is_empty())
        .and_then(|base| installed_client_in(Path::new(&base)));
    select_client_with_installed(saved, bundled, installed)
}

fn installed_client_in(local_app_data: &Path) -> Option<PathBuf> {
    let path = local_app_data.join("Programs/Cinnabar/bedrock-client.exe");
    path.is_file().then_some(path)
}

fn select_client_with_installed(
    saved: Option<PathBuf>,
    bundled: Option<PathBuf>,
    installed: Option<PathBuf>,
) -> Option<PathBuf> {
    let managed_bundle = saved.as_ref().is_some_and(|path| {
        path.parent().is_some_and(|directory| {
            directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Cinnabar"))
                && directory
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| managed_package(&name.to_string_lossy()))
        })
    });
    if managed_bundle {
        installed.or(bundled).or(saved)
    } else {
        saved.or(installed).or(bundled)
    }
}

fn managed_package(name: &str) -> bool {
    if name.eq_ignore_ascii_case(crate::PRODUCT_NAME)
        || name.eq_ignore_ascii_case("CinnabarClicker")
    {
        return true;
    }
    let Some((product, version)) = name.rsplit_once('-') else {
        return false;
    };
    product.eq_ignore_ascii_case(crate::PRODUCT_NAME)
        && version.split('.').count() == 3
        && version.split('.').all(|part| part.parse::<u32>().is_ok())
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
        std::env::var_os("LOCALAPPDATA")
            .map(|base| PathBuf::from(base).join("Cinnaroids/settings.json"))
    }

    pub fn load() -> (Self, Option<String>) {
        let Some(base) = std::env::var_os("LOCALAPPDATA") else {
            return (Self::default(), Some("Settings folder unavailable.".into()));
        };
        let base = PathBuf::from(base);
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
    fn updated_package_replaces_a_saved_managed_bundle() {
        let updated = PathBuf::from("C:/Apps/Cinnaroids-2.0.0/Cinnabar/bedrock-client.exe");
        for old in [
            "C:/Apps/Cinnaroids/Cinnabar/bedrock-client.exe",
            "C:/Apps/Cinnaroids-1.9.0/Cinnabar/bedrock-client.exe",
        ] {
            assert_eq!(
                select_client_with_installed(Some(PathBuf::from(old)), Some(updated.clone()), None),
                Some(updated.clone())
            );
        }
        let custom = PathBuf::from("C:/Apps/Cinnaroids-custom/Cinnabar/bedrock-client.exe");
        assert_eq!(
            select_client_with_installed(Some(custom.clone()), Some(updated), None),
            Some(custom)
        );
    }

    #[test]
    fn renamed_bundle_replaces_legacy_bundle_but_keeps_custom_client() {
        let bundle = PathBuf::from("C:/Apps/Cinnaroids/Cinnabar/bedrock-client.exe");
        let legacy = PathBuf::from("C:/Apps/CinnabarClicker/Cinnabar/bedrock-client.exe");
        assert_eq!(
            select_client_with_installed(Some(legacy.clone()), Some(bundle.clone()), None),
            Some(bundle.clone())
        );
        let custom = PathBuf::from("C:/Custom/Cinnabar/bedrock-client.exe");
        assert_eq!(
            select_client_with_installed(Some(custom.clone()), Some(bundle.clone()), None),
            Some(custom)
        );
        let custom = PathBuf::from("C:/Apps/CinnabarClicker/Custom/bedrock-client.exe");
        assert_eq!(
            select_client_with_installed(Some(custom.clone()), Some(bundle.clone()), None),
            Some(custom)
        );
        assert_eq!(
            select_client_with_installed(Some(legacy.clone()), None, None),
            Some(legacy)
        );
        assert_eq!(
            select_client_with_installed(None, Some(bundle.clone()), None),
            Some(bundle)
        );
    }

    #[test]
    fn installed_client_replaces_managed_bundles_but_keeps_an_explicit_custom_client() {
        let installed =
            PathBuf::from("C:/Users/User/AppData/Local/Programs/Cinnabar/bedrock-client.exe");
        let bundled = PathBuf::from("C:/Apps/Cinnaroids-2.0.0/Cinnabar/bedrock-client.exe");
        for saved in [
            None,
            Some(PathBuf::from(
                "C:/Apps/Cinnaroids/Cinnabar/bedrock-client.exe",
            )),
            Some(PathBuf::from(
                "C:/Apps/CinnabarClicker/Cinnabar/bedrock-client.exe",
            )),
        ] {
            assert_eq!(
                select_client_with_installed(saved, Some(bundled.clone()), Some(installed.clone())),
                Some(installed.clone())
            );
        }
        let custom = PathBuf::from("C:/Development/Cinnabar/bedrock-client.exe");
        assert_eq!(
            select_client_with_installed(Some(custom.clone()), Some(bundled), Some(installed)),
            Some(custom)
        );
    }

    #[test]
    fn installed_discovery_requires_the_standard_executable_to_be_a_file() {
        let directory = std::env::temp_dir().join(format!(
            "cinnaroids-discovery-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        let installed = directory.join("Programs/Cinnabar/bedrock-client.exe");
        assert!(installed_client_in(&directory).is_none());
        fs::create_dir_all(&installed).unwrap();
        assert!(installed_client_in(&directory).is_none());
        fs::remove_dir(&installed).unwrap();
        fs::write(&installed, b"test executable").unwrap();
        assert_eq!(installed_client_in(&directory), Some(installed));
        fs::remove_dir_all(directory).unwrap();
    }
}
