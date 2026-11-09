//! Writes the phone's own H.264 stream to MP4 without re-encoding, so recording costs almost no CPU.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use mp4::{AvcConfig, Bytes, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};

const TRACK: u32 = 1;
const MICROSECONDS: u32 = 1_000_000;
const LAST_FRAME_MICROSECONDS: u32 = 16_667;
const NAL_SPS: u8 = 7;
const NAL_PPS: u8 = 8;

/// Splits Annex B data into NAL units, without their start codes.
pub fn nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= data.len() {
        if data[index..index + 3] == [0, 0, 1] {
            starts.push((index, index + 3));
            index += 3;
        } else {
            index += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(position, &(_, begin))| {
            let mut end = starts.get(position + 1).map_or(data.len(), |&(next, _)| next);
            // A four-byte start code leaves one zero behind.
            if position + 1 < starts.len() && end > begin && data[end - 1] == 0 {
                end -= 1;
            }
            &data[begin..end]
        })
        .filter(|unit| !unit.is_empty())
        .collect()
}

fn nal_type(unit: &[u8]) -> u8 {
    unit[0] & 0x1f
}

/// Length-prefixed frame data for MP4. Parameter sets live in the file header instead.
pub fn to_length_prefixed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    for unit in nal_units(data) {
        if !matches!(nal_type(unit), NAL_SPS | NAL_PPS) {
            out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
            out.extend_from_slice(unit);
        }
    }
    out
}

fn parameter_set(config: &[u8], kind: u8) -> Option<Vec<u8>> {
    nal_units(config).into_iter().find(|unit| nal_type(unit) == kind).map(<[u8]>::to_vec)
}

struct PendingFrame {
    pts: u64,
    is_sync: bool,
    bytes: Vec<u8>,
}

pub struct Recorder {
    writer: Mp4Writer<BufWriter<File>>,
    path: PathBuf,
    first_pts: u64,
    pending: Option<PendingFrame>,
}

impl Recorder {
    pub fn create(path: &Path, width: u32, height: u32, config: &[u8], first_pts: u64) -> Result<Self, String> {
        let (Some(seq_param_set), Some(pic_param_set)) =
            (parameter_set(config, NAL_SPS), parameter_set(config, NAL_PPS))
        else {
            return Err("the stream has no H.264 parameter sets".to_string());
        };
        let brand = |name: &str| name.parse().map_err(|_| "invalid MP4 brand".to_string());
        let file = File::create(path).map_err(|error| error.to_string())?;
        let header = Mp4Config {
            major_brand: brand("isom")?,
            minor_version: 512,
            compatible_brands: vec![brand("isom")?, brand("iso2")?, brand("avc1")?, brand("mp41")?],
            timescale: 1000,
        };
        let mut writer = Mp4Writer::write_start(BufWriter::new(file), &header).map_err(|error| error.to_string())?;
        writer
            .add_track(&TrackConfig {
                track_type: TrackType::Video,
                timescale: MICROSECONDS,
                language: "und".to_string(),
                media_conf: MediaConfig::AvcConfig(AvcConfig {
                    width: width as u16,
                    height: height as u16,
                    seq_param_set,
                    pic_param_set,
                }),
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { writer, path: path.to_path_buf(), first_pts, pending: None })
    }

    /// A frame's duration is only known once the next one arrives, so each is held back by one.
    pub fn add_frame(&mut self, pts: u64, is_sync: bool, annex_b: &[u8]) -> Result<(), String> {
        let frame = PendingFrame { pts, is_sync, bytes: to_length_prefixed(annex_b) };
        if let Some(previous) = self.pending.replace(frame) {
            let duration = pts.saturating_sub(previous.pts).clamp(1, u64::from(u32::MAX)) as u32;
            self.write(previous, duration)?;
        }
        Ok(())
    }

    fn write(&mut self, frame: PendingFrame, duration: u32) -> Result<(), String> {
        let sample = Mp4Sample {
            start_time: frame.pts.saturating_sub(self.first_pts),
            duration,
            rendering_offset: 0,
            is_sync: frame.is_sync,
            bytes: Bytes::from(frame.bytes),
        };
        self.writer.write_sample(TRACK, &sample).map_err(|error| error.to_string())
    }

    pub fn finish(mut self) -> Result<PathBuf, String> {
        if let Some(frame) = self.pending.take() {
            self.write(frame, LAST_FRAME_MICROSECONDS)?;
        }
        self.writer.write_end().map_err(|error| error.to_string())?;
        Ok(self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_three_and_four_byte_start_codes() {
        let data = [0, 0, 0, 1, 0x67, 9, 0, 0, 1, 0x68, 8, 0, 0, 0, 1, 0x65, 7];
        let units = nal_units(&data);
        assert_eq!(units, vec![&[0x67, 9][..], &[0x68, 8][..], &[0x65, 7][..]]);
    }

    #[test]
    fn frames_drop_parameter_sets_and_gain_lengths() {
        let data = [0, 0, 0, 1, 0x67, 9, 0, 0, 0, 1, 0x65, 7, 6];
        assert_eq!(to_length_prefixed(&data), vec![0, 0, 0, 3, 0x65, 7, 6]);
    }

    #[test]
    fn writes_a_playable_header_and_rejects_missing_config() {
        let folder = std::env::temp_dir().join(format!("openvysor-rec-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("test.mp4");
        assert!(Recorder::create(&path, 2, 2, &[0, 0, 1, 0x65], 0).is_err());

        let config = [0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 0, 1, 0x68, 0xce];
        let mut recorder = Recorder::create(&path, 2, 2, &config, 100).unwrap();
        recorder.add_frame(100, true, &[0, 0, 0, 1, 0x65, 1]).unwrap();
        recorder.add_frame(200, false, &[0, 0, 0, 1, 0x41, 2]).unwrap();
        let written = recorder.finish().unwrap();

        let bytes = std::fs::read(written).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        assert!(bytes.windows(4).any(|window| window == b"moov"));
    }
}
