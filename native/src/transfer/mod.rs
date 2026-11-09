use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::adb::Adb;
use crate::error::{AppError, Result};

const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOADS: &str = "/sdcard/Download/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferAction {
    Installed,
    Sent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    pub name: String,
    pub action: TransferAction,
}

pub fn is_apk(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("apk"))
}

/// `adb install` can exit successfully while printing a failure, so the text decides.
pub fn check_install_output(output: &str) -> Result<()> {
    if output.lines().any(|line| line.trim() == "Success") {
        return Ok(());
    }
    let reason = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Failure"))
        .map_or_else(|| output.trim().to_string(), |reason| reason.trim().to_string());
    Err(AppError::AdbFailed(format!("install failed: {reason}")))
}

/// Installs an APK, or copies any other file or folder to the phone's Download folder.
pub fn send_path(adb: &Adb, serial: &str, path: &Path) -> Result<TransferResult> {
    let name = path
        .file_name()
        .ok_or_else(|| AppError::Invalid(format!("path {}", path.display())))?
        .to_string_lossy()
        .into_owned();
    if !path.exists() {
        return Err(AppError::Invalid(format!("{name} does not exist")));
    }

    let mut args: Vec<OsString> = vec!["-s".into(), serial.into()];
    if is_apk(path) {
        args.extend(["install".into(), "-r".into(), path.into()]);
        let output = String::from_utf8_lossy(&adb.run(&args, TRANSFER_TIMEOUT)?).into_owned();
        check_install_output(&output)?;
        Ok(TransferResult { name, action: TransferAction::Installed })
    } else {
        args.extend(["push".into(), path.into(), DOWNLOADS.into()]);
        adb.run(&args, TRANSFER_TIMEOUT)?;
        Ok(TransferResult { name, action: TransferAction::Sent })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_install_success() {
        assert!(check_install_output("Performing Streamed Install\r\nSuccess\r\n").is_ok());
    }

    #[test]
    fn reports_the_install_failure_reason() {
        let error = check_install_output("adb: failed\nFailure [INSTALL_FAILED_OLDER_SDK: x]\n").unwrap_err();
        assert!(error.to_string().contains("INSTALL_FAILED_OLDER_SDK"));
    }

    #[test]
    fn does_not_treat_a_success_substring_as_success() {
        assert!(check_install_output("NotSuccess").is_err());
    }
}
