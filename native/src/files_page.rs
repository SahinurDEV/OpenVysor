//! The Files page: browse the phone, download, and drop files to upload.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{Align, Context, CursorIcon, Layout, ScrollArea, Sense, Ui, pos2, vec2};

use crate::error::AppError;
use crate::state::Core;
use crate::theme::{self, ButtonKind, Glyph, palette, semibold};
use crate::transfer::files::{self, RemoteEntry};

const ROW_HEIGHT: f32 = 44.0;

#[derive(Default)]
struct Listing {
    loaded: Option<(String, String)>,
    loading: bool,
    entries: Vec<RemoteEntry>,
    error: Option<String>,
}

type Message = Arc<Mutex<Option<Result<String, String>>>>;

pub struct FilesPage {
    path: String,
    listing: Arc<Mutex<Listing>>,
    message: Message,
}

impl Default for FilesPage {
    fn default() -> Self {
        Self { path: files::ROOT.to_string(), listing: Arc::default(), message: Arc::default() }
    }
}

fn run_transfer(ctx: &Context, message: &Message, start: String, job: impl FnOnce() -> Result<String, AppError> + Send + 'static) {
    *message.lock().unwrap() = Some(Ok(start));
    let (slot, ctx) = (message.clone(), ctx.clone());
    thread::spawn(move || {
        *slot.lock().unwrap() = Some(job().map_err(|error| error.to_string()));
        ctx.request_repaint();
    });
}

impl FilesPage {
    fn load_if_needed(&self, ctx: &Context, core: &Arc<Core>, serial: &str) {
        let target = (serial.to_string(), self.path.clone());
        let mut listing = self.listing.lock().unwrap();
        if listing.loaded.as_ref() == Some(&target) {
            return;
        }
        *listing = Listing { loaded: Some(target.clone()), loading: true, ..Listing::default() };
        drop(listing);
        let (core, slot, ctx) = (core.clone(), self.listing.clone(), ctx.clone());
        thread::spawn(move || {
            let result = core.registry.adb_for_online(&target.0).and_then(|adb| files::list(&adb, &target.0, &target.1));
            let mut listing = slot.lock().unwrap();
            if listing.loaded.as_ref() == Some(&target) {
                listing.loading = false;
                match result {
                    Ok(entries) => listing.entries = entries,
                    Err(error) => listing.error = Some(error.to_string()),
                }
            }
            ctx.request_repaint();
        });
    }

    fn refresh(&self) {
        self.listing.lock().unwrap().loaded = None;
    }

    fn download(&self, ctx: &Context, core: &Arc<Core>, serial: &str, entry: &RemoteEntry) {
        let (core, serial, remote) = (core.clone(), serial.to_string(), files::child(&self.path, &entry.name));
        run_transfer(ctx, &self.message, format!("Downloading {}…", entry.name), move || {
            let adb = core.registry.adb_for_online(&serial)?;
            let saved = files::download(&adb, &serial, &remote)?;
            Ok(format!("Saved to {}", saved.display()))
        });
    }

    fn upload(&self, ctx: &Context, core: &Arc<Core>, serial: &str, paths: Vec<PathBuf>) {
        let (core, serial, folder, listing) = (core.clone(), serial.to_string(), self.path.clone(), self.listing.clone());
        let count = paths.len();
        run_transfer(ctx, &self.message, "Uploading…".to_string(), move || {
            let adb = core.registry.adb_for_online(&serial)?;
            for path in &paths {
                files::upload(&adb, &serial, path, &folder)?;
            }
            listing.lock().unwrap().loaded = None;
            Ok(if count == 1 { "Uploaded 1 item".to_string() } else { format!("Uploaded {count} items") })
        });
    }

    /// Returns whether the row was opened and whether its download was pressed.
    fn entry_row(&self, ui: &mut Ui, entry: &RemoteEntry) -> (bool, bool) {
        let colors = palette(ui);
        let mut download = false;
        let row = ui.allocate_ui_with_layout(vec2(ui.available_width(), ROW_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
            download = theme::button(ui, ButtonKind::Quiet, "Download", true).clicked();
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 8.0, colors.muted);
            }
            let glyph = if entry.is_dir { Glyph::Folder } else { Glyph::File };
            let tint = if entry.is_dir { colors.primary } else { colors.secondary };
            theme::glyph(ui.painter(), glyph, pos2(rect.left() + 22.0, rect.center().y), 18.0, tint);
            let job = theme::one_line(&entry.name, semibold(14.0), colors.foreground, rect.width() - 56.0);
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, colors.foreground);
            response
        });
        let cursor = if entry.is_dir { CursorIcon::PointingHand } else { CursorIcon::Default };
        (entry.is_dir && row.inner.on_hover_cursor(cursor).clicked(), download)
    }

    pub fn show(&mut self, ui: &mut Ui, core: &Arc<Core>, serial: &str) {
        self.load_if_needed(ui.ctx(), core, serial);
        let colors = palette(ui);

        let dropped: Vec<PathBuf> = ui.input(|input| input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.upload(ui.ctx(), core, serial, dropped);
        }

        ui.horizontal(|ui| {
            if theme::button(ui, ButtonKind::Secondary, "Up", self.path != files::ROOT).clicked() {
                self.path = files::parent(&self.path);
            }
            if theme::button(ui, ButtonKind::Quiet, "Refresh", true).clicked() {
                self.refresh();
            }
            ui.add_space(6.0);
            let shown = self.path.strip_prefix(files::ROOT).unwrap_or(&self.path);
            ui.label(format!("Phone storage{shown}"));
        });
        ui.weak("Drop files on this window to upload them to this folder");
        match &*self.message.lock().unwrap() {
            Some(Ok(text)) => {
                ui.colored_label(colors.success, text);
            }
            Some(Err(text)) => {
                ui.colored_label(colors.danger, text);
            }
            None => {}
        }
        ui.add_space(6.0);

        let listing = self.listing.lock().unwrap();
        if listing.loading {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| ui.spinner());
            return;
        }
        if let Some(error) = &listing.error {
            ui.colored_label(colors.danger, error);
            return;
        }
        if listing.entries.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| ui.weak("This folder is empty"));
            return;
        }
        let entries = listing.entries.clone();
        drop(listing);

        let mut enter = None;
        let mut save = None;
        ui.spacing_mut().item_spacing.y = 2.0;
        ScrollArea::vertical().auto_shrink(false).show_rows(ui, ROW_HEIGHT, entries.len(), |ui, range| {
            for entry in &entries[range] {
                let (open, download) = self.entry_row(ui, entry);
                if download {
                    save = Some(entry.clone());
                } else if open {
                    enter = Some(entry.name.clone());
                }
            }
        });
        if let Some(entry) = save {
            self.download(ui.ctx(), core, serial, &entry);
        }
        if let Some(name) = enter {
            self.path = files::child(&self.path, &name);
        }
    }
}
