use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use super::commands::fetch_details;
use super::device::{ConnectionType, Device, DeviceDetails, DeviceState, parse_devices};
use super::{Adb, DEFAULT_TIMEOUT};
use crate::error::{AppError, Result};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Device details (mainly battery) are refreshed every 30 polls, about 30 seconds.
const DETAILS_REFRESH_TICKS: u64 = 30;
/// A phone that did not answer is asked again after this many polls, not on every
/// poll, because each attempt can wait seconds and holds up noticing unplugs.
const DETAILS_RETRY_TICKS: u64 = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
struct Snapshot {
    adb: Option<Adb>,
    status: AdbStatus,
    devices: Vec<Device>,
}

#[derive(Default)]
pub struct DeviceRegistry {
    snapshot: Mutex<Snapshot>,
}

impl DeviceRegistry {
    pub fn status(&self) -> AdbStatus {
        self.snapshot.lock().unwrap().status.clone()
    }

    pub fn devices(&self) -> Vec<Device> {
        self.snapshot.lock().unwrap().devices.clone()
    }

    pub fn adb(&self) -> Result<Adb> {
        self.snapshot.lock().unwrap().adb.clone().ok_or(AppError::AdbNotFound)
    }

    /// Returns the serial only if adb itself reports it as a wireless device.
    pub fn adb_for_wireless(&self, serial: &str) -> Result<Adb> {
        let snapshot = self.snapshot.lock().unwrap();
        let known = snapshot
            .devices
            .iter()
            .any(|device| device.serial == serial && device.connection == ConnectionType::Wireless);
        if !known {
            return Err(AppError::UnknownDevice(serial.to_string()));
        }
        snapshot.adb.clone().ok_or(AppError::AdbNotFound)
    }

    /// Returns adb only for a device that adb itself reports as online, so a
    /// serial from the UI can never reach a command line unchecked.
    pub fn adb_for_online(&self, serial: &str) -> Result<Adb> {
        let snapshot = self.snapshot.lock().unwrap();
        let device = snapshot
            .devices
            .iter()
            .find(|device| device.serial == serial)
            .ok_or_else(|| AppError::UnknownDevice(serial.to_string()))?;
        if device.state != DeviceState::Online {
            return Err(AppError::DeviceNotReady(serial.to_string()));
        }
        snapshot.adb.clone().ok_or(AppError::AdbNotFound)
    }

    fn store(&self, adb: Option<Adb>, status: AdbStatus, devices: Vec<Device>) -> bool {
        let mut snapshot = self.snapshot.lock().unwrap();
        let changed = snapshot.status != status || snapshot.devices != devices;
        snapshot.adb = adb;
        snapshot.status = status;
        snapshot.devices = devices;
        changed
    }
}

#[derive(Default)]
struct DetailsCache {
    entries: HashMap<String, (DeviceDetails, u64)>,
    /// When fetching each phone's details last failed.
    failed: HashMap<String, u64>,
}

impl DetailsCache {
    fn fill(&mut self, devices: &mut [Device]) {
        let online = |serial: &String| devices.iter().any(|d| &d.serial == serial && d.state == DeviceState::Online);
        self.entries.retain(|serial, _| online(serial));
        self.failed.retain(|serial, _| online(serial));
        for device in devices.iter_mut().filter(|d| d.state == DeviceState::Online) {
            device.details = self.entries.get(&device.serial).map(|(details, _)| details.clone());
        }
    }

    fn is_due(&self, serial: &str, tick: u64) -> bool {
        if self.failed.get(serial).is_some_and(|at| tick.saturating_sub(*at) < DETAILS_RETRY_TICKS) {
            return false;
        }
        self.entries.get(serial).is_none_or(|(_, at)| tick.saturating_sub(*at) >= DETAILS_REFRESH_TICKS)
    }

    /// Fetches what is missing or old, and reports whether anything was fetched.
    fn refresh(&mut self, adb: &Adb, devices: &[Device], tick: u64) -> bool {
        let mut fetched = false;
        for device in devices.iter().filter(|d| d.state == DeviceState::Online) {
            if !self.is_due(&device.serial, tick) {
                continue;
            }
            match fetch_details(adb, &device.serial) {
                Ok(details) => {
                    self.entries.insert(device.serial.clone(), (details, tick));
                    self.failed.remove(&device.serial);
                    fetched = true;
                }
                Err(error) => {
                    log::warn!("details for {}: {error}", device.serial);
                    self.failed.insert(device.serial.clone(), tick);
                }
            }
        }
        fetched
    }
}

fn status_for(adb: &Adb, version: Option<String>, error: Option<String>) -> AdbStatus {
    AdbStatus {
        available: true,
        path: Some(adb.path().display().to_string()),
        version,
        error,
    }
}

/// Polls adb in the background and calls `on_change` whenever the status or the
/// device list differs from the previous poll.
pub fn spawn_watcher<F>(registry: Arc<DeviceRegistry>, on_change: F)
where
    F: Fn(&AdbStatus, &[Device]) + Send + 'static,
{
    thread::spawn(move || {
        let publish = |adb: Option<Adb>, status: AdbStatus, devices: Vec<Device>| {
            if registry.store(adb, status.clone(), devices.clone()) {
                on_change(&status, &devices);
            }
        };
        let mut cache = DetailsCache::default();
        let mut located: Option<(Adb, Option<String>)> = None;
        let mut tick = 0u64;
        loop {
            if located.is_none()
                && let Ok(adb) = Adb::locate() {
                    if let Err(error) = adb.start_server() {
                        log::warn!("adb start-server: {error}");
                    }
                    let version = adb.version().ok();
                    log::info!("using adb at {} ({version:?})", adb.path().display());
                    located = Some((adb, version));
                }

            let mut lost = false;
            match &located {
                Some((adb, version)) => match adb.run_text(&["devices", "-l"], DEFAULT_TIMEOUT) {
                    Ok(output) => {
                        let mut devices = parse_devices(&output);
                        let status = status_for(adb, version.clone(), None);
                        cache.fill(&mut devices);
                        // The phone can be used right away; its details follow.
                        publish(Some(adb.clone()), status.clone(), devices.clone());
                        if cache.refresh(adb, &devices, tick) {
                            cache.fill(&mut devices);
                            publish(Some(adb.clone()), status, devices);
                        }
                    }
                    // adb was removed or moved, so look for it again on the next poll.
                    Err(_) if !adb.path().is_file() => {
                        log::warn!("adb is gone from {}", adb.path().display());
                        lost = true;
                        publish(None, AdbStatus::default(), Vec::new());
                    }
                    Err(error) => {
                        log::warn!("adb poll failed: {error}");
                        let _ = adb.start_server();
                        let status = status_for(adb, version.clone(), Some(error.to_string()));
                        publish(Some(adb.clone()), status, Vec::new());
                    }
                },
                None => publish(None, AdbStatus::default(), Vec::new()),
            }
            if lost {
                located = None;
            }
            tick += 1;
            thread::sleep(POLL_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_that_failed_is_not_asked_again_on_every_poll() {
        let mut cache = DetailsCache::default();
        assert!(cache.is_due("a", 0));
        cache.failed.insert("a".to_string(), 5);
        assert!(!cache.is_due("a", 6));
        assert!(cache.is_due("a", 5 + DETAILS_RETRY_TICKS));
    }

    #[test]
    fn details_are_refreshed_only_when_old() {
        let mut cache = DetailsCache::default();
        cache.entries.insert("a".to_string(), (DeviceDetails::default(), 10));
        assert!(!cache.is_due("a", 11));
        assert!(cache.is_due("a", 10 + DETAILS_REFRESH_TICKS));
    }
}
