//! H.264 decoding, and the conversion to pixels every platform shares.

use eframe::egui::Color32;

#[cfg(windows)]
mod media_foundation;
#[cfg(not(windows))]
mod software;

#[cfg(windows)]
pub use media_foundation::Decoder;
#[cfg(not(windows))]
pub use software::Decoder;

/// Below this many pixels, starting threads costs more than it saves.
const PARALLEL_FROM_PIXELS: usize = 200_000;
const MAX_CONVERT_THREADS: usize = 4;

/// Converts limited-range NV12 to opaque pixels, `width / step` by `height / step`
/// of them. `plane_height` is the coded height, which can be larger than the
/// visible `height`. Each output pixel averages the brightness it covers, which
/// keeps small text readable.
#[allow(clippy::too_many_arguments)]
pub fn nv12_to_pixels(
    data: &[u8],
    stride: usize,
    plane_height: usize,
    width: usize,
    height: usize,
    step: usize,
    bt709: bool,
    out: &mut Vec<Color32>,
) {
    let (rv, gu, gv, bu) = if bt709 { (459, 55, 136, 541) } else { (409, 100, 208, 516) };
    let step = step.max(1);
    let width = width.min(stride);
    let height = height.min(plane_height);
    let (out_width, out_height) = (width / step, height / step);
    let chroma_start = stride * plane_height;
    if out_width == 0 || out_height == 0 || data.len() < chroma_start + stride * height.div_ceil(2) {
        out.clear();
        return;
    }
    out.resize(out_width * out_height, Color32::BLACK);
    let (luma, chroma) = data.split_at(chroma_start);
    let covered = (step * step) as i32;

    let convert = |first_row: usize, rows: &mut [Color32]| {
        for (index, row) in rows.chunks_exact_mut(out_width).enumerate() {
            let source_row = (first_row + index) * step;
            let chroma_row = &chroma[(source_row / 2) * stride..][..width];
            for (column, pixel) in row.iter_mut().enumerate() {
                let x = column * step;
                let brightness = (0..step)
                    .map(|down| luma[(source_row + down) * stride..][x..x + step].iter().map(|&value| i32::from(value)).sum::<i32>())
                    .sum::<i32>();
                let y = 298 * (brightness / covered - 16);
                let pair = x & !1;
                let u = i32::from(chroma_row[pair]) - 128;
                let v = i32::from(chroma_row[(pair + 1).min(width - 1)]) - 128;
                let clamp = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
                *pixel = Color32::from_rgb(clamp(y + rv * v), clamp(y - gu * u - gv * v), clamp(y + bu * u));
            }
        }
    };

    if width * height < PARALLEL_FROM_PIXELS {
        convert(0, out);
        return;
    }
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(MAX_CONVERT_THREADS);
    let rows_each = out_height.div_ceil(workers);
    std::thread::scope(|scope| {
        for (index, rows) in out.chunks_mut(rows_each * out_width).enumerate() {
            let convert = &convert;
            scope.spawn(move || convert(index * rows_each, rows));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(y: u8, u: u8, v: u8) -> Vec<u8> {
        let mut data = vec![y; 16];
        data.extend([u, v].repeat(4));
        data
    }

    #[test]
    fn converts_black_white_and_red() {
        let mut out = Vec::new();
        nv12_to_pixels(&solid(16, 128, 128), 4, 4, 4, 4, 1, false, &mut out);
        assert_eq!(out, vec![Color32::BLACK; 16]);
        nv12_to_pixels(&solid(235, 128, 128), 4, 4, 4, 4, 1, false, &mut out);
        assert_eq!(out, vec![Color32::WHITE; 16]);
        nv12_to_pixels(&solid(81, 90, 240), 4, 4, 4, 4, 1, false, &mut out);
        let red = out[0];
        assert!(red.r() > 250 && red.g() < 5 && red.b() < 5, "{red:?}");
    }

    #[test]
    fn crops_to_the_visible_size() {
        let mut out = Vec::new();
        nv12_to_pixels(&solid(235, 128, 128), 4, 4, 2, 2, 1, false, &mut out);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn a_short_buffer_gives_no_picture_instead_of_a_panic() {
        let mut out = vec![Color32::RED];
        nv12_to_pixels(&[0; 8], 4, 4, 4, 4, 1, false, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn stepping_averages_the_brightness_it_covers() {
        let mut data = vec![235, 16, 235, 16, 235, 16, 235, 16, 235, 16, 235, 16, 235, 16, 235, 16];
        data.extend([128, 128].repeat(4));
        let mut out = Vec::new();
        nv12_to_pixels(&data, 4, 4, 4, 4, 2, false, &mut out);
        assert_eq!(out.len(), 4);
        // Halfway between black and white on the limited range.
        assert!(out.iter().all(|pixel| (i32::from(pixel.r()) - 127).abs() < 3), "{out:?}");
    }

    #[test]
    fn threaded_conversion_matches_a_single_thread() {
        let (width, height) = (640usize, 480usize);
        let mut data: Vec<u8> = (0..width * height).map(|index| (index % 200 + 16) as u8).collect();
        data.extend((0..width * height / 2).map(|index| (index % 100 + 80) as u8));
        let mut threaded = Vec::new();
        nv12_to_pixels(&data, width, height, width, height, 1, false, &mut threaded);
        let mut joined = Vec::new();
        let chroma = &data[width * height..];
        for row in 0..height {
            let mut single = Vec::new();
            let mut strip = data[row * width..][..width].to_vec();
            strip.extend_from_slice(&chroma[(row / 2) * width..][..width]);
            nv12_to_pixels(&strip, width, 1, width, 1, 1, false, &mut single);
            joined.extend(single);
        }
        assert_eq!(threaded, joined);
    }
}
