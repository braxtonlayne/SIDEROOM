//! Crockford base32 helpers shared by invite and recovery codes.
//!
//! Alphabet excludes I, L, O, U. Decoding is lenient: case-insensitive,
//! `I`/`L` read as `1`, `O` as `0`, dashes and whitespace ignored.

use crate::error::AuthError;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generate `len` random Crockford symbols (5 bits of entropy each).
pub fn random_symbols(len: usize) -> Result<String, AuthError> {
    let mut out = String::with_capacity(len);
    let mut chunk = [0u8; 64];
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(chunk.len());
        getrandom::fill(&mut chunk[..n]).map_err(|_| AuthError::Random)?;
        for b in &chunk[..n] {
            // 32 symbols: masking to 5 bits is exactly uniform.
            out.push(char::from(ALPHABET[usize::from(b & 31)]));
        }
        remaining -= n;
    }
    Ok(out)
}

/// Normalise user input to canonical uppercase symbols without separators.
/// Returns `None` if any character is not part of the (lenient) alphabet.
pub fn normalize(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        };
        if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

/// Insert a dash every `size` symbols.
pub fn group(canonical: &str, size: usize) -> String {
    let mut out = String::with_capacity(canonical.len() + canonical.len() / size);
    for (i, c) in canonical.chars().enumerate() {
        if i > 0 && i % size == 0 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_len_and_alphabet() {
        for len in [0, 1, 10, 64, 65, 200] {
            let s = random_symbols(len).unwrap();
            assert_eq!(s.len(), len);
            assert!(s.bytes().all(|b| ALPHABET.contains(&b)));
        }
    }

    #[test]
    fn normalize_and_group() {
        assert_eq!(normalize("ab-cd ef").unwrap(), "ABCDEF");
        assert_eq!(normalize("oOiIlL").unwrap(), "001111");
        assert!(normalize("u").is_none());
        assert_eq!(group("ABCDEFGH", 3), "ABC-DEF-GH");
    }
}
