//! Read-only discovery of an already-running Cinnabar client.

use std::{
    ffi::OsString,
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, STILL_ACTIVE,
    },
    Globalization::{CSTR_EQUAL, CompareStringOrdinal},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
};

struct Handle(HANDLE);

pub struct ClientStartLock(Handle);

impl ClientStartLock {
    /// Serializes launcher starts across instances without blocking the UI thread.
    pub fn acquire() -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0},
            System::Threading::{CreateMutexW, WaitForSingleObject},
        };
        let name: Vec<u16> = format!("Local\\{}.ClientStart", crate::PRODUCT_NAME)
            .encode_utf16()
            .chain([0])
            .collect();
        let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err("Could not coordinate Cinnabar startup.".into());
        }
        let handle = Handle(raw);
        match unsafe { WaitForSingleObject(handle.0, 5000) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
            _ => Err("Another Cinnaroids instance is starting Cinnabar. Try again.".into()),
        }
    }
}

impl Drop for ClientStartLock {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::System::Threading::ReleaseMutex(self.0.0) };
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Handles are constructed only after the corresponding API succeeds.
        unsafe { CloseHandle(self.0) };
    }
}

/// Find a running client at this exact executable path; never start a process.
pub fn running_client(executable: &Path) -> Result<Option<u32>, String> {
    let expected = canonical_path(executable)?;
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(format!(
            "Could not list running clients: {}",
            std::io::Error::last_os_error()
        ));
    }
    let snapshot = Handle(raw);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let client_name: Vec<u16> = "bedrock-client.exe".encode_utf16().collect();
    let mut available = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    loop {
        if available == 0 {
            let error = unsafe { GetLastError() };
            return if error == ERROR_NO_MORE_FILES {
                Ok(None)
            } else {
                Err(format!(
                    "Could not list running clients: {}",
                    std::io::Error::from_raw_os_error(error as i32)
                ))
            };
        }
        let name_length = entry
            .szExeFile
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(entry.szExeFile.len());
        if wide_equal(&entry.szExeFile[..name_length], &client_name) {
            // A process can exit between the snapshot and inspection. Access
            // denied is an error so the caller does not launch a duplicate.
            if let Some(path) = process_path(entry.th32ProcessID)? {
                if wide_equal(&canonical_path(&path)?, &expected) {
                    return Ok(Some(entry.th32ProcessID));
                }
            }
        }
        available = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
}

/// Check both the PID's lifetime and executable path before trusting its status.
pub fn client_is_running(pid: u32, executable: &Path) -> bool {
    let Ok(Some(actual)) = process_path(pid) else {
        return false;
    };
    same_executable(&actual, executable)
}

fn process_path(pid: u32) -> Result<Option<PathBuf>, String> {
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        let error = unsafe { GetLastError() };
        if error == ERROR_INVALID_PARAMETER {
            return Ok(None);
        }
        return Err(process_error(
            pid,
            std::io::Error::from_raw_os_error(error as i32),
        ));
    }
    let process = Handle(raw);
    if !is_active(&process, pid)? {
        return Ok(None);
    }
    // Win32 extended-length paths are bounded by 32,767 UTF-16 characters.
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        let error = std::io::Error::last_os_error();
        if !is_active(&process, pid)? {
            return Ok(None);
        }
        return Err(process_error(pid, error));
    }
    if !is_active(&process, pid)? {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(OsString::from_wide(
        &buffer[..length as usize],
    ))))
}

fn is_active(process: &Handle, pid: u32) -> Result<bool, String> {
    let mut code = 0;
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return Err(process_error(pid, std::io::Error::last_os_error()));
    }
    Ok(code == STILL_ACTIVE as u32)
}

fn process_error(pid: u32, error: std::io::Error) -> String {
    format!("Could not inspect running Cinnabar (PID {pid}): {error}")
}

fn canonical_path(path: &Path) -> Result<Vec<u16>, String> {
    fs::canonicalize(path)
        .map(|path| path.as_os_str().encode_wide().collect())
        .map_err(|error| format!("Could not resolve client path {}: {error}", path.display()))
}

fn same_executable(first: &Path, second: &Path) -> bool {
    match (canonical_path(first), canonical_path(second)) {
        (Ok(first), Ok(second)) => wide_equal(&first, &second),
        _ => false,
    }
}

fn wide_equal(first: &[u16], second: &[u16]) -> bool {
    let (Ok(first_length), Ok(second_length)) =
        (i32::try_from(first.len()), i32::try_from(second.len()))
    else {
        return false;
    };
    // Ordinal comparison respects Windows case rules without Unicode loss.
    unsafe {
        CompareStringOrdinal(
            first.as_ptr(),
            first_length,
            second.as_ptr(),
            second_length,
            1,
        ) == CSTR_EQUAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn canonical_paths_match_case_and_dot_segments_but_not_another_installation() {
        let directory = std::env::temp_dir().join(format!(
            "cinnaroids-process-paths-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        let first = directory.join("first/bedrock-client.exe");
        let second = directory.join("second/bedrock-client.exe");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, b"client").unwrap();
        fs::write(&second, b"client").unwrap();
        assert!(same_executable(
            &first,
            &first.with_file_name("BEDROCK-CLIENT.EXE")
        ));
        assert!(same_executable(
            &first,
            &directory.join("second/../first/bedrock-client.exe")
        ));
        assert!(same_executable(&first, &fs::canonicalize(&first).unwrap()));
        assert!(!same_executable(&first, &second));
        assert!(!same_executable(
            &first,
            &directory.join("missing/bedrock-client.exe")
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
