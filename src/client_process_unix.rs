//! Discover the exact running client and serialize registration across launchers.

use std::{
    fs::{self, File, OpenOptions},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct RegistrationLock(File);

impl RegistrationLock {
    pub fn acquire() -> Result<Self, String> {
        let base = crate::settings::data_directory().ok_or("Settings folder unavailable.")?;
        fs::create_dir_all(&base).map_err(|error| error.to_string())?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(base.join("cinnaroids-registration.lock"))
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Self(file));
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(format!("Could not coordinate Cinnabar attachment: {error}"));
            }
            if Instant::now() >= deadline {
                return Err("Another Cinnaroids instance is registering modules.".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for RegistrationLock {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

pub fn running_client(executable: &Path) -> Result<Option<u32>, String> {
    let expected = fs::canonicalize(executable).map_err(|error| error.to_string())?;
    for pid in process_ids()? {
        if process_path(pid)
            .and_then(|path| fs::canonicalize(path).ok())
            .as_ref()
            == Some(&expected)
        {
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

pub fn client_is_running(pid: u32, executable: &Path) -> bool {
    match (
        process_path(pid).and_then(|path| fs::canonicalize(path).ok()),
        fs::canonicalize(executable),
    ) {
        (Some(actual), Ok(expected)) => actual == expected,
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn process_ids() -> Result<Vec<u32>, String> {
    let entries = fs::read_dir("/proc").map_err(|error| error.to_string())?;
    Ok(entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect())
}

#[cfg(target_os = "linux")]
fn process_path(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(target_os = "macos")]
fn process_ids() -> Result<Vec<u32>, String> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid="])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("Could not list running clients.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|pid| pid.parse().ok())
        .collect())
}

#[cfg(target_os = "macos")]
fn process_path(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length =
        unsafe { libc::proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if length <= 0 {
        return None;
    }
    buffer.truncate(buffer.iter().position(|byte| *byte == 0)?);
    Some(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_current_process_only_at_its_real_executable() {
        let executable = std::env::current_exe().unwrap();
        assert!(client_is_running(std::process::id(), &executable));
        assert!(running_client(&executable).unwrap().is_some());
        assert!(!client_is_running(
            std::process::id(),
            Path::new("/missing/cinnabar")
        ));
        assert!(!client_is_running(u32::MAX, &executable));
    }
}
