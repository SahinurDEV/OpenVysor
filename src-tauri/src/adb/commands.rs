use std::time::Duration;

use super::device::{DeviceDetails, getprop_value, parse_battery, parse_wm_size};
use super::{Adb, DEFAULT_TIMEOUT};
use crate::error::{AppError, Result};

const PROPS_TIMEOUT: Duration = Duration::from_secs(5);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(20);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub fn fetch_details(adb: &Adb, serial: &str) -> Result<DeviceDetails> {
    let props = adb.device_text(serial, &["shell", "getprop"], PROPS_TIMEOUT)?;
    let size = adb.device_text(serial, &["shell", "wm", "size"], PROPS_TIMEOUT).unwrap_or_default();
    let battery = adb
        .device_text(serial, &["shell", "dumpsys", "battery"], PROPS_TIMEOUT)
        .unwrap_or_default();
    Ok(DeviceDetails {
        manufacturer: getprop_value(&props, "ro.product.manufacturer"),
        model: getprop_value(&props, "ro.product.model"),
        android_version: getprop_value(&props, "ro.build.version.release"),
        sdk: getprop_value(&props, "ro.build.version.sdk"),
        resolution: parse_wm_size(&size),
        battery: parse_battery(&battery),
    })
}

pub fn screenshot(adb: &Adb, serial: &str) -> Result<Vec<u8>> {
    let image = adb.run(&["-s", serial, "exec-out", "screencap", "-p"], SCREENSHOT_TIMEOUT)?;
    if image.starts_with(PNG_SIGNATURE) {
        Ok(image)
    } else {
        Err(AppError::AdbFailed("the device did not return a PNG screenshot".into()))
    }
}

pub fn create_forward(adb: &Adb, serial: &str, socket: &str) -> Result<u16> {
    let remote = format!("localabstract:{socket}");
    let output = adb.device_text(serial, &["forward", "tcp:0", &remote], DEFAULT_TIMEOUT)?;
    output
        .trim()
        .parse()
        .map_err(|_| AppError::AdbFailed(format!("unexpected forward output: {output}")))
}

pub fn remove_forward(adb: &Adb, serial: &str, port: u16) -> Result<()> {
    let local = format!("tcp:{port}");
    adb.device_text(serial, &["forward", "--remove", &local], DEFAULT_TIMEOUT).map(|_| ())
}
