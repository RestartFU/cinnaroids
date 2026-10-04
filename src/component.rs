//! Installs the personal component and launches its explicitly authorized host.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

const COMPONENT: &[u8] = include_bytes!("../assets/cinnaroids.component.wasm");
const PANEL_FONT: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
const FONT_LICENSE: &[u8] = include_bytes!("../assets/fonts/Inter-OFL-1.1.txt");
const GRANTS: [&str; 5] = [
    "CINNABAR_MOD_PLAYERS",
    "CINNABAR_MOD_CAMERA",
    "CINNABAR_MOD_CONTROLS",
    "CINNABAR_MOD_INTERACTION",
    "CINNABAR_MOD_SETTINGS",
];
static WRITE_ID: AtomicU64 = AtomicU64::new(0);

pub struct ModComponent {
    path: PathBuf,
    font_path: PathBuf,
}

impl ModComponent {
    pub fn new() -> Result<Self, String> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or("Settings folder unavailable.")?;
        let base = PathBuf::from(base);
        let path = base.join("Cinnaroids/mods/cinnaroids.component.wasm");
        install_component(&path, COMPONENT)?;
        let font_path = base.join("Cinnaroids/fonts/Inter-Medium.ttf");
        for (path, bytes) in [
            (&font_path, PANEL_FONT),
            (&font_path.with_file_name("Inter-OFL-1.1.txt"), FONT_LICENSE),
        ] {
            if !fs::read(path).is_ok_and(|current| current == bytes) {
                write_atomic(path, bytes, false)
                    .map_err(|error| format!("Could not install panel font: {error}"))?;
            }
        }
        migrate_preferences(
            &path.with_extension("settings.json"),
            &base.join("CinnabarClicker/settings.json"),
        )
        .map_err(|error| format!("Could not migrate module preferences: {error}"))?;
        Ok(Self { path, font_path })
    }

    pub fn launch(&self, executable: &Path) -> Result<(), String> {
        let mut file =
            File::open(executable).map_err(|error| format!("Could not read Cinnabar: {error}"))?;
        if !supports_modules(&mut file)
            .map_err(|error| format!("Could not read Cinnabar: {error}"))?
        {
            return Err(
                "Choose the bundled Cinnabar client with Cinnaroids module support.".into(),
            );
        }
        let logs = self.path.parent().unwrap().parent().unwrap().join("logs");
        fs::create_dir_all(&logs)
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let log = File::create(logs.join("cinnabar.log"))
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let errors = log
            .try_clone()
            .map_err(|error| format!("Could not open launch log: {error}"))?;
        let mut command = launch_command(&self.path, &self.font_path, executable)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errors))
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| format!("Could not start Cinnabar: {error}"))?;
        Ok(())
    }
}

fn launch_command(component: &Path, font: &Path, executable: &Path) -> Result<Command, String> {
    let directory = executable.parent().ok_or("Cinnabar folder unavailable.")?;
    let mut command = Command::new(executable);
    let assets = directory.join("assets/compiled/vanilla-v2193.mcbea");
    if assets.is_file() {
        command.arg("--assets").arg(assets);
    }
    command
        .current_dir(directory)
        .env("CINNABAR_MOD_COMPONENT", component)
        .env("CINNABAR_MOD_FONT", font);
    for grant in GRANTS {
        command.env(grant, "1");
    }
    Ok(command)
}

fn install_component(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if !bytes.starts_with(b"\0asm\x0d\0\x01\0") {
        return Err("Bundled Cinnaroids component is invalid.".into());
    }
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    write_atomic(path, bytes, false)
        .map_err(|error| format!("Could not install Cinnaroids: {error}"))
}

fn migrate_preferences(destination: &Path, legacy: &Path) -> std::io::Result<()> {
    if destination.exists() {
        return Ok(());
    }
    let bytes = match fs::read(legacy) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let Some(preferences) = migrated_preferences(&bytes) else {
        return Ok(());
    };
    write_atomic(destination, &preferences, true)
}

fn migrated_preferences(bytes: &[u8]) -> Option<Vec<u8>> {
    let values: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let values = values.as_object()?;
    let mut preferences = serde_json::Map::new();
    for key in ["cps", "toggle_key", "aim_strength", "aim_mode", "dark_mode"] {
        if let Some(value) = values.get(key) {
            preferences.insert(key.into(), value.clone());
        }
    }
    serde_json::to_vec_pretty(&preferences).ok()
}

fn write_atomic(path: &Path, bytes: &[u8], only_if_absent: bool) -> std::io::Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Missing parent folder"))?,
    )?;
    let id = WRITE_ID.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("pending-{}-{id}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if only_if_absent {
            match fs::hard_link(&temp, path) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                result => result,
            }
        } else {
            fs::rename(&temp, path)
        }
    })();
    let _ = fs::remove_file(temp);
    result
}

/// Refuse older and default-feature clients before creating an unsupported session.
fn supports_modules(reader: &mut impl Read) -> std::io::Result<bool> {
    let mut found = [false; GRANTS.len()];
    let mut buffer = vec![0u8; 64 * 1024 + 32];
    let mut retained = 0;
    loop {
        let count = reader.read(&mut buffer[retained..])?;
        if count == 0 {
            return Ok(found.into_iter().all(|value| value));
        }
        let end = retained + count;
        for (index, signature) in GRANTS.iter().enumerate() {
            found[index] |= buffer[..end]
                .windows(signature.len())
                .any(|bytes| bytes == signature.as_bytes());
        }
        if found.into_iter().all(|value| value) {
            return Ok(true);
        }
        retained = end.min(32);
        buffer.copy_within(end - retained..end, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let id = WRITE_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cinnaroids-launcher-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn installation_replaces_module_but_preserves_guest_preferences_after_drop() {
        let directory = scratch();
        let path = directory.join("module.wasm");
        fs::write(&path, b"old component").unwrap();
        let preferences = path.with_extension("settings.json");
        fs::write(&preferences, b"{\"cps\":25}").unwrap();
        install_component(&path, COMPONENT).unwrap();
        drop(ModComponent {
            path: path.clone(),
            font_path: directory.join("font.ttf"),
        });
        assert_eq!(fs::read(&path).unwrap(), COMPONENT);
        assert_eq!(fs::read(preferences).unwrap(), b"{\"cps\":25}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preference_migration_never_replaces_new_settings_or_copies_runtime_switches() {
        let directory = scratch();
        let destination = directory.join("module.settings.json");
        let legacy = directory.join("legacy.json");
        let original = br#"{"cps":18,"toggle_key":"VK:65","aim_strength":70,"aim_mode":"continuous","dark_mode":false,"enabled":true,"aim_enabled":true,"cinnabar_path":"private-path"}"#;
        fs::write(&legacy, original).unwrap();
        migrate_preferences(&destination, &legacy).unwrap();
        let values: serde_json::Value =
            serde_json::from_slice(&fs::read(&destination).unwrap()).unwrap();
        assert_eq!(values["cps"], 18);
        assert_eq!(values["toggle_key"], "VK:65");
        assert_eq!(values["aim_mode"], "continuous");
        assert_eq!(values.as_object().unwrap().len(), 5);
        assert_eq!(fs::read(&legacy).unwrap(), original);
        write_atomic(&destination, b"{\"cps\":30}", false).unwrap();
        migrate_preferences(&destination, &legacy).unwrap();
        write_atomic(&destination, b"should not overwrite", true).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"{\"cps\":30}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_legacy_preferences_use_guest_defaults() {
        assert!(migrated_preferences(b"not json").is_none());
        assert!(migrated_preferences(b"[]").is_none());
    }

    #[test]
    fn capability_check_requires_every_grant_even_across_read_boundaries() {
        struct Chunks(std::io::Cursor<Vec<u8>>);
        impl Read for Chunks {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(7);
                self.0.read(&mut buffer[..count])
            }
        }
        let bytes = GRANTS.join("\0").into_bytes();
        assert!(supports_modules(&mut Chunks(std::io::Cursor::new(bytes))).unwrap());
        let older = GRANTS[..2].join("\0").into_bytes();
        assert!(!supports_modules(&mut std::io::Cursor::new(older)).unwrap());
    }

    #[test]
    fn child_launch_sets_component_grants_and_private_assets_only_for_child() {
        let directory = scratch();
        let executable = directory.join("bedrock-client.exe");
        let component = directory.join("module.wasm");
        let font = directory.join("font.ttf");
        let assets = directory.join("assets/compiled/vanilla-v2193.mcbea");
        fs::create_dir_all(assets.parent().unwrap()).unwrap();
        fs::write(&assets, b"carrier").unwrap();
        let command = launch_command(&component, &font, &executable).unwrap();
        assert_eq!(command.get_current_dir(), Some(directory.as_path()));
        let arguments: Vec<_> = command
            .get_args()
            .map(|argument| argument.to_os_string())
            .collect();
        assert_eq!(
            arguments,
            vec![
                std::ffi::OsString::from("--assets"),
                assets.into_os_string()
            ]
        );
        let environment: std::collections::HashMap<_, _> = command
            .get_envs()
            .map(|(key, value)| (key.to_os_string(), value.unwrap().to_os_string()))
            .collect();
        assert_eq!(
            environment.get(std::ffi::OsStr::new("CINNABAR_MOD_COMPONENT")),
            Some(&component.into_os_string())
        );
        for grant in GRANTS {
            assert_eq!(
                environment.get(std::ffi::OsStr::new(grant)),
                Some(&std::ffi::OsString::from("1"))
            );
        }
        assert_eq!(
            environment.get(std::ffi::OsStr::new("CINNABAR_MOD_FONT")),
            Some(&font.into_os_string())
        );
        assert_eq!(environment.len(), GRANTS.len() + 2);
        fs::remove_dir_all(directory).unwrap();
    }
}
