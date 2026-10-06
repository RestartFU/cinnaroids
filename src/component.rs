//! Registers a personal component for the installed client's bounded WASM loader.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const COMPONENT: &[u8] = include_bytes!("../assets/cinnaroids.component.wasm");
pub const UNSUPPORTED_CLIENT: &str = "This Cinnabar build does not support Cinnaroids modules. A module-enabled Cinnabar build is required.";
const GRANTS: [&str; 9] = [
    "CINNABAR_MOD_PLAYERS",
    "CINNABAR_MOD_CAMERA",
    "CINNABAR_MOD_CONTROLS",
    "CINNABAR_MOD_INTERACTION",
    "CINNABAR_MOD_SETTINGS",
    "CINNABAR_MOD_PACKET_DELAY",
    "CINNABAR_MOD_PACKET_DELAY_VISUAL",
    "CINNABAR_MOD_BLOCK_HIGHLIGHTS",
    "CINNABAR_MOD_FULLBRIGHT",
];
const LIVE_ATTACHMENT_MARKER: &str = "local-mod.status.json";
const REGISTRATION_BYTES: usize = 16 * 1024;
static WRITE_ID: AtomicU64 = AtomicU64::new(0);
static EXITING: AtomicBool = AtomicBool::new(false);
static OWNED_REGISTRATION: Mutex<Option<(PathBuf, PathBuf, String)>> = Mutex::new(None);

/// Retires only this launcher's selected request before GPUI exits.
pub fn disable_on_exit() -> Result<(), String> {
    EXITING.store(true, Ordering::Release);
    let _lock = crate::client_process::RegistrationLock::acquire()?;
    let owned = OWNED_REGISTRATION
        .lock()
        .map_err(|_| "Registration ownership unavailable")?;
    if let Some((path, component, id)) = owned.as_ref() {
        disable_owned(path, component, id)?;
    }
    Ok(())
}

fn disable_owned(path: &Path, component: &Path, id: &str) -> Result<(), String> {
    let Some(bytes) = read_bounded_file(path, "module registration")? else {
        return Ok(());
    };
    if bytes.len() > REGISTRATION_BYTES {
        return Err("Module registration exceeds the size limit.".into());
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if value["request_id"].as_str() != Some(id)
        || value["component"].as_str().map(Path::new) != Some(component)
        || value["enabled"] != true
    {
        return Ok(());
    }
    value["enabled"] = false.into();
    let bytes = serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?;
    write_atomic(path, &bytes, false).map_err(|error| format!("Could not disable modules: {error}"))
}

#[derive(Clone)]
pub struct ModComponent {
    path: PathBuf,
    assets_changed: Arc<AtomicBool>,
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
            return Ok(Some("Waiting for Cinnabar".into()));
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
                "Waiting for Cinnabar".into()
            },
        ))
    }
}

impl ModComponent {
    pub fn new() -> Result<Self, String> {
        let base = crate::settings::data_directory().ok_or("Settings folder unavailable.")?;
        let path = base.join("Cinnaroids/mods/cinnaroids.component.wasm");
        let assets_changed = install_component(&path, COMPONENT)?;
        migrate_preferences(
            &path.with_extension("settings.json"),
            &base.join("CinnabarClicker/settings.json"),
        )
        .map_err(|error| format!("Could not migrate module preferences: {error}"))?;
        Ok(Self {
            path,
            assets_changed: Arc::new(AtomicBool::new(assets_changed)),
        })
    }

    pub fn attach(&self, executable: &Path) -> Result<AttachRequest, String> {
        if crate::settings::installed_client_path().as_deref() != Some(executable) {
            return Err("Cinnaroids requires the standard installed Cinnabar client.".into());
        }
        let _registration_lock = crate::client_process::RegistrationLock::acquire()?;
        if EXITING.load(Ordering::Acquire) {
            return Err("Cinnaroids is closing.".into());
        }
        let mut file =
            File::open(executable).map_err(|error| format!("Could not read Cinnabar: {error}"))?;
        if !supports_modules(&mut file)
            .map_err(|error| format!("Could not read Cinnabar: {error}"))?
        {
            return Err(UNSUPPORTED_CLIENT.into());
        }
        let client_pid = crate::client_process::running_client(executable)?;
        let registration_path =
            crate::settings::registration_path().ok_or("Settings folder unavailable.")?;
        let status_path = registration_path.with_file_name(LIVE_ATTACHMENT_MARKER);
        let existing = read_bounded_file(&registration_path, "module registration")?;
        let acknowledged = read_bounded_file(&status_path, "module status")?
            .filter(|bytes| bytes.len() <= REGISTRATION_BYTES)
            .and_then(|bytes| serde_json::from_slice::<HostStatus>(&bytes).ok())
            .filter(|status| {
                status.version == 1
                    && crate::client_process::client_is_running(status.client_pid, executable)
            });
        let reusable = existing
            .as_deref()
            .and_then(|bytes| {
                matching_registration_id(
                    bytes,
                    &self.path,
                    self.assets_changed.load(Ordering::Relaxed),
                )
            })
            .filter(|id| !acknowledgment_requires_refresh(id, acknowledged.as_ref()));
        let id = match reusable {
            Some(id) => id,
            None => {
                let id = format!(
                    "{}-{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    WRITE_ID.fetch_add(1, Ordering::Relaxed)
                );
                let bytes = registration(&id, &self.path)?;
                write_atomic(&registration_path, &bytes, false)
                    .map_err(|error| format!("Could not register Cinnaroids: {error}"))?;
                id
            }
        };
        self.assets_changed.store(false, Ordering::Relaxed);
        *OWNED_REGISTRATION
            .lock()
            .map_err(|_| "Registration ownership unavailable")? =
            Some((registration_path, self.path.clone(), id.clone()));
        Ok(AttachRequest {
            id,
            executable: executable.into(),
            client_pid,
            status_path,
            created: Instant::now(),
        })
    }
}

fn read_bounded_file(path: &Path, description: &str) -> Result<Option<Vec<u8>>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read {description}: {error}")),
    };
    let mut bytes = Vec::new();
    file.take((REGISTRATION_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {description}: {error}"))?;
    Ok(Some(bytes))
}

fn acknowledgment_requires_refresh(id: &str, acknowledged: Option<&HostStatus>) -> bool {
    acknowledged.is_some_and(|status| {
        // A failed request must change identity to leave the host's quarantine.
        // Reconcile a live loaded request that disagrees with the file on disk.
        (status.request_id == id && status.state == "error")
            || (status.request_id != id && status.state == "loaded")
    })
}

fn matching_registration_id(
    bytes: &[u8],
    component: &Path,
    assets_changed: bool,
) -> Option<String> {
    // Reopening an unchanged launcher must not reset an already-loaded module.
    // Replaced assets require a fresh request so an old acknowledgment cannot satisfy it.
    if assets_changed || bytes.len() > REGISTRATION_BYTES {
        return None;
    }
    let current: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let id = current.get("request_id")?.as_str()?;
    let expected: serde_json::Value =
        serde_json::from_slice(&registration(id, component).ok()?).ok()?;
    (current == expected).then(|| id.to_owned())
}

fn registration(id: &str, component: &Path) -> Result<Vec<u8>, String> {
    if id.is_empty()
        || id.len() > 64
        || id.chars().any(char::is_control)
        || !component.is_absolute()
    {
        return Err("Invalid local module registration.".into());
    }
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1, "request_id": id, "enabled": true,
        "component": component, "font": null,
        "grants": {"environment": false, "players": true, "camera": true,
            "controls": true, "interaction": true, "settings": true, "packet_delay": true,
            "block_highlights": true, "fullbright": true}
    }))
    .map_err(|error| format!("Could not encode module registration: {error}"))?;
    if bytes.len() > REGISTRATION_BYTES {
        return Err("Module registration exceeds the size limit.".into());
    }
    Ok(bytes)
}

fn install_component(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if !bytes.starts_with(b"\0asm\x0d\0\x01\0") {
        return Err("Bundled Cinnaroids component is invalid.".into());
    }
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(false);
    }
    write_atomic(path, bytes, false)
        .map(|()| true)
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
        assert!(install_component(&path, COMPONENT).unwrap());
        assert!(!install_component(&path, COMPONENT).unwrap());
        drop(ModComponent {
            path: path.clone(),
            assets_changed: Arc::new(AtomicBool::new(false)),
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
    fn matching_instances_reuse_the_request_but_replaced_assets_require_a_fresh_ack() {
        let component_path = std::env::temp_dir().join("Cinnaroids/mod.wasm");
        let component = component_path.as_path();
        let bytes = registration("first-instance", component).unwrap();
        assert_eq!(
            matching_registration_id(&bytes, component, false).as_deref(),
            Some("first-instance")
        );
        assert!(matching_registration_id(&bytes, component, true).is_none());
        for (field, value) in [
            ("version", serde_json::json!(2)),
            ("enabled", serde_json::json!(false)),
            ("component", serde_json::json!("C:/Other/mod.wasm")),
            ("font", serde_json::json!("C:/Other/font.ttf")),
            ("grants", serde_json::json!({"controls": true})),
            ("request_id", serde_json::json!("invalid\nid")),
        ] {
            let mut changed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            changed[field] = value;
            assert!(
                matching_registration_id(&serde_json::to_vec(&changed).unwrap(), component, false)
                    .is_none(),
                "accepted changed {field}"
            );
        }
        assert!(
            matching_registration_id(&vec![b' '; REGISTRATION_BYTES + 1], component, false)
                .is_none()
        );
    }

    #[test]
    fn registration_is_explicit_bounded_and_does_not_enable_gameplay_modules() {
        let component_path = std::env::temp_dir().join("Cinnaroids/mod.wasm");
        let component = component_path.as_path();
        let bytes = registration("request-1", component).unwrap();
        assert!(bytes.len() <= REGISTRATION_BYTES);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["request_id"], "request-1");
        assert_eq!(value["grants"]["controls"], true);
        assert_eq!(value["grants"]["block_highlights"], true);
        assert_eq!(value["grants"]["fullbright"], true);
        assert_eq!(value["grants"]["environment"], false);
        assert!(value["font"].is_null());
        assert!(value.get("clicker_enabled").is_none());
        assert!(registration("", component).is_err());
        assert!(registration(&"x".repeat(65), component).is_err());
        assert!(registration("request-2", Path::new("relative.wasm")).is_err());
        let oversized = std::env::temp_dir().join("x".repeat(REGISTRATION_BYTES));
        assert!(registration("request-3", &oversized).is_err());
    }

    #[test]
    fn closing_disables_owned_registration_without_removing_preferences() {
        let directory = scratch();
        let path = directory.join("local-mod.json");
        let component = directory.join("module.wasm");
        let preferences = component.with_extension("settings.json");
        fs::write(&preferences, b"{\"cps\":24}").unwrap();
        let original = registration("owned", &component).unwrap();
        fs::write(&path, &original).unwrap();
        disable_owned(&path, &component, "owned").unwrap();
        let mut expected: serde_json::Value = serde_json::from_slice(&original).unwrap();
        expected["enabled"] = false.into();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
            expected
        );
        let disabled = fs::read(&path).unwrap();
        disable_owned(&path, &component, "owned").unwrap();
        assert_eq!(fs::read(&path).unwrap(), disabled);
        assert_eq!(fs::read(&preferences).unwrap(), b"{\"cps\":24}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn closing_leaves_newer_or_foreign_registrations_untouched() {
        let directory = scratch();
        let path = directory.join("local-mod.json");
        let component = directory.join("module.wasm");
        disable_owned(&path, &component, "owned").unwrap();
        for (id, registered_component) in [
            ("newer", component.clone()),
            ("owned", directory.join("other.wasm")),
        ] {
            let original = registration(id, &registered_component).unwrap();
            fs::write(&path, &original).unwrap();
            disable_owned(&path, &component, "owned").unwrap();
            assert_eq!(fs::read(&path).unwrap(), original);
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_or_divergent_live_acknowledgments_require_a_new_request() {
        let mut status = HostStatus {
            version: 1,
            request_id: "current-request".into(),
            client_pid: 100,
            state: "loaded".into(),
            message: None,
        };
        assert!(!acknowledgment_requires_refresh("current-request", None));
        assert!(!acknowledgment_requires_refresh(
            "current-request",
            Some(&status)
        ));
        status.state = "error".into();
        assert!(acknowledgment_requires_refresh(
            "current-request",
            Some(&status)
        ));
        // The previous error can remain while the host processes a fresh request.
        assert!(!acknowledgment_requires_refresh(
            "new-request",
            Some(&status)
        ));
        status.state = "loaded".into();
        assert!(acknowledgment_requires_refresh(
            "new-request",
            Some(&status)
        ));
        status.request_id = "new-request".into();
        assert!(!acknowledgment_requires_refresh(
            "new-request",
            Some(&status)
        ));
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
            Some("Waiting for Cinnabar")
        );
        fs::write(
            &request.status_path,
            br#"{"version":1,"request_id":"another-request","client_pid":0,"state":"loaded"}"#,
        )
        .unwrap();
        assert_eq!(
            request.status().unwrap().as_deref(),
            Some("Waiting for Cinnabar")
        );
        fs::write(&request.status_path, b"invalid").unwrap();
        assert_eq!(
            request.status().unwrap().as_deref(),
            Some("Waiting for Cinnabar")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
