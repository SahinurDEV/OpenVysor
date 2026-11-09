//! The Notifications page. It reads the phone only while the page is open.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{Align, Context, Label, Layout, RichText, ScrollArea, Sense, Ui, Vec2, vec2};

use crate::adb::notifications::{self, PhoneNotification};
use crate::mirroring::apps::validate_package;
use crate::state::Core;
use crate::theme::{self, ButtonKind, palette, semibold};

const POLL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Feed {
    serial: Option<String>,
    loading: bool,
    fetched_at: Option<Instant>,
    items: Vec<PhoneNotification>,
    error: Option<String>,
}

#[derive(Default)]
pub struct NotificationsPage {
    feed: Arc<Mutex<Feed>>,
}

fn app_label(package: &str) -> String {
    let last = package.rsplit('.').next().unwrap_or(package);
    let mut letters = last.chars();
    letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
}

impl NotificationsPage {
    /// Their text is private, so it is dropped as soon as the page is left.
    pub fn clear(&self) {
        *self.feed.lock().unwrap() = Feed::default();
    }

    fn poll(&self, ctx: &Context, core: &Arc<Core>, serial: &str) {
        let mut feed = self.feed.lock().unwrap();
        let changed = feed.serial.as_deref() != Some(serial);
        if changed {
            *feed = Feed { serial: Some(serial.to_string()), ..Feed::default() };
        }
        if feed.loading || feed.fetched_at.is_some_and(|at| at.elapsed() < POLL) {
            return;
        }
        feed.loading = true;
        drop(feed);
        let (core, slot, ctx, serial) = (core.clone(), self.feed.clone(), ctx.clone(), serial.to_string());
        thread::spawn(move || {
            let result = core.registry.adb_for_online(&serial).and_then(|adb| notifications::fetch(&adb, &serial));
            let mut feed = slot.lock().unwrap();
            // The page may have been left or switched to another phone meanwhile.
            if feed.serial.as_deref() == Some(serial.as_str()) {
                feed.loading = false;
                feed.fetched_at = Some(Instant::now());
                match result {
                    Ok(items) => (feed.items, feed.error) = (items, None),
                    Err(error) => feed.error = Some(error.to_string()),
                }
            }
            ctx.request_repaint();
        });
    }

    fn opt_in(ui: &mut Ui, core: &Arc<Core>) {
        theme::card(ui, |ui| {
            ui.label(RichText::new("See your phone's notifications here").font(semibold(16.0)));
            ui.label("Notifications can include private messages. They are read only while this page is open, and are never saved.");
            ui.add_space(8.0);
            if theme::button(ui, ButtonKind::Primary, "Turn on", true).clicked() {
                core.update_settings(|settings| settings.notifications = true);
            }
        });
    }

    pub fn show(&self, ui: &mut Ui, core: &Arc<Core>, serial: &str) {
        if !core.settings.lock().unwrap().notifications {
            Self::opt_in(ui, core);
            return;
        }
        self.poll(ui.ctx(), core, serial);
        ui.ctx().request_repaint_after(POLL);

        let colors = palette(ui);
        let feed = self.feed.lock().unwrap();
        if let Some(error) = &feed.error {
            ui.colored_label(colors.danger, error);
        }
        if feed.items.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                if feed.fetched_at.is_none() {
                    ui.spinner();
                } else {
                    ui.weak("No notifications right now");
                }
            });
            return;
        }
        let items = feed.items.clone();
        drop(feed);

        let mut open = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for item in &items {
                theme::card(ui, |ui| {
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 44.0), Layout::right_to_left(Align::Center), |ui| {
                        if validate_package(&item.package).is_ok() && theme::button(ui, ButtonKind::Secondary, "Open app", true).clicked() {
                            open = Some(item.package.clone());
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                            let label = app_label(&item.package);
                            theme::avatar(ui.painter(), rect.center(), 17.0, &label);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let heading = if item.title.is_empty() { label.as_str() } else { item.title.as_str() };
                                ui.add(Label::new(RichText::new(heading).font(semibold(14.0))).truncate());
                                ui.add(Label::new(RichText::new(&item.text).small().weak()).truncate());
                            });
                        });
                    });
                });
                ui.add_space(8.0);
            }
        });
        if let Some(package) = open {
            let title = app_label(&package);
            core.open_phone(ui.ctx(), serial, Some(package), title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_an_app_from_its_package() {
        assert_eq!(app_label("com.whatsapp"), "Whatsapp");
        assert_eq!(app_label(""), "");
    }
}
