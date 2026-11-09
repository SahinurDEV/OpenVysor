pub mod adb;
pub mod commands;
pub mod error;
pub mod mirroring;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use adb::connection::{DeviceRegistry, spawn_watcher};
use commands::windows::WINDOW_PREFIX;
use commands::{AppState, DevicesPayload, EVENT_DEVICES};
use mirroring::MirrorManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Info).build())
        .setup(|app| {
            let registry = Arc::new(DeviceRegistry::default());
            let handle = app.handle().clone();
            spawn_watcher(registry.clone(), move |status, devices| {
                let payload = DevicesPayload { status: status.clone(), devices: devices.to_vec() };
                let _ = handle.emit(EVENT_DEVICES, payload);
            });
            app.manage(AppState {
                registry,
                mirrors: Arc::new(MirrorManager::new()),
                windows: Mutex::new(Vec::new()),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                if label.starts_with(WINDOW_PREFIX) {
                    let state = window.state::<AppState>();
                    state.windows.lock().unwrap().retain(|entry| entry.label != label);
                    let mirrors = state.mirrors.clone();
                    // Stopping runs adb, which must not block the main thread.
                    std::thread::spawn(move || {
                        let _ = mirrors.stop(&label);
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_devices,
            commands::active_mirrors,
            commands::list_apps,
            commands::start_mirror,
            commands::stop_mirror,
            commands::mirror_input,
            commands::take_screenshot,
            commands::windows::open_mirror_window,
            commands::windows::arrange_mirror_windows,
            commands::windows::focus_adjacent_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OpenVysor");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<AppState>().mirrors.stop_all();
        }
    });
}
