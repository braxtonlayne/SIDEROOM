//! Web Push message encryption: RFC 8291 over the `aes128gcm` content coding of RFC 8188,
//! single-record messages only (push services cap a message at 4096 bytes anyway).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;

use crate::PushError;

/// Record size announced in the header. A message is one record, so this is also the
/// limit of `padding + payload + 17` bytes.
pub const RECORD_SIZE: u32 = 4096;
/// Largest payload that fits one record: record size minus the GCM tag (16) and the
/// delimiter byte (1).
pub const MAX_PAYLOAD: usize = RECORD_SIZE as usize - 17;

/// Fresh random bytes from the OS.
pub(crate) fn random<const N: usize>() -> Result<[u8; N], PushError> {
    let mut b = [0_u8; N];
    getrandom::fill(&mut b).map_err(|e| PushError::Crypto(format!("no randomness: {e}")))?;
    Ok(b)
}

/// A fresh P-256 secret scalar.
pub(crate) fn random_secret() -> Result<SecretKey, PushError> {
    // A uniformly random 32-byte string is a valid scalar except with probability ~2^-32;
    // retry rather than bias the value.
    for _ in 0..8 {
        if let Ok(k) = SecretKey::from_slice(&random::<32>()?) {
            return Ok(k);
        }
    }
    Err(PushError::Crypto("could not draw a P-256 key".into()))
}

/// Encrypt `plaintext` for the browser subscription `(ua_public, auth_secret)`.
/// `ua_public` is the 65-byte uncompressed `p256dh` key, `auth_secret` the 16-byte `auth`.
pub fn encrypt(
    ua_public: &[u8],
    auth_secret: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, PushError> {
    encrypt_with(
        ua_public,
        auth_secret,
        plaintext,
        &random_secret()?,
        random::<16>()?,
    )
}

/// [`encrypt`] with the ephemeral key and salt supplied (for the RFC 8291 test vector).
pub fn encrypt_with(
    ua_public: &[u8],
    auth_secret: &[u8],
    plaintext: &[u8],
    as_secret: &SecretKey,
    salt: [u8; 16],
) -> Result<Vec<u8>, PushError> {
    if plaintext.len() > MAX_PAYLOAD {
        return Err(PushError::PayloadTooLarge);
    }
    if auth_secret.len() != 16 {
        return Err(PushError::BadSubscription("auth secret must be 16 bytes"));
    }
    let ua = PublicKey::from_sec1_bytes(ua_public)
        .map_err(|_| PushError::BadSubscription("p256dh is not a P-256 point"))?;
    let ua_bytes = ua.to_encoded_point(false);
    let as_public = as_secret.public_key().to_encoded_point(false);

    let shared = p256::ecdh::diffie_hellman(as_secret.to_nonzero_scalar(), ua.as_affine());
    // PRK_key = HKDF-Extract(auth_secret, ecdh); IKM = Expand(PRK_key, key_info, 32).
    let mut key_info = Vec::with_capacity(14 + 65 + 65);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(ua_bytes.as_bytes());
    key_info.extend_from_slice(as_public.as_bytes());
    let (_, prk_key) = Hkdf::<Sha256>::extract(Some(auth_secret), shared.raw_secret_bytes());
    let mut ikm = [0_u8; 32];
    prk_key
        .expand(&key_info, &mut ikm)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;

    let prk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut cek = [0_u8; 16];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;
    let mut nonce = [0_u8; 12];
    prk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;

    // One record: data, then the 0x02 "last record" delimiter.
    let mut record = Vec::with_capacity(plaintext.len() + 1);
    record.extend_from_slice(plaintext);
    record.push(2);
    let cipher =
        Aes128Gcm::new_from_slice(&cek).map_err(|_| PushError::Crypto("aes key length".into()))?;
    let sealed = cipher
        .encrypt(&Nonce::from(nonce), record.as_slice())
        .map_err(|_| PushError::Crypto("aes-gcm seal".into()))?;

    let key_id = as_public.as_bytes();
    let mut body = Vec::with_capacity(16 + 4 + 1 + key_id.len() + sealed.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(u8::try_from(key_id.len()).unwrap_or(65));
    body.extend_from_slice(key_id);
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// Decrypt a body produced for the subscription whose private key is `ua_secret`.
/// Used by tests (and as the independent check of [`encrypt`]).
pub fn decrypt(
    ua_secret: &SecretKey,
    auth_secret: &[u8],
    body: &[u8],
) -> Result<Vec<u8>, PushError> {
    if body.len() < 16 + 4 + 1 + 65 + 17 {
        return Err(PushError::Crypto("body too short".into()));
    }
    let salt = &body[..16];
    let idlen = usize::from(body[20]);
    let key_id = body
        .get(21..21 + idlen)
        .ok_or_else(|| PushError::Crypto("bad key id".into()))?;
    let sealed = &body[21 + idlen..];
    let as_pub = PublicKey::from_sec1_bytes(key_id)
        .map_err(|_| PushError::Crypto("bad sender key".into()))?;
    let ua_public = ua_secret.public_key().to_encoded_point(false);
    let shared = p256::ecdh::diffie_hellman(ua_secret.to_nonzero_scalar(), as_pub.as_affine());
    let mut key_info = Vec::new();
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(ua_public.as_bytes());
    key_info.extend_from_slice(key_id);
    let (_, prk_key) = Hkdf::<Sha256>::extract(Some(auth_secret), shared.raw_secret_bytes());
    let mut ikm = [0_u8; 32];
    prk_key
        .expand(&key_info, &mut ikm)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;
    let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0_u8; 16];
    let mut nonce = [0_u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;
    prk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| PushError::Crypto("hkdf expand".into()))?;
    let cipher =
        Aes128Gcm::new_from_slice(&cek).map_err(|_| PushError::Crypto("aes key length".into()))?;
    let mut plain = cipher
        .decrypt(&Nonce::from(nonce), sealed)
        .map_err(|_| PushError::Crypto("aes-gcm open".into()))?;
    while plain.last() == Some(&0) {
        plain.pop();
    }
    if plain.pop() != Some(2) {
        return Err(PushError::Crypto("missing record delimiter".into()));
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;

    fn d(s: &str) -> Vec<u8> {
        B64.decode(s).expect("base64url")
    }

    /// RFC 8291 section 5 / appendix A worked example.
    #[test]
    fn matches_the_rfc_8291_test_vector() {
        let plaintext = b"When I grow up, I want to be a watermelon";
        let as_secret =
            SecretKey::from_slice(&d("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
        let ua_public = d(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let auth = d("BTBZMqHH6r4Tts7J_aSIgg");
        let salt: [u8; 16] = d("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let body = encrypt_with(&ua_public, &auth, plaintext, &as_secret, salt).unwrap();
        let want = d(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        );
        assert_eq!(body, want);
        // And the receiving side recovers the text.
        let ua_secret =
            SecretKey::from_slice(&d("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94")).unwrap();
        assert_eq!(decrypt(&ua_secret, &auth, &body).unwrap(), plaintext);
    }

    #[test]
    fn round_trips_random_keys_and_rejects_oversize() {
        let ua = random_secret().unwrap();
        let ua_pub = ua.public_key().to_encoded_point(false);
        let auth = random::<16>().unwrap();
        let body = encrypt(ua_pub.as_bytes(), &auth, b"hello \xF0\x9F\x91\x8B").unwrap();
        assert_eq!(
            decrypt(&ua, &auth, &body).unwrap(),
            b"hello \xF0\x9F\x91\x8B"
        );
        let big = vec![b'x'; MAX_PAYLOAD + 1];
        assert!(matches!(
            encrypt(ua_pub.as_bytes(), &auth, &big),
            Err(PushError::PayloadTooLarge)
        ));
        assert!(matches!(
            encrypt(&[1, 2, 3], &auth, b"x"),
            Err(PushError::BadSubscription(_))
        ));
        assert!(
            decrypt(&ua, &[0; 16], &body).is_err(),
            "wrong auth secret fails"
        );
    }
}
