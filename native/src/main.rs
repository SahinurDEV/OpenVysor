#![cfg_attr(not(test), windows_subsystem = "windows")]

use eframe::egui::ViewportBuilder;
use openvysor_native::app::App;

/// `--open <package>` opens that app in a window as soon as a phone is ready.
fn package_to_open() -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    arguments.find(|argument| argument == "--open").and_then(|_| arguments.next())
}

fn main() -> eframe::Result {
    env_logger::builder().filter_level(log::LevelFilter::Warn).init();
    let mut viewport = ViewportBuilder::default()
        .with_title("OpenVysor")
        .with_inner_size([1040.0, 680.0])
        .with_min_inner_size([760.0, 520.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../resources/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // Windows redraw when a frame arrives, so waiting for vsync would only add delay.
    options.glow_options.vsync = false;
    let package = package_to_open();
    eframe::run_native("OpenVysor", options, Box::new(|context| Ok(Box::new(App::new(context, package)))))
}
