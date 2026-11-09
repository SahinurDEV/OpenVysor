//! H.264 decoding through the decoder built into Windows.

use std::mem::ManuallyDrop;

use eframe::egui::Color32;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::Result;

use super::nv12_to_pixels;

// MF_SDK_VERSION << 16 | MF_API_VERSION
const MF_VERSION: u32 = 0x0002_0070;
const BT709: u32 = 1;

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
