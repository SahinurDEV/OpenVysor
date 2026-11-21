//! Checks that need a signed-in desktop session, but no phone.
//!
//! cargo test --release --test desktop -- --ignored

#![cfg(any(windows, target_os = "macos"))]

use eframe::egui::{Context, RawInput, ViewportCommand, ViewportId};
use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use openvysor_native::tray;

#[test]
#[ignore = "needs a desktop session"]
fn tray_icon_and_global_shortcut_register() {
    let ctx = Context::default();
    ctx.set_embed_viewports(false);
    let tray = tray::install(&ctx).expect("the tray icon was not created");
    if !tray.has_shortcut() {
        // Only one program can own a combination, such as an OpenVysor that is already open.
        let modifier = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        let manager = GlobalHotKeyManager::new().unwrap();
        let error = manager
            .register(HotKey::new(Some(modifier | Modifiers::SHIFT), Code::KeyO))
            .unwrap_err();
        panic!(
            "the global shortcut could not be registered: {error}. Quit any running OpenVysor and try again."
        );
    }
    drop(tray);
    println!("tray icon created and Ctrl+Shift+O registered");
}

#[test]
fn showing_the_main_window_restores_and_focuses_it() {
    let ctx = Context::default();
    ctx.set_embed_viewports(false);
    let output = ctx.run_ui(RawInput::default(), |_| tray::show_main_window(&ctx));
    let commands = &output.viewport_output[&ViewportId::ROOT].commands;
    assert!(
        commands.contains(&ViewportCommand::Minimized(false)),
        "{commands:?}"
    );
    assert!(commands.contains(&ViewportCommand::Focus), "{commands:?}");
}
