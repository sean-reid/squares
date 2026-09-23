use crate::color::{oklab_to_srgb, srgb_to_oklab};
use crate::geometry::Square;

const Q: f64 = 65535.0;
const CH: usize = 3;

/// The photo as summed-area tables over quantized Oklab, so the mean color and
/// the color variance of any pixel rectangle cost a handful of lookups.
pub struct Target {
    pub width: usize,
    pub height: usize,
    stride: usize,
    sum: Vec<i64>,
    sq: Vec<i64>,
}

/// Maps tiling coordinates (width by 1) onto a centered crop of the photo.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub scale: f64,
    pub ox: f64,
    pub oy: f64,
    /// Fraction of the cropped dimension that is cut away.
    pub crop: f64,
}

impl Target {
    pub fn from_rgba(rgba: &[u8], width: usize, height: usize) -> Target {
        assert_eq!(rgba.len(), width * height * 4);
        let stride = width + 1;
        let n = stride * (height + 1) * CH;
        let mut sum = vec![0i64; n];
        let mut sq = vec![0i64; n];
        for y in 0..height {
            let mut row_sum = [0i64; CH];
            let mut row_sq = [0i64; CH];
            for x in 0..width {
                let p = (y * width + x) * 4;
                let lab = srgb_to_oklab(rgba[p], rgba[p + 1], rgba[p + 2]);
                let q = [
                    (lab[0].clamp(0.0, 1.0) * Q).round() as i64,
                    ((lab[1] + 0.5).clamp(0.0, 1.0) * Q).round() as i64,
                    ((lab[2] + 0.5).clamp(0.0, 1.0) * Q).round() as i64,
                ];
                let above = ((y * stride) + x + 1) * CH;
                let here = (((y + 1) * stride) + x + 1) * CH;
                for c in 0..CH {
                    row_sum[c] += q[c];
                    row_sq[c] += q[c] * q[c];
                    sum[here + c] = sum[above + c] + row_sum[c];
                    sq[here + c] = sq[above + c] + row_sq[c];
                }
            }
        }
        Target {
            width,
            height,
            stride,
            sum,
            sq,
        }
    }

    pub fn frame(&self, tiling_width: f64) -> Frame {
        let w = self.width as f64;
        let h = self.height as f64;
        let r0 = w / h;
        if tiling_width <= r0 {
            let cw = h * tiling_width;
            Frame {
                scale: h,
                ox: (w - cw) / 2.0,
                oy: 0.0,
                crop: 1.0 - cw / w,
            }
        } else {
            let ch = w / tiling_width;
            Frame {
                scale: ch,
                ox: 0.0,
                oy: (h - ch) / 2.0,
                crop: 1.0 - ch / h,
            }
        }
    }

    fn pixel_rect(&self, s: &Square, f: &Frame) -> (usize, usize, usize, usize) {
        let x0 = ((f.ox + s.x * f.scale).round().max(0.0) as usize).min(self.width);
        let y0 = ((f.oy + s.y * f.scale).round().max(0.0) as usize).min(self.height);
        let x1 = ((f.ox + (s.x + s.side) * f.scale).round().max(0.0) as usize).min(self.width);
        let y1 = ((f.oy + (s.y + s.side) * f.scale).round().max(0.0) as usize).min(self.height);
        (
            x0,
            y0,
            x1.max(x0 + 1).min(self.width),
            y1.max(y0 + 1).min(self.height),
        )
    }

    fn stats(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> (f64, [f64; CH], [f64; CH]) {
        let area = ((x1 - x0) * (y1 - y0)) as f64;
        let mut s = [0.0; CH];
        let mut q = [0.0; CH];
        let a = (y0 * self.stride + x0) * CH;
        let b = (y0 * self.stride + x1) * CH;
        let c = (y1 * self.stride + x0) * CH;
        let d = (y1 * self.stride + x1) * CH;
        for k in 0..CH {
            s[k] = (self.sum[d + k] - self.sum[b + k] - self.sum[c + k] + self.sum[a + k]) as f64;
            q[k] = (self.sq[d + k] - self.sq[b + k] - self.sq[c + k] + self.sq[a + k]) as f64;
        }
        (area, s, q)
    }

    /// Sum over the square's pixels of squared Oklab distance to their mean.
    pub fn error(&self, s: &Square, f: &Frame) -> f64 {
        let (x0, y0, x1, y1) = self.pixel_rect(s, f);
        if x1 <= x0 || y1 <= y0 {
            return 0.0;
        }
        let (area, sum, sq) = self.stats(x0, y0, x1, y1);
        let mut e = 0.0;
        for k in 0..CH {
            e += sq[k] - sum[k] * sum[k] / area;
        }
        e / (Q * Q)
    }

    pub fn mean_rgb(&self, s: &Square, f: &Frame) -> [u8; 3] {
        let (x0, y0, x1, y1) = self.pixel_rect(s, f);
        if x1 <= x0 || y1 <= y0 {
            return [0, 0, 0];
        }
        let (area, sum, _) = self.stats(x0, y0, x1, y1);
        oklab_to_srgb([
            sum[0] / area / Q,
            sum[1] / area / Q - 0.5,
            sum[2] / area / Q - 0.5,
        ])
    }

    pub fn pixels(&self) -> f64 {
        (self.width * self.height) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_image_has_zero_error() {
        let rgba: Vec<u8> = (0..16 * 8).flat_map(|_| [10u8, 200, 30, 255]).collect();
        let t = Target::from_rgba(&rgba, 16, 8);
        let f = t.frame(2.0);
        let s = Square {
            id: 0,
            x: 0.0,
            y: 0.0,
            side: 1.0,
        };
        assert!(t.error(&s, &f).abs() < 1e-9);
        assert_eq!(t.mean_rgb(&s, &f), [10, 200, 30]);
        assert!(f.crop.abs() < 1e-12);
    }

    #[test]
    fn frame_crops_the_longer_dimension() {
        let rgba = vec![0u8; 20 * 10 * 4];
        let t = Target::from_rgba(&rgba, 20, 10);
        let f = t.frame(1.0);
        assert!((f.crop - 0.5).abs() < 1e-12);
        assert!((f.ox - 5.0).abs() < 1e-12);
        let f = t.frame(4.0);
        assert!((f.crop - 0.5).abs() < 1e-12);
        assert!((f.oy - 2.5).abs() < 1e-12);
    }
}
