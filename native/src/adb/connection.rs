use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use super::commands::fetch_details;
use super::device::{ConnectionType, Device, DeviceDetails, DeviceState, parse_devices};
use super::{Adb, DEFAULT_TIMEOUT};
use crate::error::{AppError, Result};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Device details (mainly battery) are refreshed every 15 polls, about 30 seconds.
const DETAILS_REFRESH_TICKS: u64 = 15;

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

struct DetailsCache {
    entries: HashMap<String, (DeviceDetails, u64)>,
}

impl DetailsCache {
    fn apply(&mut self, adb: &Adb, devices: &mut [Device], tick: u64) {
        self.entries.retain(|serial, _| {
            devices.iter().any(|d| &d.serial == serial && d.state == DeviceState::Online)
        });
        for device in devices.iter_mut().filter(|d| d.state == DeviceState::Online) {
            let stale = self
                .entries
                .get(&device.serial)
                .is_none_or(|(_, at)| tick.saturating_sub(*at) >= DETAILS_REFRESH_TICKS);
            if stale {
                match fetch_details(adb, &device.serial) {
                    Ok(details) => {
                        self.entries.insert(device.serial.clone(), (details, tick));
                    }
                    Err(error) => log::warn!("details for {}: {error}", device.serial),
                }
            }
            device.details = self.entries.get(&device.serial).map(|(details, _)| details.clone());
        }
    }
}

fn poll(adb: &Adb, cache: &mut DetailsCache, tick: u64) -> Result<Vec<Device>> {
    let output = adb.run_text(&["devices", "-l"], DEFAULT_TIMEOUT)?;
    let mut devices = parse_devices(&output);
    cache.apply(adb, &mut devices, tick);
    Ok(devices)
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
        let mut cache = DetailsCache { entries: HashMap::new() };
        let mut located: Option<(Adb, Option<String>)> = None;
        let mut tick = 0u64;
        loop {
            if located.is_none() {
                if let Ok(adb) = Adb::locate() {
                    if let Err(error) = adb.start_server() {
                        log::warn!("adb start-server: {error}");
                    }
                    let version = adb.version().ok();
                    log::info!("using adb at {} ({version:?})", adb.path().display());
                    located = Some((adb, version));
                }
            }

            let (adb, status, devices) = match &located {
                Some((adb, version)) => match poll(adb, &mut cache, tick) {
                    Ok(devices) => (Some(adb.clone()), status_for(adb, version.clone(), None), devices),
                    Err(error) => {
                        log::warn!("adb poll failed: {error}");
                        let _ = adb.start_server();
                        let status = status_for(adb, version.clone(), Some(error.to_string()));
                        (Some(adb.clone()), status, Vec::new())
                    }
                },
                None => (None, AdbStatus::default(), Vec::new()),
            };

            if registry.store(adb, status.clone(), devices.clone()) {
                on_change(&status, &devices);
            }
            tick += 1;
            thread::sleep(POLL_INTERVAL);
        }
    });
}
