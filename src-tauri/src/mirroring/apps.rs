use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::protocol::SCRCPY_VERSION;
use super::session::{DEVICE_SERVER_PATH, push_server};
use crate::adb::Adb;
use crate::error::{AppError, Result};

const LIST_TIMEOUT: Duration = Duration::from_secs(40);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub package: String,
    pub system: bool,
}

/// Parses the ` * Name   package` lines printed by the server's `list_apps`.
pub fn parse_app_list(output: &str) -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (system, rest) = match line.split_at_checked(2)? {
                ("* ", rest) => (true, rest),
                ("- ", rest) => (false, rest),
                _ => return None,
            };
            let (name, package) = rest.trim_end().rsplit_once(char::is_whitespace)?;
            Some(AppInfo { name: name.trim().to_string(), package: package.to_string(), system })
        })
        .collect();
    apps.sort_by_key(|app| (app.system, app.name.to_lowercase()));
    apps
}

/// Package names reach the device, so only the characters Android allows may pass.
pub fn validate_package(package: &str) -> Result<()> {
    let valid = !package.is_empty()
        && package.len() <= 255
        && package.contains('.')
        && package.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(AppError::Invalid(format!("package name {package}")))
    }
}

pub fn list_apps(adb: &Adb, serial: &str, server_path: &Path) -> Result<Vec<AppInfo>> {
    push_server(adb, serial, server_path)?;
    let classpath = format!("CLASSPATH={DEVICE_SERVER_PATH}");
    let args = [
        "-s",
        serial,
        "shell",
        &classpath,
        "app_process",
        "/",
        "com.genymobile.scrcpy.Server",
        SCRCPY_VERSION,
        "list_apps=true",
        "log_level=info",
    ];
    Ok(parse_app_list(&adb.run_text(&args, LIST_TIMEOUT)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = "[server] INFO: Device: [Nothing] A059 (Android 16)\n\
[server] INFO: List of apps:\n \
* Settings                       com.android.settings\n \
- Google Maps                    com.google.android.apps.maps\n \
- A very long application name that overflows the column com.example.app\n";

    #[test]
    fn parses_names_with_spaces_and_lists_user_apps_first() {
        let apps = parse_app_list(OUTPUT);
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[1].name, "Google Maps");
        assert_eq!(apps[1].package, "com.google.android.apps.maps");
        assert!(!apps[0].system);
        assert_eq!(apps[2].name, "Settings");
        assert!(apps[2].system);
    }

    #[test]
    fn ignores_log_lines() {
        assert!(parse_app_list("[server] INFO: nothing\n\n").is_empty());
    }

    #[test]
    fn validates_package_names() {
        assert!(validate_package("com.android.settings").is_ok());
        assert!(validate_package("org.foo_bar.App1").is_ok());
        for bad in ["", "nodots", "a.b; reboot", "+com.a.b", "?maps.x", "a.b c"] {
            assert!(validate_package(bad).is_err(), "{bad} was accepted");
        }
    }
}
