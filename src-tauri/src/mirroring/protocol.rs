//! Wire format of scrcpy-server v4.1. The server rejects any other client
//! version, so this constant and `resources/scrcpy-server` must change together.

use serde::Deserialize;

use crate::error::{AppError, Result};

pub const SCRCPY_VERSION: &str = "4.1";
pub const CODEC_H264: u32 = 0x6832_3634;
pub const HEADER_SIZE: usize = 12;
pub const DEVICE_NAME_SIZE: usize = 64;

const FLAG_SESSION: u8 = 0x80;
const FLAG_CONFIG: u64 = 1 << 62;
const FLAG_KEY_FRAME: u64 = 1 << 61;
const PTS_MASK: u64 = FLAG_KEY_FRAME - 1;
const MAX_PACKET_SIZE: u32 = 32 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 300;
const MAX_CLIPBOARD_BYTES: usize = (1 << 18) - 14;
const MAX_APP_NAME_BYTES: usize = 255;

/// `Session` is sent on start and again after each rotation or size change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamHeader {
    Session { width: u32, height: u32 },
    Media { pts: u64, config: bool, key_frame: bool, size: u32 },
}

pub fn parse_header(header: &[u8; HEADER_SIZE]) -> Result<StreamHeader> {
    let word = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    if header[0] & FLAG_SESSION != 0 {
        return Ok(StreamHeader::Session { width: word(4), height: word(8) });
    }
    let pts_flags = u64::from_be_bytes(header[..8].try_into().unwrap());
    let size = word(8);
    if size == 0 || size > MAX_PACKET_SIZE {
        return Err(AppError::Protocol(format!("invalid packet size {size}")));
    }
    Ok(StreamHeader::Media {
        pts: pts_flags & PTS_MASK,
        config: pts_flags & FLAG_CONFIG != 0,
        key_frame: pts_flags & FLAG_KEY_FRAME != 0,
        size,
    })
}

pub fn parse_device_name(buffer: &[u8; DEVICE_NAME_SIZE]) -> String {
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

/// Holds the codec config so it can be prepended to the following frame, since
/// a decoder without out-of-band config needs parameter sets in the key frame.
#[derive(Default)]
pub struct PacketMerger {
    pending_config: Option<Vec<u8>>,
}

impl PacketMerger {
    pub fn on_config(&mut self, config: &[u8]) {
        self.pending_config = Some(config.to_vec());
    }

    pub fn on_frame(&mut self, frame: Vec<u8>) -> Vec<u8> {
        match self.pending_config.take() {
            Some(mut merged) => {
                merged.extend_from_slice(&frame);
                merged
            }
            None => frame,
        }
    }
}

/// Kinds of the packets sent to the UI, in the first byte of each message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiPacket {
    Session = 0,
    Config = 1,
    KeyFrame = 2,
    DeltaFrame = 3,
}

/// UI packet layout: kind (`u8`), PTS (`u64` big-endian), payload.
pub fn encode_ui_packet(kind: UiPacket, pts: u64, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(9 + payload.len());
    packet.push(kind as u8);
    packet.extend_from_slice(&pts.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

pub fn encode_ui_session(width: u32, height: u32) -> Vec<u8> {
    let mut payload = [0u8; 8];
    payload[..4].copy_from_slice(&width.to_be_bytes());
    payload[4..].copy_from_slice(&height.to_be_bytes());
    encode_ui_packet(UiPacket::Session, 0, &payload)
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ControlMessage {
    Keycode { action: u8, keycode: u32, repeat: u32, metastate: u32 },
    Text { text: String },
    Touch {
        action: u8,
        // Signed because the well-known ids are negative and a u64 near 2^64
        // cannot be represented as a JavaScript number.
        pointer_id: i64,
        x: i32,
        y: i32,
        width: u16,
        height: u16,
        pressure: f32,
        action_button: u32,
        buttons: u32,
    },
    Scroll { x: i32, y: i32, width: u16, height: u16, hscroll: f32, vscroll: f32, buttons: u32 },
    BackOrScreenOn { action: u8 },
    ExpandNotificationPanel,
    CollapsePanels,
    SetClipboard { sequence: u64, paste: bool, text: String },
    SetDisplayPower { on: bool },
    RotateDevice,
    ResetVideo,
    // Only the backend may launch apps, after validating the package name.
    #[serde(skip_deserializing)]
    StartApp { name: String },
}

fn float_to_u16fp(value: f32) -> u16 {
    let scaled = (value.clamp(0.0, 1.0) * 65536.0) as u32;
    scaled.min(0xffff) as u16
}

fn float_to_i16fp(value: f32) -> i16 {
    let scaled = (value.clamp(-1.0, 1.0) * 32768.0) as i32;
    scaled.clamp(-0x8000, 0x7fff) as i16
}

fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn put_position(out: &mut Vec<u8>, x: i32, y: i32, width: u16, height: u16) {
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&y.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}

impl ControlMessage {
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        match self {
            ControlMessage::Keycode { action, keycode, repeat, metastate } => {
                out.push(0);
                out.push(*action);
                out.extend_from_slice(&keycode.to_be_bytes());
                out.extend_from_slice(&repeat.to_be_bytes());
                out.extend_from_slice(&metastate.to_be_bytes());
            }
            ControlMessage::Text { text } => {
                out.push(1);
                put_string(&mut out, truncate_utf8(text, MAX_TEXT_BYTES));
            }
            ControlMessage::Touch {
                action,
                pointer_id,
                x,
                y,
                width,
                height,
                pressure,
                action_button,
                buttons,
            } => {
                out.push(2);
                out.push(*action);
                out.extend_from_slice(&pointer_id.to_be_bytes());
                put_position(&mut out, *x, *y, *width, *height);
                out.extend_from_slice(&float_to_u16fp(*pressure).to_be_bytes());
                out.extend_from_slice(&action_button.to_be_bytes());
                out.extend_from_slice(&buttons.to_be_bytes());
            }
            ControlMessage::Scroll { x, y, width, height, hscroll, vscroll, buttons } => {
                out.push(3);
                put_position(&mut out, *x, *y, *width, *height);
                out.extend_from_slice(&float_to_i16fp(hscroll / 16.0).to_be_bytes());
                out.extend_from_slice(&float_to_i16fp(vscroll / 16.0).to_be_bytes());
                out.extend_from_slice(&buttons.to_be_bytes());
            }
            ControlMessage::BackOrScreenOn { action } => {
                out.push(4);
                out.push(*action);
            }
            ControlMessage::ExpandNotificationPanel => out.push(5),
            ControlMessage::CollapsePanels => out.push(7),
            ControlMessage::SetClipboard { sequence, paste, text } => {
                out.push(9);
                out.extend_from_slice(&sequence.to_be_bytes());
                out.push(u8::from(*paste));
                put_string(&mut out, truncate_utf8(text, MAX_CLIPBOARD_BYTES));
            }
            ControlMessage::SetDisplayPower { on } => {
                out.push(10);
                out.push(u8::from(*on));
            }
            ControlMessage::RotateDevice => out.push(11),
            ControlMessage::ResetVideo => out.push(17),
            ControlMessage::StartApp { name } => {
                let name = truncate_utf8(name, MAX_APP_NAME_BYTES);
                out.push(16);
                out.push(name.len() as u8);
                out.extend_from_slice(name.as_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_header() {
        let mut header = [0u8; 12];
        header[0] = 0x80;
        header[4..8].copy_from_slice(&1080u32.to_be_bytes());
        header[8..12].copy_from_slice(&2392u32.to_be_bytes());
        assert_eq!(
            parse_header(&header).unwrap(),
            StreamHeader::Session { width: 1080, height: 2392 }
        );
    }

    #[test]
    fn parses_media_flags_and_pts() {
        let mut header = [0u8; 12];
        header[..8].copy_from_slice(&(FLAG_KEY_FRAME | 1234).to_be_bytes());
        header[8..12].copy_from_slice(&99u32.to_be_bytes());
        assert_eq!(
            parse_header(&header).unwrap(),
            StreamHeader::Media { pts: 1234, config: false, key_frame: true, size: 99 }
        );

        header[..8].copy_from_slice(&FLAG_CONFIG.to_be_bytes());
        assert!(matches!(
            parse_header(&header).unwrap(),
            StreamHeader::Media { config: true, key_frame: false, .. }
        ));
    }

    #[test]
    fn rejects_bad_packet_sizes() {
        assert!(parse_header(&[0u8; 12]).is_err());
        let mut header = [0u8; 12];
        header[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_header(&header).is_err());
    }

    #[test]
    fn reads_nul_padded_device_name() {
        let mut buffer = [0u8; DEVICE_NAME_SIZE];
        buffer[..4].copy_from_slice(b"A059");
        assert_eq!(parse_device_name(&buffer), "A059");
    }

    #[test]
    fn merges_config_into_next_frame_only() {
        let mut merger = PacketMerger::default();
        merger.on_config(&[1, 2]);
        assert_eq!(merger.on_frame(vec![3]), vec![1, 2, 3]);
        assert_eq!(merger.on_frame(vec![4]), vec![4]);
    }

    #[test]
    fn ui_packet_layout() {
        assert_eq!(
            encode_ui_packet(UiPacket::KeyFrame, 258, &[9]),
            vec![2, 0, 0, 0, 0, 0, 0, 1, 2, 9]
        );
        assert_eq!(encode_ui_session(1, 2)[9..], [0, 0, 0, 1, 0, 0, 0, 2]);
    }

    #[test]
    fn serializes_keycode() {
        let bytes = ControlMessage::Keycode { action: 1, keycode: 3, repeat: 2, metastate: 65 }.serialize();
        assert_eq!(bytes, vec![0, 1, 0, 0, 0, 3, 0, 0, 0, 2, 0, 0, 0, 65]);
    }

    #[test]
    fn serializes_touch_to_32_bytes() {
        let bytes = ControlMessage::Touch {
            action: 0,
            pointer_id: -2,
            x: 100,
            y: 200,
            width: 1080,
            height: 2392,
            pressure: 1.0,
            action_button: 0,
            buttons: 0,
        }
        .serialize();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes[0], 2);
        assert_eq!(bytes[2..10], [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe]);
        assert_eq!(bytes[10..14], 100i32.to_be_bytes());
        assert_eq!(bytes[18..20], 1080u16.to_be_bytes());
        assert_eq!(bytes[22..24], [0xff, 0xff]);
    }

    #[test]
    fn serializes_scroll_to_21_bytes() {
        let bytes = ControlMessage::Scroll {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
            hscroll: 0.0,
            vscroll: -1.0,
            buttons: 0,
        }
        .serialize();
        assert_eq!(bytes.len(), 21);
        // -1/16 in i16 fixed point is -2048.
        assert_eq!(bytes[15..17], (-2048i16).to_be_bytes());
    }

    #[test]
    fn truncates_text_on_char_boundary() {
        let text = "é".repeat(200);
        let bytes = ControlMessage::Text { text }.serialize();
        let length = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
        assert_eq!(length, 300);
        assert_eq!(bytes.len(), 305);
    }

    #[test]
    fn serializes_start_app_with_one_byte_length() {
        let bytes = ControlMessage::StartApp { name: "a.b".into() }.serialize();
        assert_eq!(bytes, vec![16, 3, b'a', b'.', b'b']);
    }

    #[test]
    fn the_ui_cannot_send_start_app() {
        let parsed = serde_json::from_str::<ControlMessage>(r#"{"type":"startApp","name":"x"}"#);
        assert!(parsed.is_err());
    }

    #[test]
    fn deserializes_from_ui_json() {
        let message: ControlMessage = serde_json::from_str(
            r#"{"type":"backOrScreenOn","action":1}"#,
        )
        .unwrap();
        assert_eq!(message, ControlMessage::BackOrScreenOn { action: 1 });
        assert_eq!(ControlMessage::ResetVideo.serialize(), vec![17]);
    }
}
