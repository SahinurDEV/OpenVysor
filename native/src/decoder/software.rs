//! H.264 decoding with OpenH264, for systems without the Windows decoder.

use eframe::egui::Color32;
use openh264::Error;
use openh264::decoder::Decoder as OpenH264;
use openh264::formats::YUVSource;

use super::nv12_to_pixels;

type Result<T> = std::result::Result<T, Error>;

pub struct Decoder {
    inner: OpenH264,
    /// The newest decoded picture. OpenH264 reuses its own buffers on the next
    /// decode, so the picture is kept here until it is presented.
    nv12: Vec<u8>,
    width: usize,
    height: usize,
    waiting: bool,
}

impl Decoder {
    pub fn new() -> Result<Self> {
        Ok(Self { inner: OpenH264::new()?, nv12: Vec::new(), width: 0, height: 0, waiting: false })
    }

    /// Converts the newest decoded picture into `out`, keeping one pixel in
    /// `step` each way. Returns false if no new picture is waiting.
    pub fn present(&mut self, width: usize, height: usize, step: usize, out: &mut Vec<Color32>) -> Result<bool> {
        if !std::mem::take(&mut self.waiting) {
            return Ok(false);
        }
        // OpenH264 does not report the color matrix. Phone screens are HD, where BT.709 is the norm.
        nv12_to_pixels(&self.nv12, self.width, self.height, width, height, step, true, out);
        Ok(true)
    }

    /// Decodes one access unit in Annex B form and reports whether a picture came out.
    pub fn decode(&mut self, data: &[u8], _pts: u64) -> Result<bool> {
        let Some(picture) = self.inner.decode(data)? else { return Ok(false) };
        let (width, height) = picture.dimensions();
        let (y_stride, u_stride, v_stride) = picture.strides();
        let chroma_rows = height.div_ceil(2);

        self.nv12.resize(width * height + width * chroma_rows, 0);
        let (luma, chroma) = self.nv12.split_at_mut(width * height);
        for (row, target) in luma.chunks_exact_mut(width).enumerate() {
            target.copy_from_slice(&picture.y()[row * y_stride..][..width]);
        }
        for (row, target) in chroma.chunks_exact_mut(width).enumerate() {
            let u = &picture.u()[row * u_stride..];
            let v = &picture.v()[row * v_stride..];
            for (pair, (&u, &v)) in target.chunks_exact_mut(2).zip(u.iter().zip(v)) {
                pair[0] = u;
                pair[1] = v;
            }
        }
        self.width = width;
        self.height = height;
        self.waiting = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::encoder::Encoder;
    use openh264::formats::YUVBuffer;

    #[test]
    fn decodes_a_real_h264_frame() {
        let (width, height) = (64, 48);
        let mut yuv = vec![235; width * height];
        yuv.extend(vec![128; width * height / 2]);
        let source = YUVBuffer::from_vec(yuv, width, height);
        let stream = Encoder::new().unwrap().encode(&source).unwrap().to_vec();

        let mut decoder = Decoder::new().unwrap();
        assert!(decoder.decode(&stream, 0).unwrap());
        let mut out = Vec::new();
        assert!(decoder.present(width, height, 1, &mut out).unwrap());
        assert_eq!(out.len(), width * height);
        assert!(out.iter().all(|pixel| pixel.r() > 240 && pixel.g() > 240 && pixel.b() > 240), "{:?}", out[0]);
        assert!(!decoder.present(width, height, 1, &mut out).unwrap());
    }
}
