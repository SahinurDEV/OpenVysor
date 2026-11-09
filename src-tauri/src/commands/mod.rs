pub mod windows;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::adb::commands::screenshot;
use crate::adb::wireless;
use crate::adb::connection::{AdbStatus, DeviceRegistry};
use crate::adb::device::Device;
use crate::error::{AppError, Result};
use crate::mirroring::apps::{AppInfo, validate_package};
use crate::mirroring::protocol::ControlMessage;
use crate::mirroring::session::MirrorOptions;
use crate::mirroring::{MirrorManager, StartRequest};
use crate::transfer::{self, TransferResult};
use windows::TrackedWindow;

pub const EVENT_DEVICES: &str = "devices-changed";
pub const EVENT_MIRROR_ENDED: &str = "mirror-ended";

pub struct AppState {
    pub registry: Arc<DeviceRegistry>,
    pub mirrors: Arc<MirrorManager>,
    pub windows: Mutex<Vec<TrackedWindow>>,
    pub recordings: Mutex<HashMap<String, File>>,
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

    let sink = Arc::new(move |packet: Vec<u8>| channel.send(InvokeResponseBody::Raw(packet)).is_ok());
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

#[tauri::command]
pub fn set_clipboard_sync(state: State<'_, AppState>, enabled: bool) {
    state.mirrors.set_clipboard_sync(enabled);
}

#[tauri::command(async)]
pub fn send_files(
    state: State<'_, AppState>,
    serial: String,
    paths: Vec<PathBuf>,
) -> Result<Vec<TransferResult>> {
    let adb = state.registry.adb_for_online(&serial)?;
    paths.iter().map(|path| transfer::send_path(&adb, &serial, path)).collect()
}

#[tauri::command(async)]
pub fn wireless_pair(state: State<'_, AppState>, address: String, code: String) -> Result<()> {
    wireless::pair(&state.registry.adb()?, &address, &code)
}

#[tauri::command(async)]
pub fn wireless_connect(state: State<'_, AppState>, address: String) -> Result<String> {
    wireless::connect(&state.registry.adb()?, &address)
}

#[tauri::command(async)]
pub fn wireless_disconnect(state: State<'_, AppState>, serial: String) -> Result<()> {
    wireless::disconnect(&state.registry.adb_for_wireless(&serial)?, &serial)
}

#[tauri::command(async)]
pub fn wireless_poll_qr(state: State<'_, AppState>, name: String, password: String) -> Result<bool> {
    wireless::poll_qr_pairing(&state.registry.adb()?, &name, &password)
}

#[tauri::command(async)]
pub fn recording_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session: String,
    extension: String,
) -> Result<String> {
    if extension != "mp4" && extension != "webm" {
        return Err(AppError::Invalid(format!("extension {extension}")));
    }
    let folder = app
        .path()
        .video_dir()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .join("OpenVysor");
    std::fs::create_dir_all(&folder)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = folder.join(format!("recording-{stamp}.{extension}"));
    state.recordings.lock().unwrap().insert(session, File::create(&path)?);
    Ok(path.display().to_string())
}

// Deliberately not async: chunks must reach the file in the order they were sent.
#[tauri::command]
pub fn recording_chunk(state: State<'_, AppState>, request: Request<'_>) -> Result<()> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::Invalid("recording data".into()));
    };
    let session = request
        .headers()
        .get("x-session")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Invalid("recording session".into()))?;
    let mut recordings = state.recordings.lock().unwrap();
    let file = recordings.get_mut(session).ok_or_else(|| AppError::NoSession(session.to_string()))?;
    file.write_all(bytes)?;
    Ok(())
}

#[tauri::command]
pub fn recording_stop(state: State<'_, AppState>, session: String) -> Result<()> {
    if let Some(file) = state.recordings.lock().unwrap().remove(&session) {
        file.sync_all()?;
    }
    Ok(())
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
