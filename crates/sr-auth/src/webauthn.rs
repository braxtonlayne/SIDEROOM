//! Pure-Rust WebAuthn relying party (F-14 / D-161 spike, ENT-AUTH-010 step 1).
//!
//! Scope: registration and assertion verification with `none` attestation
//! only (SIDE ROOM never verifies attestation statements), algorithms ES256
//! (-7), EdDSA (-8) and RS256 (-257), built directly on RustCrypto (`p256`,
//! `ed25519-dalek`, `crypto-bigint`) and `ciborium`. No `openssl`, no native
//! code. RS256 verification is a small hand-written RSASSA-PKCS1-v1_5 public
//! operation on `crypto-bigint` rather than the `rsa` crate, because every
//! `rsa` release carries the unpatched RUSTSEC-2023-0071 (Marvin) advisory and
//! `cargo deny` fails on it; the advisory concerns private-key operations,
//! which a relying party never performs. See docs/dev/passkey-spike-report.md.
//!
//! The module is database-free and clock-free like the rest of the crate:
//! the caller supplies Unix seconds and persists [`StoredCredential`]s and
//! pending ceremonies ([`ChallengeBook`] is an in-memory single-use store
//! suitable for one process; a multi-node deployment would keep the same
//! shape in Postgres).
//!
//! Spec references are to W3C WebAuthn Level 2: section 7.1 (registration
//! steps) and 7.2 (authentication steps).

use crate::error::AuthError;
use ciborium::value::Value;
use crypto_bigint::{
    BoxedUint, Integer as _, Odd,
    modular::{BoxedMontyForm, BoxedMontyParams},
};
use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Cursor;
use subtle::ConstantTimeEq;

/// COSE algorithm identifiers SIDE ROOM accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoseAlg {
    /// ECDSA with P-256 and SHA-256 (COSE -7).
    Es256,
    /// EdDSA over Ed25519 (COSE -8).
    EdDsa,
    /// RSASSA-PKCS1-v1_5 with SHA-256 (COSE -257).
    Rs256,
}

impl CoseAlg {
    /// The COSE number.
    pub fn id(self) -> i64 {
        match self {
            Self::Es256 => -7,
            Self::EdDsa => -8,
            Self::Rs256 => -257,
        }
    }

    /// Parse a COSE number; anything outside the allow-list is `None`.
    pub fn from_id(id: i64) -> Option<Self> {
        match id {
            -7 => Some(Self::Es256),
            -8 => Some(Self::EdDsa),
            -257 => Some(Self::Rs256),
            _ => None,
        }
    }

    /// The allow-list in preference order, as advertised to authenticators.
    pub const ALL: [Self; 3] = [Self::Es256, Self::EdDsa, Self::Rs256];
}

/// Everything that can go wrong while verifying a ceremony. Messages never
/// include credential material.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum WebauthnError {
    /// Operating system RNG failure.
    #[error("secure random source unavailable")]
    Random,
    /// `clientDataJSON` is not valid UTF-8 JSON of the expected shape.
    #[error("malformed clientDataJSON")]
    MalformedClientData,
    /// `clientDataJSON.type` is not the expected ceremony.
    #[error("wrong ceremony type")]
    WrongCeremonyType,
    /// The challenge does not match the one issued.
    #[error("challenge mismatch")]
    ChallengeMismatch,
    /// The ceremony expired before it finished.
    #[error("ceremony expired")]
    Expired,
    /// The ceremony id is unknown or was already consumed (replay).
    #[error("unknown or already used ceremony")]
    UnknownCeremony,
    /// The origin is not on the relying party's allow-list.
    #[error("origin not allowed")]
    OriginMismatch,
    /// The client reported a cross-origin (iframe) ceremony.
    #[error("cross-origin ceremonies are refused")]
    CrossOrigin,
    /// `rpIdHash` is not SHA-256 of the relying party id.
    #[error("rpIdHash mismatch")]
    RpIdHashMismatch,
    /// The user-presence flag is not set.
    #[error("user presence flag missing")]
    UserPresenceMissing,
    /// User verification was required but the UV flag is not set.
    #[error("user verification flag missing")]
    UserVerificationMissing,
    /// Inconsistent authenticator flags.
    #[error("invalid authenticator flags")]
    InvalidFlags,
    /// `authenticatorData` is truncated, oversized or has trailing bytes.
    #[error("malformed authenticator data")]
    MalformedAuthData,
    /// CBOR could not be decoded, or is not the expected structure.
    #[error("malformed CBOR")]
    MalformedCbor,
    /// Attestation format other than `none`.
    #[error("only `none` attestation is accepted")]
    UnsupportedAttestation,
    /// COSE algorithm outside ES256 / EdDSA / RS256, or mismatching key type.
    #[error("unsupported or inconsistent algorithm")]
    UnsupportedAlgorithm,
    /// The COSE public key is invalid (bad point, short RSA modulus, ...).
    #[error("invalid public key")]
    InvalidPublicKey,
    /// The credential id in the response differs from the attested one.
    #[error("credential id mismatch")]
    CredentialIdMismatch,
    /// The assertion signature does not verify.
    #[error("bad signature")]
    BadSignature,
    /// The signature counter did not increase (possible cloned authenticator).
    #[error("signature counter regression")]
    CounterRegression,
    /// The backup-eligible flag changed since registration.
    #[error("backup eligibility changed")]
    BackupEligibilityChanged,
    /// The assertion's user handle does not belong to the credential owner.
    #[error("user handle mismatch")]
    UserHandleMismatch,
}

impl From<AuthError> for WebauthnError {
    fn from(_: AuthError) -> Self {
        Self::Random
    }
}

/// User verification policy for a ceremony.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserVerification {
    /// UV flag must be set.
    Required,
    /// Ask for it, accept without.
    Preferred,
    /// Do not ask.
    Discouraged,
}

impl UserVerification {
    fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Preferred => "preferred",
            Self::Discouraged => "discouraged",
        }
    }
}

/// The relying party: its id (an effective domain) and exact origins.
#[derive(Debug, Clone)]
pub struct RelyingParty {
    /// RP id, e.g. `chat.example.com`.
    pub id: String,
    /// Display name sent to authenticators.
    pub name: String,
    /// Exact allowed origins, e.g. `https://chat.example.com`.
    pub origins: Vec<String>,
}

impl RelyingParty {
    fn rp_id_hash(&self) -> [u8; 32] {
        Sha256::digest(self.id.as_bytes()).into()
    }
}

/// Ceremony lifetime, seconds.
pub const CEREMONY_TTL_SECS: i64 = 300;

/// A pending ceremony: the challenge we issued and its policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCeremony {
    challenge: [u8; 32],
    expires_at: i64,
    user_verification: UserVerification,
}

impl PendingCeremony {
    /// The challenge to hand to the client (base64url, no padding).
    pub fn challenge_b64(&self) -> String {
        BASE64URL_NOPAD.encode(&self.challenge)
    }
}

/// In-memory single-use ceremony store. [`ChallengeBook::take`] removes the
/// entry, so a replayed finish call finds nothing.
#[derive(Debug, Default)]
pub struct ChallengeBook {
    pending: HashMap<String, PendingCeremony>,
}

impl ChallengeBook {
    /// Empty book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a ceremony under a caller-chosen id (for example the session
    /// or a random ceremony token).
    pub fn insert(&mut self, id: impl Into<String>, ceremony: PendingCeremony) {
        self.pending.insert(id.into(), ceremony);
    }

    /// Consume a ceremony. Returns [`WebauthnError::UnknownCeremony`] when it
    /// was never issued or was already used.
    pub fn take(&mut self, id: &str) -> Result<PendingCeremony, WebauthnError> {
        self.pending
            .remove(id)
            .ok_or(WebauthnError::UnknownCeremony)
    }

    /// Drop everything that expired before `now`.
    pub fn purge(&mut self, now: i64) {
        self.pending.retain(|_, c| c.expires_at >= now);
    }
}

/// A verified credential, ready to persist. `cose_key` is the COSE_Key CBOR
/// exactly as the authenticator produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCredential {
    /// Credential id.
    pub credential_id: Vec<u8>,
    /// COSE_Key bytes.
    pub cose_key: Vec<u8>,
    /// Algorithm of the key.
    pub alg: CoseAlg,
    /// Last seen signature counter (0 for authenticators that do not count).
    pub sign_count: u32,
    /// Backup-eligible flag at registration (multi-device / synced passkey).
    pub backup_eligible: bool,
    /// Current backup state.
    pub backup_state: bool,
    /// Authenticator AAGUID (not used for any decision).
    pub aaguid: [u8; 16],
}

// ---------------------------------------------------------------------------
// Options (begin ceremonies)
// ---------------------------------------------------------------------------

fn new_ceremony(
    now: i64,
    user_verification: UserVerification,
) -> Result<PendingCeremony, WebauthnError> {
    Ok(PendingCeremony {
        challenge: crate::random::bytes::<32>()?,
        expires_at: now.saturating_add(CEREMONY_TTL_SECS),
        user_verification,
    })
}

fn descriptor(id: &[u8]) -> serde_json::Value {
    serde_json::json!({ "type": "public-key", "id": BASE64URL_NOPAD.encode(id) })
}

/// Begin registration: returns the pending ceremony and the JSON for
/// `navigator.credentials.create({ publicKey })` (binary fields base64url).
pub fn begin_registration(
    rp: &RelyingParty,
    user_handle: &[u8],
    user_name: &str,
    display_name: &str,
    exclude_credential_ids: &[Vec<u8>],
    user_verification: UserVerification,
    now: i64,
) -> Result<(PendingCeremony, serde_json::Value), WebauthnError> {
    let ceremony = new_ceremony(now, user_verification)?;
    let params: Vec<_> = CoseAlg::ALL
        .iter()
        .map(|a| serde_json::json!({ "type": "public-key", "alg": a.id() }))
        .collect();
    let exclude: Vec<_> = exclude_credential_ids
        .iter()
        .map(|i| descriptor(i))
        .collect();
    let options = serde_json::json!({
        "rp": { "id": rp.id, "name": rp.name },
        "user": {
            "id": BASE64URL_NOPAD.encode(user_handle),
            "name": user_name,
            "displayName": display_name,
        },
        "challenge": ceremony.challenge_b64(),
        "pubKeyCredParams": params,
        "timeout": CEREMONY_TTL_SECS * 1000,
        "excludeCredentials": exclude,
        "authenticatorSelection": {
            "residentKey": "required",
            "requireResidentKey": true,
            "userVerification": user_verification.as_str(),
        },
        "attestation": "none",
    });
    Ok((ceremony, options))
}

/// Begin an assertion. An empty `allow_credential_ids` is the discoverable
/// (username-less) flow.
pub fn begin_authentication(
    rp: &RelyingParty,
    allow_credential_ids: &[Vec<u8>],
    user_verification: UserVerification,
    now: i64,
) -> Result<(PendingCeremony, serde_json::Value), WebauthnError> {
    let ceremony = new_ceremony(now, user_verification)?;
    let allow: Vec<_> = allow_credential_ids.iter().map(|i| descriptor(i)).collect();
    let options = serde_json::json!({
        "rpId": rp.id,
        "challenge": ceremony.challenge_b64(),
        "timeout": CEREMONY_TTL_SECS * 1000,
        "allowCredentials": allow,
        "userVerification": user_verification.as_str(),
    });
    Ok((ceremony, options))
}

// ---------------------------------------------------------------------------
// Shared checks
// ---------------------------------------------------------------------------

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_BE: u8 = 0x08;
const FLAG_BS: u8 = 0x10;
const FLAG_AT: u8 = 0x40;
const FLAG_ED: u8 = 0x80;

fn check_client_data(
    rp: &RelyingParty,
    ceremony: &PendingCeremony,
    expected_type: &str,
    client_data_json: &[u8],
    now: i64,
) -> Result<(), WebauthnError> {
    if now > ceremony.expires_at {
        return Err(WebauthnError::Expired);
    }
    let text =
        std::str::from_utf8(client_data_json).map_err(|_| WebauthnError::MalformedClientData)?;
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|_| WebauthnError::MalformedClientData)?;
    let get_str = |k: &str| v.get(k).and_then(|x| x.as_str());
    if get_str("type").ok_or(WebauthnError::MalformedClientData)? != expected_type {
        return Err(WebauthnError::WrongCeremonyType);
    }
    let challenge = BASE64URL_NOPAD
        .decode(
            get_str("challenge")
                .ok_or(WebauthnError::MalformedClientData)?
                .as_bytes(),
        )
        .map_err(|_| WebauthnError::MalformedClientData)?;
    if !bool::from(challenge.as_slice().ct_eq(&ceremony.challenge)) {
        return Err(WebauthnError::ChallengeMismatch);
    }
    let origin = get_str("origin").ok_or(WebauthnError::MalformedClientData)?;
    if !rp.origins.iter().any(|o| o == origin) {
        return Err(WebauthnError::OriginMismatch);
    }
    if v.get("crossOrigin").and_then(|x| x.as_bool()) == Some(true) {
        return Err(WebauthnError::CrossOrigin);
    }
    Ok(())
}

struct ParsedAuthData<'a> {
    flags: u8,
    sign_count: u32,
    /// Bytes after the 37-byte header (attested credential data and/or extensions).
    rest: &'a [u8],
}

fn parse_auth_data<'a>(
    rp: &RelyingParty,
    ceremony: &PendingCeremony,
    data: &'a [u8],
) -> Result<ParsedAuthData<'a>, WebauthnError> {
    if data.len() < 37 {
        return Err(WebauthnError::MalformedAuthData);
    }
    let expected = rp.rp_id_hash();
    if !bool::from(data[..32].ct_eq(&expected)) {
        return Err(WebauthnError::RpIdHashMismatch);
    }
    let flags = data[32];
    if flags & FLAG_UP == 0 {
        return Err(WebauthnError::UserPresenceMissing);
    }
    if ceremony.user_verification == UserVerification::Required && flags & FLAG_UV == 0 {
        return Err(WebauthnError::UserVerificationMissing);
    }
    // Backup state without eligibility is impossible (spec 6.1.3).
    if flags & FLAG_BS != 0 && flags & FLAG_BE == 0 {
        return Err(WebauthnError::InvalidFlags);
    }
    let sign_count = u32::from_be_bytes([data[33], data[34], data[35], data[36]]);
    Ok(ParsedAuthData {
        flags,
        sign_count,
        rest: &data[37..],
    })
}

/// Decode exactly one CBOR item from the front of `bytes`; returns the value
/// and the number of bytes it occupied.
fn cbor_prefix(bytes: &[u8]) -> Result<(Value, usize), WebauthnError> {
    let mut cur = Cursor::new(bytes);
    let v: Value = ciborium::from_reader(&mut cur).map_err(|_| WebauthnError::MalformedCbor)?;
    let used = usize::try_from(cur.position()).map_err(|_| WebauthnError::MalformedCbor)?;
    Ok((v, used))
}

/// Decode a CBOR document that must occupy all of `bytes`.
fn cbor_exact(bytes: &[u8]) -> Result<Value, WebauthnError> {
    let (v, used) = cbor_prefix(bytes)?;
    if used != bytes.len() {
        return Err(WebauthnError::MalformedCbor);
    }
    Ok(v)
}

fn map_entries(v: &Value) -> Result<&Vec<(Value, Value)>, WebauthnError> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err(WebauthnError::MalformedCbor),
    }
}

/// Look up an integer key; duplicate keys are rejected.
fn int_key(map: &[(Value, Value)], key: i64) -> Result<Option<&Value>, WebauthnError> {
    let mut found = None;
    for (k, v) in map {
        if let Value::Integer(i) = k
            && i128::from(*i) == i128::from(key)
        {
            if found.is_some() {
                return Err(WebauthnError::MalformedCbor);
            }
            found = Some(v);
        }
    }
    Ok(found)
}

fn as_i64(v: &Value) -> Result<i64, WebauthnError> {
    match v {
        Value::Integer(i) => {
            i64::try_from(i128::from(*i)).map_err(|_| WebauthnError::MalformedCbor)
        }
        _ => Err(WebauthnError::MalformedCbor),
    }
}

fn as_bytes(v: &Value) -> Result<&[u8], WebauthnError> {
    match v {
        Value::Bytes(b) => Ok(b),
        _ => Err(WebauthnError::MalformedCbor),
    }
}

// ---------------------------------------------------------------------------
// RS256 (RSASSA-PKCS1-v1_5 / SHA-256), public operation only
// ---------------------------------------------------------------------------

/// Minimum RSA modulus size accepted, bits.
const RSA_MIN_BITS: u32 = 2048;
/// Maximum RSA modulus size accepted, bits (bounds verification cost).
const RSA_MAX_BITS: u32 = 4096;
/// DER `DigestInfo` prefix for SHA-256 (RFC 8017 section 9.2 note 1).
const SHA256_DIGEST_INFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

/// An RSA public key prepared for repeated verification.
struct RsaKey {
    params: BoxedMontyParams,
    modulus: BoxedUint,
    exponent: BoxedUint,
    /// Modulus length in bytes (the signature and encoded message length).
    k: usize,
}

impl RsaKey {
    fn new(n: &[u8], e: &[u8]) -> Result<Self, WebauthnError> {
        let bad = || WebauthnError::InvalidPublicKey;
        let modulus = BoxedUint::from_be_slice_vartime(n);
        let bits = modulus.bits();
        if !(RSA_MIN_BITS..=RSA_MAX_BITS).contains(&bits) {
            return Err(bad());
        }
        let exponent = BoxedUint::from_be_slice_vartime(e);
        // e: odd, at least 3 and at most 32 bits (65537 in practice).
        if exponent.bits() > 32 || exponent.bits() < 2 || !bool::from(exponent.is_odd()) {
            return Err(bad());
        }
        let odd = Option::<Odd<BoxedUint>>::from(Odd::new(modulus.clone())).ok_or_else(bad)?;
        let params = BoxedMontyParams::new(odd);
        Ok(Self {
            params,
            modulus,
            exponent,
            k: usize::try_from(bits.div_ceil(8)).map_err(|_| bad())?,
        })
    }

    /// RSASSA-PKCS1-v1_5-VERIFY: recompute the expected encoded message and
    /// compare it whole (no parsing of the recovered block, which is what
    /// makes Bleichenbacher-style forgeries impossible).
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), WebauthnError> {
        let bad = WebauthnError::BadSignature;
        if signature.len() != self.k {
            return Err(bad);
        }
        let prec = self.modulus.bits_precision();
        let s =
            BoxedUint::from_be_slice(signature, prec).map_err(|_| WebauthnError::BadSignature)?;
        if s >= self.modulus {
            return Err(bad);
        }
        let m = BoxedMontyForm::new(s, &self.params)
            .pow(&self.exponent)
            .retrieve()
            .to_be_bytes();
        // `m` is `prec / 8` bytes, zero-extended; the block is its last k bytes.
        let em = m.get(m.len().saturating_sub(self.k)..).ok_or(bad.clone())?;
        let expected = emsa_pkcs1_v15_sha256(message, self.k).ok_or(bad.clone())?;
        if em.len() == expected.len() && bool::from(em.ct_eq(&expected)) {
            Ok(())
        } else {
            Err(bad)
        }
    }
}

/// EMSA-PKCS1-v1_5 encoding for SHA-256 (RFC 8017 section 9.2).
fn emsa_pkcs1_v15_sha256(message: &[u8], k: usize) -> Option<Vec<u8>> {
    let hash = Sha256::digest(message);
    let t_len = SHA256_DIGEST_INFO.len() + hash.len();
    let ps_len = k.checked_sub(t_len + 3)?;
    if ps_len < 8 {
        return None;
    }
    let mut em = Vec::with_capacity(k);
    em.extend_from_slice(&[0x00, 0x01]);
    em.resize(2 + ps_len, 0xff);
    em.push(0x00);
    em.extend_from_slice(&SHA256_DIGEST_INFO);
    em.extend_from_slice(&hash);
    Some(em)
}

// ---------------------------------------------------------------------------
// Public key handling
// ---------------------------------------------------------------------------

/// A parsed, validated credential public key.
enum PublicKey {
    Es256(p256::ecdsa::VerifyingKey),
    EdDsa(ed25519_dalek::VerifyingKey),
    Rs256(RsaKey),
}

/// Parse and validate a COSE_Key. The algorithm must be on the allow-list and
/// consistent with `kty` and `crv`; the key material must be valid.
fn parse_cose_key(bytes: &[u8]) -> Result<(CoseAlg, PublicKey), WebauthnError> {
    let value = cbor_exact(bytes)?;
    let map = map_entries(&value)?;
    let kty = as_i64(int_key(map, 1)?.ok_or(WebauthnError::MalformedCbor)?)?;
    let alg_id = as_i64(int_key(map, 3)?.ok_or(WebauthnError::UnsupportedAlgorithm)?)?;
    let alg = CoseAlg::from_id(alg_id).ok_or(WebauthnError::UnsupportedAlgorithm)?;
    match (alg, kty) {
        (CoseAlg::Es256, 2) => {
            let crv = as_i64(int_key(map, -1)?.ok_or(WebauthnError::MalformedCbor)?)?;
            if crv != 1 {
                return Err(WebauthnError::UnsupportedAlgorithm);
            }
            let x = as_bytes(int_key(map, -2)?.ok_or(WebauthnError::MalformedCbor)?)?;
            let y = as_bytes(int_key(map, -3)?.ok_or(WebauthnError::MalformedCbor)?)?;
            if x.len() != 32 || y.len() != 32 {
                return Err(WebauthnError::InvalidPublicKey);
            }
            let mut sec1 = Vec::with_capacity(65);
            sec1.push(0x04);
            sec1.extend_from_slice(x);
            sec1.extend_from_slice(y);
            // Rejects points that are not on the curve.
            let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1)
                .map_err(|_| WebauthnError::InvalidPublicKey)?;
            Ok((alg, PublicKey::Es256(key)))
        }
        (CoseAlg::EdDsa, 1) => {
            let crv = as_i64(int_key(map, -1)?.ok_or(WebauthnError::MalformedCbor)?)?;
            if crv != 6 {
                return Err(WebauthnError::UnsupportedAlgorithm);
            }
            let x = as_bytes(int_key(map, -2)?.ok_or(WebauthnError::MalformedCbor)?)?;
            let x: [u8; 32] = x.try_into().map_err(|_| WebauthnError::InvalidPublicKey)?;
            let key = ed25519_dalek::VerifyingKey::from_bytes(&x)
                .map_err(|_| WebauthnError::InvalidPublicKey)?;
            Ok((alg, PublicKey::EdDsa(key)))
        }
        (CoseAlg::Rs256, 3) => {
            let n = as_bytes(int_key(map, -1)?.ok_or(WebauthnError::MalformedCbor)?)?;
            let e = as_bytes(int_key(map, -2)?.ok_or(WebauthnError::MalformedCbor)?)?;
            Ok((alg, PublicKey::Rs256(RsaKey::new(n, e)?)))
        }
        // Algorithm and key type disagree (for example alg -7 with an OKP key).
        _ => Err(WebauthnError::UnsupportedAlgorithm),
    }
}

impl PublicKey {
    /// Verify `signature` over `message` (the signature scheme hashes it).
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), WebauthnError> {
        let bad = |_| WebauthnError::BadSignature;
        match self {
            Self::Es256(k) => {
                use p256::ecdsa::signature::Verifier;
                let sig = p256::ecdsa::Signature::from_der(signature).map_err(bad)?;
                k.verify(message, &sig).map_err(bad)
            }
            Self::EdDsa(k) => {
                let sig = ed25519_dalek::Signature::from_slice(signature).map_err(bad)?;
                k.verify_strict(message, &sig).map_err(bad)
            }
            Self::Rs256(k) => k.verify(message, signature),
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Verify a registration response (spec 7.1) with `none` attestation.
///
/// `raw_id` is the response's `rawId`; `ceremony` must have been obtained from
/// [`ChallengeBook::take`] (single use). Returns the credential to persist
/// with `sign_count` from the authenticator.
pub fn finish_registration(
    rp: &RelyingParty,
    ceremony: &PendingCeremony,
    raw_id: &[u8],
    client_data_json: &[u8],
    attestation_object: &[u8],
    now: i64,
) -> Result<StoredCredential, WebauthnError> {
    check_client_data(rp, ceremony, "webauthn.create", client_data_json, now)?;

    let att = cbor_exact(attestation_object)?;
    let att = map_entries(&att)?;
    let mut fmt = None;
    let mut att_stmt = None;
    let mut auth_data = None;
    for (k, v) in att {
        let Value::Text(k) = k else {
            return Err(WebauthnError::MalformedCbor);
        };
        let slot = match k.as_str() {
            "fmt" => &mut fmt,
            "attStmt" => &mut att_stmt,
            "authData" => &mut auth_data,
            _ => continue,
        };
        if slot.replace(v).is_some() {
            return Err(WebauthnError::MalformedCbor);
        }
    }
    match fmt {
        Some(Value::Text(f)) if f == "none" => {}
        Some(Value::Text(_)) => return Err(WebauthnError::UnsupportedAttestation),
        _ => return Err(WebauthnError::MalformedCbor),
    }
    match att_stmt {
        Some(Value::Map(m)) if m.is_empty() => {}
        _ => return Err(WebauthnError::UnsupportedAttestation),
    }
    let auth_data = as_bytes(auth_data.ok_or(WebauthnError::MalformedCbor)?)?;

    let parsed = parse_auth_data(rp, ceremony, auth_data)?;
    if parsed.flags & FLAG_AT == 0 {
        return Err(WebauthnError::MalformedAuthData);
    }
    let rest = parsed.rest;
    if rest.len() < 18 {
        return Err(WebauthnError::MalformedAuthData);
    }
    let mut aaguid = [0u8; 16];
    aaguid.copy_from_slice(&rest[..16]);
    let id_len = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
    if id_len == 0 || id_len > 1023 || rest.len() < 18 + id_len {
        return Err(WebauthnError::MalformedAuthData);
    }
    let credential_id = &rest[18..18 + id_len];
    let after_id = &rest[18 + id_len..];
    let (_key_value, key_len) = cbor_prefix(after_id)?;
    let cose_key = &after_id[..key_len];
    let tail = &after_id[key_len..];
    if parsed.flags & FLAG_ED != 0 {
        // Extension outputs: one CBOR map covering the remaining bytes.
        if !matches!(cbor_exact(tail)?, Value::Map(_)) {
            return Err(WebauthnError::MalformedAuthData);
        }
    } else if !tail.is_empty() {
        return Err(WebauthnError::MalformedAuthData);
    }

    if !bool::from(credential_id.ct_eq(raw_id)) {
        return Err(WebauthnError::CredentialIdMismatch);
    }
    let (alg, _key) = parse_cose_key(cose_key)?;

    Ok(StoredCredential {
        credential_id: credential_id.to_vec(),
        cose_key: cose_key.to_vec(),
        alg,
        sign_count: parsed.sign_count,
        backup_eligible: parsed.flags & FLAG_BE != 0,
        backup_state: parsed.flags & FLAG_BS != 0,
        aaguid,
    })
}

// ---------------------------------------------------------------------------
// Assertion
// ---------------------------------------------------------------------------

/// What a successful assertion tells the caller to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionOutcome {
    /// New signature counter to store.
    pub sign_count: u32,
    /// Current backup state to store.
    pub backup_state: bool,
    /// Whether the user was verified (PIN / biometric) for this assertion.
    pub user_verified: bool,
}

/// One assertion response as received from the client.
#[derive(Debug, Clone, Copy)]
pub struct AssertionResponse<'a> {
    /// `rawId`.
    pub raw_id: &'a [u8],
    /// `response.clientDataJSON`.
    pub client_data_json: &'a [u8],
    /// `response.authenticatorData`.
    pub authenticator_data: &'a [u8],
    /// `response.signature` (DER for ES256, raw for EdDSA, PKCS#1 v1.5 for RS256).
    pub signature: &'a [u8],
    /// `response.userHandle`, present for discoverable credentials.
    pub user_handle: Option<&'a [u8]>,
}

/// Verify an assertion (spec 7.2) against the stored credential.
///
/// `expected_user_handle` is the user handle the credential was registered
/// with (the account's opaque id); when the response carries one it must match.
pub fn finish_authentication(
    rp: &RelyingParty,
    ceremony: &PendingCeremony,
    credential: &StoredCredential,
    expected_user_handle: Option<&[u8]>,
    response: &AssertionResponse<'_>,
    now: i64,
) -> Result<AssertionOutcome, WebauthnError> {
    if !bool::from(response.raw_id.ct_eq(credential.credential_id.as_slice())) {
        return Err(WebauthnError::CredentialIdMismatch);
    }
    if let (Some(got), Some(want)) = (response.user_handle, expected_user_handle)
        && !bool::from(got.ct_eq(want))
    {
        return Err(WebauthnError::UserHandleMismatch);
    }
    check_client_data(rp, ceremony, "webauthn.get", response.client_data_json, now)?;
    let parsed = parse_auth_data(rp, ceremony, response.authenticator_data)?;
    // An assertion carries no attested credential data; extensions only.
    if parsed.flags & FLAG_AT != 0 {
        return Err(WebauthnError::InvalidFlags);
    }
    if parsed.flags & FLAG_ED != 0 {
        if !matches!(cbor_exact(parsed.rest)?, Value::Map(_)) {
            return Err(WebauthnError::MalformedAuthData);
        }
    } else if !parsed.rest.is_empty() {
        return Err(WebauthnError::MalformedAuthData);
    }

    let (alg, key) = parse_cose_key(&credential.cose_key)?;
    if alg != credential.alg {
        return Err(WebauthnError::UnsupportedAlgorithm);
    }
    let mut message = Vec::with_capacity(response.authenticator_data.len() + 32);
    message.extend_from_slice(response.authenticator_data);
    message.extend_from_slice(&Sha256::digest(response.client_data_json));
    key.verify(&message, response.signature)?;

    let backup_eligible = parsed.flags & FLAG_BE != 0;
    if backup_eligible != credential.backup_eligible {
        return Err(WebauthnError::BackupEligibilityChanged);
    }
    // Counter (spec 7.2 step 21): authenticators that never count report 0
    // both times; otherwise it must strictly increase.
    if (parsed.sign_count != 0 || credential.sign_count != 0)
        && parsed.sign_count <= credential.sign_count
    {
        return Err(WebauthnError::CounterRegression);
    }
    Ok(AssertionOutcome {
        sign_count: parsed.sign_count,
        backup_state: parsed.flags & FLAG_BS != 0,
        user_verified: parsed.flags & FLAG_UV != 0,
    })
}

#[cfg(test)]
mod tests;
