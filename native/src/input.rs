//! Maps desktop input to Android input.

use eframe::egui::{Key, Modifiers, Pos2, Rect, Vec2};

pub const ACTION_DOWN: u8 = 0;
pub const ACTION_UP: u8 = 1;
pub const ACTION_MOVE: u8 = 2;
pub const POINTER_ID_FINGER: i64 = -2;

pub const KEYCODE_HOME: u32 = 3;
pub const KEYCODE_BACK: u32 = 4;
pub const KEYCODE_VOLUME_UP: u32 = 24;
pub const KEYCODE_VOLUME_DOWN: u32 = 25;
pub const KEYCODE_APP_SWITCH: u32 = 187;
pub const KEYCODE_CUT: u32 = 277;
pub const KEYCODE_COPY: u32 = 278;

const KEYCODE_A: u32 = 29;
const KEYCODE_0: u32 = 7;
const KEYCODE_F1: u32 = 131;

pub const STRIP_HEIGHT: f32 = 32.0;
const LONG_SIDE: f32 = 760.0;
const MAX_SCROLL: f32 = 16.0;
/// Pixels of a smooth-scrolling touchpad that count as one wheel notch.
const POINTS_PER_NOTCH: f32 = 50.0;

fn sequential(key: Key, first: Key, base: u32) -> u32 {
    base + key as u32 - first as u32
}

/// Keys that produce text are left out unless Ctrl or Alt is held, because
/// their characters arrive separately and work with any keyboard layout.
pub fn android_keycode(key: Key, modifiers: Modifiers) -> Option<u32> {
    let chord = modifiers.ctrl || modifiers.alt;
    Some(match key {
        Key::Enter => 66,
        Key::Backspace => 67,
        Key::Delete => 112,
        Key::Tab => 61,
        Key::Escape => 111,
        Key::ArrowUp => 19,
        Key::ArrowDown => 20,
        Key::ArrowLeft => 21,
        Key::ArrowRight => 22,
        Key::Home => 122,
        Key::End => 123,
        Key::PageUp => 92,
        Key::PageDown => 93,
        Key::Insert => 124,
        Key::Space if chord => 62,
        _ if (Key::A as u32..=Key::Z as u32).contains(&(key as u32)) && chord => {
            sequential(key, Key::A, KEYCODE_A)
        }
        _ if (Key::Num0 as u32..=Key::Num9 as u32).contains(&(key as u32)) && chord => {
            sequential(key, Key::Num0, KEYCODE_0)
        }
        _ if (Key::F1 as u32..=Key::F12 as u32).contains(&(key as u32)) => sequential(key, Key::F1, KEYCODE_F1),
        _ => return None,
    })
}

pub fn android_metastate(modifiers: Modifiers) -> u32 {
    let mut state = 0;
    if modifiers.shift {
        state |= 0x1 | 0x40;
    }
    if modifiers.alt {
        state |= 0x2 | 0x10;
    }
    if modifiers.ctrl {
        state |= 0x1000 | 0x2000;
    }
    state
}

/// The largest rectangle with the video's shape, centered in `area`.
pub fn fit_inside(area: Rect, video: [u32; 2]) -> Rect {
    if video[0] == 0 || video[1] == 0 {
        return Rect::from_center_size(area.center(), Vec2::ZERO);
    }
    let (width, height) = (video[0] as f32, video[1] as f32);
    let scale = (area.width() / width).min(area.height() / height);
    Rect::from_center_size(area.center(), Vec2::new(width * scale, height * scale))
}

/// Converts a window position into video coordinates, clamped to the frame.
pub fn to_video_point(position: Pos2, rect: Rect, video: [u32; 2]) -> (i32, i32) {
    let convert = |offset: f32, extent: f32, side: u32| {
        (((offset / extent) * side as f32).round() as i32).clamp(0, side as i32 - 1)
    };
    (
        convert(position.x - rect.left(), rect.width(), video[0]),
        convert(position.y - rect.top(), rect.height(), video[1]),
    )
}

/// `delta` is in wheel notches when `lines` is set, or in points otherwise.
pub fn wheel_to_scroll(delta: Vec2, lines: bool) -> (f32, f32) {
    let divisor = if lines { 1.0 } else { POINTS_PER_NOTCH };
    let convert = |value: f32| (value / divisor).clamp(-MAX_SCROLL, MAX_SCROLL);
    (convert(delta.x), convert(delta.y))
}

/// Window size that matches the video, so the phone shape follows rotation.
pub fn phone_window_size(video: [u32; 2]) -> Option<Vec2> {
    let (width, height) = (video[0] as f32, video[1] as f32);
    if width == 0.0 || height == 0.0 {
        return None;
    }
    Some(if height >= width {
        Vec2::new((LONG_SIDE * width / height).round(), LONG_SIDE + STRIP_HEIGHT)
    } else {
        Vec2::new(LONG_SIDE, (LONG_SIDE * height / width).round() + STRIP_HEIGHT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    const CTRL: Modifiers = Modifiers::CTRL;

    #[test]
    fn maps_special_keys() {
        assert_eq!(android_keycode(Key::Enter, Modifiers::NONE), Some(66));
        assert_eq!(android_keycode(Key::F1, Modifiers::NONE), Some(131));
        assert_eq!(android_keycode(Key::F12, Modifiers::NONE), Some(142));
    }

    #[test]
    fn text_keys_are_only_sent_as_keys_in_a_chord() {
        assert_eq!(android_keycode(Key::A, Modifiers::NONE), None);
        assert_eq!(android_keycode(Key::Space, Modifiers::NONE), None);
        assert_eq!(android_keycode(Key::A, CTRL), Some(29));
        assert_eq!(android_keycode(Key::Z, CTRL), Some(54));
        assert_eq!(android_keycode(Key::Num5, CTRL), Some(12));
    }

    #[test]
    fn builds_metastate() {
        assert_eq!(android_metastate(Modifiers::NONE), 0);
        assert_eq!(android_metastate(Modifiers::SHIFT), 0x41);
        assert_eq!(android_metastate(CTRL), 0x3000);
    }

    #[test]
    fn fits_a_tall_video_and_centers_it() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(200.0, 200.0));
        let rect = fit_inside(area, [100, 200]);
        assert_eq!(rect.size(), Vec2::new(100.0, 200.0));
        assert_eq!(rect.left(), 50.0);
    }

    #[test]
    fn converts_and_clamps_positions() {
        let rect = Rect::from_min_size(pos2(10.0, 0.0), Vec2::new(100.0, 200.0));
        assert_eq!(to_video_point(pos2(60.0, 100.0), rect, [1000, 2000]), (500, 1000));
        assert_eq!(to_video_point(pos2(-50.0, 900.0), rect, [1000, 2000]), (0, 1999));
    }

    #[test]
    fn scales_and_limits_scrolling() {
        assert_eq!(wheel_to_scroll(Vec2::new(0.0, 1.0), true), (0.0, 1.0));
        assert_eq!(wheel_to_scroll(Vec2::new(-50.0, 0.0), false), (-1.0, 0.0));
        assert_eq!(wheel_to_scroll(Vec2::new(0.0, 99.0), true), (0.0, 16.0));
    }

    #[test]
    fn sizes_the_window_to_the_phone_shape() {
        assert_eq!(phone_window_size([380, 760]), Some(Vec2::new(380.0, 792.0)));
        assert_eq!(phone_window_size([760, 380]), Some(Vec2::new(760.0, 412.0)));
        assert_eq!(phone_window_size([0, 0]), None);
    }
}
