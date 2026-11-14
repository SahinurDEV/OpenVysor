pub mod apps;
pub mod process;
pub mod protocol;
pub mod session;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use crate::adb::Adb;
use crate::error::{AppError, Result};
use apps::AppInfo;
use process::JobObject;
use protocol::ControlMessage;
use session::{ClipboardHandler, MirrorOptions, PacketSink, Session, SessionParts, push_server};

/// Sessions are keyed by an id chosen by the caller, because one device can
/// now have several at once (one per app window).
#[derive(Default)]
pub struct MirrorManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    starting: Mutex<HashSet<String>>,
    /// Phones already holding the server. Held during a push, so windows opening
    /// together share one upload and then start side by side.
    pushed: Mutex<HashSet<String>>,
    job: Option<JobObject>,
    clipboard_handler: OnceLock<ClipboardHandler>,
    clipboard_sync: AtomicBool,
    /// The last text copied on the computer, so a window opened later still gets it.
    computer_clipboard: Mutex<Option<String>>,
}

pub struct StartRequest<'a> {
    pub key: &'a str,
    pub serial: &'a str,
    pub server_path: &'a Path,
    pub options: MirrorOptions,
    pub start_app: Option<&'a str>,
}

impl MirrorManager {
    pub fn new() -> Self {
        let job = JobObject::new()
            .inspect_err(|error| log::warn!("job object unavailable: {error}"))
            .ok();
        Self { job, ..Self::default() }
    }

    /// `on_end` runs if the stream stops without `stop` being called, such as an unplug.
    pub fn start(
        self: &Arc<Self>,
        adb: &Adb,
        request: StartRequest<'_>,
        sink: PacketSink,
        on_end: Box<dyn FnOnce(String) + Send>,
    ) -> Result<Arc<Session>> {
        let StartRequest { key, serial, server_path, options, start_app } = request;
        if self.sessions.lock().unwrap().contains_key(key)
            || !self.starting.lock().unwrap().insert(key.to_string())
        {
            return Err(AppError::SessionExists(key.to_string()));
        }

        let manager = self.clone();
        let end_key = key.to_string();
        let session_slot = Arc::new(Mutex::new(None::<u32>));
        let end_slot = session_slot.clone();
        let parts = SessionParts {
            sink,
            on_end: Box::new(move |reason| {
                let scid = *end_slot.lock().unwrap();
                manager.remove_ended(&end_key, scid);
                on_end(reason);
            }),
            on_clipboard: self
                .clipboard_handler
                .get()
                .cloned()
                .unwrap_or_else(|| Arc::new(|_| {})),
        };

        let result = self
            .ensure_server(adb, serial, server_path)
            .and_then(|()| Session::start(adb, serial, options, start_app, self.job.as_ref(), parts));
        if result.is_err() {
            // The stored copy may be gone, so the next attempt uploads it again.
            self.pushed.lock().unwrap().remove(serial);
        }
        // Publish before clearing `starting`, so a second start is always rejected.
        if let Ok(session) = &result {
            *session_slot.lock().unwrap() = Some(session.scid());
            self.sessions.lock().unwrap().insert(key.to_string(), session.clone());
            // A stream that ended before it was published could not remove itself,
            // and would block the next start with "session exists".
            if session.has_ended() {
                self.remove_ended(key, Some(session.scid()));
            } else {
                self.send_computer_clipboard(session);
            }
        }
        self.starting.lock().unwrap().remove(key);
        result
    }

    pub fn set_clipboard_handler(&self, handler: ClipboardHandler) {
        let _ = self.clipboard_handler.set(handler);
    }

    pub fn set_clipboard_sync(&self, enabled: bool) {
        self.clipboard_sync.store(enabled, Ordering::Relaxed);
    }

    pub fn clipboard_sync_enabled(&self) -> bool {
        self.clipboard_sync.load(Ordering::Relaxed)
    }

    /// Records text the computer's clipboard now holds because a phone sent it.
    pub fn remember_clipboard(&self, text: &str) {
        *self.computer_clipboard.lock().unwrap() = Some(text.to_string());
    }

    /// Text copied before a window opened would otherwise never reach that phone.
    fn send_computer_clipboard(&self, session: &Session) {
        if !self.clipboard_sync_enabled() {
            return;
        }
        if let Some(text) = self.computer_clipboard.lock().unwrap().clone() {
            let message = ControlMessage::SetClipboard { sequence: 0, paste: false, text };
            if let Err(error) = session.send(&message) {
                log::warn!("clipboard to {}: {error}", session.serial());
            }
        }
    }

    /// Sends text to each phone once, however many windows it has open.
    pub fn broadcast_clipboard(&self, text: &str) {
        *self.computer_clipboard.lock().unwrap() = Some(text.to_string());
        let sessions = self.sessions.lock().unwrap();
        let mut seen = HashSet::new();
        for session in sessions.values().filter(|session| seen.insert(session.serial().to_string())) {
            let message =
                ControlMessage::SetClipboard { sequence: 0, paste: false, text: text.to_string() };
            if let Err(error) = session.send(&message) {
                log::warn!("clipboard to {}: {error}", session.serial());
            }
        }
    }

    fn ensure_server(&self, adb: &Adb, serial: &str, server_path: &Path) -> Result<()> {
        let mut pushed = self.pushed.lock().unwrap();
        if !pushed.contains(serial) {
            push_server(adb, serial, server_path)?;
            pushed.insert(serial.to_string());
        }
        Ok(())
    }

    pub fn list_apps(&self, adb: &Adb, serial: &str, server_path: &Path) -> Result<Vec<AppInfo>> {
        self.ensure_server(adb, serial, server_path)?;
        let result = apps::list_apps(adb, serial);
        if result.is_err() {
            self.pushed.lock().unwrap().remove(serial);
        }
        result
    }

    fn remove_ended(&self, key: &str, scid: Option<u32>) {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(key).is_some_and(|session| Some(session.scid()) == scid)
            && let Some(session) = sessions.remove(key) {
                drop(sessions);
                session.stop();
            }
    }

    pub fn stop(&self, key: &str) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .remove(key)
            .ok_or_else(|| AppError::NoSession(key.to_string()))?;
        session.stop();
        Ok(())
    }

    pub fn stop_all(&self) {
        let sessions: Vec<_> = self.sessions.lock().unwrap().drain().map(|(_, s)| s).collect();
        // Each stop waits on adb, so together they take no longer than one.
        thread::scope(|scope| {
            for session in &sessions {
                scope.spawn(|| session.stop());
            }
        });
    }

    pub fn send(&self, key: &str, message: &ControlMessage) -> Result<()> {
        self.sessions
            .lock()
            .unwrap()
            .get(key)
            .ok_or_else(|| AppError::NoSession(key.to_string()))?
            .send(message)
    }

    pub fn active_sessions(&self) -> Vec<String> {
        self.sessions.lock().unwrap().keys().cloned().collect()
    }
}
