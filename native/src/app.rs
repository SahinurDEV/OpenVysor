//! The main window: devices, apps, wireless pairing and settings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, CentralPanel, Color32, ComboBox, Context, CursorIcon, FontId, Frame, Label, Layout, Margin, Panel,
    Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Theme, Ui, Vec2, pos2, vec2,
};
use qrcode::{Color, QrCode};

use crate::adb::device::{ConnectionType, Device, DeviceState};
use crate::adb::wireless;
use crate::mirroring::apps::AppInfo;
use crate::state::{Core, Settings};
use crate::theme::{self, ButtonKind, Glyph, Palette, palette, semibold};

const NAV_WIDTH: f32 = 220.0;
const CARD_MIN_WIDTH: f32 = 360.0;
const TILE_MIN_WIDTH: f32 = 220.0;
const TILE_HEIGHT: f32 = 64.0;
const SETTINGS_WIDTH: f32 = 760.0;
const QR_SIDE: f32 = 208.0;
const QR_POLL: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Devices,
    Apps,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PairMode {
    Qr,
    Code,
    Address,
}

fn hex_token(bytes: usize) -> String {
    let mut values = vec![0u8; bytes];
    // Pairing secrets must never fall back to something predictable.
    getrandom::fill(&mut values).expect("the system random source is unavailable");
    values.iter().map(|value| format!("{value:02x}")).collect()
}

fn pairing_qr_text(name: &str, password: &str) -> String {
    format!("WIFI:T:ADB;S:{name};P:{password};;")
}

#[derive(Default)]
struct QrStatus {
    paired: bool,
    error: Option<String>,
}

struct QrPairing {
    modules: Vec<bool>,
    width: usize,
    status: Arc<Mutex<QrStatus>>,
    stop: Arc<AtomicBool>,
}

impl QrPairing {
    fn start(core: &Arc<Core>, ctx: &Context) -> Option<Self> {
        let name = format!("studio-{}", hex_token(4));
        let password = hex_token(8);
        let code = QrCode::new(pairing_qr_text(&name, &password)).ok()?;
        let width = code.width();
        let modules = code.to_colors().into_iter().map(|color| color == Color::Dark).collect();
        let status = Arc::new(Mutex::new(QrStatus::default()));
        let stop = Arc::new(AtomicBool::new(false));

        let (thread_core, thread_ctx, thread_status, thread_stop) = (core.clone(), ctx.clone(), status.clone(), stop.clone());
        thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let result = thread_core
                    .registry
                    .adb()
                    .and_then(|adb| wireless::poll_qr_pairing(&adb, &name, &password));
                let mut status = thread_status.lock().unwrap();
                match result {
                    Ok(true) => {
                        status.paired = true;
                        status.error = None;
                        thread_ctx.request_repaint();
                        return;
                    }
                    Ok(false) => status.error = None,
                    Err(error) => status.error = Some(error.to_string()),
                }
                drop(status);
                thread_ctx.request_repaint();
                thread::sleep(QR_POLL);
            }
        });
        Some(Self { modules, width, status, stop })
    }

    fn draw(&self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(QR_SIDE), Sense::hover());
        ui.painter().rect_filled(rect, 10.0, Color32::WHITE);
        let area = rect.shrink(14.0);
        let cell = area.width() / self.width as f32;
        for (index, _) in self.modules.iter().enumerate().filter(|(_, dark)| **dark) {
            let (column, row) = (index % self.width, index / self.width);
            let corner = area.min + Vec2::new(column as f32 * cell, row as f32 * cell);
            ui.painter().rect_filled(Rect::from_min_size(corner, Vec2::splat(cell + 0.5)), 0.0, Color32::BLACK);
        }
    }
}

impl Drop for QrPairing {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct AppList {
    serial: Option<String>,
    loading: bool,
    apps: Vec<AppInfo>,
    error: Option<String>,
}

type TaskResult = Arc<Mutex<Option<Result<String, String>>>>;

pub struct App {
    core: Arc<Core>,
    page: Page,
    show_wireless: bool,
    pair_mode: PairMode,
    qr: Option<QrPairing>,
    address: String,
    code: String,
    pair_busy: Arc<AtomicBool>,
    pair_result: TaskResult,
    app_list: Arc<Mutex<AppList>>,
    selected_serial: Option<String>,
    search: String,
    show_system_apps: bool,
    package_to_open: Option<String>,
    #[cfg(debug_assertions)]
    preview: String,
}

pub fn device_name(device: &Device) -> String {
    device
        .details
        .as_ref()
        .and_then(|details| details.model.clone())
        .or_else(|| device.model.clone())
        .unwrap_or_else(|| "Android device".to_string())
}

fn state_label(state: DeviceState) -> &'static str {
    match state {
        DeviceState::Online => "Ready",
        DeviceState::Offline => "Offline",
        DeviceState::Unauthorized => "Needs authorization",
        DeviceState::Authorizing => "Authorizing",
        DeviceState::Connecting => "Connecting",
        DeviceState::NoPermissions => "No permission",
        DeviceState::Other => "Unavailable",
    }
}

const NO_PERMISSION_HELP: &str = if cfg!(windows) {
    "Windows could not open this device. Reinstall its USB driver."
} else if cfg!(target_os = "linux") {
    "Linux blocked access to this phone. Install your distribution's Android udev rules, then replug the cable."
} else {
    "This computer could not open the phone. Replug the cable."
};

const ADB_INSTALL_COMMAND: &str = if cfg!(windows) {
    "winget install Google.PlatformTools"
} else if cfg!(target_os = "macos") {
    "brew install android-platform-tools"
} else {
    "sudo apt install adb"
};

fn state_help(state: DeviceState) -> Option<&'static str> {
    match state {
        DeviceState::Unauthorized => Some("Unlock your phone and tap Allow on the “Allow USB debugging?” prompt."),
        DeviceState::Offline => Some("The phone is not responding. Replug the cable or toggle USB debugging."),
        DeviceState::NoPermissions => Some(NO_PERMISSION_HELP),
        _ => None,
    }
}

fn columns_for(width: f32, minimum: f32, gap: f32) -> usize {
    (((width + gap) / (minimum + gap)).floor() as usize).max(1)
}

fn page_header(ui: &mut Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.heading(title);
            ui.weak(subtitle);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), actions);
    });
    ui.add_space(18.0);
}

fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(semibold(13.0)).color(palette(ui).secondary));
    ui.add_space(2.0);
}

/// One line of a settings card: description on the left, control on the right.
fn setting_row(ui: &mut Ui, title: &str, hint: &str, control: impl FnOnce(&mut Ui)) {
    let size = vec2(ui.available_width(), 44.0);
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
        control(ui);
        ui.allocate_ui_with_layout(ui.available_size(), Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(3.0);
            ui.label(RichText::new(title).font(semibold(14.0)));
            ui.add(Label::new(RichText::new(hint).small().weak()).truncate());
        });
    });
}

fn row_divider(ui: &mut Ui) {
    let colors = palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, colors.border);
}

fn numbered_step(ui: &mut Ui, number: usize, text: &str) {
    let colors = palette(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 12.0, colors.muted);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, number.to_string(), semibold(12.0), colors.foreground);
        ui.label(text);
    });
}

fn choice_combo<T: Copy + PartialEq>(ui: &mut Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    let current = options.iter().find(|(option, _)| option == value).map_or("Custom", |(_, label)| label);
    ComboBox::from_id_salt(id).width(130.0).selected_text(current).show_ui(ui, |ui| {
        for (option, label) in options {
            ui.selectable_value(value, *option, *label);
        }
    });
}

impl App {
    pub fn new(context: &eframe::CreationContext<'_>, package_to_open: Option<String>) -> Self {
        theme::apply(&context.egui_ctx);
        let core = Core::new(&context.egui_ctx);
        #[allow(unused_mut)]
        let mut app = Self {
            core,
            page: Page::Devices,
            show_wireless: false,
            pair_mode: PairMode::Qr,
            qr: None,
            address: String::new(),
            code: String::new(),
            pair_busy: Arc::default(),
            pair_result: Arc::default(),
            app_list: Arc::default(),
            selected_serial: None,
            search: String::new(),
            show_system_apps: false,
            package_to_open,
            #[cfg(debug_assertions)]
            preview: std::env::var("OPENVYSOR_PREVIEW").unwrap_or_default(),
        };
        #[cfg(debug_assertions)]
        app.apply_preview();
        app
    }

    /// Debug builds can open a given screen with sample data, so screenshots
    /// of the design never show what is really on the phone.
    #[cfg(debug_assertions)]
    fn apply_preview(&mut self) {
        match self.preview.as_str() {
            "apps" => self.page = Page::Apps,
            "settings" => self.page = Page::Settings,
            "wireless" => self.show_wireless = true,
            "light" => self.core.settings.lock().unwrap().dark = false,
            "dark" => self.core.settings.lock().unwrap().dark = true,
            _ => {}
        }
    }

    #[cfg(debug_assertions)]
    fn fill_preview_apps(&self, serial: &str) -> bool {
        if self.preview != "apps" {
            return false;
        }
        let names = ["Calculator", "Calendar", "Camera", "Chrome", "Clock", "Contacts", "Drive", "Files", "Gmail", "Maps", "Messages", "Music", "Notes", "Photos", "Weather", "YouTube"];
        let apps = names.iter().map(|name| AppInfo { name: (*name).to_string(), package: format!("sample.{name}"), system: false }).collect();
        *self.app_list.lock().unwrap() = AppList { serial: Some(serial.to_string()), loading: false, apps, error: None };
        true
    }

    fn visible_devices(&self) -> Vec<Device> {
        #[cfg(debug_assertions)]
        if self.preview == "empty" {
            return Vec::new();
        }
        self.core.registry.devices()
    }

    fn open_requested_package(&mut self, ctx: &Context) {
        if self.package_to_open.is_none() {
            return;
        }
        let devices = self.core.registry.devices();
        let Some(device) = devices.iter().find(|device| device.state == DeviceState::Online) else { return };
        if let Some(package) = self.package_to_open.take() {
            let title = package.rsplit('.').next().unwrap_or(&package).to_string();
            self.core.open_phone(ctx, &device.serial, Some(package), title);
        }
    }

    fn nav_item(&mut self, ui: &mut Ui, page: Page, glyph: Glyph, label: &str, colors: &Palette) {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
        let active = self.page == page;
        if active || response.hovered() {
            ui.painter().rect_filled(rect, 8.0, if active { colors.muted } else { colors.muted.gamma_multiply(0.5) });
        }
        if active {
            let bar = Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, 18.0));
            ui.painter().rect_filled(bar, 2.0, colors.primary);
        }
        let tint = if active { colors.foreground } else { colors.secondary };
        theme::glyph(ui.painter(), glyph, pos2(rect.left() + 24.0, rect.center().y), 18.0, tint);
        let font = if active { semibold(14.0) } else { FontId::proportional(14.0) };
        ui.painter().text(pos2(rect.left() + 44.0, rect.center().y), egui::Align2::LEFT_CENTER, label, font, tint);
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.page = page;
        }
    }

    fn navigation(&mut self, ui: &mut Ui) {
        let colors = palette(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            theme::logo(ui.painter(), rect);
            ui.add_space(2.0);
            ui.label(RichText::new("OpenVysor").font(semibold(17.0)).color(colors.foreground));
        });
        ui.add_space(20.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        self.nav_item(ui, Page::Devices, Glyph::Phone, "Devices", colors);
        self.nav_item(ui, Page::Apps, Glyph::Grid, "Apps", colors);
        self.nav_item(ui, Page::Settings, Glyph::Gear, "Settings", colors);

        let status = self.core.registry.status();
        let windows = self.core.phones().len();
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            let (dot, text) = if status.available { (colors.success, "ADB connected") } else { (colors.danger, "ADB not found") };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, dot);
                ui.label(RichText::new(text).small().color(colors.secondary));
            });
            if windows > 0 {
                let label = if windows == 1 { "1 window open".to_string() } else { format!("{windows} windows open") };
                ui.label(RichText::new(label).small().color(colors.secondary));
            }
        });
    }

    fn devices_page(&mut self, ui: &mut Ui) {
        let status = self.core.registry.status();
        let devices = self.visible_devices();
        let online = devices.iter().filter(|device| device.state == DeviceState::Online).count();
        let subtitle = match online {
            0 => "Connect a phone to get started".to_string(),
            1 => "1 phone ready".to_string(),
            count => format!("{count} phones ready"),
        };
        page_header(ui, "Devices", &subtitle, |ui| {
            let label = if self.show_wireless { "Hide Wi-Fi" } else { "Connect over Wi-Fi" };
            if theme::button(ui, ButtonKind::Secondary, label, status.available).clicked() {
                self.show_wireless = !self.show_wireless;
                self.qr = None;
            }
            if !self.core.phones().is_empty() && theme::button(ui, ButtonKind::Quiet, "Arrange windows", true).clicked() {
                self.core.arrange(ui.ctx());
            }
        });
        if self.show_wireless {
            self.wireless_panel(ui);
            ui.add_space(12.0);
        }
        if !status.available {
            self.adb_missing(ui);
        } else if devices.is_empty() {
            self.no_devices(ui);
        }

        const GAP: f32 = 14.0;
        let columns = columns_for(ui.available_width(), CARD_MIN_WIDTH, GAP);
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        for row in devices.chunks(columns) {
            ui.columns(columns, |slots| {
                for (slot, device) in slots.iter_mut().zip(row) {
                    self.device_card(slot, device);
                }
            });
        }
    }

    fn adb_missing(&self, ui: &mut Ui) {
        theme::card(ui, |ui| {
            ui.label(RichText::new("ADB was not found").font(semibold(16.0)));
            ui.label("OpenVysor uses Android's ADB tool. Install it from a terminal, then restart OpenVysor:");
            ui.add_space(4.0);
            Frame::NONE.fill(palette(ui).muted).corner_radius(8).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(Label::new(RichText::new(ADB_INSTALL_COMMAND).monospace()));
            });
        });
    }

    fn no_devices(&self, ui: &mut Ui) {
        let colors = palette(ui);
        ui.add_space(36.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 36.0, colors.muted);
            theme::glyph(ui.painter(), Glyph::Phone, rect.center(), 34.0, colors.secondary);
            ui.add_space(6.0);
            ui.label(RichText::new("No phone connected").font(semibold(18.0)));
            ui.weak("Plug in your Android phone with a USB cable");
            ui.add_space(12.0);
        });
        let width = 440.0_f32.min(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.allocate_ui(vec2(width, 0.0), |ui| {
                theme::card(ui, |ui| {
                    numbered_step(ui, 1, "On your phone, open Settings, then Developer options");
                    numbered_step(ui, 2, "Turn on USB debugging");
                    numbered_step(ui, 3, "Connect the USB cable and tap Allow on the phone");
                });
            });
        });
    }

    fn device_card(&mut self, ui: &mut Ui, device: &Device) {
        let colors = palette(ui);
        theme::card(ui, |ui| {
            let online = device.state == DeviceState::Online;
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                ui.painter().rect_filled(rect, 12.0, colors.primary.gamma_multiply(0.14));
                theme::glyph(ui.painter(), Glyph::Phone, rect.center(), 28.0, colors.primary);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(device_name(device)).font(semibold(17.0)));
                    let connection = match device.connection {
                        ConnectionType::Usb => "USB",
                        ConnectionType::Wireless => "Wi-Fi",
                        ConnectionType::Emulator => "Emulator",
                    };
                    let version = device.details.as_ref().and_then(|d| d.android_version.as_ref());
                    let text = version.map_or_else(|| connection.to_string(), |v| format!("{connection}  ·  Android {v}"));
                    ui.weak(text);
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    theme::pill(ui, state_label(device.state), if online { colors.success } else { colors.warning });
                });
            });
            if let Some(details) = &device.details {
                ui.add_space(4.0);
                self.device_facts(ui, details.resolution.as_deref(), details.battery, colors);
            }
            if let Some(help) = state_help(device.state) {
                ui.colored_label(colors.warning, help);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::button(ui, ButtonKind::Primary, "Mirror screen", online).clicked() {
                    self.core.open_phone(ui.ctx(), &device.serial, None, device_name(device));
                }
                if theme::button(ui, ButtonKind::Secondary, "Open an app", online).clicked() {
                    self.selected_serial = Some(device.serial.clone());
                    self.page = Page::Apps;
                }
                if device.connection == ConnectionType::Wireless
                    && theme::button(ui, ButtonKind::Quiet, "Disconnect", true).clicked()
                {
                    self.disconnect_wireless(&device.serial);
                }
            });
        });
    }

    fn device_facts(&self, ui: &mut Ui, resolution: Option<&str>, battery: Option<u8>, colors: &Palette) {
        ui.horizontal(|ui| {
            if let Some(resolution) = resolution {
                ui.label(RichText::new(resolution).small().color(colors.secondary));
            }
            if let Some(battery) = battery {
                ui.add_space(8.0);
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 12.0), Sense::hover());
                ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, colors.secondary), StrokeKind::Middle);
                let fill = Rect::from_min_size(rect.min, vec2(rect.width() * f32::from(battery.min(100)) / 100.0, rect.height())).shrink(2.0);
                let color = if battery <= 15 { colors.danger } else { colors.success };
                ui.painter().rect_filled(fill, 1.5, color);
                ui.label(RichText::new(format!("{battery}%")).small().color(colors.secondary));
            }
        });
    }

    fn disconnect_wireless(&self, serial: &str) {
        let (core, serial) = (self.core.clone(), serial.to_string());
        thread::spawn(move || {
            let result = core.registry.adb_for_wireless(&serial).and_then(|adb| wireless::disconnect(&adb, &serial));
            if let Err(error) = result {
                log::warn!("disconnect: {error}");
            }
        });
    }

    fn wireless_panel(&mut self, ui: &mut Ui) {
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                theme::glyph(ui.painter(), Glyph::Wifi, rect.center(), 20.0, palette(ui).primary);
                ui.label(RichText::new("Connect over Wi-Fi").font(semibold(16.0)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let modes = [(PairMode::Address, "IP address"), (PairMode::Code, "Pairing code"), (PairMode::Qr, "QR code")];
                    if theme::segmented(ui, &modes, &mut self.pair_mode) {
                        self.qr = None;
                        *self.pair_result.lock().unwrap() = None;
                    }
                });
            });
            ui.add_space(10.0);
            match self.pair_mode {
                PairMode::Qr => self.qr_panel(ui),
                PairMode::Code | PairMode::Address => self.address_panel(ui),
            }
        });
    }

    fn qr_panel(&mut self, ui: &mut Ui) {
        if self.qr.is_none() {
            self.qr = QrPairing::start(&self.core, ui.ctx());
        }
        let colors = palette(ui);
        let Some(qr) = &self.qr else {
            ui.colored_label(colors.danger, "The QR code could not be created.");
            return;
        };
        ui.horizontal_top(|ui| {
            qr.draw(ui);
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.add_space(8.0);
                numbered_step(ui, 1, "Connect the phone and this computer to the same Wi-Fi");
                numbered_step(ui, 2, "On the phone, open Developer options, then Wireless debugging");
                numbered_step(ui, 3, "Tap “Pair device with QR code” and scan this code");
                ui.add_space(10.0);
                let status = qr.status.lock().unwrap();
                if status.paired {
                    ui.colored_label(colors.success, "Paired. The phone will appear in your devices.");
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("Waiting for the phone…");
                    });
                }
                if let Some(error) = &status.error {
                    ui.colored_label(colors.danger, error);
                }
            });
        });
    }

    fn address_panel(&mut self, ui: &mut Ui) {
        let colors = palette(ui);
        let pairing = self.pair_mode == PairMode::Code;
        ui.weak(if pairing {
            "On the phone, open Wireless debugging, then “Pair device with pairing code”."
        } else {
            "Use the IP address and port shown at the top of Wireless debugging."
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut self.address).hint_text("192.168.1.20:37000").desired_width(240.0).margin(vec2(10.0, 9.0)));
            if pairing {
                ui.add(TextEdit::singleline(&mut self.code).hint_text("6-digit code").desired_width(130.0).margin(vec2(10.0, 9.0)));
            }
            let busy = self.pair_busy.load(Ordering::Relaxed);
            let enabled = !busy && !self.address.trim().is_empty();
            if theme::button(ui, ButtonKind::Primary, if pairing { "Pair" } else { "Connect" }, enabled).clicked() {
                self.submit_address(ui.ctx(), pairing);
            }
            if busy {
                ui.spinner();
            }
        });
        match &*self.pair_result.lock().unwrap() {
            Some(Ok(text)) => {
                ui.colored_label(colors.success, text);
            }
            Some(Err(text)) => {
                ui.colored_label(colors.danger, text);
            }
            None => {}
        }
    }

    fn submit_address(&self, ctx: &Context, pairing: bool) {
        let (core, ctx) = (self.core.clone(), ctx.clone());
        let (busy, slot) = (self.pair_busy.clone(), self.pair_result.clone());
        let (address, code) = (self.address.trim().to_string(), self.code.trim().to_string());
        busy.store(true, Ordering::Relaxed);
        *slot.lock().unwrap() = None;
        thread::spawn(move || {
            let result = core.registry.adb().and_then(|adb| {
                if pairing {
                    wireless::pair(&adb, &address, &code).map(|()| "Paired. Now connect using the IP address tab.".to_string())
                } else {
                    wireless::connect(&adb, &address)
                }
            });
            *slot.lock().unwrap() = Some(result.map_err(|error| error.to_string()));
            busy.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    fn apps_page(&mut self, ui: &mut Ui) {
        let colors = palette(ui);
        let devices: Vec<Device> = self.visible_devices().into_iter().filter(|d| d.state == DeviceState::Online).collect();
        if !devices.iter().any(|device| Some(&device.serial) == self.selected_serial.as_ref()) {
            self.selected_serial = devices.first().map(|device| device.serial.clone());
        }
        page_header(ui, "Apps", "Each app opens in its own phone-shaped window", |ui| {
            if devices.len() > 1 {
                let selected = devices.iter().find(|d| Some(&d.serial) == self.selected_serial.as_ref()).map_or_else(String::new, device_name);
                ComboBox::from_id_salt("device").selected_text(selected).show_ui(ui, |ui| {
                    for device in &devices {
                        ui.selectable_value(&mut self.selected_serial, Some(device.serial.clone()), device_name(device));
                    }
                });
            }
        });
        let Some(serial) = self.selected_serial.clone() else {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No phone connected").font(semibold(16.0)));
                ui.weak("Connect a phone to see its apps");
            });
            return;
        };
        self.load_apps_if_needed(ui.ctx(), &serial);

        let list = self.app_list.lock().unwrap();
        if list.loading {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.spinner();
                ui.weak("Loading apps…");
            });
            return;
        }
        if let Some(error) = &list.error {
            ui.colored_label(colors.danger, error);
        }

        ui.horizontal(|ui| {
            let edit = TextEdit::singleline(&mut self.search)
                .hint_text("Search apps")
                .desired_width(300.0)
                .margin(Margin { left: 34, right: 10, top: 9, bottom: 9 });
            let response = ui.add(edit);
            theme::glyph(ui.painter(), Glyph::Search, pos2(response.rect.left() + 18.0, response.rect.center().y), 16.0, colors.secondary);
            ui.add_space(12.0);
            theme::switch(ui, &mut self.show_system_apps);
            ui.label(RichText::new("Show system apps").color(colors.secondary));
        });
        ui.add_space(10.0);

        let needle = self.search.to_lowercase();
        let matching: Vec<&AppInfo> = list
            .apps
            .iter()
            .filter(|app| self.show_system_apps || !app.system)
            .filter(|app| app.name.to_lowercase().contains(&needle))
            .collect();
        if matching.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| ui.weak("No apps match your search"));
            return;
        }

        const GAP: f32 = 10.0;
        let width = ui.available_width() - 14.0;
        let columns = columns_for(width, TILE_MIN_WIDTH, GAP);
        let tile_width = (width - GAP * (columns - 1) as f32) / columns as f32;
        let rows = matching.len().div_ceil(columns);
        let mut chosen = None;
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        ScrollArea::vertical().auto_shrink(false).show_rows(ui, TILE_HEIGHT, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    for app in matching.iter().skip(row * columns).take(columns) {
                        if app_tile(ui, app, tile_width, colors) {
                            chosen = Some((*app).clone());
                        }
                    }
                });
            }
        });
        drop(list);
        if let Some(app) = chosen {
            self.core.open_phone(ui.ctx(), &serial, Some(app.package), app.name);
        }
    }

    fn load_apps_if_needed(&self, ctx: &Context, serial: &str) {
        #[cfg(debug_assertions)]
        if self.fill_preview_apps(serial) {
            return;
        }
        let mut list = self.app_list.lock().unwrap();
        if list.serial.as_deref() == Some(serial) {
            return;
        }
        *list = AppList { serial: Some(serial.to_string()), loading: true, ..AppList::default() };
        drop(list);
        let (core, slot, ctx, serial) = (self.core.clone(), self.app_list.clone(), ctx.clone(), serial.to_string());
        thread::spawn(move || {
            let result = core
                .registry
                .adb_for_online(&serial)
                .and_then(|adb| core.mirrors.list_apps(&adb, &serial, &core.server_path()?));
            let mut list = slot.lock().unwrap();
            if list.serial.as_deref() == Some(serial.as_str()) {
                list.loading = false;
                match result {
                    Ok(apps) => list.apps = apps,
                    Err(error) => list.error = Some(error.to_string()),
                }
            }
            ctx.request_repaint();
        });
    }

    fn settings_page(&mut self, ui: &mut Ui) {
        page_header(ui, "Settings", "Quality changes apply to newly opened windows", |_| {});
        let status = self.core.registry.status();
        let mut settings = self.core.settings.lock().unwrap().clone();
        let before = settings.clone();
        ui.set_max_width(SETTINGS_WIDTH);
        section_settings(ui, &mut settings, status.version);
        if settings != before {
            ui.ctx().set_theme(if settings.dark { Theme::Dark } else { Theme::Light });
            self.core.update_settings(|current| *current = settings);
        }
    }
}

fn app_tile(ui: &mut Ui, app: &AppInfo, width: f32, colors: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let border = if hovered { colors.primary } else { colors.border };
        ui.painter().rect(rect, 10.0, colors.card, Stroke::new(1.0, border), StrokeKind::Inside);
        theme::avatar(ui.painter(), pos2(rect.left() + 34.0, rect.center().y), 20.0, &app.name);
        let text_left = rect.left() + 66.0;
        let text_width = rect.right() - text_left - 12.0;
        let job = theme::one_line(&app.name, semibold(14.0), colors.foreground, text_width);
        let galley = ui.painter().layout_job(job);
        if app.system {
            ui.painter().galley(pos2(text_left, rect.center().y - galley.size().y), galley, colors.foreground);
            ui.painter().text(pos2(text_left, rect.center().y + 2.0), egui::Align2::LEFT_TOP, "System", FontId::proportional(12.0), colors.secondary);
        } else {
            ui.painter().galley(pos2(text_left, rect.center().y - galley.size().y / 2.0), galley, colors.foreground);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn section_settings(ui: &mut Ui, settings: &mut Settings, adb_version: Option<String>) {
    section_title(ui, "Appearance");
    theme::card(ui, |ui| {
        setting_row(ui, "Theme", "How OpenVysor looks", |ui| {
            theme::segmented(ui, &[(false, "Light"), (true, "Dark")], &mut settings.dark);
        });
    });
    ui.add_space(14.0);

    section_title(ui, "Mirroring");
    theme::card(ui, |ui| {
        setting_row(ui, "Resolution", "Lower is smoother on slow connections", |ui| {
            choice_combo(ui, "resolution", &mut settings.options.max_size, &[(0, "Native"), (2400, "2400p"), (1920, "1920p"), (1600, "1600p"), (1280, "1280p"), (1024, "1024p")]);
        });
        row_divider(ui);
        setting_row(ui, "Quality", "Higher is sharper and uses more bandwidth", |ui| {
            choice_combo(ui, "bitrate", &mut settings.options.bit_rate, &[(2_000_000, "2 Mbps"), (4_000_000, "4 Mbps"), (8_000_000, "8 Mbps"), (16_000_000, "16 Mbps"), (32_000_000, "32 Mbps")]);
        });
        row_divider(ui);
        setting_row(ui, "Frame rate", "Matches the phone's own limit at most", |ui| {
            choice_combo(ui, "fps", &mut settings.options.max_fps, &[(30, "30 FPS"), (60, "60 FPS"), (90, "90 FPS"), (120, "120 FPS")]);
        });
        row_divider(ui);
        setting_row(ui, "Phone sound on this computer", "The phone goes silent while mirroring", |ui| {
            theme::switch(ui, &mut settings.options.audio);
        });
    });
    ui.add_space(14.0);

    section_title(ui, "Privacy");
    theme::card(ui, |ui| {
        setting_row(ui, "Clipboard sync", "Text copied on either side, including passwords, reaches the other", |ui| {
            theme::switch(ui, &mut settings.clipboard_sync);
        });
    });
    ui.add_space(14.0);

    section_title(ui, "About");
    theme::card(ui, |ui| {
        setting_row(ui, "ADB", adb_version.as_deref().unwrap_or("Unavailable"), |_| {});
    });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_theme(if self.core.settings.lock().unwrap().dark { Theme::Dark } else { Theme::Light });

        self.open_requested_package(&ctx);
        for phone in self.core.phones() {
            let window = phone.clone();
            ctx.show_viewport_deferred(phone.id, phone.viewport_builder(), move |ui, _| window.show(ui));
        }

        let colors = palette(ui);
        let sidebar = Panel::left("navigation")
            .exact_size(NAV_WIDTH)
            .resizable(false)
            .frame(Frame::NONE.fill(colors.sidebar).inner_margin(Margin::same(14)))
            .show(ui, |ui| self.navigation(ui))
            .response
            .rect;
        CentralPanel::default()
            .frame(Frame::NONE.fill(colors.background).inner_margin(Margin::symmetric(32, 26)))
            .show(ui, |ui| match self.page {
                Page::Devices => {
                    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.devices_page(ui));
                }
                Page::Apps => self.apps_page(ui),
                Page::Settings => {
                    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.settings_page(ui));
                }
            });
        // Only the inner edge: a stroke on the panel also outlines the window.
        ui.painter().vline(sidebar.right(), sidebar.y_range(), Stroke::new(1.0, colors.border));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.core.mirrors.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_android_pairing_qr_text() {
        assert_eq!(pairing_qr_text("studio-1", "abc"), "WIFI:T:ADB;S:studio-1;P:abc;;");
    }

    #[test]
    fn tokens_are_hex_of_the_requested_size() {
        let token = hex_token(4);
        assert_eq!(token.len(), 8);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn layouts_always_have_at_least_one_column() {
        assert_eq!(columns_for(100.0, 360.0, 14.0), 1);
        assert_eq!(columns_for(734.0, 360.0, 14.0), 2);
        assert_eq!(columns_for(733.0, 360.0, 14.0), 1);
    }
}
