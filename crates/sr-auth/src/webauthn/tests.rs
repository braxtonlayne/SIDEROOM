//! Tests for the WebAuthn relying party. Authenticators are simulated with
//! RustCrypto signing keys (RSA signing is hand-rolled on `crypto-bigint`
//! from a fixed throwaway test key; one signature is produced by the OpenSSL
//! command line as an independent interoperability vector).

use super::*;
use ciborium::value::Integer;
use ed25519_dalek::Signer as _;

const RP_ID: &str = "chat.example.com";
const ORIGIN: &str = "https://chat.example.com";
const NOW: i64 = 1_800_000_000;
const USER: &[u8] = b"user-handle-0001";

fn rp() -> RelyingParty {
    RelyingParty {
        id: RP_ID.into(),
        name: "SIDE ROOM".into(),
        origins: vec![ORIGIN.into()],
    }
}

fn int(i: i64) -> Value {
    Value::Integer(Integer::from(i))
}

fn cbor(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(v, &mut out).unwrap();
    out
}

fn hex(s: &str) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(s.trim().as_bytes()).unwrap()
}

// --- simulated authenticators ---------------------------------------------

enum Auth {
    Es256(p256::ecdsa::SigningKey),
    EdDsa(ed25519_dalek::SigningKey),
    Rs256 { n: Vec<u8>, d: Vec<u8> },
}

fn rsa_fixture() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let lines: Vec<&str> = include_str!("rsa_fixture.txt").lines().collect();
    (hex(lines[0]), hex(lines[1]), hex(lines[2]))
}

impl Auth {
    fn es256() -> Self {
        Self::Es256(p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).unwrap())
    }
    fn eddsa() -> Self {
        Self::EdDsa(ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]))
    }
    fn rs256() -> Self {
        let (n, d, _) = rsa_fixture();
        Self::Rs256 { n, d }
    }

    fn cose_key_value(&self) -> Value {
        match self {
            Self::Es256(sk) => {
                let point = sk.verifying_key().to_sec1_point(false);
                Value::Map(vec![
                    (int(1), int(2)),
                    (int(3), int(-7)),
                    (int(-1), int(1)),
                    (int(-2), Value::Bytes(point.x().unwrap().to_vec())),
                    (int(-3), Value::Bytes(point.y().unwrap().to_vec())),
                ])
            }
            Self::EdDsa(sk) => Value::Map(vec![
                (int(1), int(1)),
                (int(3), int(-8)),
                (int(-1), int(6)),
                (
                    int(-2),
                    Value::Bytes(sk.verifying_key().to_bytes().to_vec()),
                ),
            ]),
            Self::Rs256 { n, .. } => Value::Map(vec![
                (int(1), int(3)),
                (int(3), int(-257)),
                (int(-1), Value::Bytes(n.clone())),
                (int(-2), Value::Bytes(vec![0x01, 0x00, 0x01])),
            ]),
        }
    }

    fn cose_key(&self) -> Vec<u8> {
        cbor(&self.cose_key_value())
    }

    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        match self {
            Self::Es256(sk) => {
                let sig: p256::ecdsa::Signature = sk.sign(msg);
                sig.to_der().as_bytes().to_vec()
            }
            Self::EdDsa(sk) => sk.sign(msg).to_bytes().to_vec(),
            Self::Rs256 { n, d } => rsa_sign(n, d, msg),
        }
    }
}

/// s = EM^d mod n for the fixed test key.
fn rsa_sign(n: &[u8], d: &[u8], msg: &[u8]) -> Vec<u8> {
    let modulus = BoxedUint::from_be_slice_vartime(n);
    let k = n.len();
    let em = emsa_pkcs1_v15_sha256(msg, k).unwrap();
    let prec = modulus.bits_precision();
    let m = BoxedUint::from_be_slice(&em, prec).unwrap();
    let params = BoxedMontyParams::new(Odd::new(modulus).unwrap());
    let exp = BoxedUint::from_be_slice_vartime(d);
    let s = BoxedMontyForm::new(m, &params)
        .pow(&exp)
        .retrieve()
        .to_be_bytes();
    s[s.len() - k..].to_vec()
}

// --- builders ---------------------------------------------------------------

struct Flags;
impl Flags {
    const UP: u8 = FLAG_UP;
    const UV: u8 = FLAG_UV;
}

fn auth_data(
    rp_id: &str,
    flags: u8,
    counter: u32,
    attested: Option<(&[u8], &[u8])>,
    tail: &[u8],
) -> Vec<u8> {
    let mut d = Sha256::digest(rp_id.as_bytes()).to_vec();
    d.push(flags);
    d.extend_from_slice(&counter.to_be_bytes());
    if let Some((id, key)) = attested {
        d.extend_from_slice(&[0xAA; 16]);
        d.extend_from_slice(&u16::try_from(id.len()).unwrap().to_be_bytes());
        d.extend_from_slice(id);
        d.extend_from_slice(key);
    }
    d.extend_from_slice(tail);
    d
}

fn client_data(kind: &str, challenge_b64: &str, origin: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": kind, "challenge": challenge_b64, "origin": origin, "crossOrigin": false,
    }))
    .unwrap()
}

fn att_object(fmt: &str, stmt: Value, auth_data: Vec<u8>) -> Vec<u8> {
    cbor(&Value::Map(vec![
        (Value::Text("fmt".into()), Value::Text(fmt.into())),
        (Value::Text("attStmt".into()), stmt),
        (Value::Text("authData".into()), Value::Bytes(auth_data)),
    ]))
}

fn empty_stmt() -> Value {
    Value::Map(vec![])
}

/// A full simulated registration; returns the pieces so tests can tamper.
struct Reg {
    ceremony: PendingCeremony,
    cred_id: Vec<u8>,
    cdj: Vec<u8>,
    att: Vec<u8>,
}

fn register(auth: &Auth, flags: u8, counter: u32) -> Reg {
    let (ceremony, _opts) = begin_registration(
        &rp(),
        USER,
        "alice",
        "Alice",
        &[],
        UserVerification::Preferred,
        NOW,
    )
    .unwrap();
    let cred_id = vec![0x42; 32];
    let key = auth.cose_key();
    let ad = auth_data(RP_ID, flags | FLAG_AT, counter, Some((&cred_id, &key)), &[]);
    Reg {
        cdj: client_data("webauthn.create", &ceremony.challenge_b64(), ORIGIN),
        att: att_object("none", empty_stmt(), ad),
        ceremony,
        cred_id,
    }
}

fn finish(reg: &Reg) -> Result<StoredCredential, WebauthnError> {
    finish_registration(
        &rp(),
        &reg.ceremony,
        &reg.cred_id,
        &reg.cdj,
        &reg.att,
        NOW + 1,
    )
}

fn registered(auth: &Auth) -> StoredCredential {
    finish(&register(auth, Flags::UP | Flags::UV, 0)).unwrap()
}

struct Assertion {
    ceremony: PendingCeremony,
    cdj: Vec<u8>,
    ad: Vec<u8>,
    sig: Vec<u8>,
}

fn assert_with(auth: &Auth, cred: &StoredCredential, flags: u8, counter: u32) -> Assertion {
    let (ceremony, _o) =
        begin_authentication(&rp(), &[], UserVerification::Preferred, NOW).unwrap();
    let cdj = client_data("webauthn.get", &ceremony.challenge_b64(), ORIGIN);
    let ad = auth_data(RP_ID, flags, counter, None, &[]);
    let mut msg = ad.clone();
    msg.extend_from_slice(&Sha256::digest(&cdj));
    let _ = cred;
    Assertion {
        sig: auth.sign(&msg),
        ceremony,
        cdj,
        ad,
    }
}

fn verify(a: &Assertion, cred: &StoredCredential) -> Result<AssertionOutcome, WebauthnError> {
    verify_with(a, cred, Some(USER))
}

fn verify_with(
    a: &Assertion,
    cred: &StoredCredential,
    handle: Option<&[u8]>,
) -> Result<AssertionOutcome, WebauthnError> {
    finish_authentication(
        &rp(),
        &a.ceremony,
        cred,
        Some(USER),
        &AssertionResponse {
            raw_id: &cred.credential_id,
            client_data_json: &a.cdj,
            authenticator_data: &a.ad,
            signature: &a.sig,
            user_handle: handle,
        },
        NOW + 1,
    )
}

// --- happy paths ------------------------------------------------------------

fn roundtrip(auth: Auth, alg: CoseAlg) {
    let cred = registered(&auth);
    assert_eq!(cred.alg, alg);
    assert_eq!(cred.sign_count, 0);
    assert_eq!(cred.credential_id, vec![0x42; 32]);
    assert_eq!(cred.aaguid, [0xAA; 16]);
    let a = assert_with(&auth, &cred, Flags::UP | Flags::UV, 5);
    let out = verify(&a, &cred).unwrap();
    assert_eq!(out.sign_count, 5);
    assert!(out.user_verified);
    // A later assertion with a higher counter also verifies.
    let mut cred2 = cred.clone();
    cred2.sign_count = out.sign_count;
    let a2 = assert_with(&auth, &cred2, Flags::UP, 6);
    assert!(!verify(&a2, &cred2).unwrap().user_verified);
}

#[test]
fn es256_register_and_assert() {
    roundtrip(Auth::es256(), CoseAlg::Es256);
}

#[test]
fn eddsa_register_and_assert() {
    roundtrip(Auth::eddsa(), CoseAlg::EdDsa);
}

#[test]
fn rs256_register_and_assert() {
    roundtrip(Auth::rs256(), CoseAlg::Rs256);
}

#[test]
fn rs256_verifies_an_openssl_signature() {
    // Independent vector: `openssl dgst -sha256 -sign` over a fixed message.
    let (n, _d, sig) = rsa_fixture();
    let key = RsaKey::new(&n, &[1, 0, 1]).unwrap();
    key.verify(b"sideroom-rs256-interop", &sig).unwrap();
    assert_eq!(
        key.verify(b"sideroom-rs256-interoq", &sig),
        Err(WebauthnError::BadSignature)
    );
}

#[test]
fn rs256_rejects_malformed_signatures() {
    let (n, _d, sig) = rsa_fixture();
    let key = RsaKey::new(&n, &[1, 0, 1]).unwrap();
    // Wrong length, all zero, all 0xff (>= n), and a flipped bit.
    assert!(key.verify(b"x", &sig[1..]).is_err());
    assert!(key.verify(b"x", &vec![0u8; sig.len()]).is_err());
    assert!(key.verify(b"x", &vec![0xffu8; sig.len()]).is_err());
    let mut flipped = sig.clone();
    flipped[100] ^= 1;
    assert!(key.verify(b"sideroom-rs256-interop", &flipped).is_err());
}

#[test]
fn registration_with_extension_data_and_backup_flags() {
    let auth = Auth::es256();
    let (ceremony, _) =
        begin_registration(&rp(), USER, "a", "A", &[], UserVerification::Required, NOW).unwrap();
    let id = vec![9u8; 16];
    let key = auth.cose_key();
    let ext = cbor(&Value::Map(vec![(
        Value::Text("credProps".into()),
        Value::Bool(true),
    )]));
    let flags = Flags::UP | Flags::UV | FLAG_AT | FLAG_ED | FLAG_BE | FLAG_BS;
    let ad = auth_data(RP_ID, flags, 0, Some((&id, &key)), &ext);
    let cred = finish_registration(
        &rp(),
        &ceremony,
        &id,
        &client_data("webauthn.create", &ceremony.challenge_b64(), ORIGIN),
        &att_object("none", empty_stmt(), ad),
        NOW,
    )
    .unwrap();
    assert!(cred.backup_eligible && cred.backup_state);
}

#[test]
fn options_json_has_the_expected_shape() {
    let (c, o) = begin_registration(
        &rp(),
        USER,
        "alice",
        "Alice",
        &[vec![1, 2, 3]],
        UserVerification::Preferred,
        NOW,
    )
    .unwrap();
    assert_eq!(o["challenge"], c.challenge_b64());
    assert_eq!(o["rp"]["id"], RP_ID);
    assert_eq!(o["attestation"], "none");
    assert_eq!(o["authenticatorSelection"]["residentKey"], "required");
    let algs: Vec<i64> = o["pubKeyCredParams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["alg"].as_i64().unwrap())
        .collect();
    assert_eq!(algs, vec![-7, -8, -257]);
    assert_eq!(o["excludeCredentials"][0]["id"], "AQID");
    let (c2, o2) = begin_authentication(&rp(), &[], UserVerification::Required, NOW).unwrap();
    assert_ne!(c.challenge_b64(), c2.challenge_b64());
    assert_eq!(o2["rpId"], RP_ID);
    assert_eq!(o2["userVerification"], "required");
    assert!(o2["allowCredentials"].as_array().unwrap().is_empty());
}

// --- registration negatives -------------------------------------------------

#[test]
fn registration_wrong_origin() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    r.cdj = client_data(
        "webauthn.create",
        &r.ceremony.challenge_b64(),
        "https://evil.example",
    );
    assert_eq!(finish(&r), Err(WebauthnError::OriginMismatch));
    // Prefix and scheme tricks are not matches either.
    for o in [
        "https://chat.example.com.evil.io",
        "http://chat.example.com",
        "https://chat.example.com/",
    ] {
        r.cdj = client_data("webauthn.create", &r.ceremony.challenge_b64(), o);
        assert_eq!(finish(&r), Err(WebauthnError::OriginMismatch), "{o}");
    }
}

#[test]
fn registration_cross_origin_is_refused() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    r.cdj = serde_json::to_vec(&serde_json::json!({
        "type": "webauthn.create", "challenge": r.ceremony.challenge_b64(),
        "origin": ORIGIN, "crossOrigin": true,
    }))
    .unwrap();
    assert_eq!(finish(&r), Err(WebauthnError::CrossOrigin));
}

#[test]
fn registration_wrong_rp_id_hash() {
    let (ceremony, _) =
        begin_registration(&rp(), USER, "a", "A", &[], UserVerification::Preferred, NOW).unwrap();
    let key = Auth::es256().cose_key();
    let ad = auth_data(
        "other.example.com",
        Flags::UP | FLAG_AT,
        0,
        Some((&[1; 8], &key)),
        &[],
    );
    let r = Reg {
        cdj: client_data("webauthn.create", &ceremony.challenge_b64(), ORIGIN),
        att: att_object("none", empty_stmt(), ad),
        ceremony,
        cred_id: vec![1; 8],
    };
    assert_eq!(finish(&r), Err(WebauthnError::RpIdHashMismatch));
}

#[test]
fn registration_wrong_ceremony_type_and_challenge() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    r.cdj = client_data("webauthn.get", &r.ceremony.challenge_b64(), ORIGIN);
    assert_eq!(finish(&r), Err(WebauthnError::WrongCeremonyType));
    r.cdj = client_data(
        "webauthn.create",
        &BASE64URL_NOPAD.encode(&[0u8; 32]),
        ORIGIN,
    );
    assert_eq!(finish(&r), Err(WebauthnError::ChallengeMismatch));
    r.cdj = client_data("webauthn.create", "not base64 !!", ORIGIN);
    assert_eq!(finish(&r), Err(WebauthnError::MalformedClientData));
    r.cdj = b"{not json".to_vec();
    assert_eq!(finish(&r), Err(WebauthnError::MalformedClientData));
    r.cdj = vec![0xff, 0xfe];
    assert_eq!(finish(&r), Err(WebauthnError::MalformedClientData));
}

#[test]
fn registration_expired_ceremony() {
    let r = register(&Auth::es256(), Flags::UP, 0);
    let late = NOW + CEREMONY_TTL_SECS + 1;
    assert_eq!(
        finish_registration(&rp(), &r.ceremony, &r.cred_id, &r.cdj, &r.att, late),
        Err(WebauthnError::Expired)
    );
}

#[test]
fn replayed_challenge_is_refused() {
    let mut book = ChallengeBook::new();
    let r = register(&Auth::es256(), Flags::UP, 0);
    book.insert("sess-1", r.ceremony.clone());
    let first = book.take("sess-1").unwrap();
    assert!(finish_registration(&rp(), &first, &r.cred_id, &r.cdj, &r.att, NOW).is_ok());
    // The same finish call again finds no ceremony.
    assert_eq!(book.take("sess-1"), Err(WebauthnError::UnknownCeremony));
    assert_eq!(
        book.take("never-issued"),
        Err(WebauthnError::UnknownCeremony)
    );
    // Expired entries can be purged.
    book.insert("old", r.ceremony);
    book.purge(NOW + CEREMONY_TTL_SECS + 10);
    assert_eq!(book.take("old"), Err(WebauthnError::UnknownCeremony));
}

#[test]
fn a_response_for_another_ceremony_is_refused() {
    let auth = Auth::es256();
    let cred = registered(&auth);
    let a = assert_with(&auth, &cred, Flags::UP, 1);
    let other = assert_with(&auth, &cred, Flags::UP, 1);
    let mixed = Assertion {
        ceremony: other.ceremony,
        ..a
    };
    assert_eq!(verify(&mixed, &cred), Err(WebauthnError::ChallengeMismatch));
}

#[test]
fn registration_flag_checks() {
    let auth = Auth::es256();
    // No user presence.
    assert_eq!(
        finish(&register(&auth, 0, 0)),
        Err(WebauthnError::UserPresenceMissing)
    );
    // Backup state without eligibility.
    assert_eq!(
        finish(&register(&auth, Flags::UP | FLAG_BS, 0)),
        Err(WebauthnError::InvalidFlags)
    );
    // UV required but absent.
    let (ceremony, _) =
        begin_registration(&rp(), USER, "a", "A", &[], UserVerification::Required, NOW).unwrap();
    let key = auth.cose_key();
    let ad = auth_data(RP_ID, Flags::UP | FLAG_AT, 0, Some((&[1; 8], &key)), &[]);
    assert_eq!(
        finish_registration(
            &rp(),
            &ceremony,
            &[1; 8],
            &client_data("webauthn.create", &ceremony.challenge_b64(), ORIGIN),
            &att_object("none", empty_stmt(), ad),
            NOW
        ),
        Err(WebauthnError::UserVerificationMissing)
    );
    // AT flag missing in a registration.
    let mut r = register(&auth, Flags::UP, 0);
    let ad = auth_data(RP_ID, Flags::UP, 0, None, &[]);
    r.att = att_object("none", empty_stmt(), ad);
    assert_eq!(finish(&r), Err(WebauthnError::MalformedAuthData));
}

#[test]
fn registration_rejects_other_attestation_formats() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    let key = Auth::es256().cose_key();
    let ad = auth_data(RP_ID, Flags::UP | FLAG_AT, 0, Some((&r.cred_id, &key)), &[]);
    r.att = att_object("packed", empty_stmt(), ad.clone());
    assert_eq!(finish(&r), Err(WebauthnError::UnsupportedAttestation));
    // `none` must have an empty statement.
    let stmt = Value::Map(vec![(Value::Text("sig".into()), Value::Bytes(vec![1]))]);
    r.att = att_object("none", stmt, ad);
    assert_eq!(finish(&r), Err(WebauthnError::UnsupportedAttestation));
}

#[test]
fn registration_credential_id_must_match_raw_id() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    r.cred_id = vec![0x43; 32];
    assert_eq!(finish(&r), Err(WebauthnError::CredentialIdMismatch));
}

// --- malformed CBOR / authData ---------------------------------------------

#[test]
fn registration_malformed_cbor() {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    let good = r.att.clone();
    // Garbage, empty, truncated, and trailing bytes after a valid object.
    for bad in [
        vec![0xff, 0xff, 0xff],
        vec![],
        good[..good.len() / 2].to_vec(),
        [good.clone(), vec![0]].concat(),
    ] {
        r.att = bad;
        assert_eq!(finish(&r), Err(WebauthnError::MalformedCbor));
    }
    // Not a map.
    r.att = cbor(&Value::Array(vec![]));
    assert_eq!(finish(&r), Err(WebauthnError::MalformedCbor));
    // Duplicate "fmt".
    r.att = cbor(&Value::Map(vec![
        (Value::Text("fmt".into()), Value::Text("none".into())),
        (Value::Text("fmt".into()), Value::Text("none".into())),
        (Value::Text("attStmt".into()), empty_stmt()),
        (Value::Text("authData".into()), Value::Bytes(vec![0; 37])),
    ]));
    assert_eq!(finish(&r), Err(WebauthnError::MalformedCbor));
    // Missing authData.
    r.att = cbor(&Value::Map(vec![
        (Value::Text("fmt".into()), Value::Text("none".into())),
        (Value::Text("attStmt".into()), empty_stmt()),
    ]));
    assert_eq!(finish(&r), Err(WebauthnError::MalformedCbor));
}

#[test]
fn registration_malformed_auth_data() {
    let auth = Auth::es256();
    let key = auth.cose_key();
    let id = [0x42u8; 32];
    let mut r = register(&auth, Flags::UP, 0);
    let cases: Vec<Vec<u8>> = vec![
        // Shorter than the 37-byte header.
        vec![0; 36],
        // Attested data cut off inside the credential id.
        auth_data(RP_ID, Flags::UP | FLAG_AT, 0, Some((&id, &key)), &[])[..37 + 20].to_vec(),
        // Trailing junk after the key without the ED flag.
        auth_data(RP_ID, Flags::UP | FLAG_AT, 0, Some((&id, &key)), &[0x00]),
        // ED flag but the extension block is not a map.
        auth_data(
            RP_ID,
            Flags::UP | FLAG_AT | FLAG_ED,
            0,
            Some((&id, &key)),
            &cbor(&int(1)),
        ),
        // ED flag with undecodable bytes.
        auth_data(
            RP_ID,
            Flags::UP | FLAG_AT | FLAG_ED,
            0,
            Some((&id, &key)),
            &[0xff],
        ),
    ];
    for ad in cases {
        r.att = att_object("none", empty_stmt(), ad);
        assert!(
            matches!(
                finish(&r),
                Err(WebauthnError::MalformedAuthData | WebauthnError::MalformedCbor)
            ),
            "{:?}",
            finish(&r)
        );
    }
    // Zero-length credential id.
    let mut ad = auth_data(RP_ID, Flags::UP | FLAG_AT, 0, None, &[]);
    ad.extend_from_slice(&[0xAA; 16]);
    ad.extend_from_slice(&0u16.to_be_bytes());
    ad.extend_from_slice(&key);
    r.att = att_object("none", empty_stmt(), ad);
    assert_eq!(finish(&r), Err(WebauthnError::MalformedAuthData));
    // Credential id longer than 1023 bytes.
    let long = vec![1u8; 1024];
    let ad = auth_data(RP_ID, Flags::UP | FLAG_AT, 0, Some((&long, &key)), &[]);
    r.att = att_object("none", empty_stmt(), ad);
    r.cred_id = long;
    assert_eq!(finish(&r), Err(WebauthnError::MalformedAuthData));
}

// --- algorithm / key negatives ---------------------------------------------

fn register_with_key(key: Value) -> Result<StoredCredential, WebauthnError> {
    let mut r = register(&Auth::es256(), Flags::UP, 0);
    let ad = auth_data(
        RP_ID,
        Flags::UP | FLAG_AT,
        0,
        Some((&r.cred_id, &cbor(&key))),
        &[],
    );
    r.att = att_object("none", empty_stmt(), ad);
    finish(&r)
}

fn ec_key_with(alg: i64, kty: i64, crv: i64, x: Vec<u8>, y: Vec<u8>) -> Value {
    Value::Map(vec![
        (int(1), int(kty)),
        (int(3), int(alg)),
        (int(-1), int(crv)),
        (int(-2), Value::Bytes(x)),
        (int(-3), Value::Bytes(y)),
    ])
}

#[test]
fn registration_rejects_unlisted_algorithms() {
    let Value::Map(ok) = Auth::es256().cose_key_value() else {
        unreachable!()
    };
    let x = ok[3].1.clone();
    let y = ok[4].1.clone();
    let (Value::Bytes(x), Value::Bytes(y)) = (x, y) else {
        unreachable!()
    };
    // ES384 (-35), ES512 (-36), RS1 (-65535), PS256 (-37), unknown.
    for alg in [-35, -36, -37, -65535, 0, 7] {
        assert_eq!(
            register_with_key(ec_key_with(alg, 2, 1, x.clone(), y.clone())),
            Err(WebauthnError::UnsupportedAlgorithm),
            "alg {alg}"
        );
    }
    // Algorithm missing entirely.
    let no_alg = Value::Map(vec![(int(1), int(2)), (int(-1), int(1))]);
    assert_eq!(
        register_with_key(no_alg),
        Err(WebauthnError::UnsupportedAlgorithm)
    );
    // ES256 declared on a P-384 curve id or on an OKP key.
    assert_eq!(
        register_with_key(ec_key_with(-7, 2, 2, x.clone(), y.clone())),
        Err(WebauthnError::UnsupportedAlgorithm)
    );
    assert_eq!(
        register_with_key(ec_key_with(-7, 1, 6, x.clone(), y.clone())),
        Err(WebauthnError::UnsupportedAlgorithm)
    );
    // EdDSA declared on an EC2 key.
    assert_eq!(
        register_with_key(ec_key_with(-8, 2, 1, x, y)),
        Err(WebauthnError::UnsupportedAlgorithm)
    );
}

#[test]
fn registration_rejects_invalid_keys() {
    // Point not on the curve.
    assert_eq!(
        register_with_key(ec_key_with(-7, 2, 1, vec![1; 32], vec![2; 32])),
        Err(WebauthnError::InvalidPublicKey)
    );
    // Wrong coordinate length.
    assert_eq!(
        register_with_key(ec_key_with(-7, 2, 1, vec![1; 31], vec![2; 32])),
        Err(WebauthnError::InvalidPublicKey)
    );
    // Ed25519 key of the wrong length.
    let short = Value::Map(vec![
        (int(1), int(1)),
        (int(3), int(-8)),
        (int(-1), int(6)),
        (int(-2), Value::Bytes(vec![1; 31])),
    ]);
    assert_eq!(
        register_with_key(short),
        Err(WebauthnError::InvalidPublicKey)
    );
    // RSA below 2048 bits, with an even exponent, and with a huge exponent.
    let (n, _, _) = rsa_fixture();
    let rsa = |n: Vec<u8>, e: Vec<u8>| {
        Value::Map(vec![
            (int(1), int(3)),
            (int(3), int(-257)),
            (int(-1), Value::Bytes(n)),
            (int(-2), Value::Bytes(e)),
        ])
    };
    assert_eq!(
        register_with_key(rsa(n[..128].to_vec(), vec![1, 0, 1])),
        Err(WebauthnError::InvalidPublicKey)
    );
    assert_eq!(
        register_with_key(rsa(n.clone(), vec![2])),
        Err(WebauthnError::InvalidPublicKey)
    );
    assert_eq!(
        register_with_key(rsa(n.clone(), vec![1])),
        Err(WebauthnError::InvalidPublicKey)
    );
    assert_eq!(
        register_with_key(rsa(n.clone(), vec![1, 0, 0, 0, 0, 1])),
        Err(WebauthnError::InvalidPublicKey)
    );
    // Even modulus.
    let mut even = n.clone();
    *even.last_mut().unwrap() &= !1;
    assert_eq!(
        register_with_key(rsa(even, vec![1, 0, 1])),
        Err(WebauthnError::InvalidPublicKey)
    );
    // Duplicate key labels are refused.
    let dup = Value::Map(vec![(int(1), int(2)), (int(1), int(2)), (int(3), int(-7))]);
    assert_eq!(register_with_key(dup), Err(WebauthnError::MalformedCbor));
}

#[test]
fn assertion_with_a_mismatching_stored_algorithm_is_refused() {
    let auth = Auth::es256();
    let mut cred = registered(&auth);
    cred.alg = CoseAlg::EdDsa; // database row disagrees with the key
    let a = assert_with(&auth, &cred, Flags::UP, 1);
    assert_eq!(verify(&a, &cred), Err(WebauthnError::UnsupportedAlgorithm));
}

#[test]
fn assertion_signed_with_another_algorithm_or_key_fails() {
    let cred = registered(&Auth::es256());
    // EdDSA / RSA signature presented for an ES256 credential.
    for other in [Auth::eddsa(), Auth::rs256()] {
        let a = assert_with(&other, &cred, Flags::UP, 1);
        assert_eq!(verify(&a, &cred), Err(WebauthnError::BadSignature));
    }
    // Another ES256 key.
    let other = Auth::Es256(p256::ecdsa::SigningKey::from_slice(&[9u8; 32]).unwrap());
    let a = assert_with(&other, &cred, Flags::UP, 1);
    assert_eq!(verify(&a, &cred), Err(WebauthnError::BadSignature));
    // And the reverse: an ES256 signature for an Ed25519 and an RSA credential.
    for target in [Auth::eddsa(), Auth::rs256()] {
        let c = registered(&target);
        let a = assert_with(&Auth::es256(), &c, Flags::UP, 1);
        assert_eq!(verify(&a, &c), Err(WebauthnError::BadSignature));
    }
}

// --- assertion negatives ----------------------------------------------------

fn fresh(auth: &Auth) -> (StoredCredential, Assertion) {
    let cred = registered(auth);
    let a = assert_with(auth, &cred, Flags::UP | Flags::UV, 3);
    (cred, a)
}

#[test]
fn assertion_bad_signature_for_every_algorithm() {
    for auth in [Auth::es256(), Auth::eddsa(), Auth::rs256()] {
        let (cred, mut a) = fresh(&auth);
        assert!(verify(&a, &cred).is_ok());
        // Flip a bit near the end of the signature, truncate, and empty it.
        let last = a.sig.len() - 2;
        a.sig[last] ^= 0x01;
        assert_eq!(verify(&a, &cred), Err(WebauthnError::BadSignature));
        a.sig.truncate(10);
        assert_eq!(verify(&a, &cred), Err(WebauthnError::BadSignature));
        a.sig.clear();
        assert_eq!(verify(&a, &cred), Err(WebauthnError::BadSignature));
    }
}

#[test]
fn assertion_tampered_signed_data_fails() {
    for auth in [Auth::es256(), Auth::eddsa(), Auth::rs256()] {
        let (cred, a) = fresh(&auth);
        // Counter bumped after signing.
        let mut t = Assertion {
            ad: a.ad.clone(),
            cdj: a.cdj.clone(),
            sig: a.sig.clone(),
            ceremony: a.ceremony.clone(),
        };
        t.ad[36] = 9;
        assert_eq!(verify(&t, &cred), Err(WebauthnError::BadSignature));
        // clientDataJSON rewritten with extra whitespace (same meaning, different hash).
        let mut t = Assertion {
            ad: a.ad.clone(),
            cdj: a.cdj.clone(),
            sig: a.sig.clone(),
            ceremony: a.ceremony.clone(),
        };
        t.cdj.push(b' ');
        assert_eq!(verify(&t, &cred), Err(WebauthnError::BadSignature));
    }
}

#[test]
fn assertion_wrong_origin_and_ceremony_type() {
    let auth = Auth::es256();
    let (cred, mut a) = fresh(&auth);
    a.cdj = client_data(
        "webauthn.get",
        &a.ceremony.challenge_b64(),
        "https://evil.example",
    );
    assert_eq!(verify(&a, &cred), Err(WebauthnError::OriginMismatch));
    a.cdj = client_data("webauthn.create", &a.ceremony.challenge_b64(), ORIGIN);
    assert_eq!(verify(&a, &cred), Err(WebauthnError::WrongCeremonyType));
}

#[test]
fn assertion_wrong_rp_id_hash() {
    let auth = Auth::es256();
    let cred = registered(&auth);
    let (ceremony, _) = begin_authentication(&rp(), &[], UserVerification::Preferred, NOW).unwrap();
    let cdj = client_data("webauthn.get", &ceremony.challenge_b64(), ORIGIN);
    let ad = auth_data("evil.example.com", Flags::UP, 4, None, &[]);
    let mut msg = ad.clone();
    msg.extend_from_slice(&Sha256::digest(&cdj));
    // Validly signed, but for another RP id.
    let a = Assertion {
        sig: auth.sign(&msg),
        ceremony,
        cdj,
        ad,
    };
    assert_eq!(verify(&a, &cred), Err(WebauthnError::RpIdHashMismatch));
}

#[test]
fn assertion_replayed_challenge_and_expiry() {
    let auth = Auth::es256();
    let (cred, a) = fresh(&auth);
    let mut book = ChallengeBook::new();
    book.insert("login-1", a.ceremony.clone());
    let c = book.take("login-1").unwrap();
    let ok = Assertion {
        ceremony: c,
        cdj: a.cdj.clone(),
        ad: a.ad.clone(),
        sig: a.sig.clone(),
    };
    assert!(verify(&ok, &cred).is_ok());
    assert_eq!(book.take("login-1"), Err(WebauthnError::UnknownCeremony));
    // Past the TTL.
    let r = AssertionResponse {
        raw_id: &cred.credential_id,
        client_data_json: &a.cdj,
        authenticator_data: &a.ad,
        signature: &a.sig,
        user_handle: None,
    };
    assert_eq!(
        finish_authentication(
            &rp(),
            &a.ceremony,
            &cred,
            None,
            &r,
            NOW + CEREMONY_TTL_SECS + 1
        ),
        Err(WebauthnError::Expired)
    );
}

#[test]
fn assertion_counter_regression() {
    let auth = Auth::es256();
    let mut cred = registered(&auth);
    cred.sign_count = 10;
    // Equal and lower counters are refused; higher is accepted.
    for n in [10u32, 9, 0] {
        let a = assert_with(&auth, &cred, Flags::UP, n);
        assert_eq!(
            verify(&a, &cred),
            Err(WebauthnError::CounterRegression),
            "counter {n}"
        );
    }
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP, 11), &cred)
            .unwrap()
            .sign_count,
        11
    );
    // Authenticators that never count (0 -> 0) are fine.
    cred.sign_count = 0;
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP, 0), &cred)
            .unwrap()
            .sign_count,
        0
    );
    // Counting authenticator that restarts at 0 is a regression.
    cred.sign_count = 1;
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP, 0), &cred),
        Err(WebauthnError::CounterRegression)
    );
}

#[test]
fn assertion_flag_checks() {
    let auth = Auth::es256();
    let cred = registered(&auth);
    // No user presence.
    assert_eq!(
        verify(&assert_with(&auth, &cred, 0, 1), &cred),
        Err(WebauthnError::UserPresenceMissing)
    );
    // AT flag in an assertion.
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP | FLAG_AT, 1), &cred),
        Err(WebauthnError::InvalidFlags)
    );
    // BS without BE.
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP | FLAG_BS, 1), &cred),
        Err(WebauthnError::InvalidFlags)
    );
    // BE appearing after registration without it.
    assert_eq!(
        verify(&assert_with(&auth, &cred, Flags::UP | FLAG_BE, 1), &cred),
        Err(WebauthnError::BackupEligibilityChanged)
    );
    // UV required by the ceremony but missing.
    let (ceremony, _) = begin_authentication(&rp(), &[], UserVerification::Required, NOW).unwrap();
    let cdj = client_data("webauthn.get", &ceremony.challenge_b64(), ORIGIN);
    let ad = auth_data(RP_ID, Flags::UP, 1, None, &[]);
    let mut msg = ad.clone();
    msg.extend_from_slice(&Sha256::digest(&cdj));
    let a = Assertion {
        sig: auth.sign(&msg),
        ceremony,
        cdj,
        ad,
    };
    assert_eq!(
        verify(&a, &cred),
        Err(WebauthnError::UserVerificationMissing)
    );
}

#[test]
fn assertion_malformed_auth_data() {
    let auth = Auth::es256();
    let cred = registered(&auth);
    let mut a = assert_with(&auth, &cred, Flags::UP, 1);
    a.ad = vec![0; 10];
    assert_eq!(verify(&a, &cred), Err(WebauthnError::MalformedAuthData));
    // Trailing byte without the ED flag, even if validly signed.
    let mut a = assert_with(&auth, &cred, Flags::UP, 1);
    a.ad.push(0);
    let mut msg = a.ad.clone();
    msg.extend_from_slice(&Sha256::digest(&a.cdj));
    a.sig = auth.sign(&msg);
    assert_eq!(verify(&a, &cred), Err(WebauthnError::MalformedAuthData));
}

#[test]
fn assertion_credential_and_user_handle_binding() {
    let auth = Auth::es256();
    let cred = registered(&auth);
    let a = assert_with(&auth, &cred, Flags::UP, 1);
    // Discoverable login: userHandle present and matching, absent, or foreign.
    assert!(verify_with(&a, &cred, Some(USER)).is_ok());
    assert!(verify_with(&a, &cred, None).is_ok());
    assert_eq!(
        verify_with(&a, &cred, Some(b"someone-else")),
        Err(WebauthnError::UserHandleMismatch)
    );
    // rawId of a different credential.
    let r = AssertionResponse {
        raw_id: &[0u8; 32],
        client_data_json: &a.cdj,
        authenticator_data: &a.ad,
        signature: &a.sig,
        user_handle: None,
    };
    assert_eq!(
        finish_authentication(&rp(), &a.ceremony, &cred, None, &r, NOW),
        Err(WebauthnError::CredentialIdMismatch)
    );
}

#[test]
fn assertion_with_corrupt_stored_key_is_refused() {
    let auth = Auth::es256();
    let mut cred = registered(&auth);
    let a = assert_with(&auth, &cred, Flags::UP, 1);
    cred.cose_key = vec![0xff, 0x00];
    assert_eq!(verify(&a, &cred), Err(WebauthnError::MalformedCbor));
}

// --- robustness ----------------------------------------------------------------

/// Cheap stand-in for a fuzz target (cargo-fuzz is not part of the toolchain here): both
/// parsers must reject, never panic on, bit-flipped, truncated and extended input. A flipped
/// byte in a signed message can never verify.
#[test]
fn mutated_input_is_rejected_without_panicking() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for auth in [Auth::es256(), Auth::eddsa()] {
        let reg = register(&auth, Flags::UP | Flags::UV, 0);
        let cred = finish(&reg).unwrap();
        let a = assert_with(&auth, &cred, Flags::UP | Flags::UV, 3);
        assert!(verify(&a, &cred).is_ok());
        for round in 0..400 {
            let which = usize::try_from(next() % 4).unwrap();
            let mutate = |bytes: &[u8], r: &mut dyn FnMut() -> u64| -> Vec<u8> {
                let mut b = bytes.to_vec();
                match round % 3 {
                    0 if !b.is_empty() => {
                        let i = usize::try_from(r()).unwrap() % b.len();
                        b[i] ^= 1 << (r() % 8);
                    }
                    1 if !b.is_empty() => b.truncate(usize::try_from(r()).unwrap() % b.len()),
                    _ => b.extend_from_slice(&r().to_le_bytes()),
                }
                b
            };
            let mut reg2 = Reg {
                ceremony: reg.ceremony.clone(),
                cred_id: reg.cred_id.clone(),
                cdj: reg.cdj.clone(),
                att: reg.att.clone(),
            };
            match which {
                0 => reg2.att = mutate(&reg.att, &mut next),
                1 => reg2.cdj = mutate(&reg.cdj, &mut next),
                _ => {}
            }
            if which < 2 {
                // Any outcome but a panic is fine; an unchanged-meaning mutation may still pass.
                let _ = finish(&reg2);
            } else {
                let mut a2 = Assertion {
                    ceremony: a.ceremony.clone(),
                    cdj: a.cdj.clone(),
                    ad: a.ad.clone(),
                    sig: a.sig.clone(),
                };
                if which == 2 {
                    a2.ad = mutate(&a.ad, &mut next);
                } else {
                    a2.sig = mutate(&a.sig, &mut next);
                }
                assert!(
                    verify(&a2, &cred).is_err(),
                    "a mutated signed message must not verify"
                );
            }
        }
    }
}
