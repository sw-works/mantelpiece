//! Human-typeable short codes shown on the TV and embedded in QR links.

use rand::RngCore;

/// Crockford-style alphabet without 0/O, 1/I/L, and U, so a code read aloud
/// across the room survives.
pub const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generate a code of `len` characters from [`ALPHABET`] using `rng`.
pub fn generate(rng: &mut impl RngCore, len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rng.fill_bytes(&mut bytes);
    bytes
        .into_iter()
        .map(|b| ALPHABET[usize::from(b) % ALPHABET.len()] as char)
        .collect()
}

/// Uppercase and map look-alike characters onto the alphabet so user typing is forgiving.
/// Returns `None` if any character is outside the alphabet after normalisation.
pub fn normalize(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    for c in input.trim().chars() {
        if c == '-' || c == ' ' {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let c = match c {
            '0' => 'Q',
            '1' => 'J',
            other => other,
        };
        if !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    (!out.is_empty()).then_some(out)
}

/// Deterministic generator for tests. Not for production use.
#[cfg(test)]
pub(crate) mod test_rng {
    use rand::RngCore;

    #[derive(Debug)]
    pub struct SeqRng(pub u64);

    impl RngCore for SeqRng {
        fn next_u32(&mut self) -> u32 {
            u32::try_from(self.next_u64() & 0xFFFF_FFFF).expect("masked to 32 bits")
        }

        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for chunk in dest.chunks_mut(8) {
                let bytes = self.next_u64().to_le_bytes();
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_rng::SeqRng;
    use super::*;

    #[test]
    fn generated_code_uses_alphabet_only() {
        let mut rng = SeqRng(7);
        let code = generate(&mut rng, 8);
        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|b| ALPHABET.contains(&b)));
    }

    #[test]
    fn normalize_is_forgiving() {
        assert_eq!(normalize(" ab2c-3d ").unwrap(), "AB2C3D");
        assert_eq!(normalize("o").unwrap(), "Q");
        assert_eq!(normalize("l").unwrap(), "J");
        assert!(normalize("u").is_none());
        assert!(normalize("").is_none());
        assert!(normalize("!").is_none());
    }
}
