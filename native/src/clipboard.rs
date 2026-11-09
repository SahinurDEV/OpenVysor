use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crate::mirroring::MirrorManager;

const POLL_INTERVAL: Duration = Duration::from_millis(800);

/// Remembers the last synced text, so a value received from one side is not
/// mistaken for a new copy on the other side and echoed back.
#[derive(Default)]
pub struct ClipboardTracker {
    last: Option<String>,
}

impl ClipboardTracker {
    /// Records `text` and returns whether it differs from the last synced value.
    pub fn changed(&mut self, text: &str) -> bool {
        if self.last.as_deref() == Some(text) {
            return false;
        }
        self.last = Some(text.to_string());
        true
    }
}

/// Runs the clipboard on one thread and returns the sender for device text.
/// Windows clipboard access is exclusive, so reads and writes must not overlap.
pub fn spawn(mirrors: Arc<MirrorManager>) -> Sender<String> {
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                log::warn!("clipboard unavailable: {error}");
                return;
            }
        };
        let mut tracker = ClipboardTracker::default();
        // Whatever was copied before the app started is not sent to the phone.
        if let Ok(text) = clipboard.get_text() {
            tracker.changed(&text);
        }
        loop {
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(text) => {
                    if mirrors.clipboard_sync_enabled() && tracker.changed(&text) {
                        if let Err(error) = clipboard.set_text(text) {
                            log::warn!("could not write the clipboard: {error}");
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if mirrors.clipboard_sync_enabled() {
                        if let Ok(text) = clipboard.get_text() {
                            if tracker.changed(&text) {
                                mirrors.broadcast_clipboard(&text);
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_new_text() {
        let mut tracker = ClipboardTracker::default();
        assert!(tracker.changed("a"));
        assert!(!tracker.changed("a"));
        assert!(tracker.changed("b"));
        assert!(tracker.changed("a"));
    }

    #[test]
    fn text_from_the_phone_is_not_echoed_back() {
        let mut tracker = ClipboardTracker::default();
        assert!(tracker.changed("from phone"));
        // The next poll reads the same text from the Windows clipboard.
        assert!(!tracker.changed("from phone"));
    }
}
