//! Installs Google's platform-tools for people who do not have adb yet.
//!
//! The package is pinned by version and SHA-256, so a changed or damaged download
//! is never unpacked. Linux is left to the system package, which also brings the
//! USB access rules adb needs there.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use sha2::{Digest, Sha256};

use super::hidden_command;
use crate::error::{AppError, Result};

const BASE_URL: &str = "https://dl.google.com/android/repository/";
const MAX_DOWNLOAD_SECONDS: &str = "300";
const FOLDER: &str = "platform-tools";

const PACKAGE: Option<(&str, &str)> = if cfg!(windows) {
    Some(("platform-tools_r37.0.0-win.zip", "4fe305812db074cea32903a489d061eb4454cbc90a49e8fea677f4b7af764918"))
} else if cfg!(target_os = "macos") {
    Some(("platform-tools_r37.0.0-darwin.zip", "094a1395683c509fd4d48667da0d8b5ef4d42b2abfcd29f2e8149e2f989357c7"))
} else {
    None
};

pub const SUPPORTED: bool = PACKAGE.is_some();

fn parent_folder() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("OpenVysor"))
}

pub fn tools_folder() -> Option<PathBuf> {
    Some(parent_folder()?.join(FOLDER))
}

/// The system's own copy, by full path, so a file planted next to the app is never run.
fn system_tool(name: &str) -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join(format!("{name}.exe"))
    } else {
        Path::new("/usr/bin").join(name)
    }
}

fn run<S: AsRef<OsStr>>(tool: &str, args: &[S], failure: &str) -> Result<()> {
    let output = hidden_command(&system_tool(tool)).args(args).stdin(Stdio::null()).output()?;
    if output.status.success() {
        return Ok(());
    }
    log::warn!("{tool}: {}", String::from_utf8_lossy(&output.stderr).trim());
    Err(AppError::AdbSetup(failure.to_string()))
}

fn has_checksum(bytes: &[u8], expected: &str) -> bool {
    let actual: String = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    actual == expected
}

fn download_and_unpack(work: &Path, name: &str, checksum: &str) -> Result<()> {
    let archive = work.join(name);
    let url = format!("{BASE_URL}{name}");
    let args = [
        OsStr::new("--fail"),
        OsStr::new("--location"),
        OsStr::new("--silent"),
        OsStr::new("--show-error"),
        OsStr::new("--proto"),
        OsStr::new("=https"),
        OsStr::new("--max-time"),
        OsStr::new(MAX_DOWNLOAD_SECONDS),
        OsStr::new("--output"),
        archive.as_os_str(),
        OsStr::new(&url),
    ];
    run("curl", &args, "The download failed. Check your internet connection and try again.")?;
    if !has_checksum(&std::fs::read(&archive)?, checksum) {
        return Err(AppError::AdbSetup("The download did not match Google's package, so it was not installed.".to_string()));
    }
    run("tar", &[OsStr::new("-xf"), archive.as_os_str(), OsStr::new("-C"), work.as_os_str()], "The download could not be unpacked.")
}

/// Puts adb in the app's own folder. The device watcher finds it on its next poll.
pub fn install() -> Result<()> {
    let (name, checksum) = PACKAGE.ok_or_else(|| AppError::AdbSetup("Install adb with your package manager.".to_string()))?;
    let parent = parent_folder().ok_or_else(|| AppError::AdbSetup("This computer has no app data folder.".to_string()))?;
    let work = parent.join("platform-tools.download");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let result = download_and_unpack(&work, name, checksum).and_then(|()| {
        let target = parent.join(FOLDER);
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(work.join(FOLDER), &target)?;
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_expected_bytes_pass_the_checksum() {
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(has_checksum(b"abc", abc));
        assert!(!has_checksum(b"abd", abc));
    }

    #[test]
    fn system_tools_are_full_paths() {
        assert!(system_tool("curl").is_absolute());
    }
}
