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
    CODEC_H264, CODEC_RAW, ControlMessage, DEVICE_NAME_SIZE, DeviceMessage, HEADER_SIZE, PacketMerger,
    SCRCPY_VERSION, StreamHeader, UiPacket, encode_ui_packet, encode_ui_session, parse_device_message,
    parse_device_name, parse_header,
};
use crate::adb::commands::{create_forward, remove_forward};
use crate::adb::Adb;
use crate::error::{AppError, Result};

const PUSH_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_ATTEMPTS: u32 = 500;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(20);
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
    /// Sends phone audio to the computer. Android stops playing it on the phone meanwhile.
    #[serde(default)]
    pub audio: bool,
}

impl Default for MirrorOptions {
    fn default() -> Self {
        Self { max_size: 1600, bit_rate: 8_000_000, max_fps: 60, audio: false }
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
            "video_codec=h264".to_string(),
            format!("video_bit_rate={}", self.bit_rate),
            format!("max_fps={}", self.max_fps),
            "stay_awake=true".to_string(),
        ];
        if self.max_size != 0 {
            args.push(format!("max_size={}", self.max_size));
        }
        // Audio capture covers the whole phone, so it never belongs to one app window.
        if self.audio && !app_window {
            // Raw PCM needs no decoder and adds no encoding delay on the phone.
            args.push("audio_codec=raw".to_string());
        } else {
            args.push("audio=false".to_string());
        }
        if app_window {
            args.push(format!("new_display={APP_DISPLAY}"));
            args.push("vd_system_decorations=false".to_string());
            args.push("display_ime_policy=local".to_string());
        }
        args
    }
}

/// The copy kept on the phone, pushed once per phone each time the app runs.
fn stored_server_path() -> String {
    format!("/data/local/tmp/openvysor-server-{SCRCPY_VERSION}.jar")
}

/// A running server deletes its own jar, so each launch runs a throwaway copy.
/// The words go through adb's shell on the phone: they must stay free of user input.
pub(super) fn copy_and_set_classpath(jar: &str) -> Vec<String> {
    vec!["cp".to_string(), stored_server_path(), jar.to_string(), "&&".to_string(), format!("CLASSPATH={jar}")]
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

pub type PacketSink = Arc<dyn Fn(Vec<u8>) -> bool + Send + Sync>;
pub type ClipboardHandler = Arc<dyn Fn(String) + Send + Sync>;

pub struct SessionParts {
    pub sink: PacketSink,
    pub on_end: Box<dyn FnOnce(String) + Send>,
    pub on_clipboard: ClipboardHandler,
}

pub struct Session {
    serial: String,
    scid: u32,
    adb: Adb,
    port: u16,
    device_name: String,
    child: Mutex<Option<Child>>,
    video: TcpStream,
    audio: Option<TcpStream>,
    control: TcpStream,
    control_tx: Mutex<Option<Sender<Vec<u8>>>>,
    stopped: AtomicBool,
}

impl Session {
    pub fn start(
        adb: &Adb,
        serial: &str,
        options: MirrorOptions,
        start_app: Option<&str>,
        job: Option<&JobObject>,
        parts: SessionParts,
    ) -> Result<Arc<Self>> {
        options.validate()?;

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

        let wants_audio = options.audio && start_app.is_none();
        let handshake = connect(port, wants_audio, &mut child, &log);
        let (video, audio, control, device_name) = match handshake {
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
            audio: audio.as_ref().map(TcpStream::try_clone).transpose()?,
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

        let SessionParts { sink, on_end, on_clipboard } = parts;

        // Device messages must be consumed so the server never blocks on them.
        thread::spawn(move || pump_device_messages(control, &*on_clipboard));

        if let Some(audio) = audio {
            let audio_sink = sink.clone();
            thread::spawn(move || {
                let reason = pump_audio(audio, &*audio_sink);
                log::info!("audio stream ended: {reason}");
            });
        }

        if let Some(package) = start_app {
            session.send(&ControlMessage::StartApp { name: package.to_string() })?;
        }

        let thread_session = session.clone();
        thread::spawn(move || {
            let reason = pump_video(video, &*sink);
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
        if let Some(audio) = &self.audio {
            let _ = audio.shutdown(Shutdown::Both);
        }
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
            stored_server_path().as_ref(),
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
        .args(copy_and_set_classpath(&format!("/data/local/tmp/openvysor-{scid:08x}.jar")))
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

type Connected = (TcpStream, Option<TcpStream>, TcpStream, String);

fn connect(port: u16, wants_audio: bool, child: &mut Child, log: &ServerLog) -> Result<Connected> {
    let mut video = connect_first_socket(port, child, log)?;
    // The server accepts the sockets in exactly this order: video, audio, control.
    let audio = if wants_audio { Some(TcpStream::connect(("127.0.0.1", port))?) } else { None };
    let control = TcpStream::connect(("127.0.0.1", port))?;
    video.set_nodelay(true)?;
    if let Some(audio) = &audio {
        audio.set_nodelay(true)?;
    }
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
    Ok((video, audio, control, device_name))
}

fn pump_device_messages(mut control: TcpStream, on_clipboard: &(dyn Fn(String) + Send + Sync)) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match control.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
        }
        loop {
            match parse_device_message(&buffer) {
                Ok(Some((message, length))) => {
                    if let DeviceMessage::Clipboard(text) = message {
                        on_clipboard(text);
                    }
                    buffer.drain(..length);
                }
                Ok(None) => break,
                Err(error) => {
                    log::warn!("device message: {error}");
                    return;
                }
            }
        }
    }
}

fn pump_audio(mut audio: TcpStream, sink: &(dyn Fn(Vec<u8>) -> bool + Send + Sync)) -> String {
    let mut codec = [0u8; 4];
    if let Err(error) = audio.read_exact(&mut codec) {
        return format!("connection closed: {error}");
    }
    match u32::from_be_bytes(codec) {
        CODEC_RAW => {}
        0 => return "this device cannot capture audio".to_string(),
        other => return format!("unsupported audio codec 0x{other:08x}"),
    }
    let mut header = [0u8; HEADER_SIZE];
    loop {
        if let Err(error) = audio.read_exact(&mut header) {
            return format!("connection closed: {error}");
        }
        let (pts, config, size) = match parse_header(&header) {
            Ok(StreamHeader::Media { pts, config, size, .. }) => (pts, config, size),
            Ok(StreamHeader::Session { .. }) => continue,
            Err(error) => return error.to_string(),
        };
        let mut payload = vec![0u8; size as usize];
        if let Err(error) = audio.read_exact(&mut payload) {
            return format!("connection closed: {error}");
        }
        let kind = if config { UiPacket::AudioConfig } else { UiPacket::AudioFrame };
        if !sink(encode_ui_packet(kind, pts, &payload)) {
            return "the audio consumer went away".to_string();
        }
    }
}

/// Reads packets until the stream ends and returns the reason it ended.
fn pump_video(mut video: TcpStream, sink: &(dyn Fn(Vec<u8>) -> bool + Send + Sync)) -> String {
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
    fn audio_is_only_captured_for_the_whole_phone() {
        let options = MirrorOptions { audio: true, ..MirrorOptions::default() };
        assert!(options.server_args(1, false).contains(&"audio_codec=raw".to_string()));
        assert!(options.server_args(1, true).contains(&"audio=false".to_string()));
        assert!(MirrorOptions::default().server_args(1, false).contains(&"audio=false".to_string()));
    }

    #[test]
    fn scid_fits_31_bits() {
        assert!(new_scid() <= 0x7fff_ffff);
    }
}
