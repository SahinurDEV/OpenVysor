//! A borderless window showing one phone or one app.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, Align, CentralPanel, Color32, Context, Event, Frame, Key, Layout, MouseWheelUnit, Painter, Panel,
    PointerButton, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
    ViewportBuilder, ViewportCommand, ViewportId, pos2, vec2,
};

use crate::adb::commands::screenshot;
use crate::adb::device::{Device, DeviceState};
use crate::audio;
use crate::error::{AppError, Result};
use crate::input::{
    ACTION_DOWN, ACTION_MOVE, ACTION_UP, KEYCODE_APP_SWITCH, KEYCODE_BACK, KEYCODE_COPY, KEYCODE_CUT, KEYCODE_HOME,
    POINTER_ID_FINGER, STRIP_HEIGHT, android_keycode, android_metastate, fit_inside, phone_window_size,
    to_video_point, wheel_to_scroll,
};
use crate::mirroring::StartRequest;
use crate::mirroring::apps::validate_package;
use crate::mirroring::protocol::ControlMessage;
use crate::profiles::{KeyBinding, WorkspaceWindow, apply_quality, binding_at, set_binding};
use crate::state::Core;
use crate::theme::{self, ButtonKind};
use crate::transfer::{self, TransferAction, TransferResult};
use crate::video::{Video, packet_sink};

const NOTICE_LIFETIME: Duration = Duration::from_secs(6);
const INITIAL_SIZE: [f32; 2] = [342.0, 792.0];
const MIN_SIZE: [f32; 2] = [180.0, 240.0];
const STRIP_COLOR: Color32 = Color32::from_rgb(16, 19, 22);
const RECORD_COLOR: Color32 = Color32::from_rgb(239, 68, 68);
const ACTIVE_COLOR: Color32 = Color32::from_rgb(52, 211, 153);
const KEY_MARK_RADIUS: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyMode {
    Off,
    Play,
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Starting,
    Streaming,
    Ended,
}

struct State {
    status: Status,
    message: Option<String>,
    texture: Option<TextureHandle>,
    touching: bool,
    notice: Option<(String, Instant)>,
    audio: bool,
    applied_size: [u32; 2],
    fullscreen: bool,
    went_offline: bool,
    closed: bool,
    position: Option<[f32; 2]>,
    key_mode: KeyMode,
    pending_spot: Option<[f32; 2]>,
    bindings: Vec<KeyBinding>,
    held: Vec<usize>,
    /// Counts starts, so results from an older start never overwrite a newer one.
    generation: u64,
}

enum Action {
    Key(u32),
    ToggleAudio,
    ToggleRecording,
    Screenshot,
    ToggleFullscreen,
    Close,
    Reconnect,
    ToggleKeys,
    EditKeys,
}

#[derive(Clone, Copy)]
enum Icon {
    Close,
    Fullscreen,
    Record,
    Sound,
    Camera,
    Recents,
    Home,
    Back,
    Keys,
    Target,
}

fn paint_icon(painter: &Painter, icon: Icon, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let at = |x: f32, y: f32| center + vec2(x, y);
    let outline = |size: [f32; 2], offset: [f32; 2]| {
        let rect = Rect::from_center_size(at(offset[0], offset[1]), size.into());
        painter.rect_stroke(rect, 1.5, stroke, StrokeKind::Middle);
    };
    match icon {
        Icon::Close => {
            painter.line_segment([at(-5.0, -5.0), at(5.0, 5.0)], stroke);
            painter.line_segment([at(5.0, -5.0), at(-5.0, 5.0)], stroke);
        }
        Icon::Fullscreen => {
            for (x, y) in [(-6.0, -5.0), (6.0, -5.0), (-6.0, 5.0), (6.0, 5.0)] {
                let (inward_x, inward_y) = (if x < 0.0 { 4.0 } else { -4.0 }, if y < 0.0 { 3.5 } else { -3.5 });
                painter.line_segment([at(x, y), at(x + inward_x, y)], stroke);
                painter.line_segment([at(x, y), at(x, y + inward_y)], stroke);
            }
        }
        Icon::Record => {
            painter.circle_filled(center, 5.0, color);
        }
        Icon::Sound => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(-6.0, -2.5), at(-3.0, -2.5), at(1.0, -6.0), at(1.0, 6.0), at(-3.0, 2.5), at(-6.0, 2.5)],
                color,
                Stroke::NONE,
            ));
            painter.line_segment([at(4.0, -3.5), at(4.0, 3.5)], stroke);
            painter.line_segment([at(6.5, -5.5), at(6.5, 5.5)], stroke);
        }
        Icon::Camera => {
            outline([14.0, 10.0], [0.0, 1.0]);
            painter.circle_stroke(at(0.0, 1.0), 2.5, stroke);
        }
        Icon::Recents => outline([10.0, 10.0], [0.0, 0.0]),
        Icon::Home => {
            painter.circle_stroke(center, 5.5, stroke);
        }
        Icon::Back => {
            painter.line_segment([at(3.0, -6.0), at(-3.5, 0.0)], stroke);
            painter.line_segment([at(-3.5, 0.0), at(3.0, 6.0)], stroke);
        }
        Icon::Keys => {
            outline([15.0, 10.0], [0.0, 0.0]);
            for x in [-3.5, 0.0, 3.5] {
                painter.circle_filled(at(x, -1.5), 0.9, color);
            }
            painter.line_segment([at(-3.5, 2.0), at(3.5, 2.0)], stroke);
        }
        Icon::Target => {
            painter.circle_stroke(center, 5.0, stroke);
            painter.line_segment([at(-8.0, 0.0), at(-2.0, 0.0)], stroke);
            painter.line_segment([at(2.0, 0.0), at(8.0, 0.0)], stroke);
            painter.line_segment([at(0.0, -8.0), at(0.0, -2.0)], stroke);
            painter.line_segment([at(0.0, 2.0), at(0.0, 8.0)], stroke);
        }
    }
}

fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, color: Option<Color32>, enabled: bool) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), sense);
    if enabled && response.hovered() {
        let fill = if matches!(icon, Icon::Close) { RECORD_COLOR } else { Color32::from_white_alpha(28) };
        ui.painter().rect_filled(rect, 5.0, fill);
    }
    let tint = if enabled { color.unwrap_or(Color32::from_gray(225)) } else { Color32::from_gray(85) };
    paint_icon(ui.painter(), icon, rect.center(), tint);
    response.on_hover_text(tip).clicked()
}

pub struct Phone {
    pub id: ViewportId,
    pub video: Arc<Video>,
    core: Arc<Core>,
    ctx: Context,
    key: String,
    serial: String,
    package: Option<String>,
    title: String,
    position: Pos2,
    state: Mutex<State>,
    /// Held while a session stops and starts, so two quick reconnects run one after the other.
    launching: Mutex<()>,
}

pub fn summarize_transfer(results: &[TransferResult]) -> String {
    if let [only] = results {
        return match only.action {
            TransferAction::Installed => format!("Installed {}", only.name),
            TransferAction::Sent => format!("Sent {} to Download", only.name),
        };
    }
    let installed = results.iter().filter(|result| result.action == TransferAction::Installed).count();
    let sent = results.len() - installed;
    match (installed, sent) {
        (0, sent) => format!("Sent {sent} files to Download"),
        (installed, 0) => format!("Installed {installed} apps"),
        (installed, sent) => format!("Installed {installed} apps, sent {sent} files to Download"),
    }
}

fn timestamped(folder: Option<PathBuf>, prefix: &str, extension: &str) -> Result<PathBuf> {
    let folder = folder.ok_or_else(|| AppError::Invalid("no output folder".into()))?.join("OpenVysor");
    std::fs::create_dir_all(&folder)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    Ok(folder.join(format!("{prefix}-{stamp}.{extension}")))
}

fn binding_point(binding: &KeyBinding, video: [u32; 2]) -> (i32, i32) {
    let along = |fraction: f32, side: u32| ((fraction * side as f32).round() as i32).clamp(0, side as i32 - 1);
    (along(binding.x, video[0]), along(binding.y, video[1]))
}

impl Phone {
    pub fn new(
        core: Arc<Core>,
        ctx: Context,
        number: u32,
        serial: &str,
        package: Option<String>,
        title: String,
        position: Pos2,
    ) -> Arc<Self> {
        let audio = core.settings.lock().unwrap().options.audio && package.is_none();
        let bindings = core.profile(package.as_deref()).keys;
        let key = format!("phone-{number}");
        Arc::new(Self {
            id: ViewportId::from_hash_of(&key),
            video: Arc::new(Video::default()),
            core,
            ctx,
            key,
            serial: serial.to_string(),
            package,
            title,
            position,
            state: Mutex::new(State {
                status: Status::Starting,
                message: None,
                texture: None,
                touching: false,
                notice: None,
                audio,
                applied_size: [0, 0],
                fullscreen: false,
                went_offline: false,
                closed: false,
                position: None,
                key_mode: KeyMode::Off,
                pending_spot: None,
                bindings,
                held: Vec::new(),
                generation: 0,
            }),
            launching: Mutex::new(()),
        })
    }

    pub fn shows(&self, serial: &str, package: Option<&str>) -> bool {
        self.serial == serial && self.package.as_deref() == package
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn as_workspace_window(&self) -> WorkspaceWindow {
        WorkspaceWindow {
            serial: self.serial.clone(),
            package: self.package.clone(),
            title: self.title.clone(),
            position: self.state.lock().unwrap().position,
        }
    }

    /// A short state for lists, and whether the picture is live.
    pub fn status_text(&self) -> (&'static str, bool) {
        match self.state.lock().unwrap().status {
            Status::Starting => ("Connecting", false),
            Status::Streaming => ("Live", true),
            Status::Ended => ("Disconnected", false),
        }
    }

    /// Debug builds can show a blank picture, so input modes can be tried without a phone.
    #[cfg(debug_assertions)]
    pub fn show_sample_picture(&self, editing_keys: bool) {
        let image = egui::ColorImage::filled([1, 1], Color32::from_gray(40));
        self.video.set_sample_size([1080, 2400]);
        let mut state = self.state.lock().unwrap();
        state.status = Status::Streaming;
        state.texture = Some(self.ctx.load_texture(&self.key, image, TextureOptions::LINEAR));
        if editing_keys {
            state.key_mode = KeyMode::Edit;
        }
    }

    pub fn viewport_builder(&self) -> ViewportBuilder {
        ViewportBuilder::default()
            .with_title(&self.title)
            .with_decorations(false)
            .with_inner_size(INITIAL_SIZE)
            .with_min_inner_size(MIN_SIZE)
            .with_position(self.position)
            .with_drag_and_drop(true)
    }

    fn repaint(&self) {
        self.ctx.request_repaint_of(self.id);
    }

    fn set_notice(&self, text: impl Into<String>) {
        self.state.lock().unwrap().notice = Some((text.into(), Instant::now()));
        self.repaint();
    }

    fn send(&self, message: &ControlMessage) {
        // Fails only while the session is closing, when the input no longer matters.
        let _ = self.core.mirrors.send(&self.key, message);
    }

    fn press_key(&self, keycode: u32) {
        for action in [ACTION_DOWN, ACTION_UP] {
            self.send(&ControlMessage::Keycode { action, keycode, repeat: 0, metastate: 0 });
        }
    }

    pub fn start(self: &Arc<Self>) {
        let generation = {
            let mut state = self.state.lock().unwrap();
            state.status = Status::Starting;
            state.message = None;
            state.texture = None;
            state.touching = false;
            state.generation += 1;
            state.generation
        };
        let phone = self.clone();
        thread::spawn(move || {
            let _launching = phone.launching.lock().unwrap();
            if phone.state.lock().unwrap().generation != generation {
                // A newer start is queued behind this one and will do the work.
                return;
            }
            let _ = phone.core.mirrors.stop(&phone.key);
            let result = phone.launch(generation);
            let mut state = phone.state.lock().unwrap();
            if state.closed {
                drop(state);
                let _ = phone.core.mirrors.stop(&phone.key);
                return;
            }
            if state.generation != generation {
                return;
            }
            match result {
                // The stream may already have ended, and that must stay visible.
                Ok(()) if state.status == Status::Starting => state.status = Status::Streaming,
                Ok(()) => {}
                Err(error) => {
                    state.status = Status::Ended;
                    state.message = Some(error.to_string());
                }
            }
            drop(state);
            phone.repaint();
        });
    }

    fn open_audio(&self) -> Option<Sender<Vec<u8>>> {
        if !self.state.lock().unwrap().audio {
            return None;
        }
        audio::spawn()
            .inspect_err(|error| self.set_notice(format!("Sound is unavailable: {error}")))
            .ok()
    }

    fn launch(self: &Arc<Self>, generation: u64) -> Result<()> {
        let adb = self.core.registry.adb_for_online(&self.serial)?;
        if let Some(package) = &self.package {
            validate_package(package)?;
        }
        let server_path = self.core.server_path()?;
        let mut options = self.core.settings.lock().unwrap().options;
        if let Some(quality) = self.core.profile(self.package.as_deref()).quality {
            apply_quality(&mut options, quality);
        }
        options.audio = self.state.lock().unwrap().audio;

        let context = self.ctx.clone();
        let id = self.id;
        let sink = packet_sink(self.video.clone(), Arc::new(move || context.request_repaint_of(id)), self.open_audio());

        let phone = self.clone();
        let on_end = Box::new(move |reason: String| {
            let mut state = phone.state.lock().unwrap();
            if state.generation != generation {
                return;
            }
            state.status = Status::Ended;
            state.message = Some(reason);
            drop(state);
            phone.repaint();
        });
        let request = StartRequest {
            key: &self.key,
            serial: &self.serial,
            server_path: &server_path,
            options,
            start_app: self.package.as_deref(),
        };
        self.core.mirrors.start(&adb, request, sink, on_end).map(|_| ())
    }

    /// Resumes a window whose phone was unplugged and has come back.
    pub fn on_devices(self: &Arc<Self>, devices: &[Device]) {
        let online = devices.iter().any(|device| device.serial == self.serial && device.state == DeviceState::Online);
        let resume = {
            let mut state = self.state.lock().unwrap();
            if !online {
                state.went_offline = true;
                false
            } else {
                let came_back = std::mem::take(&mut state.went_offline);
                came_back && state.status == Status::Ended
            }
        };
        if resume {
            self.start();
        }
    }

    pub fn close(self: &Arc<Self>) {
        let position = {
            let mut state = self.state.lock().unwrap();
            state.closed = true;
            state.position
        };
        if position.is_some() {
            self.core.update_profile(self.package.as_deref(), |profile| profile.position = position);
        }
        self.core.forget_phone(self.id);
        let phone = self.clone();
        thread::spawn(move || {
            if let Some(Err(error)) = phone.video.stop_recording() {
                log::warn!("recording: {error}");
            }
            // Stopping runs adb, which must not block the window.
            let _ = phone.core.mirrors.stop(&phone.key);
        });
        self.ctx.request_repaint_of(ViewportId::ROOT);
    }

    fn toggle_recording(self: &Arc<Self>) {
        if self.video.is_recording() {
            let phone = self.clone();
            thread::spawn(move || match phone.video.stop_recording() {
                Some(Ok(path)) => phone.set_notice(format!("Saved to {}", path.display())),
                Some(Err(error)) => phone.set_notice(format!("Recording failed: {error}")),
                None => phone.repaint(),
            });
            return;
        }
        match timestamped(dirs::video_dir(), "recording", "mp4") {
            Ok(path) => {
                self.video.start_recording(path);
                // Recordings must begin on a key frame, so ask for one now.
                self.send(&ControlMessage::ResetVideo);
            }
            Err(error) => self.set_notice(error.to_string()),
        }
    }

    fn take_screenshot(self: &Arc<Self>) {
        let phone = self.clone();
        thread::spawn(move || {
            let result = phone.core.registry.adb_for_online(&phone.serial).and_then(|adb| {
                let image = screenshot(&adb, &phone.serial)?;
                let path = timestamped(dirs::picture_dir(), "screenshot", "png")?;
                std::fs::write(&path, image)?;
                Ok(path)
            });
            match result {
                Ok(path) => phone.set_notice(format!("Saved to {}", path.display())),
                Err(error) => phone.set_notice(error.to_string()),
            }
        });
    }

    fn send_files(self: &Arc<Self>, paths: Vec<PathBuf>) {
        self.set_notice("Working…");
        let phone = self.clone();
        thread::spawn(move || {
            let result = phone.core.registry.adb_for_online(&phone.serial).and_then(|adb| {
                paths
                    .iter()
                    .map(|path: &PathBuf| transfer::send_path(&adb, &phone.serial, Path::new(path)))
                    .collect::<Result<Vec<_>>>()
            });
            match result {
                Ok(results) => phone.set_notice(summarize_transfer(&results)),
                Err(error) => phone.set_notice(error.to_string()),
            }
        });
    }

    fn apply(self: &Arc<Self>, ctx: &Context, action: Action) {
        match action {
            Action::Key(keycode) => self.press_key(keycode),
            Action::ToggleAudio => {
                {
                    let mut state = self.state.lock().unwrap();
                    state.audio = !state.audio;
                }
                self.start();
            }
            Action::ToggleRecording => self.toggle_recording(),
            Action::Screenshot => self.take_screenshot(),
            Action::ToggleFullscreen => {
                let mut state = self.state.lock().unwrap();
                state.fullscreen = !state.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(state.fullscreen));
            }
            Action::Close => self.close(),
            Action::Reconnect => self.start(),
            Action::ToggleKeys => {
                let mut state = self.state.lock().unwrap();
                state.key_mode = if state.key_mode == KeyMode::Play { KeyMode::Off } else { KeyMode::Play };
                self.release_held_keys(&mut state);
            }
            Action::EditKeys => {
                let editing = {
                    let mut state = self.state.lock().unwrap();
                    state.key_mode = if state.key_mode == KeyMode::Edit { KeyMode::Off } else { KeyMode::Edit };
                    state.pending_spot = None;
                    self.release_held_keys(&mut state);
                    state.key_mode == KeyMode::Edit
                };
                if editing {
                    self.set_notice("Click a spot, then press a key. Click a mark to remove it.");
                }
            }
        }
    }

    pub fn show(self: &Arc<Self>, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.viewport().close_requested()) {
            self.close();
            return;
        }
        let mut actions = Vec::new();
        self.absorb_background_results();
        {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            if let Some(rect) = ctx.input(|input| input.viewport().outer_rect) {
                state.position = Some([rect.min.x, rect.min.y]);
            }
            if let Some(image) = self.video.take_latest() {
                match &mut state.texture {
                    Some(texture) => texture.set(image, TextureOptions::LINEAR),
                    None => state.texture = Some(ctx.load_texture(&self.key, image, TextureOptions::LINEAR)),
                }
            }
            let size = self.video.size();
            if size != state.applied_size && !state.fullscreen
                && let Some(window) = phone_window_size(size) {
                    state.applied_size = size;
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(window));
                }
            if state.notice.as_ref().is_some_and(|(_, at)| at.elapsed() > NOTICE_LIFETIME) {
                state.notice = None;
            }

            Panel::top("strip")
                .exact_size(STRIP_HEIGHT)
                .frame(Frame::NONE.fill(STRIP_COLOR))
                .show(ui, |ui| self.strip(ui, state, &mut actions));
            CentralPanel::no_frame()
                .frame(Frame::NONE.fill(Color32::BLACK))
                .show(ui, |ui| self.screen(ui, state, &mut actions));
            if let Some((_, at)) = &state.notice {
                ctx.request_repaint_after(NOTICE_LIFETIME.saturating_sub(at.elapsed()));
            }
        }
        for action in actions {
            self.apply(&ctx, action);
        }
    }

    fn absorb_background_results(&self) {
        if let Some(error) = self.video.take_error() {
            let mut state = self.state.lock().unwrap();
            state.status = Status::Ended;
            state.message = Some(error);
        }
        match self.video.take_ended_recording() {
            Some(Ok(path)) => self.set_notice(format!("Recording ended. Saved to {}", path.display())),
            Some(Err(error)) => self.set_notice(format!("Recording failed: {error}")),
            None => {}
        }
    }

    fn strip(&self, ui: &mut Ui, state: &State, actions: &mut Vec<Action>) {
        let streaming = state.status == Status::Streaming;
        let drag = ui.interact(ui.max_rect(), ui.scope_id().with("drag"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut button = |ui: &mut Ui, icon: Icon, tip: &str, color: Option<Color32>, enabled: bool, action: Action| {
                if icon_button(ui, icon, tip, color, enabled) {
                    actions.push(action);
                }
            };
            button(ui, Icon::Close, "Close", None, true, Action::Close);
            button(ui, Icon::Fullscreen, "Full screen (F11)", None, true, Action::ToggleFullscreen);
            let recording = self.video.is_recording();
            button(
                ui,
                Icon::Record,
                if recording { "Stop recording" } else { "Record to MP4" },
                recording.then_some(RECORD_COLOR),
                streaming,
                Action::ToggleRecording,
            );
            if self.package.is_none() {
                button(
                    ui,
                    Icon::Sound,
                    "Play phone sound here (the phone goes silent)",
                    state.audio.then_some(ACTIVE_COLOR),
                    state.status != Status::Starting,
                    Action::ToggleAudio,
                );
                button(ui, Icon::Camera, "Screenshot", None, streaming, Action::Screenshot);
                button(ui, Icon::Recents, "Recent apps", None, streaming, Action::Key(KEYCODE_APP_SWITCH));
                button(ui, Icon::Home, "Home", None, streaming, Action::Key(KEYCODE_HOME));
            }
            button(ui, Icon::Target, "Edit key mapping", (state.key_mode == KeyMode::Edit).then_some(ACTIVE_COLOR), streaming, Action::EditKeys);
            button(
                ui,
                Icon::Keys,
                "Use mapped keys",
                (state.key_mode == KeyMode::Play).then_some(ACTIVE_COLOR),
                streaming && !state.bindings.is_empty(),
                Action::ToggleKeys,
            );
            button(ui, Icon::Back, "Back", None, streaming, Action::Key(KEYCODE_BACK));
            ui.add_space(4.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(10.0);
                ui.add(egui::Label::new(RichText::new(&self.title).size(12.0).color(Color32::WHITE)).truncate().selectable(false));
            });
        });
    }

    fn screen(self: &Arc<Self>, ui: &mut Ui, state: &mut State, actions: &mut Vec<Action>) {
        let area = ui.max_rect();
        let size = self.video.size();
        if let (Status::Streaming, Some(texture)) = (state.status, &state.texture) {
            let rect = fit_inside(area, size);
            self.video.set_display_long_side((rect.width().max(rect.height()) * ui.ctx().pixels_per_point()).round() as u32);
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
            self.handle_input(ui, rect, size, state, actions);
            self.paint_key_marks(ui, rect, state);

            let dropped: Vec<PathBuf> = ui.input(|input| {
                input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect()
            });
            if !dropped.is_empty() {
                self.send_files(dropped);
            }
            if ui.input(|input| !input.raw.hovered_files.is_empty()) {
                ui.painter().rect_filled(area.shrink(8.0), 12.0, Color32::from_black_alpha(190));
                ui.painter().text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop to install an APK or send files",
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
        } else {
            self.placeholder(ui, state, actions);
        }

        if let Some((text, _)) = &state.notice {
            let anchor = pos2(area.center().x, area.bottom() - 24.0);
            let galley = ui.painter().layout(text.clone(), egui::FontId::proportional(13.0), Color32::WHITE, area.width() - 40.0);
            let bubble = Rect::from_center_size(anchor, galley.size()).expand(8.0);
            ui.painter().rect_filled(bubble, 8.0, Color32::from_black_alpha(220));
            ui.painter().galley(bubble.shrink(8.0).min, galley, Color32::WHITE);
        }
    }

    fn placeholder(&self, ui: &mut Ui, state: &State, actions: &mut Vec<Action>) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 2.0 - 40.0);
            if state.status == Status::Ended {
                ui.colored_label(Color32::from_gray(210), state.message.as_deref().unwrap_or("The session ended."));
                ui.add_space(8.0);
                if theme::button(ui, ButtonKind::Primary, "Reconnect", true).clicked() {
                    actions.push(Action::Reconnect);
                }
            } else {
                ui.spinner();
                ui.add_space(6.0);
                ui.colored_label(Color32::from_gray(170), format!("Connecting to {}…", self.title));
            }
        });
    }

    fn touch(&self, action: u8, position: Pos2, rect: Rect, video: [u32; 2]) {
        let (x, y) = to_video_point(position, rect, video);
        self.finger(action, POINTER_ID_FINGER, x, y, video);
    }

    fn finger(&self, action: u8, pointer_id: i64, x: i32, y: i32, video: [u32; 2]) {
        let pressure = if action == ACTION_UP { 0.0 } else { 1.0 };
        self.send(&ControlMessage::Touch {
            action,
            pointer_id,
            x,
            y,
            width: video[0] as u16,
            height: video[1] as u16,
            pressure,
            // A finger has no buttons, and Android rejects a touch that claims one.
            action_button: 0,
            buttons: 0,
        });
    }

    /// Each mapped key is its own finger, so several can be held at once.
    fn mapped_touch(&self, index: usize, action: u8, bindings: &[KeyBinding], video: [u32; 2]) {
        let (x, y) = binding_point(&bindings[index], video);
        self.finger(action, index as i64 + 1, x, y, video);
    }

    fn release_held_keys(&self, state: &mut State) {
        let video = self.video.size();
        for index in std::mem::take(&mut state.held) {
            self.mapped_touch(index, ACTION_UP, &state.bindings, video);
        }
    }

    fn save_bindings(&self, bindings: Vec<KeyBinding>) {
        self.core.update_profile(self.package.as_deref(), |profile| profile.keys = bindings);
    }

    fn edit_bindings(&self, event: &Event, rect: Rect, state: &mut State) {
        match event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if rect.contains(*pos) => {
                let spot = [(pos.x - rect.left()) / rect.width(), (pos.y - rect.top()) / rect.height()];
                match binding_at(&state.bindings, spot, [rect.width(), rect.height()], KEY_MARK_RADIUS) {
                    Some(index) => {
                        state.bindings.remove(index);
                        state.pending_spot = None;
                        self.save_bindings(state.bindings.clone());
                    }
                    None => state.pending_spot = Some(spot),
                }
            }
            Event::Key { key: Key::Escape, pressed: true, .. } => state.pending_spot = None,
            Event::Key { key, pressed: true, repeat: false, .. } => {
                if let Some(spot) = state.pending_spot.take() {
                    set_binding(&mut state.bindings, key.name(), spot);
                    self.save_bindings(state.bindings.clone());
                }
            }
            _ => {}
        }
    }

    /// Returns true when the event belongs to key mapping and must not reach the phone.
    fn key_mapping_event(&self, event: &Event, rect: Rect, video: [u32; 2], state: &mut State) -> bool {
        match state.key_mode {
            KeyMode::Off => false,
            KeyMode::Edit => {
                self.edit_bindings(event, rect, state);
                true
            }
            KeyMode::Play => match event {
                Event::Key { key, pressed, repeat, .. } => {
                    let Some(index) = state.bindings.iter().position(|binding| binding.key == key.name()) else { return false };
                    let holding = state.held.contains(&index);
                    if *pressed && !*repeat && !holding {
                        state.held.push(index);
                        self.mapped_touch(index, ACTION_DOWN, &state.bindings, video);
                    } else if !*pressed && holding {
                        state.held.retain(|held| *held != index);
                        self.mapped_touch(index, ACTION_UP, &state.bindings, video);
                    }
                    true
                }
                Event::Text(_) => true,
                Event::WindowFocused(false) => {
                    self.release_held_keys(state);
                    false
                }
                _ => false,
            },
        }
    }

    fn paint_key_marks(&self, ui: &Ui, rect: Rect, state: &State) {
        if state.key_mode == KeyMode::Off {
            return;
        }
        let at = |x: f32, y: f32| pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
        let fill = Color32::from_black_alpha(if state.key_mode == KeyMode::Edit { 170 } else { 90 });
        for binding in &state.bindings {
            let center = at(binding.x, binding.y);
            ui.painter().circle(center, KEY_MARK_RADIUS, fill, Stroke::new(1.5, ACTIVE_COLOR));
            ui.painter().text(center, egui::Align2::CENTER_CENTER, &binding.key, egui::FontId::proportional(11.0), Color32::WHITE);
        }
        if let Some([x, y]) = state.pending_spot {
            ui.painter().circle_stroke(at(x, y), KEY_MARK_RADIUS, Stroke::new(2.0, Color32::WHITE));
        }
    }

    fn handle_input(&self, ui: &mut Ui, rect: Rect, video: [u32; 2], state: &mut State, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        let hover = ctx.input(|input| input.pointer.hover_pos());
        for event in ctx.input(|input| input.events.clone()) {
            if self.key_mapping_event(&event, rect, video, state) {
                continue;
            }
            match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed, .. } => {
                    if pressed && rect.contains(pos) {
                        state.touching = true;
                        self.touch(ACTION_DOWN, pos, rect, video);
                    } else if !pressed && state.touching {
                        state.touching = false;
                        self.touch(ACTION_UP, pos, rect, video);
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Secondary, pressed: true, .. } if rect.contains(pos) => {
                    for action in [ACTION_DOWN, ACTION_UP] {
                        self.send(&ControlMessage::BackOrScreenOn { action });
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Middle, pressed: true, .. } if rect.contains(pos) => {
                    actions.push(Action::Key(KEYCODE_HOME));
                }
                Event::PointerMoved(position) if state.touching => {
                    self.touch(ACTION_MOVE, position, rect, video);
                }
                Event::MouseWheel { unit, delta, .. } => {
                    let Some(position) = hover.filter(|position| rect.contains(*position)) else { continue };
                    let (hscroll, vscroll) = wheel_to_scroll(delta, unit == MouseWheelUnit::Line);
                    let (x, y) = to_video_point(position, rect, video);
                    self.send(&ControlMessage::Scroll {
                        x,
                        y,
                        width: video[0] as u16,
                        height: video[1] as u16,
                        hscroll,
                        vscroll,
                        buttons: 0,
                    });
                }
                Event::Key { key: key @ (Key::ArrowLeft | Key::ArrowRight), pressed, modifiers, .. } if modifiers.alt => {
                    if pressed {
                        self.core.focus_adjacent(&ctx, self.id, if key == Key::ArrowLeft { -1 } else { 1 });
                    }
                }
                Event::Key { key: Key::F11, pressed, repeat, .. } => {
                    if pressed && !repeat {
                        actions.push(Action::ToggleFullscreen);
                    }
                }
                Event::Key { key, pressed, repeat, modifiers, .. } => {
                    if let Some(keycode) = android_keycode(key, modifiers) {
                        self.send(&ControlMessage::Keycode {
                            action: if pressed { ACTION_DOWN } else { ACTION_UP },
                            keycode,
                            repeat: u32::from(repeat),
                            metastate: android_metastate(modifiers),
                        });
                    }
                }
                Event::Text(text) => self.send(&ControlMessage::Text { text }),
                Event::Paste(text) => self.send(&ControlMessage::SetClipboard { sequence: 0, paste: true, text }),
                Event::Copy => actions.push(Action::Key(KEYCODE_COPY)),
                Event::Cut => actions.push(Action::Key(KEYCODE_CUT)),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(name: &str, action: TransferAction) -> TransferResult {
        TransferResult { name: name.to_string(), action }
    }

    #[test]
    fn a_mapped_key_taps_the_same_part_of_any_resolution() {
        let binding = KeyBinding { key: "W".to_string(), x: 0.5, y: 0.25 };
        assert_eq!(binding_point(&binding, [1080, 2400]), (540, 600));
        assert_eq!(binding_point(&binding, [720, 1600]), (360, 400));
        let corner = KeyBinding { key: "A".to_string(), x: 1.0, y: 1.0 };
        assert_eq!(binding_point(&corner, [1080, 2400]), (1079, 2399));
    }

    #[test]
    fn summarizes_one_file_by_name() {
        assert_eq!(summarize_transfer(&[result("a.apk", TransferAction::Installed)]), "Installed a.apk");
        assert_eq!(summarize_transfer(&[result("b.jpg", TransferAction::Sent)]), "Sent b.jpg to Download");
    }

    #[test]
    fn summarizes_several_files_by_count() {
        let mixed = [
            result("a.apk", TransferAction::Installed),
            result("b.jpg", TransferAction::Sent),
            result("c.jpg", TransferAction::Sent),
        ];
        assert_eq!(summarize_transfer(&mixed), "Installed 1 apps, sent 2 files to Download");
    }
}
