//! Lists the phone's shared storage and copies files both ways.
//!
//! File names come from the phone and from the user, so anything that reaches
//! the phone's shell is quoted. `adb push` and `adb pull` take plain arguments.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adb::Adb;
use crate::error::{AppError, Result};

pub const ROOT: &str = "/sdcard";

const LIST_TIMEOUT: Duration = Duration::from_secs(15);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
}

pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Parses `ls -1pA`, which marks folders with a trailing slash. Folders come first.
pub fn parse_listing(output: &str) -> Vec<RemoteEntry> {
    let mut entries: Vec<RemoteEntry> = output
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(|line| match line.strip_suffix('/') {
            Some(name) => RemoteEntry { name: name.to_string(), is_dir: true },
            None => RemoteEntry { name: line.to_string(), is_dir: false },
        })
        .collect();
    entries.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    entries
}

pub fn child(path: &str, name: &str) -> String {
    format!("{}/{name}", path.trim_end_matches('/'))
}

/// One folder up, stopping at the top of shared storage.
pub fn parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((above, _)) if path != ROOT && is_allowed(above) => above.to_string(),
        _ => ROOT.to_string(),
    }
}

fn is_allowed(path: &str) -> bool {
    let inside = path == ROOT || path.starts_with("/sdcard/");
    inside && !path.split('/').any(|part| part == "..")
}

fn check(path: &str) -> Result<()> {
    if is_allowed(path) { Ok(()) } else { Err(AppError::Invalid(format!("path {path}"))) }
}

pub fn list(adb: &Adb, serial: &str, path: &str) -> Result<Vec<RemoteEntry>> {
    check(path)?;
    // The slash makes `ls` follow /sdcard, which is a symbolic link on most phones.
    let command = format!("ls -1pA -- {}", shell_quote(&format!("{}/", path.trim_end_matches('/'))));
    Ok(parse_listing(&adb.device_text(serial, &["shell", &command], LIST_TIMEOUT)?))
}

/// A name not yet taken in `folder`, so a download never replaces an existing file.
fn unused_path(folder: &Path, name: &str) -> Result<PathBuf> {
    // The name comes from the phone and must not lead outside the folder.
    let safe = Path::new(name).file_name().ok_or_else(|| AppError::Invalid(format!("file name {name}")))?;
    let first = folder.join(safe);
    if !first.exists() {
        return Ok(first);
    }
    let stem = Path::new(safe).file_stem().unwrap_or(safe).to_string_lossy().into_owned();
    let extension = Path::new(safe).extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    (2..10_000)
        .map(|number| folder.join(format!("{stem} ({number}){extension}")))
        .find(|path| !path.exists())
        .ok_or_else(|| AppError::Invalid(format!("no free name for {name}")))
}

/// Copies a phone file or folder to the computer's Downloads, and returns where it went.
pub fn download(adb: &Adb, serial: &str, remote: &str) -> Result<PathBuf> {
    check(remote)?;
    // Minimal Linux systems have no configured Downloads folder.
    let downloads = dirs::download_dir().or_else(|| dirs::home_dir().map(|home| home.join("Downloads")));
    let folder = downloads.ok_or_else(|| AppError::Invalid("no Downloads folder".into()))?.join("OpenVysor");
    std::fs::create_dir_all(&folder)?;
    let name = remote.rsplit('/').next().unwrap_or(remote);
    let target = unused_path(&folder, name)?;
    let local = target.to_string_lossy().into_owned();
    adb.device_text(serial, &["pull", remote, &local], TRANSFER_TIMEOUT)?;
    Ok(target)
}

pub fn upload(adb: &Adb, serial: &str, local: &Path, folder: &str) -> Result<()> {
    check(folder)?;
    let source = local.to_string_lossy().into_owned();
    let target = format!("{}/", folder.trim_end_matches('/'));
    adb.device_text(serial, &["push", &source, &target], TRANSFER_TIMEOUT).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_names_that_try_to_break_out() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("x'; reboot; '"), r"'x'\''; reboot; '\'''");
    }

    #[test]
    fn lists_folders_first_and_keeps_odd_names() {
        let entries = parse_listing("zeta.txt\r\nAlpha/\n\nmy file (1).jpg\n");
        assert_eq!(entries[0], RemoteEntry { name: "Alpha".to_string(), is_dir: true });
        assert_eq!(entries[1].name, "my file (1).jpg");
        assert!(!entries[2].is_dir);
    }

    #[test]
    fn walks_inside_shared_storage_only() {
        assert_eq!(child("/sdcard", "DCIM"), "/sdcard/DCIM");
        assert_eq!(parent("/sdcard/DCIM/Camera"), "/sdcard/DCIM");
        assert_eq!(parent("/sdcard/DCIM"), "/sdcard");
        assert_eq!(parent("/sdcard"), "/sdcard");
    }

    #[test]
    fn refuses_paths_outside_shared_storage() {
        for bad in ["/data/data", "/sdcard/../data", "/sdcardx", ""] {
            assert!(check(bad).is_err(), "{bad} was accepted");
        }
        assert!(check("/sdcard/DCIM").is_ok());
    }

    #[test]
    fn a_download_never_replaces_a_file_or_leaves_the_folder() {
        let folder = std::env::temp_dir().join(format!("openvysor-names-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.txt"), "x").unwrap();
        assert_eq!(unused_path(&folder, "a.txt").unwrap(), folder.join("a (2).txt"));
        assert_eq!(unused_path(&folder, "b.txt").unwrap(), folder.join("b.txt"));
        assert_eq!(unused_path(&folder, "../../evil.txt").unwrap(), folder.join("evil.txt"));
        assert!(unused_path(&folder, "..").is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
