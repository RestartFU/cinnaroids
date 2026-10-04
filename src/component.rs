//! Registers a personal component for the installed client's bounded WASM loader.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
const LIVE_ATTACHMENT_MARKER: &str = "local-mod.status.json";
const REGISTRATION_BYTES: usize = 16 * 1024;
static WRITE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct ModComponent {
    path: PathBuf,
    font_path: PathBuf,
}

#[derive(Clone)]
pub struct AttachRequest {
    pub id: String,
    pub executable: PathBuf,
    pub client_pid: Option<u32>,
    status_path: PathBuf,
    created: Instant,
}

#[derive(serde::Deserialize)]
struct HostStatus {
    version: u32,
    request_id: String,
    client_pid: u32,
    state: String,
    #[serde(default)]
    message: Option<String>,
}

impl AttachRequest {
    pub fn status(&self) -> Result<Option<String>, String> {
        let file = match File::open(&self.status_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return self.waiting_status();
            }
            Err(error) => return Err(format!("Could not read module status: {error}")),
        };
        let mut bytes = Vec::new();
        file.take((REGISTRATION_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read module status: {error}"))?;
        if bytes.len() > REGISTRATION_BYTES {
            return Err("Module status exceeds the size limit.".into());
        }
        let status: HostStatus = match serde_json::from_slice(&bytes) {
            Ok(status) => status,
            Err(_) => return self.waiting_status(),
        };
        if status.version != 1 || status.request_id != self.id {
            return self.waiting_status();
        }
        if !crate::client_process::client_is_running(status.client_pid, &self.executable) {
            return Ok(Some("Ready — start Cinnabar".into()));
        }
        match status.state.as_str() {
            "loaded" => Ok(Some("Attached — Right Shift opens modules".into())),
            "disabled" => Ok(Some("Modules disabled".into())),
            "error" => Err(status
                .message
                .filter(|message| message.len() <= 1024)
                .unwrap_or_else(|| "Cinnabar could not load the module.".into())),
            _ => Ok(None),
        }
    }
}

impl AttachRequest {
    fn waiting_status(&self) -> Result<Option<String>, String> {
        Ok(Some(
            if crate::client_process::running_client(&self.executable)?.is_some() {
                if self.created.elapsed() >= Duration::from_secs(30) {
                    "No module acknowledgment — restart Cinnabar once".into()
                } else {
                    "Waiting for Cinnabar to load modules".into()
                }
            } else {
                "Ready — start Cinnabar".into()
            },
        ))
    }
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

    pub fn attach(
        &self,
        executable: &Path,
        start_if_absent: bool,
    ) -> Result<AttachRequest, String> {
        let _start_lock = crate::client_process::ClientStartLock::acquire()?;
        let mut file =
            File::open(executable).map_err(|error| format!("Could not read Cinnabar: {error}"))?;
        if !supports_modules(&mut file)
            .map_err(|error| format!("Could not read Cinnabar: {error}"))?
        {
            return Err(
                "Your installed Cinnabar needs the live-module update. Restart it once after updating.".into(),
            );
        }
        let client_pid = crate::client_process::running_client(executable)?;
        let base = std::env::var_os("LOCALAPPDATA").ok_or("Settings folder unavailable.")?;
        let registration_path = PathBuf::from(base).join("Cinnabar/local-mod.json");
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            WRITE_ID.fetch_add(1, Ordering::Relaxed)
        );
        let bytes = registration(&id, &self.path, &self.font_path)?;
        write_atomic(&registration_path, &bytes, false)
            .map_err(|error| format!("Could not register Cinnaroids: {error}"))?;
        let mut request = AttachRequest {
            id,
            executable: executable.into(),
            client_pid,
            status_path: registration_path.with_file_name(LIVE_ATTACHMENT_MARKER),
            created: Instant::now(),
        };
        if client_pid.is_some() || !start_if_absent {
            return Ok(request);
        }
        let logs = self.path.parent().unwrap().parent().unwrap().join("logs");
        fs::create_dir_all(&logs)
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let log = File::create(logs.join("cinnabar.log"))
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let errors = log
            .try_clone()
            .map_err(|error| format!("Could not open launch log: {error}"))?;
        let mut command = launch_command(executable)?;
        if let Some(pid) = crate::client_process::running_client(executable)? {
            request.client_pid = Some(pid);
            return Ok(request);
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errors))
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| format!("Could not start Cinnabar: {error}"))?;
        request.client_pid = Some(child.id());
        Ok(request)
    }
}

fn registration(id: &str, component: &Path, font: &Path) -> Result<Vec<u8>, String> {
    if id.is_empty() || id.len() > 64 || !component.is_absolute() || !font.is_absolute() {
        return Err("Invalid local module registration.".into());
    }
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1, "request_id": id, "enabled": true,
        "component": component, "font": font,
        "grants": {"environment": false, "players": true, "camera": true,
            "controls": true, "interaction": true, "settings": true}
    }))
    .map_err(|error| format!("Could not encode module registration: {error}"))?;
    if bytes.len() > REGISTRATION_BYTES {
        return Err("Module registration exceeds the size limit.".into());
    }
    Ok(bytes)
}

fn launch_command(executable: &Path) -> Result<Command, String> {
    let directory = executable.parent().ok_or("Cinnabar folder unavailable.")?;
    let mut command = Command::new(executable);
    command
        .current_dir(directory)
        .env_remove("CINNABAR_MOD_COMPONENT")
        .env_remove("CINNABAR_MOD_FONT");
    for grant in GRANTS {
        command.env_remove(grant);
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
    let signatures: Vec<_> = GRANTS.into_iter().chain([LIVE_ATTACHMENT_MARKER]).collect();
    let mut found = vec![false; signatures.len()];
    let mut buffer = vec![0u8; 64 * 1024 + 32];
    let mut retained = 0;
    loop {
        let count = reader.read(&mut buffer[retained..])?;
        if count == 0 {
            return Ok(found.iter().all(|value| *value));
        }
        let end = retained + count;
        for (index, signature) in signatures.iter().enumerate() {
            found[index] |= buffer[..end]
                .windows(signature.len())
                .any(|bytes| bytes == signature.as_bytes());
        }
        if found.iter().all(|value| *value) {
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
        let bytes = GRANTS
            .into_iter()
            .chain([LIVE_ATTACHMENT_MARKER])
            .collect::<Vec<_>>()
            .join("\0")
            .into_bytes();
        assert!(supports_modules(&mut Chunks(std::io::Cursor::new(bytes))).unwrap());
        let older = GRANTS[..2].join("\0").into_bytes();
        assert!(!supports_modules(&mut std::io::Cursor::new(older)).unwrap());
        assert!(!supports_modules(&mut std::io::Cursor::new(GRANTS.join("\0"))).unwrap());
    }

    #[test]
    fn installed_client_keeps_its_resources_and_uses_the_registered_loader() {
        let directory = scratch();
        let executable = directory.join("bedrock-client.exe");
        let assets = directory.join("assets/compiled/vanilla-v2193.mcbea");
        fs::create_dir_all(assets.parent().unwrap()).unwrap();
        fs::write(&assets, b"carrier").unwrap();
        let command = launch_command(&executable).unwrap();
        assert_eq!(command.get_current_dir(), Some(directory.as_path()));
        assert_eq!(command.get_args().count(), 0);
        let environment: std::collections::HashMap<_, _> = command
            .get_envs()
            .map(|(key, value)| (key.to_os_string(), value.map(|value| value.to_os_string())))
            .collect();
        assert_eq!(
            environment.get(std::ffi::OsStr::new("CINNABAR_MOD_COMPONENT")),
            Some(&None)
        );
        for grant in GRANTS {
            assert_eq!(environment.get(std::ffi::OsStr::new(grant)), Some(&None));
        }
        assert_eq!(
            environment.get(std::ffi::OsStr::new("CINNABAR_MOD_FONT")),
            Some(&None)
        );
        assert_eq!(environment.len(), GRANTS.len() + 2);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn registration_is_explicit_bounded_and_does_not_enable_gameplay_modules() {
        let component = Path::new("C:/Users/Test/Cinnaroids/mod.wasm");
        let font = Path::new("C:/Users/Test/Cinnaroids/font.ttf");
        let bytes = registration("request-1", component, font).unwrap();
        assert!(bytes.len() <= REGISTRATION_BYTES);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["request_id"], "request-1");
        assert_eq!(value["grants"]["controls"], true);
        assert_eq!(value["grants"]["environment"], false);
        assert!(value.get("clicker_enabled").is_none());
        assert!(registration("", component, font).is_err());
        assert!(registration(&"x".repeat(65), component, font).is_err());
        assert!(registration("request-2", Path::new("relative.wasm"), font).is_err());
        let oversized = PathBuf::from(format!("C:/{}", "x".repeat(REGISTRATION_BYTES)));
        assert!(registration("request-3", &oversized, font).is_err());
    }

    #[test]
    fn vanished_or_replaced_acknowledgment_cannot_leave_a_dead_client_attached() {
        let directory = scratch();
        let executable = directory.join("bedrock-client.exe");
        fs::write(&executable, b"fixture").unwrap();
        let request = AttachRequest {
            id: "current-request".into(),
            executable,
            client_pid: None,
            status_path: directory.join("local-mod.status.json"),
            created: Instant::now() - Duration::from_secs(31),
        };
        assert_eq!(
            request.status().unwrap().as_deref(),
            Some("Ready — start Cinnabar")
        );
        fs::write(
            &request.status_path,
            br#"{"version":1,"request_id":"another-request","client_pid":0,"state":"loaded"}"#,
        )
        .unwrap();
        assert_eq!(
            request.status().unwrap().as_deref(),
            Some("Ready — start Cinnabar")
        );
        fs::write(&request.status_path, b"invalid").unwrap();
        assert_eq!(
            request.status().unwrap().as_deref(),
            Some("Ready — start Cinnabar")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
