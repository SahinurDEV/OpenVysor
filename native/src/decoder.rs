//! H.264 decoding through the decoder built into Windows.

use std::mem::ManuallyDrop;

use eframe::egui::Color32;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::Result;

// MF_SDK_VERSION << 16 | MF_API_VERSION
const MF_VERSION: u32 = 0x0002_0070;
const BT709: u32 = 1;
/// Below this many pixels, starting threads costs more than it saves.
const PARALLEL_FROM_PIXELS: usize = 200_000;
const MAX_CONVERT_THREADS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutputFormat {
    stride: usize,
    plane_height: usize,
    bt709: bool,
}

pub struct Decoder {
    transform: IMFTransform,
    format: Option<OutputFormat>,
    output: Option<IMFSample>,
    /// The newest decoded picture, not yet converted for display.
    picture: Option<IMFSample>,
}

impl Decoder {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            MFStartup(MF_VERSION, MFSTARTUP_LITE)?;
            let transform: IMFTransform =
                CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)?;
            // Without this the decoder holds frames back to reorder them.
            transform.GetAttributes()?.SetUINT32(&CODECAPI_AVLowLatencyMode, 1)?;

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            transform.SetInputType(0, &input, 0)?;

            let mut decoder = Self { transform, format: None, output: None, picture: None };
            decoder.negotiate_output()?;
            decoder.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            decoder.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(decoder)
        }
    }

    unsafe fn negotiate_output(&mut self) -> Result<()> {
        unsafe {
            let mut index = 0;
            let media_type = loop {
                let candidate = self.transform.GetOutputAvailableType(0, index)?;
                if candidate.GetGUID(&MF_MT_SUBTYPE)? == MFVideoFormat_NV12 {
                    break candidate;
                }
                index += 1;
            };
            self.transform.SetOutputType(0, &media_type, 0)?;

            let size = media_type.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let (width, height) = ((size >> 32) as usize, (size & 0xffff_ffff) as usize);
            let stride = media_type
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map_or(width, |value| (value as i32).unsigned_abs() as usize);
            let bt709 = media_type.GetUINT32(&MF_MT_YUV_MATRIX).is_ok_and(|value| value == BT709);
            self.format = Some(OutputFormat { stride, plane_height: height, bt709 });

            // Sized for the old format, so these cannot be used after a change.
            self.output = None;
            self.picture = None;
            Ok(())
        }
    }

    unsafe fn new_output_sample(&self) -> Result<IMFSample> {
        unsafe {
            let info = self.transform.GetOutputStreamInfo(0)?;
            let buffer = MFCreateAlignedMemoryBuffer(info.cbSize, info.cbAlignment.saturating_sub(1))?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            Ok(sample)
        }
    }

    /// Converts the newest decoded picture into `out`, keeping one pixel in
    /// `step` each way. Returns false if no new picture is waiting.
    pub fn present(&mut self, width: usize, height: usize, step: usize, out: &mut Vec<Color32>) -> Result<bool> {
        let (Some(sample), Some(format)) = (self.picture.take(), self.format) else { return Ok(false) };
        unsafe {
            let buffer = sample.GetBufferByIndex(0)?;
            let mut source = std::ptr::null_mut();
            let mut length = 0;
            buffer.Lock(&mut source, None, Some(&mut length))?;
            let data = std::slice::from_raw_parts(source, length as usize);
            nv12_to_pixels(data, format.stride, format.plane_height, width, height, step, format.bt709, out);
            buffer.Unlock()?;
        }
        Ok(true)
    }

    /// Decodes one access unit in Annex B form and reports whether a picture
    /// came out. Only `present` pays for converting it, and only for the newest.
    pub fn decode(&mut self, data: &[u8], pts: u64) -> Result<bool> {
        unsafe {
            let buffer = MFCreateMemoryBuffer(data.len() as u32)?;
            let mut target = std::ptr::null_mut();
            buffer.Lock(&mut target, None, None)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), target, data.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(data.len() as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            // Microseconds to the 100 ns units Media Foundation uses.
            sample.SetSampleTime(pts as i64 * 10)?;

            let mut produced = false;
            if let Err(error) = self.transform.ProcessInput(0, &sample, 0) {
                if error.code() != MF_E_NOTACCEPTING {
                    return Err(error);
                }
                produced |= self.drain()?;
                self.transform.ProcessInput(0, &sample, 0)?;
            }
            produced |= self.drain()?;
            Ok(produced)
        }
    }

    unsafe fn drain(&mut self) -> Result<bool> {
        unsafe {
            let mut produced = false;
            loop {
                // The decoder fails if handed a sample it has already written to.
                let sample = match self.output.take() {
                    Some(sample) => sample,
                    None => self.new_output_sample()?,
                };
                let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: ManuallyDrop::new(Some(sample.clone())),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0;
                let result = self.transform.ProcessOutput(0, &mut buffers, &mut status);
                ManuallyDrop::drop(&mut buffers[0].pSample);
                ManuallyDrop::drop(&mut buffers[0].pEvents);

                match result {
                    Ok(()) => {
                        self.picture = Some(sample);
                        produced = true;
                    }
                    Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                        self.output = Some(sample);
                        return Ok(produced);
                    }
                    Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.negotiate_output()?,
                    Err(error) => return Err(error),
                }
            }
        }
    }

}

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
