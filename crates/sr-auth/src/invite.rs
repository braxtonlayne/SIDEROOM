//! Platform invite codes (doc 06 §5.1): `XXXXX-XXXXX`, Crockford base32,
//! 10 symbols (50 bits). Share link: `https://<host>/join/<code>`.

use crate::{crockford, error::AuthError};
use std::fmt;

const SYMBOLS: usize = 10;
const GROUP: usize = 5;

/// A canonical (normalised, ungrouped, uppercase) invite code.
/// Store and look up this form; display with [`InviteCode::formatted`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InviteCode(String);

impl InviteCode {
    /// Generate a new random invite code.
    pub fn generate() -> Result<Self, AuthError> {
        Ok(Self(crockford::random_symbols(SYMBOLS)?))
    }

    /// Parse user input (any case, with or without dash/spaces, `O`/`I`/`L`
    /// confusables corrected). Fails if the shape is wrong.
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let canon = crockford::normalize(input).ok_or(AuthError::Malformed)?;
        if canon.len() != SYMBOLS {
            return Err(AuthError::Malformed);
        }
        Ok(Self(canon))
    }

    /// Canonical storage form, e.g. `ABCDE12345`.
    pub fn canonical(&self) -> &str {
        &self.0
    }

    /// Display form, e.g. `ABCDE-12345`.
    pub fn formatted(&self) -> String {
        crockford::group(&self.0, GROUP)
    }

    /// The share link for this code on `host`.
    pub fn share_link(&self, host: &str) -> String {
        format!("https://{host}/join/{}", self.formatted())
    }
}

impl fmt::Debug for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Invite codes are bearer secrets until redeemed.
        f.write_str("InviteCode(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_shape() {
        let c = InviteCode::generate().unwrap();
        assert_eq!(c.canonical().len(), 10);
        let f = c.formatted();
        assert_eq!(f.len(), 11);
        assert_eq!(&f[5..6], "-");
        assert_eq!(InviteCode::parse(&f).unwrap(), c);
    }

    #[test]
    fn generated_are_unique() {
        assert_ne!(
            InviteCode::generate().unwrap(),
            InviteCode::generate().unwrap()
        );
    }

    #[test]
    fn parse_is_lenient() {
        let c = InviteCode::parse("abcde-12345").unwrap();
        assert_eq!(c.canonical(), "ABCDE12345");
        assert_eq!(InviteCode::parse(" abcde 12345 ").unwrap(), c);
        assert_eq!(InviteCode::parse("ABCDE12345").unwrap(), c);
    }

    #[test]
    fn confusables_are_corrected() {
        let c = InviteCode::parse("OILOI-LOILO").unwrap();
        assert_eq!(c.canonical(), "0110110110");
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "ABCDE",
            "ABCDE-123456",
            "ABCDE-1234U",
            "ABCDE-1234!",
            "ÅBCDE-12345",
        ] {
            assert!(InviteCode::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn alphabet_only_crockford() {
        for _ in 0..200 {
            let c = InviteCode::generate().unwrap();
            assert!(c.canonical().chars().all(|ch| !"ILOU".contains(ch)));
            assert!(c.canonical().chars().all(|ch| ch.is_ascii_alphanumeric()));
        }
    }

    #[test]
    fn share_link_and_debug() {
        let c = InviteCode::parse("ABCDE12345").unwrap();
        assert_eq!(
            c.share_link("side.room"),
            "https://side.room/join/ABCDE-12345"
        );
        assert!(!format!("{c:?}").contains("ABCDE"));
    }
}
