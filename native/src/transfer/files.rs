//! Lists the phone's shared storage and copies files both ways.
//!
//! File names come from the phone and from the user, so anything that reaches
//! the phone's shell is quoted. `adb push` and `adb pull` take plain arguments.

use std::collections::HashMap;
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
    /// Bytes, for files only. Missing when the phone could not report it.
    pub size: Option<u64>,
}

/// Starts the sizes part of a listing. File names cannot contain a slash.
const SIZES_MARKER: &str = "//sizes";

pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Parses `ls -1pA`, which marks folders with a trailing slash, optionally followed by
/// the sizes marker and `stat -c '%s %n'` lines. Folders come first.
pub fn parse_listing(output: &str) -> Vec<RemoteEntry> {
    let mut lines = output.lines().map(|line| line.trim_end_matches('\r'));
    let mut entries: Vec<RemoteEntry> = lines
        .by_ref()
        .take_while(|line| *line != SIZES_MARKER)
        .filter(|line| !line.is_empty())
        .map(|line| match line.strip_suffix('/') {
            Some(name) => RemoteEntry { name: name.to_string(), is_dir: true, size: None },
            None => RemoteEntry { name: line.to_string(), is_dir: false, size: None },
        })
        .collect();
    let sizes: HashMap<&str, u64> = lines
        .filter_map(|line| {
            let (size, name) = line.split_once(' ')?;
            Some((name, size.parse().ok()?))
        })
        .collect();
    for entry in entries.iter_mut().filter(|entry| !entry.is_dir) {
        entry.size = sizes.get(entry.name.as_str()).copied();
    }
    entries.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    entries
}

/// A short size for people, such as `4.2 MB`.
pub fn readable_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return if bytes == 1 { "1 byte".to_string() } else { format!("{bytes} bytes") };
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 { format!("{value:.1} {}", UNITS[unit]) } else { format!("{value:.0} {}", UNITS[unit]) }
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
    // The slash makes `cd` follow /sdcard, which is a symbolic link on most phones. A failed
    // `cd` or `ls` is an error; missing sizes are not, as old phones have no `stat`.
    let folder = shell_quote(&format!("{}/", path.trim_end_matches('/')));
    let command = format!("cd -- {folder} && ls -1pA && {{ echo {SIZES_MARKER}; stat -L -c '%s %n' -- * .* 2>/dev/null; true; }}");
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
        assert_eq!(entries[0], RemoteEntry { name: "Alpha".to_string(), is_dir: true, size: None });
        assert_eq!(entries[1].name, "my file (1).jpg");
        assert!(!entries[2].is_dir);
    }

    #[test]
    fn reads_file_sizes_for_odd_names() {
        // Output of the listing command on a Nothing Phone (3a), Android 16.
        let output = ".hidden
a b.txt
it's.txt
sub dir/
//sizes
6 a b.txt
2 it's.txt
3452 sub dir
2 .hidden
";
        let entries = parse_listing(output);
        let size = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap().size;
        assert_eq!(entries[0].name, "sub dir");
        assert_eq!(size("sub dir"), None, "folders show no size");
        assert_eq!(size("a b.txt"), Some(6));
        assert_eq!(size("it's.txt"), Some(2));
        assert_eq!(size(".hidden"), Some(2));
    }

    #[test]
    fn lists_names_when_sizes_are_missing_or_empty() {
        let entries = parse_listing("a.txt
b.txt
//sizes
");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.size.is_none()));
        let entries = parse_listing("x y.bin
//sizes
not-a-number x y.bin
");
        assert_eq!(entries[0].size, None);
        assert!(parse_listing("//sizes
").is_empty());
    }

    #[test]
    fn shows_sizes_people_can_read() {
        assert_eq!(readable_size(0), "0 bytes");
        assert_eq!(readable_size(1), "1 byte");
        assert_eq!(readable_size(1023), "1023 bytes");
        assert_eq!(readable_size(1024), "1.0 KB");
        assert_eq!(readable_size(4_404_019), "4.2 MB");
        assert_eq!(readable_size(15 * 1024 * 1024), "15 MB");
        assert_eq!(readable_size(3 * 1024 * 1024 * 1024), "3.0 GB");
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
