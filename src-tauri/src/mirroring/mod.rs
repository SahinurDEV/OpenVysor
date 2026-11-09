pub mod apps;
#[cfg(windows)]
pub mod process;
pub mod protocol;
pub mod session;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::adb::Adb;
use crate::error::{AppError, Result};
use apps::AppInfo;
use process::JobObject;
use protocol::ControlMessage;
use session::{MirrorOptions, Session, SessionParts};

/// Sessions are keyed by an id chosen by the caller, because one device can
/// now have several at once (one per app window).
#[derive(Default)]
pub struct MirrorManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    starting: Mutex<HashSet<String>>,
    // Each server deletes its jar once running, so a concurrent start could
    // find the file gone. Pushes and launches therefore run one at a time.
    launch_lock: Mutex<()>,
    job: Option<JobObject>,
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
        sink: Box<dyn FnMut(Vec<u8>) -> bool + Send>,
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
        };

        let result = {
            let _launch = self.launch_lock.lock().unwrap();
            Session::start(adb, serial, server_path, options, start_app, self.job.as_ref(), parts)
        };
        // Publish before clearing `starting`, so a second start is always rejected.
        if let Ok(session) = &result {
            *session_slot.lock().unwrap() = Some(session.scid());
            self.sessions.lock().unwrap().insert(key.to_string(), session.clone());
        }
        self.starting.lock().unwrap().remove(key);
        result
    }

    pub fn list_apps(&self, adb: &Adb, serial: &str, server_path: &Path) -> Result<Vec<AppInfo>> {
        let _launch = self.launch_lock.lock().unwrap();
        apps::list_apps(adb, serial, server_path)
    }

    fn remove_ended(&self, key: &str, scid: Option<u32>) {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(key).is_some_and(|session| Some(session.scid()) == scid) {
            if let Some(session) = sessions.remove(key) {
                drop(sessions);
                session.stop();
            }
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
        for session in sessions {
            session.stop();
        }
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
