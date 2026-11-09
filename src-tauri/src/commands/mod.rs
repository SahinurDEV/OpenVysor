pub mod windows;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::adb::commands::screenshot;
use crate::adb::connection::{AdbStatus, DeviceRegistry};
use crate::adb::device::Device;
use crate::error::{AppError, Result};
use crate::mirroring::apps::{AppInfo, validate_package};
use crate::mirroring::protocol::ControlMessage;
use crate::mirroring::session::MirrorOptions;
use crate::mirroring::{MirrorManager, StartRequest};
use windows::TrackedWindow;

pub const EVENT_DEVICES: &str = "devices-changed";
pub const EVENT_MIRROR_ENDED: &str = "mirror-ended";

pub struct AppState {
    pub registry: Arc<DeviceRegistry>,
    pub mirrors: Arc<MirrorManager>,
    pub windows: Mutex<Vec<TrackedWindow>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesPayload {
    pub status: AdbStatus,
    pub devices: Vec<Device>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MirrorEnded {
    session: String,
    reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorStarted {
    device_name: String,
}

fn server_path(app: &AppHandle) -> Result<PathBuf> {
    app.path()
        .resolve("resources/scrcpy-server", BaseDirectory::Resource)
        .map_err(|error| AppError::ServerStart(error.to_string()))
}

#[tauri::command]
pub fn get_devices(state: State<'_, AppState>) -> DevicesPayload {
    DevicesPayload { status: state.registry.status(), devices: state.registry.devices() }
}

#[tauri::command]
pub fn active_mirrors(state: State<'_, AppState>) -> Vec<String> {
    state.mirrors.active_sessions()
}

#[tauri::command(async)]
pub fn list_apps(app: AppHandle, state: State<'_, AppState>, serial: String) -> Result<Vec<AppInfo>> {
    let adb = state.registry.adb_for_online(&serial)?;
    state.mirrors.list_apps(&adb, &serial, &server_path(&app)?)
}

#[tauri::command(async)]
pub fn start_mirror(
    app: AppHandle,
    state: State<'_, AppState>,
    session: String,
    serial: String,
    options: MirrorOptions,
    package: Option<String>,
    channel: Channel<InvokeResponseBody>,
) -> Result<MirrorStarted> {
    let adb = state.registry.adb_for_online(&serial)?;
    if let Some(package) = &package {
        validate_package(package)?;
    }
    let path = server_path(&app)?;

    let sink = Box::new(move |packet: Vec<u8>| channel.send(InvokeResponseBody::Raw(packet)).is_ok());
    let end_app = app.clone();
    let end_session = session.clone();
    let on_end = Box::new(move |reason: String| {
        let _ = end_app.emit(EVENT_MIRROR_ENDED, MirrorEnded { session: end_session, reason });
    });

    let request = StartRequest {
        key: &session,
        serial: &serial,
        server_path: &path,
        options,
        start_app: package.as_deref(),
    };
    let started = state.mirrors.start(&adb, request, sink, on_end)?;
    Ok(MirrorStarted { device_name: started.device_name().to_string() })
}

#[tauri::command(async)]
pub fn stop_mirror(state: State<'_, AppState>, session: String) -> Result<()> {
    state.mirrors.stop(&session)
}

// Deliberately not async: async commands run on a thread pool, which could
// reorder a touch down, move and up sequence.
#[tauri::command]
pub fn mirror_input(state: State<'_, AppState>, session: String, message: ControlMessage) -> Result<()> {
    state.mirrors.send(&session, &message)
}

#[tauri::command(async)]
pub fn take_screenshot(app: AppHandle, state: State<'_, AppState>, serial: String) -> Result<String> {
    let adb = state.registry.adb_for_online(&serial)?;
    let image = screenshot(&adb, &serial)?;
    let folder = app
        .path()
        .picture_dir()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .join("OpenVysor");
    std::fs::create_dir_all(&folder)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = folder.join(format!("screenshot-{stamp}.png"));
    std::fs::write(&path, image)?;
    Ok(path.display().to_string())
}
