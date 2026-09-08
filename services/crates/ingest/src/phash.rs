//! 64-bit DCT perceptual hash. Near-duplicates (bursts, five phones at one landmark) land
//! within a few bits of each other; unrelated photos differ by around 32.

use image::{imageops::FilterType, DynamicImage};

const SIZE: usize = 32;
const LOW: usize = 8;

pub fn phash(img: &DynamicImage) -> u64 {
    let gray = img
        .resize_exact(SIZE as u32, SIZE as u32, FilterType::Triangle)
        .to_luma8();
    let pixels: Vec<f64> = gray.pixels().map(|p| f64::from(p.0[0])).collect();
    let dct = dct_2d(&pixels);

    // Top-left LOW x LOW block, skipping the DC term at (0,0).
    let mut coeffs = Vec::with_capacity(LOW * LOW - 1);
    for y in 0..LOW {
        for x in 0..LOW {
            if x == 0 && y == 0 {
                continue;
            }
            coeffs.push(dct[y * SIZE + x]);
        }
    }
    let mut sorted = coeffs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = f64::midpoint(sorted[sorted.len() / 2 - 1], sorted[sorted.len() / 2]);

    let mut hash = 0u64;
    let mut bit = 0;
    for y in 0..LOW {
        for x in 0..LOW {
            if x == 0 && y == 0 {
                continue;
            }
            if dct[y * SIZE + x] > median {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn dct_2d(input: &[f64]) -> Vec<f64> {
    let n = SIZE;
    let cos = |i: usize, k: usize| {
        (((2 * i + 1) as f64) * (k as f64) * std::f64::consts::PI / (2.0 * n as f64)).cos()
    };
    let mut rows = vec![0.0; n * n];
    for y in 0..n {
        for k in 0..n {
            rows[y * n + k] = (0..n).map(|x| input[y * n + x] * cos(x, k)).sum();
        }
    }
    let mut out = vec![0.0; n * n];
    for x in 0..n {
        for k in 0..n {
            out[k * n + x] = (0..n).map(|y| rows[y * n + x] * cos(y, k)).sum();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_images::{gradient, noise};

    #[test]
    fn identical_images_hash_equal() {
        let a = gradient(200, 150, 0);
        assert_eq!(phash(&a), phash(&a.clone()));
    }

    #[test]
    fn resized_and_slightly_shifted_images_are_close() {
        let a = noise(256, 256, 1);
        let b = a.resize_exact(128, 128, FilterType::Triangle);
        assert!(hamming(phash(&a), phash(&b)) <= 8);
        let c = a.crop_imm(2, 2, 250, 250);
        assert!(hamming(phash(&a), phash(&c)) <= 12);
    }

    #[test]
    fn different_images_are_far() {
        let a = noise(256, 256, 1);
        let b = gradient(256, 256, 0);
        let c = noise(256, 256, 6);
        assert!(hamming(phash(&a), phash(&b)) >= 20);
        assert!(hamming(phash(&a), phash(&c)) >= 16);
    }
}
