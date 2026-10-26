//! `safe_key`: the one check a key from user input passes before it reaches a presign or an S3 call
//! (roadmap section 2, "BYOS: keys").
//!
//! Refused: empty, more than 1024 bytes, a leading `/`, control characters, backslashes, empty segments
//! (`a//b`) and `.` or `..` segments. A trailing `/` is allowed only when `allow_folder` is set.
//! NFC normalisation is NOT done here: it needs the `unicode-normalization` crate, which this wave may not add.
//! A caller that owns that dependency normalises first.

/// Longest key in bytes.
pub const MAX_KEY_BYTES: usize = 1024;

/// Why a key was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// No key.
    #[error("the key is empty")]
    Empty,
    /// More than [`MAX_KEY_BYTES`].
    #[error("the key is longer than 1024 bytes")]
    TooLong,
    /// Starts with `/`.
    #[error("the key starts with a slash")]
    LeadingSlash,
    /// A control character or a backslash.
    #[error("the key has a control character or backslash")]
    Control,
    /// `a//b`.
    #[error("the key has an empty path segment")]
    EmptySegment,
    /// `.` or `..`.
    #[error("the key has a dot segment")]
    DotSegment,
}

/// Check a key (or, with `allow_folder`, a folder key ending in `/`).
pub fn safe_key(key: &str, allow_folder: bool) -> Result<(), KeyError> {
    if key.is_empty() {
        return Err(KeyError::Empty);
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(KeyError::TooLong);
    }
    if key.starts_with('/') {
        return Err(KeyError::LeadingSlash);
    }
    if key.chars().any(|c| c.is_control() || c == '\\') {
        return Err(KeyError::Control);
    }
    let body = if allow_folder {
        key.strip_suffix('/').unwrap_or(key)
    } else {
        key
    };
    for seg in body.split('/') {
        match seg {
            "" => return Err(KeyError::EmptySegment),
            "." | ".." => return Err(KeyError::DotSegment),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_keys_pass() {
        for k in [
            "a",
            "a/b.mp4",
            "media/Ünï/ç.cbz",
            "a b/c d",
            ".hidden",
            "a/.b",
        ] {
            assert_eq!(safe_key(k, false), Ok(()), "{k}");
        }
        assert_eq!(safe_key("media/shows/", true), Ok(()));
        assert_eq!(safe_key(&"x".repeat(MAX_KEY_BYTES), false), Ok(()));
    }

    #[test]
    fn every_refusal_is_pinned() {
        let long = "x".repeat(MAX_KEY_BYTES + 1);
        let cases: [(&str, KeyError); 11] = [
            ("", KeyError::Empty),
            (&long, KeyError::TooLong),
            ("/a", KeyError::LeadingSlash),
            ("a\0b", KeyError::Control),
            ("a\nb", KeyError::Control),
            ("a\\b", KeyError::Control),
            ("a//b", KeyError::EmptySegment),
            ("a/", KeyError::EmptySegment),
            ("../a", KeyError::DotSegment),
            ("a/./b", KeyError::DotSegment),
            ("a/..", KeyError::DotSegment),
        ];
        for (k, why) in cases {
            assert_eq!(safe_key(k, false), Err(why), "{k:?}");
        }
        assert_eq!(safe_key("a//", true), Err(KeyError::EmptySegment));
        assert_eq!(safe_key("/", true), Err(KeyError::LeadingSlash));
    }
}
