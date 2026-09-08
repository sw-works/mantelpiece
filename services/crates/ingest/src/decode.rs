//! Bytes to pixels. HEIC needs libheif, which is a separate build step
//! (open question 5), so it is gated behind a feature that is not on yet.

use image::DynamicImage;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("unsupported format: {0}")]
    Unsupported(&'static str),
    #[error("decode failed: {0}")]
    Image(#[from] image::ImageError),
}

/// Sniff the container from magic bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Jpeg,
    Png,
    Heif,
    Mp4,
    Unknown,
}

pub fn sniff(bytes: &[u8]) -> Container {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Container::Jpeg;
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Container::Png;
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return match &bytes[8..12] {
            b"heic" | b"heix" | b"hevc" | b"mif1" | b"msf1" | b"heif" => Container::Heif,
            _ => Container::Mp4,
        };
    }
    Container::Unknown
}

pub fn decode(bytes: &[u8]) -> Result<DynamicImage, DecodeError> {
    match sniff(bytes) {
        Container::Heif => Err(DecodeError::Unsupported("heic")),
        Container::Mp4 => Err(DecodeError::Unsupported("video")),
        _ => Ok(image::load_from_memory(bytes)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_containers() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE1]), Container::Jpeg);
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), Container::Png);
        let mut heic = vec![0, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic");
        heic.extend_from_slice(&[0; 8]);
        assert_eq!(sniff(&heic), Container::Heif);
        let mut mp4 = vec![0, 0, 0, 0x18];
        mp4.extend_from_slice(b"ftypisom");
        mp4.extend_from_slice(&[0; 8]);
        assert_eq!(sniff(&mp4), Container::Mp4);
        assert_eq!(sniff(b"hello"), Container::Unknown);
        assert!(matches!(
            decode(&heic),
            Err(DecodeError::Unsupported("heic"))
        ));
    }
}
