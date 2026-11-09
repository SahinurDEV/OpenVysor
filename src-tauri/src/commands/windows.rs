use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use tauri::{AppHandle, LogicalPosition, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::AppState;
use crate::error::{AppError, Result};
use crate::mirroring::apps::validate_package;

pub const WINDOW_PREFIX: &str = "mirror-";
const PHONE_WIDTH: f64 = 342.0;
const PHONE_HEIGHT: f64 = 792.0;
const MARGIN: f64 = 16.0;
const GAP: f64 = 10.0;
const ROW_OFFSET: f64 = 36.0;

static NEXT_WINDOW: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedWindow {
    pub label: String,
    pub serial: String,
    pub package: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhoneWindowInfo<'a> {
    session: &'a str,
    serial: &'a str,
    package: Option<&'a str>,
    title: &'a str,
}

/// Lays windows out left to right, starting a new row when one would not fit.
/// Returns each window's horizontal offset and row number.
pub fn row_positions(widths: &[f64], area_width: f64) -> Vec<(f64, u32)> {
    let mut x = MARGIN;
    let mut row = 0;
    widths
        .iter()
        .map(|width| {
            if x > MARGIN && x + width + MARGIN > area_width {
                x = MARGIN;
                row += 1;
            }
            let position = (x, row);
            x += width + GAP;
            position
        })
        .collect()
}

struct Area {
    x: f64,
    y: f64,
    width: f64,
}

fn screen_area(app: &AppHandle) -> Area {
    let monitor = app
        .get_webview_window("main")
        .and_then(|window| window.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    match monitor {
        Some(monitor) => {
            let scale = monitor.scale_factor();
            Area {
                x: f64::from(monitor.position().x) / scale,
                y: f64::from(monitor.position().y) / scale,
                width: f64::from(monitor.size().width) / scale,
            }
        }
        None => Area { x: 0.0, y: 0.0, width: 1280.0 },
    }
}

fn to_screen(area: &Area, (x, row): (f64, u32)) -> (f64, f64) {
    (area.x + x, area.y + MARGIN + f64::from(row) * ROW_OFFSET)
}

fn focus(window: &WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Opens a phone-shaped window. With a package it shows only that app on its
/// own virtual display, without one it shows the whole phone.
#[tauri::command(async)]
pub fn open_mirror_window(
    app: AppHandle,
    state: State<'_, AppState>,
    serial: String,
    package: Option<String>,
    title: String,
) -> Result<String> {
    state.registry.adb_for_online(&serial)?;
    if let Some(package) = &package {
        validate_package(package)?;
    }

    // The lock is released before the window is built, because window events
    // on the main thread need it while this thread waits for the main thread.
    let (label, index) = {
        let mut windows = state.windows.lock().unwrap();
        let existing = windows
            .iter()
            .find(|window| window.serial == serial && window.package == package)
            .map(|window| window.label.clone());
        if let Some(label) = existing {
            drop(windows);
            if let Some(window) = app.get_webview_window(&label) {
                focus(&window);
            }
            return Ok(label);
        }
        let label = format!("{WINDOW_PREFIX}{}", NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
        windows.push(TrackedWindow {
            label: label.clone(),
            serial: serial.clone(),
            package: package.clone(),
        });
        (label, windows.len())
    };

    let area = screen_area(&app);
    let widths = vec![PHONE_WIDTH; index];
    let (x, y) = to_screen(&area, row_positions(&widths, area.width)[index - 1]);
    let info = PhoneWindowInfo {
        session: &label,
        serial: &serial,
        package: package.as_deref(),
        title: &title,
    };
    let script = format!(
        "window.__OPENVYSOR_PHONE__ = {};",
        serde_json::to_string(&info).map_err(|error| AppError::Invalid(error.to_string()))?
    );

    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(&title)
        .initialization_script(&script)
        .inner_size(PHONE_WIDTH, PHONE_HEIGHT)
        .min_inner_size(200.0, 280.0)
        .position(x, y)
        .decorations(false)
        .shadow(true)
        .build();

    if let Err(error) = built {
        state.windows.lock().unwrap().retain(|window| window.label != label);
        return Err(AppError::Invalid(error.to_string()));
    }
    Ok(label)
}

#[tauri::command(async)]
pub fn arrange_mirror_windows(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let tracked = state.windows.lock().unwrap().clone();
    let windows: Vec<WebviewWindow> =
        tracked.iter().filter_map(|window| app.get_webview_window(&window.label)).collect();
    let area = screen_area(&app);
    let widths: Vec<f64> = windows
        .iter()
        .map(|window| {
            let scale = window.scale_factor().unwrap_or(1.0);
            window.outer_size().map_or(PHONE_WIDTH, |size| f64::from(size.width) / scale)
        })
        .collect();
    for (window, slot) in windows.iter().zip(row_positions(&widths, area.width)) {
        let (x, y) = to_screen(&area, slot);
        let _ = window.unminimize();
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    Ok(())
}

/// Focuses the window to the left (`direction` < 0) or right, wrapping around.
#[tauri::command(async)]
pub fn focus_adjacent_window(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    direction: i32,
) -> Result<()> {
    let tracked = state.windows.lock().unwrap().clone();
    let mut windows: Vec<(i32, WebviewWindow)> = tracked
        .iter()
        .filter_map(|entry| app.get_webview_window(&entry.label))
        .filter_map(|candidate| Some((candidate.outer_position().ok()?.x, candidate)))
        .collect();
    windows.sort_by_key(|(x, _)| *x);
    let Some(current) = windows.iter().position(|(_, other)| other.label() == window.label()) else {
        return Ok(());
    };
    let count = windows.len() as i32;
    let target = (current as i32 + direction.signum()).rem_euclid(count) as usize;
    focus(&windows[target].1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_windows_side_by_side() {
        assert_eq!(row_positions(&[342.0, 342.0], 1920.0), vec![(16.0, 0), (368.0, 0)]);
    }

    #[test]
    fn wraps_to_a_new_row_when_the_screen_is_full() {
        let positions = row_positions(&[342.0; 3], 800.0);
        assert_eq!(positions[1], (368.0, 0));
        assert_eq!(positions[2], (16.0, 1));
    }

    #[test]
    fn keeps_a_window_wider_than_the_screen_on_the_first_row() {
        assert_eq!(row_positions(&[2000.0], 800.0), vec![(16.0, 0)]);
    }
}
