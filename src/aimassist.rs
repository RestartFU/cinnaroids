//! Publishes a configured personal component through Cinnabar's existing reload lane.

use crate::settings::AimMode;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

const TEMPLATE: &[u8] = include_bytes!("../assets/aimassist.component.wasm");
const MAGIC: &[u8; 16] = b"CNBR_AIM_CFG_v1!";
static WRITE_ID: AtomicU64 = AtomicU64::new(0);

pub struct Activation {
    pub enabled: bool,
    stop_revision: u64,
}

impl Activation {
    pub fn new(stop_revision: u64) -> Self {
        Self {
            enabled: false,
            stop_revision,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, stop_revision: u64) {
        // An explicit enable supersedes stop events that already happened.
        if enabled {
            self.stop_revision = stop_revision;
        }
        self.enabled = enabled;
    }

    pub fn poll_stop(&mut self, stop_revision: u64, available: bool) -> Option<&'static str> {
        let stopped = stop_revision != self.stop_revision;
        self.stop_revision = stop_revision;
        if !self.enabled {
            return None;
        }
        let reason = if stopped {
            "Stopped by F10."
        } else if !available {
            "Stopped: emergency-stop hook unavailable."
        } else {
            return None;
        };
        self.enabled = false;
        Some(reason)
    }
}

pub struct AimAssist {
    path: PathBuf,
}

impl AimAssist {
    pub fn new() -> Result<Self, String> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or("Settings folder unavailable.")?;
        let path = PathBuf::from(base)
            .join("CinnabarClicker")
            .join("mods")
            .join("aimassist.component.wasm");
        let aim = Self { path };
        aim.publish(false, 35, AimMode::WhileClicking)?;
        Ok(aim)
    }

    pub fn component_path(&self) -> &Path {
        &self.path
    }

    pub fn publish(&self, enabled: bool, strength: u32, mode: AimMode) -> Result<(), String> {
        let bytes = configured_component(TEMPLATE, enabled, strength, mode)?;
        if fs::read(&self.path).is_ok_and(|current| current == bytes) {
            return Ok(());
        }
        write_atomic(&self.path, &bytes)
            .map_err(|error| format!("Could not update aim assist: {error}"))
    }

    pub fn launch(&self, executable: &Path) -> Result<(), String> {
        if !supports_gameplay_mods(executable)? {
            return Err("Choose the bundled Cinnabar client with aim assist support.".into());
        }
        let logs = self.path.parent().unwrap().parent().unwrap().join("logs");
        fs::create_dir_all(&logs)
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let log = File::create(logs.join("cinnabar.log"))
            .map_err(|error| format!("Could not create launch log: {error}"))?;
        let errors = log
            .try_clone()
            .map_err(|error| format!("Could not open launch log: {error}"))?;
        let directory = executable.parent().ok_or("Cinnabar folder unavailable.")?;
        let mut command = Command::new(executable);
        let assets = directory.join("assets/compiled/vanilla-v2193.mcbea");
        if assets.is_file() {
            command.arg("--assets").arg(assets);
        }
        command
            .current_dir(directory)
            .env("CINNABAR_MOD_COMPONENT", self.component_path())
            .env("CINNABAR_MOD_PLAYERS", "1")
            .env("CINNABAR_MOD_CAMERA", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errors))
            .creation_flags(0x0800_0000) // Suppress a console; the game's window remains visible.
            .spawn()
            .map_err(|error| format!("Could not start Cinnabar: {error}"))?;
        Ok(())
    }
}

impl Drop for AimAssist {
    fn drop(&mut self) {
        let _ = self.publish(false, 35, AimMode::WhileClicking);
    }
}

fn configured_component(
    template: &[u8],
    enabled: bool,
    strength: u32,
    mode: AimMode,
) -> Result<Vec<u8>, String> {
    if !template.starts_with(b"\0asm\x0d\0\x01\0") {
        return Err("Bundled aim assist component is invalid.".into());
    }
    let mut matches = template
        .windows(MAGIC.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == MAGIC).then_some(index));
    let offset = matches
        .next()
        .ok_or("Aim assist configuration is missing.")?;
    if matches.next().is_some() || offset + 24 > template.len() {
        return Err("Aim assist configuration is invalid.".into());
    }
    let mut bytes = template.to_vec();
    bytes[offset + 16] = u8::from(enabled);
    bytes[offset + 17] = strength.min(100) as u8;
    bytes[offset + 18] = match mode {
        AimMode::Continuous => 0,
        AimMode::WhileClicking => 1,
    };
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
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
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// Reject default-feature and older clients before launching a session that cannot load the module.
fn supports_gameplay_mods(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|error| format!("Could not read Cinnabar: {error}"))?;
    let signatures: [&[u8]; 2] = [b"CINNABAR_MOD_CAMERA", b"CINNABAR_MOD_PLAYERS"];
    let mut found = [false; 2];
    let mut buffer = vec![0u8; 64 * 1024 + 32];
    let mut retained = 0;
    loop {
        let count = file
            .read(&mut buffer[retained..])
            .map_err(|error| format!("Could not read Cinnabar: {error}"))?;
        if count == 0 {
            return Ok(found.into_iter().all(|value| value));
        }
        let end = retained + count;
        for (index, signature) in signatures.iter().enumerate() {
            found[index] |= buffer[..end]
                .windows(signature.len())
                .any(|bytes| bytes == *signature);
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

    #[test]
    fn enabling_after_a_pending_stop_does_not_disable_again() {
        let mut activation = Activation::new(4);
        activation.set_enabled(true, 5);
        assert_eq!(activation.poll_stop(5, true), None);
        assert!(activation.enabled);
        assert_eq!(activation.poll_stop(6, true), Some("Stopped by F10."));
        assert!(!activation.enabled);
        assert_eq!(activation.poll_stop(6, true), None);
        activation.set_enabled(true, 6);
        assert_eq!(activation.poll_stop(6, true), None);
        assert!(activation.enabled);
    }

    #[test]
    fn losing_the_stop_hook_disarms_and_reports_the_reason() {
        let mut activation = Activation::new(0);
        activation.set_enabled(true, 0);
        assert_eq!(
            activation.poll_stop(0, false),
            Some("Stopped: emergency-stop hook unavailable.")
        );
        assert!(!activation.enabled);
        assert_eq!(activation.poll_stop(0, false), None);
    }

    #[test]
    fn component_patch_preserves_length_and_all_bytes_outside_settings() {
        let mut fixture = b"\0asm\x0d\0\x01\0prefix".to_vec();
        let offset = fixture.len();
        fixture.extend_from_slice(MAGIC);
        fixture.extend_from_slice(&[0, 35, 1, 0, 0, 0, 0, 0]);
        fixture.extend_from_slice(b"suffix");
        let configured = configured_component(&fixture, true, 500, AimMode::Continuous).unwrap();
        assert_eq!(configured.len(), fixture.len());
        assert_eq!(&configured[offset + 16..offset + 19], &[1, 100, 0]);
        assert_eq!(&configured[..offset + 16], &fixture[..offset + 16]);
        assert_eq!(&configured[offset + 19..], &fixture[offset + 19..]);
        let disabled = configured_component(&configured, false, 0, AimMode::WhileClicking).unwrap();
        assert_eq!(&disabled[offset + 16..offset + 19], &[0, 0, 1]);
    }

    #[test]
    fn ambiguous_and_truncated_configurations_are_rejected() {
        let mut fixture = b"\0asm\x0d\0\x01\0".to_vec();
        fixture.extend_from_slice(MAGIC);
        assert!(configured_component(&fixture, true, 35, AimMode::Continuous).is_err());
        fixture.extend_from_slice(&[0; 8]);
        fixture.extend_from_slice(MAGIC);
        fixture.extend_from_slice(&[0; 8]);
        assert!(configured_component(&fixture, true, 35, AimMode::Continuous).is_err());
    }

    #[test]
    fn real_bundle_has_one_config_and_valid_component_header() {
        let patched = configured_component(TEMPLATE, true, 35, AimMode::WhileClicking).unwrap();
        assert_eq!(patched.len(), TEMPLATE.len());
    }

    #[test]
    fn atomic_publishing_replaces_an_existing_module() {
        let id = WRITE_ID.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("cinnabar-aim-write-{}-{id}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("module.wasm");
        write_atomic(&path, b"disabled").unwrap();
        write_atomic(&path, b"enabled").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"enabled");
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
