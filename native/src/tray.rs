//! The tray icon and the global shortcut. Linux is left out because the tray
//! library needs GTK there, which the app otherwise does not depend on.

use eframe::egui::{Context, ViewportCommand, ViewportId};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const SHORTCUT_MODIFIER: Modifiers = if cfg!(target_os = "macos") { Modifiers::META } else { Modifiers::CONTROL };

/// Keeps the icon and the shortcut registered for as long as the app runs.
pub struct Tray {
    _icon: TrayIcon,
    _hotkeys: Option<GlobalHotKeyManager>,
}

pub fn show_main_window(ctx: &Context) {
    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
    ctx.request_repaint_of(ViewportId::ROOT);
}

fn register_shortcut(ctx: &Context) -> Option<GlobalHotKeyManager> {
    let manager = GlobalHotKeyManager::new().inspect_err(|error| log::warn!("global shortcut: {error}")).ok()?;
    let shortcut = HotKey::new(Some(SHORTCUT_MODIFIER | Modifiers::SHIFT), Code::KeyO);
    // Another program may already own this combination, which costs only the shortcut.
    manager.register(shortcut).inspect_err(|error| log::warn!("global shortcut: {error}")).ok()?;
    let ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            show_main_window(&ctx);
        }
    }));
    Some(manager)
}

/// Must run on the main thread, after the event loop has started.
pub fn install(ctx: &Context) -> Option<Tray> {
    let image = eframe::icon_data::from_png_bytes(include_bytes!("../resources/icon.png")).ok()?;
    let icon = Icon::from_rgba(image.rgba, image.width, image.height).ok()?;
    let show = MenuItem::new("Show OpenVysor", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append(&show).ok()?;
    menu.append(&quit).ok()?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("OpenVysor")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .inspect_err(|error| log::warn!("tray icon: {error}"))
        .ok()?;

    let (show_id, quit_id, menu_ctx, click_ctx) = (show.id().clone(), quit.id().clone(), ctx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            show_main_window(&menu_ctx);
        } else if event.id == quit_id {
            menu_ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            menu_ctx.request_repaint_of(ViewportId::ROOT);
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main_window(&click_ctx);
        }
    }));
    Some(Tray { _icon: tray, _hotkeys: register_shortcut(ctx) })
}
