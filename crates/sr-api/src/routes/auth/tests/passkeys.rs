//! Passkey endpoint tests with a software ES256 authenticator (ENT-AUTH-010).

use super::*;
use ciborium::value::{Integer, Value as Cbor};
use data_encoding::BASE64URL_NOPAD as B64;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use sha2::{Digest, Sha256};

const ORIGIN: &str = "http://localhost:8080";

/// A throwaway authenticator holding one credential.
struct Soft {
    key: SigningKey,
    id: Vec<u8>,
    counter: u32,
    handle: Vec<u8>,
}

fn int(i: i64) -> Cbor {
    Cbor::Integer(Integer::from(i))
}

fn cbor(v: &Cbor) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(v, &mut out).unwrap();
    out
}

fn dec(v: &Value) -> Vec<u8> {
    B64.decode(v.as_str().unwrap().as_bytes()).unwrap()
}

impl Soft {
    fn new(seed: u8) -> Self {
        Self {
            key: SigningKey::from_slice(&[seed; 32]).unwrap(),
            id: vec![seed; 24],
            counter: 0,
            handle: Vec::new(),
        }
    }

    fn client_data(kind: &str, options: &Value, origin: &str) -> Vec<u8> {
        json!({
            "type": kind,
            "challenge": options["challenge"],
            "origin": origin,
            "crossOrigin": false,
        })
        .to_string()
        .into_bytes()
    }

    /// `navigator.credentials.create` against a begin response.
    fn create(&mut self, options: &Value, origin: &str) -> Value {
        self.handle = dec(&options["user"]["id"]);
        let point = self.key.verifying_key().to_sec1_point(false);
        let cose = cbor(&Cbor::Map(vec![
            (int(1), int(2)),
            (int(3), int(-7)),
            (int(-1), int(1)),
            (int(-2), Cbor::Bytes(point.x().unwrap().to_vec())),
            (int(-3), Cbor::Bytes(point.y().unwrap().to_vec())),
        ]));
        let rp_id = options["rp"]["id"].as_str().unwrap();
        let mut ad = Sha256::digest(rp_id.as_bytes()).to_vec();
        ad.push(0x45); // UP | UV | AT
        ad.extend_from_slice(&self.counter.to_be_bytes());
        ad.extend_from_slice(&[0u8; 16]);
        ad.extend_from_slice(&u16::try_from(self.id.len()).unwrap().to_be_bytes());
        ad.extend_from_slice(&self.id);
        ad.extend_from_slice(&cose);
        let att = cbor(&Cbor::Map(vec![
            (Cbor::Text("fmt".into()), Cbor::Text("none".into())),
            (Cbor::Text("attStmt".into()), Cbor::Map(vec![])),
            (Cbor::Text("authData".into()), Cbor::Bytes(ad)),
        ]));
        json!({
            "rawId": B64.encode(&self.id),
            "response": {
                "clientDataJSON": B64.encode(&Self::client_data("webauthn.create", options, origin)),
                "attestationObject": B64.encode(&att),
            },
        })
    }

    /// `navigator.credentials.get`; the counter moves to `counter`.
    fn get_with(&mut self, options: &Value, origin: &str, counter: u32, uv: bool) -> Value {
        self.counter = counter;
        let rp_id = options["rpId"].as_str().unwrap();
        let mut ad = Sha256::digest(rp_id.as_bytes()).to_vec();
        ad.push(if uv { 0x05 } else { 0x01 });
        ad.extend_from_slice(&counter.to_be_bytes());
        let cd = Self::client_data("webauthn.get", options, origin);
        let mut msg = ad.clone();
        msg.extend_from_slice(&Sha256::digest(&cd));
        let sig: Signature = self.key.sign(&msg);
        json!({
            "rawId": B64.encode(&self.id),
            "response": {
                "clientDataJSON": B64.encode(&cd),
                "authenticatorData": B64.encode(&ad),
                "signature": B64.encode(sig.to_der().as_bytes()),
                "userHandle": B64.encode(&self.handle),
            },
        })
    }

    fn get(&mut self, options: &Value) -> Value {
        let c = self.counter + 1;
        self.get_with(options, ORIGIN, c, true)
    }
}

async fn reg_begin(h: &H, s: &str, password: &str) -> Resp {
    h.post(
        "/api/v1/sr/auth/passkeys/register/begin",
        json!({ "password": password }),
    )
    .session(s)
    .send()
    .await
}

/// Register `auth` for the session's account; returns the finish response.
async fn register(h: &H, s: &str, auth: &mut Soft, name: &str) -> Resp {
    let b = reg_begin(h, s, PW).await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.body);
    let cred = auth.create(&b.body["options"], ORIGIN);
    h.post(
        "/api/v1/sr/auth/passkeys/register/finish",
        json!({"ceremony_id": b.body["ceremony_id"], "name": name, "credential": cred}),
    )
    .session(s)
    .send()
    .await
}

async fn login_begin(h: &H) -> Resp {
    h.post("/api/v1/sr/auth/passkeys/login/begin", json!({}))
        .send()
        .await
}

async fn passkey_login(h: &H, auth: &mut Soft) -> Resp {
    let b = login_begin(h).await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.body);
    let cred = auth.get(&b.body["options"]);
    h.post(
        "/api/v1/sr/auth/passkeys/login/finish",
        json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
    )
    .send()
    .await
}

fn alice(h: &H) {
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
}

async fn finish_login(h: &H, b: &Resp, cred: Value) -> Resp {
    h.post(
        "/api/v1/sr/auth/passkeys/login/finish",
        json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
    )
    .send()
    .await
}

#[tokio::test]
async fn register_list_login_rename_and_remove() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let mut auth = Soft::new(7);

    let r = register(&h, &s, &mut auth, "  Work   laptop ").await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["passkey"]["name"], "Work laptop");
    // The first factor with no recovery codes yet issues a batch.
    assert_eq!(r.body["recovery_codes"].as_array().unwrap().len(), 10);
    let id = r.body["passkey"]["id"].as_str().unwrap().to_owned();

    let list = h.get("/api/v1/sr/auth/passkeys").session(&s).send().await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body.as_array().unwrap().len(), 1);
    assert!(list.body[0]["last_used_at"].is_null());

    // Passkey-only login: no password, no second step.
    let l = passkey_login(&h, &mut auth).await;
    assert_eq!(l.status, StatusCode::OK, "{:?}", l.body);
    assert_eq!(l.body["status"], "ok");
    let new_session = l.cookie("sr_session").expect("session cookie");
    let me = h
        .get("/api/v1/sr/auth/passkeys")
        .session(&new_session)
        .send()
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert!(!me.body[0]["last_used_at"].is_null());

    // Rename and validation.
    let uri = format!("/api/v1/sr/auth/passkeys/{id}");
    let ren = h
        .call(Method::PATCH, &uri)
        .json(json!({"name": "Phone"}))
        .session(&s)
        .send()
        .await;
    assert_eq!(ren.status, StatusCode::OK, "{:?}", ren.body);
    assert_eq!(ren.body["name"], "Phone");
    for bad in ["   ".to_owned(), "x".repeat(65)] {
        let r = h
            .call(Method::PATCH, &uri)
            .json(json!({ "name": bad }))
            .session(&s)
            .send()
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }

    // Removal needs the password.
    let wrong = h
        .call(Method::DELETE, &uri)
        .json(json!({"password": "nope nope nope"}))
        .session(&s)
        .send()
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let still = h.get("/api/v1/sr/auth/passkeys").session(&s).send().await;
    assert_eq!(still.body.as_array().unwrap().len(), 1);
    let del = h
        .call(Method::DELETE, &uri)
        .json(json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let gone = passkey_login(&h, &mut auth).await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED);
    // The last second factor is gone, so are the recovery codes.
    assert!(h.store.recovery_list_unused(1).await.unwrap().is_empty());
    let actions: Vec<String> = h.store.audit_rows().into_iter().map(|r| r.1).collect();
    assert!(actions.contains(&"passkey.register".to_owned()));
    assert!(actions.contains(&"passkey.delete".to_owned()));
}

#[tokio::test]
async fn passkeys_belong_to_their_account() {
    let h = harness();
    alice(&h);
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let sa = h.login("alice").await;
    let sb = h.login("bob").await;
    let mut a = Soft::new(60);
    let r = register(&h, &sa, &mut a, "a").await;
    let id = r.body["passkey"]["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/v1/sr/auth/passkeys/{id}");
    let ren = h
        .call(Method::PATCH, &uri)
        .json(json!({"name": "mine now"}))
        .session(&sb)
        .send()
        .await;
    assert_eq!(ren.status, StatusCode::NOT_FOUND);
    let del = h
        .call(Method::DELETE, &uri)
        .json(json!({"password": PW}))
        .session(&sb)
        .send()
        .await;
    assert_eq!(del.status, StatusCode::NOT_FOUND);
    let list = h.get("/api/v1/sr/auth/passkeys").session(&sb).send().await;
    assert!(list.body.as_array().unwrap().is_empty());
    let list = h.get("/api/v1/sr/auth/passkeys").session(&sa).send().await;
    assert_eq!(list.body[0]["name"], "a");
}

#[tokio::test]
async fn registration_needs_a_session_the_password_and_a_fresh_ceremony() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let s2 = h.login("alice").await;

    // No session.
    let anon = h
        .post(
            "/api/v1/sr/auth/passkeys/register/begin",
            json!({"password": PW}),
        )
        .send()
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    // Session without CSRF.
    let nocsrf = h
        .post(
            "/api/v1/sr/auth/passkeys/register/begin",
            json!({"password": PW}),
        )
        .session_no_csrf(&s)
        .send()
        .await;
    assert_eq!(nocsrf.status, StatusCode::FORBIDDEN);
    // Wrong password.
    assert_eq!(
        reg_begin(&h, &s, "wrong wrong wrong").await.status,
        StatusCode::FORBIDDEN
    );

    // A ceremony cannot be finished twice.
    let mut auth = Soft::new(9);
    let b = reg_begin(&h, &s, PW).await;
    let cred = auth.create(&b.body["options"], ORIGIN);
    let body = json!({"ceremony_id": b.body["ceremony_id"], "credential": cred});
    let url = "/api/v1/sr/auth/passkeys/register/finish";
    let ok = h.post(url, body.clone()).session(&s).send().await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    assert_eq!(ok.body["passkey"]["name"], "Passkey");
    let replay = h.post(url, body.clone()).session(&s).send().await;
    assert_eq!(replay.status, StatusCode::BAD_REQUEST);

    // Another session of the same account cannot finish it either.
    let mut other = Soft::new(10);
    let b = reg_begin(&h, &s, PW).await;
    let cred = other.create(&b.body["options"], ORIGIN);
    let r = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
        )
        .session(&s2)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // The same authenticator cannot be registered twice.
    let b = reg_begin(&h, &s, PW).await;
    assert_eq!(
        b.body["options"]["excludeCredentials"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let cred = auth.create(&b.body["options"], ORIGIN);
    let dup = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(dup.status, StatusCode::BAD_REQUEST);
    assert!(dup.body["message"].as_str().unwrap().contains("already"));
}

#[tokio::test]
async fn registration_refuses_wrong_origin_attestation_and_names() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let url = "/api/v1/sr/auth/passkeys/register/finish";
    let mut auth = Soft::new(11);

    let b = reg_begin(&h, &s, PW).await;
    let cred = auth.create(&b.body["options"], "https://evil.example");
    let r = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    let b = reg_begin(&h, &s, PW).await;
    let cred = auth.create(&b.body["options"], ORIGIN);
    let r = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "name": " ", "credential": cred}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    let b = reg_begin(&h, &s, PW).await;
    let mut cred = auth.create(&b.body["options"], ORIGIN);
    cred["response"]["attestationObject"] = json!("AAAA");
    let r = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "credential": cred}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    let b = reg_begin(&h, &s, PW).await;
    let junk = json!({"rawId": "!!", "response": {"clientDataJSON": "", "attestationObject": ""}});
    let r = h
        .post(
            url,
            json!({"ceremony_id": b.body["ceremony_id"], "credential": junk}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    // Unknown ceremony.
    let r = h
        .post(url, json!({"ceremony_id": "nope", "credential": junk}))
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(
        h.get("/api/v1/sr/auth/passkeys")
            .session(&s)
            .send()
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn passkey_login_negative_cases() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let mut auth = Soft::new(12);
    assert_eq!(
        register(&h, &s, &mut auth, "k").await.status,
        StatusCode::OK
    );

    // Unregistered authenticator.
    let mut stranger = Soft::new(13);
    stranger.handle = auth.handle.clone();
    assert_eq!(
        passkey_login(&h, &mut stranger).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Wrong origin.
    let b = login_begin(&h).await;
    let c = auth.get_with(&b.body["options"], "https://evil.example", 10, true);
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // User verification missing (primary login requires it).
    let b = login_begin(&h).await;
    let c = auth.get_with(&b.body["options"], ORIGIN, 11, false);
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Tampered signature.
    let b = login_begin(&h).await;
    let mut c = auth.get_with(&b.body["options"], ORIGIN, 12, true);
    let mut sig = dec(&c["response"]["signature"]);
    let last = sig.len() - 1;
    sig[last] ^= 1;
    c["response"]["signature"] = json!(B64.encode(&sig));
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Missing user handle (not a discoverable response).
    let b = login_begin(&h).await;
    let mut c = auth.get_with(&b.body["options"], ORIGIN, 13, true);
    c["response"]["userHandle"] = Value::Null;
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Someone else's user handle.
    let b = login_begin(&h).await;
    let mut c = auth.get_with(&b.body["options"], ORIGIN, 14, true);
    c["response"]["userHandle"] = json!(B64.encode(&[1u8; 32]));
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // A ceremony is single use, valid or not, and expires.
    let b = login_begin(&h).await;
    let c = auth.get_with(&b.body["options"], ORIGIN, 20, true);
    assert_eq!(finish_login(&h, &b, c.clone()).await.status, StatusCode::OK);
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );
    let b = login_begin(&h).await;
    let c = auth.get_with(&b.body["options"], ORIGIN, 21, true);
    h.clock.advance_ms(6 * 60_000);
    assert_eq!(
        finish_login(&h, &b, c).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Blocked accounts get their state only after a valid assertion.
    h.store.set_state(1, AccountState::Deactivated);
    let l = passkey_login(&h, &mut auth).await;
    assert_eq!(l.status, StatusCode::FORBIDDEN, "{:?}", l.body);
}

#[tokio::test]
async fn counter_regression_is_an_audit_event_and_a_failed_login() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let mut auth = Soft::new(14);
    assert_eq!(
        register(&h, &s, &mut auth, "k").await.status,
        StatusCode::OK
    );

    let b = login_begin(&h).await;
    let c = auth.get_with(&b.body["options"], ORIGIN, 5, true);
    assert_eq!(finish_login(&h, &b, c).await.status, StatusCode::OK);

    // A clone replays with a counter that did not advance.
    for counter in [5, 3] {
        let b = login_begin(&h).await;
        let c = auth.get_with(&b.body["options"], ORIGIN, counter, true);
        assert_eq!(
            finish_login(&h, &b, c).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let regressions = h
        .store
        .audit_rows()
        .into_iter()
        .filter(|r| r.1 == "passkey.counter_regression")
        .count();
    assert_eq!(regressions, 2);
    // The stored counter did not move backwards.
    let stored = h.store.passkey_list(1).await.unwrap();
    assert_eq!(stored[0].sign_count, 5);
}

#[tokio::test]
async fn passkeys_are_a_second_factor_on_password_login() {
    let h = harness();
    alice(&h);
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let mut auth = Soft::new(15);
    assert_eq!(
        register(&h, &s, &mut auth, "k").await.status,
        StatusCode::OK
    );

    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(r.body["status"], "mfa_required");
    assert_eq!(r.body["methods"], json!(["passkey", "recovery"]));
    assert!(r.cookie("sr_session").is_none());
    let mfa = r.cookie("sr_mfa").unwrap();

    // The second-factor ceremony needs the pending cookie.
    let no_cookie = h
        .post(
            "/api/v1/sr/auth/passkeys/login/begin",
            json!({"second_factor": true}),
        )
        .send()
        .await;
    assert_eq!(no_cookie.status, StatusCode::UNAUTHORIZED);

    let b = h
        .post(
            "/api/v1/sr/auth/passkeys/login/begin",
            json!({"second_factor": true}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.body);
    assert_eq!(
        b.body["options"]["allowCredentials"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // A primary-login ceremony cannot satisfy the second factor.
    let primary = login_begin(&h).await;
    let c = auth.get(&primary.body["options"]);
    let code = json!({"ceremony_id": primary.body["ceremony_id"], "credential": c}).to_string();
    let bad = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "passkey", "code": code}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);

    let c = auth.get(&b.body["options"]);
    let code = json!({"ceremony_id": b.body["ceremony_id"], "credential": c}).to_string();
    let ok = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "passkey", "code": code, "trust_device": false}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    assert!(ok.cookie("sr_session").is_some());
    // The ceremony was spent.
    let again = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "passkey", "code": code}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);

    // Bob has no passkey, so no second factor is asked.
    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "bob", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(r.body["status"], "ok");
}

#[tokio::test]
async fn a_key_cannot_answer_another_accounts_second_factor() {
    let h = harness();
    alice(&h);
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let sa = h.login("alice").await;
    let mut a = Soft::new(16);
    register(&h, &sa, &mut a, "a").await;
    let sb = h.login("bob").await;
    let mut b = Soft::new(17);
    register(&h, &sb, &mut b, "b").await;

    // Bob's password login; Alice's key answers a ceremony begun for Bob.
    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "bob", "password": PW}),
        )
        .send()
        .await;
    let mfa = r.cookie("sr_mfa").unwrap();
    let begin = h
        .post(
            "/api/v1/sr/auth/passkeys/login/begin",
            json!({"second_factor": true}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    let c = a.get(&begin.body["options"]);
    let code = json!({"ceremony_id": begin.body["ceremony_id"], "credential": c}).to_string();
    let r = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "passkey", "code": code}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn email_change_accepts_a_passkey_as_the_second_factor() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    let mut auth = Soft::new(18);
    register(&h, &s, &mut auth, "k").await;

    // No proof: refused.
    let none = change_to(&h, &s, "new@x.test", None).await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
    assert_eq!(none.body["errors"]["mfa"]["_errors"][0]["code"], "required");

    let b = h
        .post("/api/v1/sr/auth/passkeys/verify/begin", json!({}))
        .session(&s)
        .send()
        .await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.body);
    let c = auth.get(&b.body["options"]);
    let code = json!({"ceremony_id": b.body["ceremony_id"], "credential": c}).to_string();
    let ok = change_to(
        &h,
        &s,
        "new@x.test",
        Some(json!({"method": "passkey", "code": code})),
    )
    .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.body);

    // verify/begin refuses an account without passkeys.
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let sb = h.login("bob").await;
    let r = h
        .post("/api/v1/sr/auth/passkeys/verify/begin", json!({}))
        .session(&sb)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn removing_totp_keeps_recovery_codes_while_passkeys_remain() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    enrol_totp(&h, &s).await;
    let mut auth = Soft::new(19);
    let r = register(&h, &s, &mut auth, "k").await;
    // Recovery codes already exist: no new batch is shown.
    assert!(r.body["recovery_codes"].is_null());
    let del = h
        .call(Method::DELETE, "/api/v1/sr/auth/totp")
        .json(json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert_eq!(h.store.recovery_list_unused(1).await.unwrap().len(), 10);
}

#[tokio::test]
async fn begin_endpoints_are_rate_limited() {
    let h = harness();
    alice(&h);
    for i in 0..60 {
        assert_eq!(login_begin(&h).await.status, StatusCode::OK, "{i}");
    }
    let r = login_begin(&h).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers.contains_key(header::RETRY_AFTER));
    // Another address is unaffected; time heals.
    assert_eq!(
        h.post("/api/v1/sr/auth/passkeys/login/begin", json!({}))
            .ip("203.0.113.9")
            .send()
            .await
            .status,
        StatusCode::OK
    );
    h.clock.advance_ms(61_000);
    assert_eq!(login_begin(&h).await.status, StatusCode::OK);

    let s = h.login("alice").await;
    for _ in 0..10 {
        reg_begin(&h, &s, PW).await;
    }
    assert_eq!(
        reg_begin(&h, &s, PW).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn an_account_holds_at_most_twenty_passkeys() {
    let h = harness();
    alice(&h);
    let s = h.login("alice").await;
    for i in 0..20u8 {
        // Stay below the 10-per-10-minutes begin limit.
        if i % 5 == 0 {
            h.clock.advance_ms(11 * 60_000);
        }
        let mut a = Soft::new(i + 40);
        let r = register(&h, &s, &mut a, "k").await;
        assert_eq!(r.status, StatusCode::OK, "{i}: {:?}", r.body);
    }
    h.clock.advance_ms(11 * 60_000);
    let b = reg_begin(&h, &s, PW).await;
    assert_eq!(b.status, StatusCode::BAD_REQUEST);
}

#[test]
fn rp_id_comes_from_the_public_url() {
    let sessions = Arc::new(MemorySessions::new());
    let store = Arc::new(MemoryAuthStore::new(sessions));
    let ids: super::super::IdSource = Arc::new(|| 1);
    let svc = AuthService::new(store, Arc::new(MemoryMailer::new()), ids).with_config(AuthConfig {
        public_url: "https://Chat.Example.com:8443/".into(),
        webauthn_origins: vec!["https://alt.example.com/".into()],
        ..AuthConfig::default()
    });
    let rp = svc.relying_party().unwrap();
    assert_eq!(rp.id, "chat.example.com");
    assert_eq!(
        rp.origins,
        vec!["https://chat.example.com:8443", "https://alt.example.com"]
    );
}
