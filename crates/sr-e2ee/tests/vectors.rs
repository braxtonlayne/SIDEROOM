//! VOX-E2E-008: interop test vectors. `vectors.json` is shared with the WASM build
//! (packages/wasm/test/e2ee.test.ts), which must produce the same ciphertexts and decrypt
//! them back, so the browser and native code agree byte for byte.
//!
//! Regenerate (only when the wire format changes on purpose, which also needs a version
//! bump in `frame::VERSION`): `SR_UPDATE_E2EE_VECTORS=1 cargo test -p sr-e2ee --test vectors`.

use serde_json::{Value, json};
use sr_e2ee::frame::{FrameDecryptor, FrameEncryptor, InstanceId};
use sr_e2ee::mls::session_cert_message;
use sr_e2ee::privacy::{call_code, pairwise_code};
use sr_e2ee::{Codec, SenderSecret};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn codec_name(c: Codec) -> &'static str {
    match c {
        Codec::Opus => "audio/opus",
        Codec::Vp8 => "video/VP8",
        Codec::H264 => "video/H264",
    }
}

/// Frames chosen to hit every layout rule.
fn cases() -> Vec<(Codec, Vec<u8>)> {
    let mut idr = vec![0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0xda, 0x01];
    idr.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80]);
    idr.extend_from_slice(&[0, 0, 1, 0x65, 0x88, 0x84, 0x21, 0x7f, 0x55, 0xaa, 0x13]);
    let sei_slice = vec![0, 0, 1, 0x06, 5, 4, 3, 0, 0, 1, 0x41, 0xe0, 9, 9, 9, 9];
    vec![
        (Codec::Opus, Vec::new()),
        (Codec::Opus, b"\xfc\xff\xfe opus frame".to_vec()),
        (
            Codec::Vp8,
            [
                &[0x10u8, 0x02, 0x00, 0x9d, 0x01, 0x2a, 0x80, 0x02, 0xe0, 0x01][..],
                &[7u8; 24],
            ]
            .concat(),
        ),
        (Codec::Vp8, [&[0x31u8, 0x02, 0x00][..], &[9u8; 16]].concat()),
        (Codec::H264, idr),
        (Codec::H264, sei_slice),
    ]
}

fn build() -> Value {
    let secret_bytes: [u8; 16] = *b"sideroom-e2ee-v1";
    let iid = InstanceId([0x11, 0x22, 0x33, 0x44]);
    let secret = SenderSecret::new(secret_bytes);
    let mut frames = Vec::new();
    for (codec, plain) in cases() {
        // A fresh encryptor per vector: the counter is always 1.
        let mut tx = FrameEncryptor::new(&secret, iid).unwrap();
        let enc = tx.encrypt(codec, &plain).unwrap();
        frames.push(json!({
            "codec": codec_name(codec),
            "plaintext": hex(&plain),
            "ciphertext": hex(&enc),
        }));
    }
    json!({
        "version": 1,
        "secret": hex(&secret_bytes),
        "instance": hex(&iid.0),
        "frames": frames,
        "pairwise": [{
            "user_a": "175928847299117063",
            "key_a": hex(b"identity key a"),
            "user_b": "42",
            "key_b": hex(b"identity key b"),
            "code": pairwise_code(175_928_847_299_117_063, b"identity key a", 42, b"identity key b"),
        }],
        "call": [{
            "epoch_authenticator": hex(b"epoch authenticator"),
            "code": call_code(b"epoch authenticator"),
        }],
        "session_cert": [{
            "user_id": "175928847299117063",
            "session_key": hex(b"session public key"),
            "message": hex(&session_cert_message(175_928_847_299_117_063, b"session public key")),
        }],
    })
}

#[test]
fn vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors.json");
    let fresh = build();
    if std::env::var("SR_UPDATE_E2EE_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, serde_json::to_string_pretty(&fresh).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored, fresh, "vectors.json is out of date");

    // And they decrypt.
    let secret = SenderSecret::from_slice(&unhex(stored["secret"].as_str().unwrap())).unwrap();
    let iid = InstanceId(
        unhex(stored["instance"].as_str().unwrap())
            .try_into()
            .unwrap(),
    );
    let rx = FrameDecryptor::new(&secret, iid);
    for f in stored["frames"].as_array().unwrap() {
        let codec = Codec::from_mime(f["codec"].as_str().unwrap()).unwrap();
        let ct = unhex(f["ciphertext"].as_str().unwrap());
        assert_eq!(
            rx.decrypt(codec, &ct).unwrap(),
            unhex(f["plaintext"].as_str().unwrap())
        );
    }
}
