pub mod adb;
pub mod clipboard;
pub mod commands;
pub mod error;
pub mod mirroring;
pub mod transfer;

use std::collections::HashMap;
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
            let mirrors = Arc::new(MirrorManager::new());
            let device_clipboard = Mutex::new(clipboard::spawn(mirrors.clone()));
            mirrors.set_clipboard_handler(Arc::new(move |text| {
                let _ = device_clipboard.lock().unwrap().send(text);
            }));
            app.manage(AppState {
                registry,
                mirrors,
                windows: Mutex::new(Vec::new()),
                recordings: Mutex::new(HashMap::new()),
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
            commands::set_clipboard_sync,
            commands::send_files,
            commands::wireless_pair,
            commands::wireless_connect,
            commands::wireless_disconnect,
            commands::wireless_poll_qr,
            commands::recording_start,
            commands::recording_chunk,
            commands::recording_stop,
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
