//! Two-tier trust in a build (ARC-SBX-001): the operator's code signature and the members' hash-pinned vouches.
//!
//! * **Signature.** The operator signs the code hash ([`crate::sandbox::code_sha256`]) with Ed25519 after the
//!   static audit passed. `sr-cli arcade sign-code` is the only signer; the server only verifies. The signed
//!   message is domain-separated ([`message`]), so a signature made for this purpose can never be replayed
//!   as an interaction signature of `sr-apps` or anything else.
//! * **Vouch.** A member in good standing says "this exact build is fine". The vouch is pinned to the hash:
//!   different code is a different build and starts at zero.
//! * **Badge.** [`headline`] is the one pure function the UI badge comes from, so the flip from a signed or
//!   vouched build to `UNVERIFIED UPDATE` after a code change is tested without a database.
//!
//! The Arcade page (ARC-ROOM-001) renders [`TrustView`]; nothing here decides what is featured. The operator's
//! `featured` flag is passed in, and only counts while the *current* code is signed: an update that is not
//! signed again loses the OPERATOR-FEATURED badge instead of inheriting it.

use data_encoding::HEXLOWER;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Domain separation of the signed message.
const SIGN_DOMAIN: &[u8] = b"sideroom/arcade-code-signature/v1\0";

/// The bytes that are signed for a code hash (lowercase hex).
pub fn message(code_sha256: &str) -> Vec<u8> {
    let mut m = SIGN_DOMAIN.to_vec();
    m.extend_from_slice(code_sha256.as_bytes());
    m
}

/// The verify key (64 hex characters) of a 32-byte signing seed.
pub fn verify_key_hex(seed: &[u8; 32]) -> String {
    HEXLOWER.encode(SigningKey::from_bytes(seed).verifying_key().as_bytes())
}

/// Sign a code hash; the result is 128 lowercase hex characters.
pub fn sign_hex(seed: &[u8; 32], code_sha256: &str) -> String {
    HEXLOWER.encode(
        &SigningKey::from_bytes(seed)
            .sign(&message(code_sha256))
            .to_bytes(),
    )
}

/// Whether `signature_hex` is `verify_key_hex`'s signature over `code_sha256`. `false` for anything malformed.
pub fn verify_hex(verify_key_hex: &str, code_sha256: &str, signature_hex: &str) -> bool {
    let (Ok(key), Ok(sig)) = (
        HEXLOWER.decode(verify_key_hex.as_bytes()),
        HEXLOWER.decode(signature_hex.as_bytes()),
    ) else {
        return false;
    };
    let (Ok(key), Ok(sig)) = (<[u8; 32]>::try_from(key), <[u8; 64]>::try_from(sig)) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    key.verify_strict(&message(code_sha256), &Signature::from_bytes(&sig))
        .is_ok()
}

/// Whether `s` looks like a SHA-256 in lowercase hex.
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The label of a build shown on cards and in the game's details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BadgeKind {
    /// Featured by the operator and the current code is signed: `[OPERATOR-FEATURED]`.
    OperatorFeatured,
    /// The current code passed the audit and carries the operator's signature: `[SWARM-SAFETY-SIGNED]`.
    SwarmSafetySigned,
    /// The code changed since a build that was signed or vouched for, and this one is neither signed nor
    /// vouched for yet.
    UnverifiedUpdate,
    /// Not signed (and nothing earlier to compare with).
    Unsigned,
}

/// The build before the current one that people had vouched for or the operator had signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PreviousBuild {
    pub code_sha256: String,
    /// The first 8 characters, for text such as "build 1a2b3c4d".
    pub short: String,
    /// Members in good standing who vouched for it.
    pub vouches: i64,
    pub signed: bool,
}

/// The operator's signature on a build, as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SignatureView {
    /// The signing key's verify key (hex), so anyone can check the signature offline.
    pub verify_key: String,
    pub signature: String,
    pub signed_at_ms: i64,
    pub audit_version: u32,
}

/// Everything the UI needs to draw a build's badge and the vouch button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TrustView {
    pub application_id: String,
    /// The hash of the code being served now; `None` when the game has no reviewed code bundle.
    pub code_sha256: Option<String>,
    pub headline: BadgeKind,
    /// Every badge that applies (`operator_featured` implies `swarm_safety_signed`).
    pub badges: Vec<BadgeKind>,
    pub signature: Option<SignatureView>,
    /// The operator has this game on the front page (it only shows as a badge while the code is signed).
    pub featured: bool,
    /// Members in good standing who vouched for the current hash.
    pub vouches: i64,
    pub vouched_by_me: bool,
    /// The viewer may vouch: a member in good standing, and the build has a hash.
    pub can_vouch: bool,
    pub previous: Option<PreviousBuild>,
}

/// The facts the headline is made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub signed: bool,
    pub featured: bool,
    pub vouches: i64,
    pub previous: Option<PreviousBuild>,
}

/// The badge of a build: featured and signed, signed, an unverified update, or unsigned.
pub fn headline(f: &Facts) -> BadgeKind {
    if f.signed {
        return if f.featured {
            BadgeKind::OperatorFeatured
        } else {
            BadgeKind::SwarmSafetySigned
        };
    }
    // Not signed. Vouches on this very hash are the community's word for it; without them, an earlier build that
    // people trusted (or the operator signed) makes this a change nobody has looked at yet.
    if f.vouches == 0 && f.previous.as_ref().is_some_and(|p| p.vouches > 0 || p.signed) {
        return BadgeKind::UnverifiedUpdate;
    }
    BadgeKind::Unsigned
}

/// All badges that apply, strongest first.
pub fn badges(f: &Facts) -> Vec<BadgeKind> {
    let mut v = Vec::new();
    if f.signed && f.featured {
        v.push(BadgeKind::OperatorFeatured);
    }
    if f.signed {
        v.push(BadgeKind::SwarmSafetySigned);
    }
    if v.is_empty() {
        v.push(headline(f));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [7; 32];
    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn a_signature_verifies_only_for_its_hash_and_key() {
        let vk = verify_key_hex(&SEED);
        let sig = sign_hex(&SEED, H1);
        assert_eq!(vk.len(), 64);
        assert_eq!(sig.len(), 128);
        assert!(verify_hex(&vk, H1, &sig));
        assert!(!verify_hex(&vk, H2, &sig), "pinned to the hash");
        let other = verify_key_hex(&[8; 32]);
        assert!(!verify_hex(&other, H1, &sig), "pinned to the key");
        assert!(!verify_hex(&vk, H1, &sig[..126]));
        assert!(!verify_hex(&vk, H1, &format!("{}zz", &sig[..126])));
        assert!(!verify_hex("", H1, &sig));
        assert!(!verify_hex(&vk, H1, ""));
    }

    #[test]
    fn signing_is_deterministic_and_domain_separated() {
        assert_eq!(sign_hex(&SEED, H1), sign_hex(&SEED, H1));
        // The same key signing the bare hash (as an interaction signature would) does not verify here.
        let bare = HEXLOWER.encode(&SigningKey::from_bytes(&SEED).sign(H1.as_bytes()).to_bytes());
        assert!(!verify_hex(&verify_key_hex(&SEED), H1, &bare));
        assert!(message(H1).starts_with(b"sideroom/arcade-code-signature/v1\0"));
    }

    #[test]
    fn sha256_shape_is_strict() {
        assert!(is_sha256_hex(H1));
        assert!(!is_sha256_hex(&H1.to_uppercase().replace('1', "A")));
        assert!(!is_sha256_hex(&H1[..63]));
        assert!(!is_sha256_hex(""));
    }

    fn prev(vouches: i64, signed: bool) -> Option<PreviousBuild> {
        Some(PreviousBuild {
            code_sha256: H1.to_owned(),
            short: H1[..8].to_owned(),
            vouches,
            signed,
        })
    }

    fn facts(signed: bool, featured: bool, vouches: i64, previous: Option<PreviousBuild>) -> Facts {
        Facts {
            signed,
            featured,
            vouches,
            previous,
        }
    }

    #[test]
    fn the_badge_flips_when_the_hash_changes() {
        // Build 1: signed and vouched for.
        assert_eq!(headline(&facts(true, false, 12, None)), BadgeKind::SwarmSafetySigned);
        // The developer ships build 2: new hash, no signature, no vouches, build 1 had 12.
        assert_eq!(
            headline(&facts(false, false, 0, prev(12, true))),
            BadgeKind::UnverifiedUpdate
        );
        // People start vouching for build 2: the warning goes, it is simply unsigned with vouches.
        assert_eq!(headline(&facts(false, false, 1, prev(12, true))), BadgeKind::Unsigned);
        // The operator signs build 2.
        assert_eq!(headline(&facts(true, false, 0, prev(12, true))), BadgeKind::SwarmSafetySigned);
    }

    #[test]
    fn nothing_to_compare_with_is_just_unsigned() {
        assert_eq!(headline(&facts(false, false, 0, None)), BadgeKind::Unsigned);
        assert_eq!(headline(&facts(false, false, 0, prev(0, false))), BadgeKind::Unsigned);
        // Only vouches on the previous build, never signed: still an unverified update.
        assert_eq!(headline(&facts(false, false, 0, prev(3, false))), BadgeKind::UnverifiedUpdate);
        // Only a signature on the previous build: also.
        assert_eq!(headline(&facts(false, false, 0, prev(0, true))), BadgeKind::UnverifiedUpdate);
    }

    #[test]
    fn featuring_only_counts_while_the_current_code_is_signed() {
        assert_eq!(headline(&facts(true, true, 0, None)), BadgeKind::OperatorFeatured);
        assert_eq!(
            badges(&facts(true, true, 0, None)),
            vec![BadgeKind::OperatorFeatured, BadgeKind::SwarmSafetySigned]
        );
        // A featured game whose code changed and is not signed again is not OPERATOR-FEATURED.
        assert_eq!(
            headline(&facts(false, true, 0, prev(5, true))),
            BadgeKind::UnverifiedUpdate
        );
        assert_eq!(badges(&facts(false, true, 0, prev(5, true))), vec![BadgeKind::UnverifiedUpdate]);
        assert_eq!(badges(&facts(true, false, 0, None)), vec![BadgeKind::SwarmSafetySigned]);
    }
}
