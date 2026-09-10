//! Pure-compute ingest steps (architecture section 3, "cheap before expensive").
//!
//! Every function here takes bytes or already-decoded pixels and returns numbers. No I/O,
//! no AWS, no model calls. The ingest Lambda wraps these around S3.

// Image maths converts between pixel integers and floats constantly; the pedantic cast
// lints add noise without catching anything here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

pub mod burst;
pub mod checksum;
pub mod decode;
pub mod embed;
pub mod exif;
pub mod phash;
pub mod quality;
pub mod renditions;

pub use decode::DecodeError;

#[cfg(test)]
pub(crate) mod test_images {
    //! Deterministic synthetic images so tests need no fixtures.

    use image::{DynamicImage, GrayImage, RgbImage};

    pub fn gradient(w: u32, h: u32, seed: u32) -> DynamicImage {
        let img = RgbImage::from_fn(w, h, |x, y| {
            let r = ((x * 255) / w.max(1)) as u8;
            let g = ((y * 255) / h.max(1)) as u8;
            image::Rgb([r, g, (seed % 256) as u8])
        });
        DynamicImage::ImageRgb8(img)
    }

    /// Smooth, low-frequency "landscape": a sum of sinusoids seeded by `seed`.
    pub fn noise(w: u32, h: u32, seed: u32) -> DynamicImage {
        use std::f64::consts::TAU;
        let s = f64::from(seed);
        let img = GrayImage::from_fn(w, h, |x, y| {
            let fx = f64::from(x) / f64::from(w);
            let fy = f64::from(y) / f64::from(h);
            let v = (fx * (3.0 + s) * TAU).sin()
                + (fy * (2.0 + s * 1.7) * TAU).cos()
                + ((fx + fy) * (5.0 + s * 0.3) * TAU).sin() * 0.5
                + (fx * fy * (7.0 + s) * TAU).cos() * 0.7;
            image::Luma([(((v + 3.2) / 6.4) * 255.0).clamp(0.0, 255.0) as u8])
        });
        DynamicImage::ImageLuma8(img)
    }

    /// Flat grey field, optionally with a sharp checker region in the middle.
    pub fn flat(w: u32, h: u32, level: u8, checker: bool) -> DynamicImage {
        let img = GrayImage::from_fn(w, h, |x, y| {
            let inside = x > w / 4 && x < 3 * w / 4 && y > h / 4 && y < 3 * h / 4;
            if checker && inside && (x + y) % 2 == 0 {
                image::Luma([255 - level])
            } else {
                image::Luma([level])
            }
        });
        DynamicImage::ImageLuma8(img)
    }
}
