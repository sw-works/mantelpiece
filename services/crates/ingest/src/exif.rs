//! EXIF extraction (F-104): timestamp with its zone, GPS, orientation, camera.

use exif::{In, Reader, Tag, Value};
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::media::CaptureTime;
use std::io::Cursor;
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExifData {
    pub captured: Option<CaptureTime>,
    pub gps: Option<GeoPoint>,
    /// EXIF orientation 1 to 8; 1 when absent.
    pub orientation: u8,
    pub camera: Option<String>,
    /// Compass bearing the camera faced (`GPSImgDirection`), degrees from north.
    pub heading: Option<f32>,
}

/// Parse EXIF from a JPEG/HEIF/PNG container, or from a raw TIFF blob. Returns defaults
/// rather than an error when there is no EXIF at all: a photo without metadata is still
/// a photo.
pub fn parse(bytes: &[u8]) -> ExifData {
    let exif = Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .or_else(|_| Reader::new().read_raw(bytes.to_vec()));
    let Ok(exif) = exif else {
        return ExifData {
            orientation: 1,
            ..ExifData::default()
        };
    };

    let ascii = |tag: Tag| -> Option<String> {
        match &exif.get_field(tag, In::PRIMARY)?.value {
            Value::Ascii(v) => v.first().map(|s| {
                String::from_utf8_lossy(s)
                    .trim_end_matches('\0')
                    .trim()
                    .to_owned()
            }),
            _ => None,
        }
    };

    let orientation = match exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(Value::Short(v)) => v
            .first()
            .copied()
            .filter(|o| (1..=8).contains(o))
            .unwrap_or(1) as u8,
        _ => 1,
    };

    let offset = ascii(Tag::OffsetTimeOriginal)
        .or_else(|| ascii(Tag::OffsetTime))
        .and_then(|s| parse_offset(&s));
    let captured = ascii(Tag::DateTimeOriginal)
        .or_else(|| ascii(Tag::DateTime))
        .and_then(|s| parse_datetime(&s))
        .map(|pdt| {
            let assumed = offset.unwrap_or(UtcOffset::UTC);
            CaptureTime::new(pdt.assume_offset(assumed), offset)
        });

    let gps = read_gps(&exif);
    #[allow(clippy::cast_possible_truncation)]
    let heading = match exif
        .get_field(Tag::GPSImgDirection, In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(Value::Rational(v)) => v
            .first()
            .map(exif::Rational::to_f64)
            .filter(|d| d.is_finite() && (0.0..=360.0).contains(d))
            .map(|d| d as f32),
        _ => None,
    };

    let camera = match (ascii(Tag::Make), ascii(Tag::Model)) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => make.or(model),
    };

    ExifData {
        captured,
        gps,
        orientation,
        camera,
        heading,
    }
}

fn parse_datetime(s: &str) -> Option<PrimitiveDateTime> {
    // EXIF format: "YYYY:MM:DD HH:MM:SS"
    let (d, t) = s.split_once(' ')?;
    let mut dp = d.split(':');
    let year: i32 = dp.next()?.parse().ok()?;
    let month: u8 = dp.next()?.parse().ok()?;
    let day: u8 = dp.next()?.parse().ok()?;
    let mut tp = t.split(':');
    let hour: u8 = tp.next()?.parse().ok()?;
    let minute: u8 = tp.next()?.parse().ok()?;
    let second: u8 = tp.next()?.split('.').next()?.parse().ok()?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    let time = Time::from_hms(hour, minute, second).ok()?;
    Some(PrimitiveDateTime::new(date, time))
}

fn parse_offset(s: &str) -> Option<UtcOffset> {
    // "+09:00" or "-05:30"
    let sign = match s.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let (h, m) = s[1..].split_once(':')?;
    let h: i8 = h.parse().ok()?;
    let m: i8 = m.parse().ok()?;
    UtcOffset::from_hms(sign * h, sign * m, 0).ok()
}

fn read_gps(exif: &exif::Exif) -> Option<GeoPoint> {
    let coord = |tag: Tag, ref_tag: Tag| -> Option<f64> {
        let value = match &exif.get_field(tag, In::PRIMARY)?.value {
            Value::Rational(v) if v.len() == 3 => {
                v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0
            }
            _ => return None,
        };
        let sign = match &exif.get_field(ref_tag, In::PRIMARY)?.value {
            Value::Ascii(v) => match v.first().and_then(|s| s.first()) {
                Some(b'S' | b'W') => -1.0,
                Some(b'N' | b'E') => 1.0,
                _ => return None,
            },
            _ => return None,
        };
        Some(sign * value)
    };
    let point = GeoPoint::new(
        coord(Tag::GPSLatitude, Tag::GPSLatitudeRef)?,
        coord(Tag::GPSLongitude, Tag::GPSLongitudeRef)?,
    );
    point.is_valid().then_some(point)
}

#[cfg(test)]
pub(crate) mod tiff_builder {
    //! Minimal little-endian TIFF writer so tests can craft EXIF without fixtures.

    pub struct Entry {
        pub tag: u16,
        pub kind: u16,
        pub count: u32,
        pub data: Vec<u8>,
    }

    pub fn ascii(tag: u16, s: &str) -> Entry {
        let mut data = s.as_bytes().to_vec();
        data.push(0);
        Entry {
            tag,
            kind: 2,
            count: data.len() as u32,
            data,
        }
    }

    pub fn short(tag: u16, v: u16) -> Entry {
        Entry {
            tag,
            kind: 3,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    pub fn long(tag: u16, v: u32) -> Entry {
        Entry {
            tag,
            kind: 4,
            count: 1,
            data: v.to_le_bytes().to_vec(),
        }
    }

    pub fn rationals(tag: u16, vals: &[(u32, u32)]) -> Entry {
        let mut data = Vec::new();
        for (n, d) in vals {
            data.extend_from_slice(&n.to_le_bytes());
            data.extend_from_slice(&d.to_le_bytes());
        }
        Entry {
            tag,
            kind: 5,
            count: vals.len() as u32,
            data,
        }
    }

    /// Layout: header, IFD0, Exif IFD, GPS IFD, then all overflow data. Pointer entries
    /// are patched in by the builder.
    pub fn build(ifd0: Vec<Entry>, exif_ifd: Vec<Entry>, gps_ifd: Vec<Entry>) -> Vec<u8> {
        let ifd_size = |n: usize| 2 + 12 * n + 4;
        let mut ifd0 = ifd0;
        let ifd0_off = 8u32;
        let has_exif = !exif_ifd.is_empty();
        let has_gps = !gps_ifd.is_empty();
        let n0 = ifd0.len() + usize::from(has_exif) + usize::from(has_gps);
        let exif_off = ifd0_off + ifd_size(n0) as u32;
        let gps_off = exif_off
            + if has_exif {
                ifd_size(exif_ifd.len()) as u32
            } else {
                0
            };
        let data_start = gps_off
            + if has_gps {
                ifd_size(gps_ifd.len()) as u32
            } else {
                0
            };
        if has_exif {
            ifd0.push(long(0x8769, exif_off));
        }
        if has_gps {
            ifd0.push(long(0x8825, gps_off));
        }
        ifd0.sort_by_key(|e| e.tag);

        let mut out = vec![b'I', b'I', 0x2A, 0x00];
        out.extend_from_slice(&ifd0_off.to_le_bytes());
        let mut overflow: Vec<u8> = Vec::new();
        for ifd in [ifd0, exif_ifd, gps_ifd] {
            if ifd.is_empty() {
                continue;
            }
            out.extend_from_slice(&(ifd.len() as u16).to_le_bytes());
            for e in ifd {
                out.extend_from_slice(&e.tag.to_le_bytes());
                out.extend_from_slice(&e.kind.to_le_bytes());
                out.extend_from_slice(&e.count.to_le_bytes());
                if e.data.len() <= 4 {
                    let mut v = e.data.clone();
                    v.resize(4, 0);
                    out.extend_from_slice(&v);
                } else {
                    let off = data_start + overflow.len() as u32;
                    out.extend_from_slice(&off.to_le_bytes());
                    overflow.extend_from_slice(&e.data);
                    if overflow.len() % 2 == 1 {
                        overflow.push(0);
                    }
                }
            }
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        assert_eq!(out.len() as u32, data_start);
        out.extend_from_slice(&overflow);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::tiff_builder::*;
    use super::*;
    use time::macros::datetime;

    #[test]
    fn full_exif_with_zone_gps_orientation_camera() {
        let tiff = build(
            vec![
                short(0x0112, 6),
                ascii(0x010F, "Apple"),
                ascii(0x0110, "iPhone 15 Pro"),
            ],
            vec![
                ascii(0x9003, "2026:04:03 14:30:00"),
                ascii(0x9011, "+09:00"),
            ],
            vec![
                ascii(0x0001, "N"),
                rationals(0x0002, &[(35, 1), (0, 1), (0, 1)]),
                ascii(0x0003, "E"),
                rationals(0x0004, &[(135, 1), (30, 1), (0, 1)]),
            ],
        );
        let d = parse(&tiff);
        let c = d.captured.expect("captured");
        assert_eq!(c.utc, datetime!(2026-04-03 05:30:00 UTC));
        assert_eq!(c.offset_seconds, Some(9 * 3600));
        assert!(!c.zone_assumed);
        let g = d.gps.expect("gps");
        assert!((g.lat - 35.0).abs() < 1e-9);
        assert!((g.lon - 135.5).abs() < 1e-9);
        assert_eq!(d.orientation, 6);
        assert_eq!(d.camera.as_deref(), Some("Apple iPhone 15 Pro"));
    }

    #[test]
    fn missing_zone_assumes_utc_and_flags_it() {
        let tiff = build(vec![], vec![ascii(0x9003, "2026:04:03 14:30:00")], vec![]);
        let d = parse(&tiff);
        let c = d.captured.unwrap();
        assert!(c.zone_assumed);
        assert_eq!(c.utc, datetime!(2026-04-03 14:30:00 UTC));
        assert_eq!(d.orientation, 1);
        assert!(d.gps.is_none());
    }

    #[test]
    fn southern_western_hemisphere_and_null_island() {
        let tiff = build(
            vec![],
            vec![],
            vec![
                ascii(0x0001, "S"),
                rationals(0x0002, &[(33, 1), (52, 1), (0, 1)]),
                ascii(0x0003, "W"),
                rationals(0x0004, &[(151, 1), (12, 1), (0, 1)]),
            ],
        );
        let g = parse(&tiff).gps.unwrap();
        assert!(g.lat < 0.0 && g.lon < 0.0);

        let tiff = build(
            vec![],
            vec![],
            vec![
                ascii(0x0001, "N"),
                rationals(0x0002, &[(0, 1), (0, 1), (0, 1)]),
                ascii(0x0003, "E"),
                rationals(0x0004, &[(0, 1), (0, 1), (0, 1)]),
            ],
        );
        assert!(parse(&tiff).gps.is_none());
    }

    #[test]
    fn the_compass_heading_is_read_when_the_phone_recorded_it() {
        let gps = |dir: Option<(u32, u32)>| {
            let mut e = vec![
                ascii(0x0001, "N"),
                rationals(0x0002, &[(43, 1), (46, 1), (0, 1)]),
                ascii(0x0003, "E"),
                rationals(0x0004, &[(11, 1), (15, 1), (0, 1)]),
            ];
            if let Some(d) = dir {
                // GPSImgDirection
                e.push(rationals(0x0011, &[d]));
            }
            build(vec![], vec![], e)
        };
        let h = parse(&gps(Some((2_475, 10)))).heading.unwrap();
        assert!((h - 247.5).abs() < 1e-4);
        assert!(parse(&gps(None)).heading.is_none());
    }

    #[test]
    fn garbage_yields_defaults() {
        let d = parse(b"not an image");
        assert_eq!(
            d,
            ExifData {
                orientation: 1,
                ..ExifData::default()
            }
        );
    }

    #[test]
    fn model_already_containing_make_is_not_doubled() {
        let tiff = build(
            vec![ascii(0x010F, "Canon"), ascii(0x0110, "Canon EOS R6")],
            vec![],
            vec![],
        );
        assert_eq!(parse(&tiff).camera.as_deref(), Some("Canon EOS R6"));
    }
}
