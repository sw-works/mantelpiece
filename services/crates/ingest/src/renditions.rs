//! Thumbnail and TV-resolution JPEG renditions (F-105), orientation applied so the TV
//! never has to read EXIF.

use image::codecs::jpeg::JpegEncoder;
use image::{imageops::FilterType, DynamicImage};

pub const THUMB_SIDE: u32 = 400;
pub const TV_SIDE: u32 = 1920;
pub const JPEG_QUALITY: u8 = 85;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rendition {
    Thumb,
    Tv,
}

impl Rendition {
    pub fn longest_side(self) -> u32 {
        match self {
            Self::Thumb => THUMB_SIDE,
            Self::Tv => TV_SIDE,
        }
    }

    pub fn key(self, household: &str, media: &str) -> String {
        match self {
            Self::Thumb => format!("renditions/{household}/{media}/thumb.jpg"),
            Self::Tv => format!("renditions/{household}/{media}/tv.jpg"),
        }
    }
}

/// Apply EXIF orientation 1 to 8 so pixels are upright.
pub fn upright(img: DynamicImage, orientation: u8) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Encode a rendition. Never upscales.
pub fn encode(img: &DynamicImage, rendition: Rendition) -> Result<Vec<u8>, image::ImageError> {
    let side = rendition.longest_side();
    let resized = if img.width().max(img.height()) > side {
        img.resize(side, side, FilterType::Triangle)
    } else {
        img.clone()
    };
    let rgb = resized.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_images::gradient;

    #[test]
    fn thumbnail_fits_and_keeps_aspect() {
        let img = gradient(4000, 3000, 0);
        let bytes = encode(&img, Rendition::Thumb).unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (400, 300));
        assert_eq!(crate::decode::sniff(&bytes), crate::decode::Container::Jpeg);
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let img = gradient(300, 200, 0);
        let back = image::load_from_memory(&encode(&img, Rendition::Tv).unwrap()).unwrap();
        assert_eq!((back.width(), back.height()), (300, 200));
    }

    #[test]
    fn orientation_six_rotates_to_portrait() {
        let img = gradient(400, 200, 0);
        let up = upright(img, 6);
        assert_eq!((up.width(), up.height()), (200, 400));
        assert_eq!(Rendition::Tv.key("h", "m"), "renditions/h/m/tv.jpg");
    }
}
