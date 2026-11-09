use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::process::JobObject;
use super::protocol::{
    CODEC_H264, ControlMessage, DEVICE_NAME_SIZE, HEADER_SIZE, PacketMerger, SCRCPY_VERSION,
    StreamHeader, UiPacket, encode_ui_packet, encode_ui_session, parse_device_name, parse_header,
};
use crate::adb::commands::{create_forward, remove_forward};
use crate::adb::Adb;
use crate::error::{AppError, Result};

pub(super) const DEVICE_SERVER_PATH: &str = "/data/local/tmp/scrcpy-server.jar";
const PUSH_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_ATTEMPTS: u32 = 100;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_LOG_LINES: usize = 12;
/// Size and density of the virtual display that hosts a single-app window.
const APP_DISPLAY: &str = "720x1600/280";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MirrorOptions {
    /// Longest screen side in pixels, or 0 for native resolution.
    pub max_size: u32,
    pub bit_rate: u32,
    pub max_fps: u32,
}

impl Default for MirrorOptions {
    fn default() -> Self {
        Self { max_size: 1600, bit_rate: 8_000_000, max_fps: 60 }
    }
}

impl MirrorOptions {
    pub fn validate(&self) -> Result<()> {
        if self.max_size != 0 && !(240..=8192).contains(&self.max_size) {
            return Err(AppError::Invalid(format!("resolution {}", self.max_size)));
        }
        if !(100_000..=200_000_000).contains(&self.bit_rate) {
            return Err(AppError::Invalid(format!("bit rate {}", self.bit_rate)));
        }
        if !(1..=240).contains(&self.max_fps) {
            return Err(AppError::Invalid(format!("frame rate {}", self.max_fps)));
        }
        Ok(())
    }

    fn server_args(&self, scid: u32, app_window: bool) -> Vec<String> {
        let mut args = vec![
            format!("scid={scid:08x}"),
            "log_level=info".to_string(),
            "tunnel_forward=true".to_string(),
            "audio=false".to_string(),
            "video_codec=h264".to_string(),
            format!("video_bit_rate={}", self.bit_rate),
            format!("max_fps={}", self.max_fps),
            "stay_awake=true".to_string(),
            "clipboard_autosync=false".to_string(),
        ];
        if self.max_size != 0 {
            args.push(format!("max_size={}", self.max_size));
        }
        if app_window {
            args.push(format!("new_display={APP_DISPLAY}"));
            args.push("vd_system_decorations=false".to_string());
            args.push("display_ime_policy=local".to_string());
        }
        args
    }
}

fn new_scid() -> u32 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let salt = std::process::id().wrapping_mul(2_654_435_761);
    (nanos ^ salt) & 0x7fff_ffff
}

type ServerLog = Arc<Mutex<VecDeque<String>>>;

fn capture_output<R: Read + Send + 'static>(source: Option<R>, sink: ServerLog) {
    let Some(source) = source else { return };
    thread::spawn(move || {
        for line in BufReader::new(source).lines().map_while(io::Result::ok) {
            log::info!("[scrcpy-server] {line}");
            let mut lines = sink.lock().unwrap();
            if lines.len() == SERVER_LOG_LINES {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    });
}

fn describe_server_log(lines: &ServerLog) -> String {
    let lines = lines.lock().unwrap();
    lines
        .iter()
        .rev()
        .find(|line| line.contains("ERROR") || line.contains("Exception"))
        .or(lines.back())
        .cloned()
        .unwrap_or_else(|| "the server exited without output".to_string())
}

pub struct SessionParts {
    pub sink: Box<dyn FnMut(Vec<u8>) -> bool + Send>,
    pub on_end: Box<dyn FnOnce(String) + Send>,
}

pub struct Session {
    serial: String,
    scid: u32,
    adb: Adb,
    port: u16,
    device_name: String,
    child: Mutex<Option<Child>>,
    video: TcpStream,
    control: TcpStream,
    control_tx: Mutex<Option<Sender<Vec<u8>>>>,
    stopped: AtomicBool,
}

impl Session {
    pub fn start(
        adb: &Adb,
        serial: &str,
        server_path: &Path,
        options: MirrorOptions,
        start_app: Option<&str>,
        job: Option<&JobObject>,
        parts: SessionParts,
    ) -> Result<Arc<Self>> {
        options.validate()?;
        adb.start_server()?;
        push_server(adb, serial, server_path)?;

        let scid = new_scid();
        let port = create_forward(adb, serial, &format!("scrcpy_{scid:08x}"))?;
        let log = ServerLog::default();

        let spawned = spawn_server(adb, serial, scid, options, start_app.is_some(), job, log.clone());
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                let _ = remove_forward(adb, serial, port);
                return Err(error);
            }
        };

        let handshake = connect(port, &mut child, &log);
        let (video, control, device_name) = match handshake {
            Ok(connected) => connected,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = remove_forward(adb, serial, port);
                return Err(error);
            }
        };

        let (control_tx, control_rx) = mpsc::channel::<Vec<u8>>();
        let session = Arc::new(Self {
            serial: serial.to_string(),
            scid,
            adb: adb.clone(),
            port,
            device_name,
            child: Mutex::new(Some(child)),
            video: video.try_clone()?,
            control: control.try_clone()?,
            control_tx: Mutex::new(Some(control_tx)),
            stopped: AtomicBool::new(false),
        });

        let mut writer = control.try_clone()?;
        thread::spawn(move || {
            for message in control_rx {
                if writer.write_all(&message).is_err() {
                    break;
                }
            }
        });

        // Device messages must be consumed so the server never blocks on them.
        let mut reader = control;
        thread::spawn(move || {
            let _ = io::copy(&mut reader, &mut io::sink());
        });

        if let Some(package) = start_app {
            session.send(&ControlMessage::StartApp { name: package.to_string() })?;
        }

        let SessionParts { mut sink, on_end } = parts;
        let thread_session = session.clone();
        thread::spawn(move || {
            let reason = pump_video(video, &mut sink);
            log::info!("video stream ended for {}: {reason}", thread_session.serial);
            if !thread_session.stopped.load(Ordering::SeqCst) {
                on_end(reason);
            }
        });

        Ok(session)
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn scid(&self) -> u32 {
        self.scid
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Queues the message for the writer thread, so the UI thread never blocks on the socket.
    pub fn send(&self, message: &ControlMessage) -> Result<()> {
        let sender = self.control_tx.lock().unwrap();
        sender
            .as_ref()
            .ok_or_else(|| AppError::NoSession(self.serial.clone()))?
            .send(message.serialize())
            .map_err(|_| AppError::NoSession(self.serial.clone()))
    }

    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.control_tx.lock().unwrap().take();
        let _ = self.video.shutdown(Shutdown::Both);
        let _ = self.control.shutdown(Shutdown::Both);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Err(error) = remove_forward(&self.adb, &self.serial, self.port) {
            // Expected when the device has already been unplugged.
            log::debug!("remove forward for {}: {error}", self.serial);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn push_server(adb: &Adb, serial: &str, server_path: &Path) -> Result<()> {
    adb.run(
        &[
            "-s".as_ref(),
            serial.as_ref(),
            "push".as_ref(),
            server_path.as_os_str(),
            DEVICE_SERVER_PATH.as_ref(),
        ],
        PUSH_TIMEOUT,
    )
    .map(|_| ())
    .map_err(|error| AppError::ServerStart(error.to_string()))
}

fn spawn_server(
    adb: &Adb,
    serial: &str,
    scid: u32,
    options: MirrorOptions,
    app_window: bool,
    job: Option<&JobObject>,
    log: ServerLog,
) -> Result<Child> {
    let mut child = adb
        .command()
        .args(["-s", serial, "shell"])
        .arg(format!("CLASSPATH={DEVICE_SERVER_PATH}"))
        .args(["app_process", "/", "com.genymobile.scrcpy.Server", SCRCPY_VERSION])
        .args(options.server_args(scid, app_window))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(job) = job {
        if let Err(error) = job.assign(&child) {
            log::warn!("could not assign the server process to the job: {error}");
        }
    }
    capture_output(child.stdout.take(), log.clone());
    capture_output(child.stderr.take(), log);
    Ok(child)
}

/// adb accepts the forwarded connection before the server is listening, then
/// closes it. Only a received dummy byte proves that the server is really there.
fn connect_first_socket(port: u16, child: &mut Child, log: &ServerLog) -> Result<TcpStream> {
    for _ in 0..CONNECT_ATTEMPTS {
        if child.try_wait()?.is_some() {
            // Give the output readers a moment to capture the failure reason.
            thread::sleep(Duration::from_millis(100));
            return Err(AppError::ServerStart(describe_server_log(log)));
        }
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut dummy = [0u8; 1];
            if stream.read_exact(&mut dummy).is_ok() {
                return Ok(stream);
            }
        }
        thread::sleep(CONNECT_RETRY_DELAY);
    }
    Err(AppError::ServerStart(format!(
        "timed out waiting for the server ({})",
        describe_server_log(log)
    )))
}

fn connect(port: u16, child: &mut Child, log: &ServerLog) -> Result<(TcpStream, TcpStream, String)> {
    let mut video = connect_first_socket(port, child, log)?;
    let control = TcpStream::connect(("127.0.0.1", port))?;
    video.set_nodelay(true)?;
    control.set_nodelay(true)?;
    video.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;

    let mut name = [0u8; DEVICE_NAME_SIZE];
    video.read_exact(&mut name)?;
    let device_name = parse_device_name(&name);

    let mut codec = [0u8; 4];
    video.read_exact(&mut codec)?;
    match u32::from_be_bytes(codec) {
        CODEC_H264 => {}
        0 => return Err(AppError::ServerStart("the device disabled video capture".into())),
        1 => {
            return Err(AppError::ServerStart(format!(
                "video configuration error ({})",
                describe_server_log(log)
            )));
        }
        other => return Err(AppError::Protocol(format!("unexpected codec 0x{other:08x}"))),
    }

    video.set_read_timeout(None)?;
    Ok((video, control, device_name))
}

/// Reads packets until the stream ends and returns the reason it ended.
fn pump_video(mut video: TcpStream, sink: &mut (dyn FnMut(Vec<u8>) -> bool + Send)) -> String {
    let mut merger = PacketMerger::default();
    let mut header = [0u8; HEADER_SIZE];
    loop {
        if let Err(error) = video.read_exact(&mut header) {
            return format!("connection closed: {error}");
        }
        let packet = match parse_header(&header) {
            Ok(StreamHeader::Session { width, height }) => encode_ui_session(width, height),
            Ok(StreamHeader::Media { pts, config, key_frame, size }) => {
                let mut payload = vec![0u8; size as usize];
                if let Err(error) = video.read_exact(&mut payload) {
                    return format!("connection closed: {error}");
                }
                if config {
                    merger.on_config(&payload);
                    encode_ui_packet(UiPacket::Config, pts, &payload)
                } else {
                    let merged = merger.on_frame(payload);
                    let kind = if key_frame { UiPacket::KeyFrame } else { UiPacket::DeltaFrame };
                    encode_ui_packet(kind, pts, &merged)
                }
            }
            Err(error) => return error.to_string(),
        };
        if !sink(packet) {
            return "the video consumer went away".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_are_valid() {
        assert!(MirrorOptions::default().validate().is_ok());
    }

    #[test]
    fn rejects_out_of_range_options() {
        let base = MirrorOptions::default();
        assert!(MirrorOptions { max_size: 10, ..base }.validate().is_err());
        assert!(MirrorOptions { bit_rate: 1, ..base }.validate().is_err());
        assert!(MirrorOptions { max_fps: 0, ..base }.validate().is_err());
        assert!(MirrorOptions { max_size: 0, ..base }.validate().is_ok());
    }

    #[test]
    fn native_resolution_omits_max_size() {
        let options = MirrorOptions { max_size: 0, ..MirrorOptions::default() };
        let args = options.server_args(0xabc, false);
        assert!(args.contains(&"scid=00000abc".to_string()));
        assert!(args.contains(&"tunnel_forward=true".to_string()));
        assert!(!args.iter().any(|arg| arg.starts_with("max_size")));
    }

    #[test]
    fn app_windows_get_their_own_display() {
        let args = MirrorOptions::default().server_args(1, true);
        assert!(args.iter().any(|arg| arg.starts_with("new_display=")));
        let args = MirrorOptions::default().server_args(1, false);
        assert!(!args.iter().any(|arg| arg.starts_with("new_display=")));
    }

    #[test]
    fn scid_fits_31_bits() {
        assert!(new_scid() <= 0x7fff_ffff);
    }
}
