//! Turns session packets into the newest picture for a window to paint.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{Color32, ColorImage};

use crate::decoder::Decoder;
use crate::mirroring::session::PacketSink;
use crate::recorder::Recorder;

const KIND_SESSION: u8 = 0;
const KIND_CONFIG: u8 = 1;
const KIND_KEY_FRAME: u8 = 2;
const KIND_DELTA_FRAME: u8 = 3;
const KIND_AUDIO_CONFIG: u8 = 4;
const KIND_AUDIO_FRAME: u8 = 5;
const PAYLOAD_OFFSET: usize = 9;
const MAX_PRESENT_GAP: Duration = Duration::from_millis(33);

pub type Repaint = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
enum Recording {
    #[default]
    Idle,
    Waiting(PathBuf),
    Active(Recorder),
}

#[derive(Default)]
pub struct Video {
    /// Only the newest picture is kept, so a slow window never falls behind live.
    latest: Mutex<Option<ColorImage>>,
    size: Mutex<[u32; 2]>,
    error: Mutex<Option<String>>,
    frames: AtomicU64,
    recording: Mutex<Recording>,
    ended_recording: Mutex<Option<Result<PathBuf, String>>>,
    display_long_side: AtomicU32,
}

impl Video {
    /// Tells the decoder how large the picture is on screen, in physical pixels.
    pub fn set_display_long_side(&self, pixels: u32) {
        self.display_long_side.store(pixels, Ordering::Relaxed);
    }

    pub fn take_latest(&self) -> Option<ColorImage> {
        self.latest.lock().unwrap().take()
    }

    #[cfg(debug_assertions)]
    pub fn set_sample_size(&self, size: [u32; 2]) {
        *self.size.lock().unwrap() = size;
    }

    pub fn size(&self) -> [u32; 2] {
        *self.size.lock().unwrap()
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }

    /// Recording begins at the next key frame.
    pub fn start_recording(&self, path: PathBuf) {
        *self.recording.lock().unwrap() = Recording::Waiting(path);
    }

    pub fn is_recording(&self) -> bool {
        !matches!(*self.recording.lock().unwrap(), Recording::Idle)
    }

    pub fn stop_recording(&self) -> Option<Result<PathBuf, String>> {
        match std::mem::take(&mut *self.recording.lock().unwrap()) {
            Recording::Idle | Recording::Waiting(_) => None,
            Recording::Active(recorder) => Some(recorder.finish()),
        }
    }

    /// The result of a recording that ended without the user pressing stop.
    pub fn take_ended_recording(&self) -> Option<Result<PathBuf, String>> {
        self.ended_recording.lock().unwrap().take()
    }

    fn end_recording(&self) {
        if let Some(result) = self.stop_recording() {
            *self.ended_recording.lock().unwrap() = Some(result);
        }
    }

    fn record_frame(&self, config: &[u8], pts: u64, key_frame: bool, data: &[u8]) {
        let mut recording = self.recording.lock().unwrap();
        if key_frame && matches!(*recording, Recording::Waiting(_)) {
            let Recording::Waiting(path) = std::mem::take(&mut *recording) else { return };
            let [width, height] = self.size();
            match Recorder::create(&path, width, height, config, pts) {
                Ok(recorder) => *recording = Recording::Active(recorder),
                Err(error) => *self.ended_recording.lock().unwrap() = Some(Err(error)),
            }
        }
        if let Recording::Active(recorder) = &mut *recording {
            if let Err(error) = recorder.add_frame(pts, key_frame, data) {
                *recording = Recording::Idle;
                *self.ended_recording.lock().unwrap() = Some(Err(error));
            }
        }
    }
}

fn read_session(payload: &[u8]) -> Option<[u32; 2]> {
    let word = |at: usize| Some(u32::from_be_bytes(payload.get(at..at + 4)?.try_into().ok()?));
    Some([word(0)?, word(4)?])
}

fn decode_loop(packets: Receiver<Vec<u8>>, video: Arc<Video>, repaint: Repaint) {
    let mut decoder = match Decoder::new() {
        Ok(decoder) => decoder,
        Err(error) => {
            *video.error.lock().unwrap() = Some(format!("the H.264 decoder failed to start: {error}"));
            repaint();
            return;
        }
    };
    let mut config = Vec::new();
    let mut pixels: Vec<Color32> = Vec::new();
    let mut presented = Instant::now();
    let mut waiting = false;
    let mut queued = None;

    loop {
        let Some(packet) = queued.take().or_else(|| packets.recv().ok()) else { break };
        let Some((&kind, _)) = packet.split_first() else { continue };
        let Some(payload) = packet.get(PAYLOAD_OFFSET..) else { continue };
        let pts = u64::from_be_bytes(packet[1..PAYLOAD_OFFSET].try_into().unwrap());
        match kind {
            KIND_SESSION => {
                let Some(size) = read_session(payload) else { continue };
                // An MP4 track has one fixed size, so rotation ends the recording.
                if video.size() != size {
                    video.end_recording();
                }
                *video.size.lock().unwrap() = size;
            }
            KIND_CONFIG => config = payload.to_vec(),
            KIND_KEY_FRAME | KIND_DELTA_FRAME => {
                video.record_frame(&config, pts, kind == KIND_KEY_FRAME, payload);
                match decoder.decode(payload, pts) {
                    Ok(produced) => waiting |= produced,
                    Err(error) => log::warn!("video decode: {error}"),
                }
            }
            _ => {}
        }

        // When frames are queued, only the last one is worth converting. The
        // time limit keeps the picture moving if the queue never empties.
        queued = packets.try_recv().ok();
        if waiting && (queued.is_none() || presented.elapsed() >= MAX_PRESENT_GAP) {
            waiting = false;
            presented = Instant::now();
            let size = video.size();
            let [width, height] = size.map(|side| side as usize);
            let step = picture_step(size[0].max(size[1]), video.display_long_side.load(Ordering::Relaxed));
            match decoder.present(width, height, step, &mut pixels) {
                Ok(true) if pixels.len() == (width / step) * (height / step) => {
                    let image = ColorImage::new([width / step, height / step], std::mem::take(&mut pixels));
                    if let Some(unused) = video.latest.lock().unwrap().replace(image) {
                        pixels = unused.pixels;
                    }
                    video.frames.fetch_add(1, Ordering::Relaxed);
                    repaint();
                }
                Ok(_) => {}
                Err(error) => log::warn!("video convert: {error}"),
            }
        }
    }
    video.end_recording();
}

/// How many video pixels make one painted pixel, never showing less than the
/// window can display. Zero means the window size is not known yet.
fn picture_step(video_long_side: u32, display_long_side: u32) -> usize {
    if display_long_side == 0 { 1 } else { (video_long_side / display_long_side).max(1) as usize }
}

/// Builds the sink a session writes to. Audio goes to `audio` when it is set.
pub fn packet_sink(video: Arc<Video>, repaint: Repaint, audio: Option<Sender<Vec<u8>>>) -> PacketSink {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || decode_loop(rx, video, repaint));
    Arc::new(move |mut packet: Vec<u8>| match packet.first() {
        Some(&KIND_AUDIO_FRAME | &KIND_AUDIO_CONFIG) => {
            if let Some(audio) = &audio {
                packet.drain(..PAYLOAD_OFFSET);
                let _ = audio.send(packet);
            }
            true
        }
        _ => tx.send(packet).is_ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_session_size() {
        assert_eq!(read_session(&[0, 0, 2, 0, 0, 0, 3, 0]), Some([512, 768]));
        assert_eq!(read_session(&[0, 0]), None);
    }

    #[test]
    fn the_picture_is_never_smaller_than_the_window() {
        assert_eq!(picture_step(2392, 0), 1);
        assert_eq!(picture_step(2392, 2392), 1);
        assert_eq!(picture_step(2392, 3000), 1);
        assert_eq!(picture_step(2392, 950), 2);
        assert_eq!(picture_step(2392, 760), 3);
    }

    #[test]
    fn stopping_before_the_first_key_frame_saves_nothing() {
        let video = Video::default();
        video.start_recording(PathBuf::from("unused.mp4"));
        assert!(video.is_recording());
        assert!(video.stop_recording().is_none());
        assert!(!video.is_recording());
    }
}
