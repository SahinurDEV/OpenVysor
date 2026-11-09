//! Plays the phone's raw 48 kHz stereo PCM through the default Windows output.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const SOURCE_RATE: f64 = 48_000.0;
/// Beyond this much queued audio, the oldest is dropped to stay close to live.
const MAX_QUEUED_FRAMES: usize = 48_000 / 8;

type Queue = Arc<Mutex<VecDeque<[f32; 2]>>>;

pub fn decode_pcm(bytes: &[u8]) -> impl Iterator<Item = [f32; 2]> + '_ {
    bytes.chunks_exact(4).map(|frame| {
        let sample = |at: usize| f32::from(i16::from_le_bytes([frame[at], frame[at + 1]])) / 32768.0;
        [sample(0), sample(2)]
    })
}

fn push_limited(queue: &mut VecDeque<[f32; 2]>, frames: impl Iterator<Item = [f32; 2]>) {
    queue.extend(frames);
    let excess = queue.len().saturating_sub(MAX_QUEUED_FRAMES);
    queue.drain(..excess);
}

/// Reads frames at `step` source frames per output frame, or silence when empty.
struct Resampler {
    step: f64,
    position: f64,
}

impl Resampler {
    fn next(&mut self, queue: &mut VecDeque<[f32; 2]>) -> [f32; 2] {
        let Some(&first) = queue.front() else { return [0.0; 2] };
        let second = queue.get(1).copied().unwrap_or(first);
        let t = self.position as f32;
        let frame = [first[0] + (second[0] - first[0]) * t, first[1] + (second[1] - first[1]) * t];
        self.position += self.step;
        while self.position >= 1.0 && !queue.is_empty() {
            queue.pop_front();
            self.position -= 1.0;
        }
        frame
    }
}

fn open_output(queue: Queue) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "no audio output device".to_string())?;
    let config = device.default_output_config().map_err(|error| error.to_string())?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported audio format {:?}", config.sample_format()));
    }
    let channels = usize::from(config.channels());
    let mut resampler = Resampler { step: SOURCE_RATE / f64::from(config.sample_rate()), position: 0.0 };
    let stream = device
        .build_output_stream::<f32, _, _>(
            config.config(),
            move |output, _| {
                let mut queue = queue.lock().unwrap();
                for frame in output.chunks_mut(channels) {
                    let [left, right] = resampler.next(&mut queue);
                    frame.fill(0.0);
                    if let [only] = frame {
                        *only = (left + right) / 2.0;
                    } else {
                        frame[0] = left;
                        frame[1] = right;
                    }
                }
            },
            |error| log::warn!("audio output: {error}"),
            None,
        )
        .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

/// Returns the sender for PCM packets. Playback stops when it is dropped.
pub fn spawn() -> Result<Sender<Vec<u8>>, String> {
    let queue = Queue::default();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let (ready_tx, ready_rx) = mpsc::channel();
    let stream_queue = queue.clone();
    // cpal streams must stay on the thread that created them.
    thread::spawn(move || {
        let stream = match open_output(stream_queue) {
            Ok(stream) => {
                let _ = ready_tx.send(Ok(()));
                stream
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
                return;
            }
        };
        for packet in rx {
            push_limited(&mut queue.lock().unwrap(), decode_pcm(&packet));
        }
        drop(stream);
    });
    ready_rx.recv().map_err(|error| error.to_string())??;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_little_endian_stereo_and_ignores_partial_frames() {
        let frames: Vec<_> = decode_pcm(&[0, 0x40, 0, 0xc0, 1]).collect();
        assert_eq!(frames, vec![[0.5, -0.5]]);
    }

    #[test]
    fn keeps_only_the_newest_audio() {
        let mut queue = VecDeque::new();
        push_limited(&mut queue, (0..MAX_QUEUED_FRAMES + 5).map(|n| [n as f32; 2]));
        assert_eq!(queue.len(), MAX_QUEUED_FRAMES);
        assert_eq!(queue[0], [5.0; 2]);
    }

    #[test]
    fn plays_silence_when_empty_and_consumes_at_the_given_rate() {
        let mut resampler = Resampler { step: 2.0, position: 0.0 };
        let mut queue = VecDeque::new();
        assert_eq!(resampler.next(&mut queue), [0.0; 2]);
        queue.extend([[1.0; 2], [2.0; 2], [3.0; 2]]);
        assert_eq!(resampler.next(&mut queue), [1.0; 2]);
        assert_eq!(queue.len(), 1);
    }
}
