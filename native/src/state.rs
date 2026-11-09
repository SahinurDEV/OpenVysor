//! State shared by the main window, the phone windows and the background threads.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{Context, Pos2, ViewportCommand, ViewportId, pos2};
use serde::{Deserialize, Serialize};

use crate::adb::connection::{DeviceRegistry, spawn_watcher};
use crate::adb::device::DeviceState;
use crate::clipboard;
use crate::error::{AppError, Result};
use crate::input::phone_window_size;
use crate::mirroring::MirrorManager;
use crate::mirroring::protocol::SCRCPY_VERSION;
use crate::mirroring::session::MirrorOptions;
use crate::phone::Phone;
use crate::profiles::{AppProfile, Workspace, WorkspaceWindow, profile_key, store_workspace};

const SERVER: &[u8] = include_bytes!("../resources/scrcpy-server");
const SCREEN_MARGIN: f32 = 16.0;
const WINDOW_GAP: f32 = 8.0;
const CASCADE_STEP: f32 = 32.0;
const CASCADE_ORIGIN: f32 = 80.0;
const CASCADE_LENGTH: u32 = 8;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub options: MirrorOptions,
    pub clipboard_sync: bool,
    pub notifications: bool,
    pub profiles: BTreeMap<String, AppProfile>,
    pub workspaces: Vec<Workspace>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: true,
            options: MirrorOptions::default(),
            clipboard_sync: true,
            notifications: false,
            profiles: BTreeMap::new(),
            workspaces: Vec::new(),
        }
    }
}

fn app_folder() -> Result<PathBuf> {
    let folder = dirs::data_local_dir()
        .ok_or_else(|| AppError::Invalid("no local data folder".into()))?
        .join("OpenVysor");
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

fn settings_path() -> Result<PathBuf> {
    Ok(app_folder()?.join("settings.json"))
}

/// The server ships inside the program and is unpacked next to the settings.
fn extract_server() -> Result<PathBuf> {
    let path = app_folder()?.join(format!("scrcpy-server-{SCRCPY_VERSION}"));
    if std::fs::read(&path).is_ok_and(|existing| existing == SERVER) {
        return Ok(path);
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, SERVER)?;
    std::fs::rename(&temporary, &path)?;
    Ok(path)
}

pub struct Core {
    pub registry: Arc<DeviceRegistry>,
    pub mirrors: Arc<MirrorManager>,
    pub server_path: Result<PathBuf>,
    pub settings: Mutex<Settings>,
    pub phones: Mutex<Vec<Arc<Phone>>>,
    next_window: AtomicU32,
}

impl Core {
    pub fn new(ctx: &Context) -> Arc<Self> {
        let settings: Settings = settings_path()
            .ok()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        let mirrors = Arc::new(MirrorManager::new());
        mirrors.set_clipboard_sync(settings.clipboard_sync);
        let device_clipboard = Mutex::new(clipboard::spawn(mirrors.clone()));
        mirrors.set_clipboard_handler(Arc::new(move |text| {
            let _ = device_clipboard.lock().unwrap().send(text);
        }));

        let core = Arc::new(Self {
            registry: Arc::new(DeviceRegistry::default()),
            mirrors,
            server_path: extract_server(),
            settings: Mutex::new(settings),
            phones: Mutex::new(Vec::new()),
            next_window: AtomicU32::new(0),
        });

        let watcher_core = core.clone();
        let watcher_ctx = ctx.clone();
        spawn_watcher(core.registry.clone(), move |_, devices| {
            for phone in watcher_core.phones() {
                phone.on_devices(devices);
            }
            watcher_ctx.request_repaint();
        });
        core
    }

    pub fn server_path(&self) -> Result<PathBuf> {
        match &self.server_path {
            Ok(path) => Ok(path.clone()),
            Err(error) => Err(AppError::ServerStart(error.to_string())),
        }
    }

    pub fn phones(&self) -> Vec<Arc<Phone>> {
        self.phones.lock().unwrap().clone()
    }

    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.lock().unwrap();
        change(&mut settings);
        self.mirrors.set_clipboard_sync(settings.clipboard_sync);
        let saved = settings_path().and_then(|path| {
            let bytes = serde_json::to_vec_pretty(&*settings)
                .map_err(|error| AppError::Invalid(error.to_string()))?;
            Ok(std::fs::write(path, bytes)?)
        });
        if let Err(error) = saved {
            log::warn!("could not save settings: {error}");
        }
    }

    pub fn profile(&self, package: Option<&str>) -> AppProfile {
        self.settings.lock().unwrap().profiles.get(profile_key(package)).cloned().unwrap_or_default()
    }

    pub fn update_profile(&self, package: Option<&str>, change: impl FnOnce(&mut AppProfile)) {
        self.update_settings(|settings| {
            let key = profile_key(package);
            let mut profile = settings.profiles.remove(key).unwrap_or_default();
            change(&mut profile);
            if profile != AppProfile::default() {
                settings.profiles.insert(key.to_string(), profile);
            }
        });
    }

    pub fn save_workspace(&self, name: &str) {
        let windows = self.phones().iter().map(|phone| phone.as_workspace_window()).collect();
        let workspace = Workspace { name: name.to_string(), windows };
        self.update_settings(|settings| store_workspace(&mut settings.workspaces, workspace));
    }

    /// Opens the windows of a workspace whose phones are connected.
    pub fn open_workspace(self: &Arc<Self>, ctx: &Context, workspace: &Workspace) {
        let devices = self.registry.devices();
        let online = |window: &&WorkspaceWindow| devices.iter().any(|device| device.serial == window.serial && device.state == DeviceState::Online);
        for window in workspace.windows.iter().filter(online) {
            self.open_phone_at(ctx, &window.serial, window.package.clone(), window.title.clone(), window.position);
        }
    }

    /// Opens a phone window, or brings forward the one already showing this target.
    pub fn open_phone(self: &Arc<Self>, ctx: &Context, serial: &str, package: Option<String>, title: String) {
        self.open_phone_at(ctx, serial, package, title, None);
    }

    fn open_phone_at(self: &Arc<Self>, ctx: &Context, serial: &str, package: Option<String>, title: String, position: Option<[f32; 2]>) {
        if let Some(open) = self.phones().iter().find(|phone| phone.shows(serial, package.as_deref())) {
            ctx.send_viewport_cmd_to(open.id, ViewportCommand::Focus);
            return;
        }
        let number = self.next_window.fetch_add(1, Ordering::Relaxed);
        let offset = CASCADE_ORIGIN + (number % CASCADE_LENGTH) as f32 * CASCADE_STEP;
        let position = position.or(self.profile(package.as_deref()).position).map_or(pos2(offset, offset), Pos2::from);
        let phone = Phone::new(self.clone(), ctx.clone(), number, serial, package, title, position);
        self.phones.lock().unwrap().push(phone.clone());
        phone.start();
        ctx.request_repaint();
    }

    #[cfg(debug_assertions)]
    pub fn open_sample_phone(self: &Arc<Self>, ctx: &Context, editing_keys: bool) {
        let phone = Phone::new(self.clone(), ctx.clone(), 0, "sample", Some("sample.Game".to_string()), "Game".to_string(), pos2(60.0, 40.0));
        phone.show_sample_picture(editing_keys);
        self.phones.lock().unwrap().push(phone);
    }

    pub fn forget_phone(&self, id: ViewportId) {
        self.phones.lock().unwrap().retain(|phone| phone.id != id);
    }

    /// Lines the phone windows up from the left edge of the screen.
    pub fn arrange(&self, ctx: &Context) {
        let mut x = SCREEN_MARGIN;
        for phone in self.phones() {
            ctx.send_viewport_cmd_to(phone.id, ViewportCommand::OuterPosition(Pos2::new(x, SCREEN_MARGIN)));
            ctx.send_viewport_cmd_to(phone.id, ViewportCommand::Focus);
            x += phone_window_size(phone.video.size()).map_or(0.0, |size| size.x) + WINDOW_GAP;
        }
    }

    /// Moves keyboard focus to the phone window before or after `current`.
    pub fn focus_adjacent(&self, ctx: &Context, current: ViewportId, step: isize) {
        let phones = self.phones();
        let Some(position) = phones.iter().position(|phone| phone.id == current) else { return };
        let next = (position as isize + step).rem_euclid(phones.len() as isize) as usize;
        ctx.send_viewport_cmd_to(phones[next].id, ViewportCommand::Focus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_keep_working() {
        let settings: Settings = serde_json::from_str(r#"{"dark":false}"#).unwrap();
        assert!(!settings.dark);
        assert!(settings.clipboard_sync);
        assert_eq!(settings.options, MirrorOptions::default());
    }
}
