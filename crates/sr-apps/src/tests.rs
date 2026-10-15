//! Service-level tests over the in-memory repository: every denial and every state
//! transition the spec promises.

use crate::{error::AppsError, memory::MemoryAppsRepo, model::*, service::*};
use data_encoding::BASE64URL_NOPAD;
use serde_json::json;
use sha2::{Digest, Sha256};
use sr_auth::oauth::{Scope, pkce_verify};
use sr_core::{ManualClock, Permissions};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

const T0: i64 = 1_800_000_000_000;
const OWNER: i64 = 1;
const OTHER: i64 = 2;

struct Env {
    svc: AppsService,
    repo: Arc<MemoryAppsRepo>,
    clock: Arc<ManualClock>,
}

fn env() -> Env {
    let repo = Arc::new(MemoryAppsRepo::new());
    repo.add_account(OWNER, "owner");
    repo.add_account(OTHER, "other");
    let clock = Arc::new(ManualClock::new(T0));
    let counter = Arc::new(AtomicI64::new(1000));
    let svc = AppsService::new(
        repo.clone(),
        clock.clone(),
        Arc::new(move || counter.fetch_add(1, Ordering::SeqCst)),
    );
    Env { svc, repo, clock }
}

async fn app(e: &Env) -> Created {
    e.svc
        .create_application(OWNER, "Helper Bot", "does help")
        .await
        .unwrap()
}

fn code_of<T>(r: Result<T, crate::OAuthError>) -> Option<&'static str> {
    r.err().map(|e| e.error)
}

#[tokio::test]
async fn creating_an_application_makes_a_bot_and_shows_secrets_once() {
    let e = env();
    let c = app(&e).await;
    assert!(!c.client_secret.is_empty() && !c.bot_token.is_empty());
    assert_eq!(c.application.release_state, ReleaseState::Development);
    assert!(!c.application.bot_public);
    let rec = e.svc.authenticate_bot(&c.bot_token).await.unwrap().unwrap();
    assert_eq!(rec.user_id, c.application.bot_user_id);
    assert_eq!(e.repo.account_active(c.application.bot_user_id), Some(true));
    // junk and tampering are refused
    assert!(e.svc.authenticate_bot("nope").await.unwrap().is_none());
    let mut parts: Vec<&str> = c.bot_token.split('.').collect();
    parts[2] = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    assert!(
        e.svc
            .authenticate_bot(&parts.join("."))
            .await
            .unwrap()
            .is_none()
    );
    // a token naming another account does not match any digest either
    let other = sr_auth::tokens::BotToken::generate(c.application.bot_user_id + 1, 5).unwrap();
    assert!(
        e.svc
            .authenticate_bot(other.expose())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn name_and_limit_validation() {
    let e = env();
    assert!(matches!(
        e.svc.create_application(OWNER, "x", "").await,
        Err(AppsError::Invalid(_))
    ));
    assert!(matches!(
        e.svc
            .create_application(OWNER, "ok name", &"d".repeat(401))
            .await,
        Err(AppsError::Invalid(_))
    ));
    for i in 0..MAX_APPLICATIONS {
        e.svc
            .create_application(OWNER, &format!("bot {i}"), "")
            .await
            .unwrap();
    }
    assert!(matches!(
        e.svc.create_application(OWNER, "one more", "").await,
        Err(AppsError::Invalid(_))
    ));
    assert!(e.svc.create_application(999, "ghost", "").await.is_err());
}

#[tokio::test]
async fn only_the_owner_sees_or_changes_an_application() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    assert!(e.svc.owned(OTHER, id).await.is_err());
    let nf = Err(AppsError::NotFound("application"));
    assert_eq!(
        e.svc
            .update_application(OTHER, id, UpdateInput::default())
            .await
            .map(|_| ()),
        nf
    );
    assert_eq!(e.svc.reset_secret(OTHER, id).await.map(|_| ()), nf);
    assert_eq!(e.svc.reset_bot_token(OTHER, id).await.map(|_| ()), nf);
    assert_eq!(e.svc.delete_application(OTHER, id).await.map(|_| ()), nf);
    assert_eq!(e.svc.audit_log(OTHER, id, 10, None).await.map(|_| ()), nf);
    assert_eq!(e.svc.submit_version(OTHER, id, None).await.map(|_| ()), nf);
    assert_eq!(e.svc.rename_bot(OTHER, id, "evil").await, nf);
    assert!(
        e.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn resets_revoke_the_old_credentials_and_are_audited() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    let new_token = e.svc.reset_bot_token(OWNER, id).await.unwrap();
    assert!(
        e.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(e.svc.authenticate_bot(&new_token).await.unwrap().is_some());
    let secret = e.svc.reset_secret(OWNER, id).await.unwrap();
    assert_ne!(secret, c.client_secret);
    let creds = |s: &str| ClientCredentials {
        client_id: id.to_string(),
        secret: Some(s.to_owned()),
    };
    assert!(
        e.svc
            .authenticate_client(&creds(&c.client_secret), true)
            .await
            .is_err()
    );
    assert!(
        e.svc
            .authenticate_client(&creds(&secret), true)
            .await
            .is_ok()
    );
    let log = e.svc.audit_log(OWNER, id, 50, None).await.unwrap();
    let actions: Vec<_> = log.iter().map(|r| r.action.as_str()).collect();
    assert_eq!(
        actions,
        vec!["app.secret_reset", "app.token_reset", "app.create"]
    );
    let dump: String = log.iter().map(|r| r.details.to_string()).collect();
    assert!(
        !dump.contains(&secret) && !dump.contains(&new_token) && !dump.contains(&c.client_secret)
    );
}

#[tokio::test]
async fn patch_validates_every_field() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    let bad = |input: UpdateInput| {
        let svc = e.svc.clone();
        async move {
            matches!(
                svc.update_application(OWNER, id, input).await,
                Err(AppsError::Invalid(_))
            )
        }
    };
    assert!(
        bad(UpdateInput {
            name: Some("a".into()),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            tags: Some(vec!["t".into(); 6]),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            redirect_uris: Some(vec!["http://evil.test/cb".into()]),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            redirect_uris: Some(vec!["javascript:alert(1)".into()]),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            privileged_intents: Some(vec!["GUILDS".into()]),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            terms_of_service_url: Some(Some("http://insecure.test".into())),
            ..Default::default()
        })
        .await
    );
    assert!(
        bad(UpdateInput {
            install_params: Some(InstallParamsInput {
                scopes: vec!["identify".into()],
                permissions: None
            }),
            ..Default::default()
        })
        .await
    );
    let ok = e
        .svc
        .update_application(
            OWNER,
            id,
            UpdateInput {
                name: Some("Renamed".into()),
                redirect_uris: Some(vec![
                    "https://a.test/cb".into(),
                    "http://localhost:3000/cb".into(),
                    "https://a.test/cb".into(),
                ]),
                privileged_intents: Some(vec!["MESSAGE_CONTENT".into()]),
                bot_public: Some(true),
                privacy_policy_url: Some(Some("https://a.test/privacy".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(ok.name, "Renamed");
    assert_eq!(ok.redirect_uris.len(), 2, "duplicates are dropped");
    assert_eq!(ok.intents_requested, intents::MESSAGE_CONTENT);
    assert!(ok.bot_public);
}

fn policy_url() -> UpdateInput {
    UpdateInput {
        privacy_policy_url: Some(Some("https://a.test/p".into())),
        ..Default::default()
    }
}

#[tokio::test]
async fn privileged_intents_follow_the_release_state() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    let a = e
        .svc
        .update_application(
            OWNER,
            id,
            UpdateInput {
                privileged_intents: Some(vec!["MESSAGE_CONTENT".into(), "GUILD_MEMBERS".into()]),
                ..policy_url()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        a.allowed_intents(),
        intents::MESSAGE_CONTENT | intents::GUILD_MEMBERS
    );
    let gw = e.svc.gateway_bot(&c.bot_token).await.unwrap().unwrap();
    assert_eq!(
        gw.allowed_intents,
        intents::MESSAGE_CONTENT | intents::GUILD_MEMBERS
    );

    let v = e
        .svc
        .submit_version(OWNER, id, Some("please"))
        .await
        .unwrap();
    assert_eq!(v.needs_review_reasons, vec!["first_release"]);
    assert_eq!(
        e.svc.application(id).await.unwrap().release_state,
        ReleaseState::InReview
    );
    assert!(matches!(
        e.svc.submit_version(OWNER, id, None).await,
        Err(AppsError::Conflict(_))
    ));
    // staff approve only what was requested
    assert!(
        e.svc
            .decide_version(
                9,
                v.id,
                VersionDecision::Approve,
                None,
                &["GUILD_PRESENCES".into()]
            )
            .await
            .is_err()
    );
    assert!(
        e.svc
            .decide_version(9, v.id, VersionDecision::Reject, None, &[])
            .await
            .is_err()
    );
    let (v, a) = e
        .svc
        .decide_version(
            9,
            v.id,
            VersionDecision::Approve,
            None,
            &["MESSAGE_CONTENT".into()],
        )
        .await
        .unwrap();
    assert_eq!(v.state, "approved");
    assert_eq!(a.release_state, ReleaseState::Released);
    assert_eq!(a.allowed_intents(), intents::MESSAGE_CONTENT);
    let gw = e.svc.gateway_bot(&c.bot_token).await.unwrap().unwrap();
    assert_eq!(gw.allowed_intents, intents::MESSAGE_CONTENT);
    assert!(matches!(
        e.svc
            .decide_version(9, v.id, VersionDecision::Reject, Some("x"), &[])
            .await,
        Err(AppsError::Conflict(_))
    ));
    // a later version adding an intent is flagged
    e.svc
        .update_application(
            OWNER,
            id,
            UpdateInput {
                privileged_intents: Some(vec!["MESSAGE_CONTENT".into(), "GUILD_PRESENCES".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let v2 = e.svc.submit_version(OWNER, id, None).await.unwrap();
    assert_eq!(v2.needs_review_reasons, vec!["intents"]);
    let gw = e.svc.gateway_bot(&c.bot_token).await.unwrap().unwrap();
    assert_eq!(gw.application.release_state, ReleaseState::InReview);
    let (_, a) = e
        .svc
        .decide_version(
            9,
            v2.id,
            VersionDecision::RequestChanges,
            Some("why presences?"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(a.release_state, ReleaseState::Released);
    assert_eq!(a.allowed_intents(), intents::MESSAGE_CONTENT);
}

#[tokio::test]
async fn submitting_needs_a_privacy_policy_and_withdrawing_returns_to_development() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    assert!(matches!(
        e.svc.submit_version(OWNER, id, None).await,
        Err(AppsError::Invalid(_))
    ));
    e.svc
        .update_application(OWNER, id, policy_url())
        .await
        .unwrap();
    let v = e.svc.submit_version(OWNER, id, None).await.unwrap();
    assert!(e.svc.withdraw_version(OTHER, id, v.id).await.is_err());
    let v = e.svc.withdraw_version(OWNER, id, v.id).await.unwrap();
    assert_eq!(v.state, "withdrawn");
    assert_eq!(
        e.svc.application(id).await.unwrap().release_state,
        ReleaseState::Development
    );
    let list = e.svc.versions_of(OWNER, id).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].1.is_empty());
}

#[tokio::test]
async fn suspension_switches_everything_off_and_back() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    assert!(e.svc.suspend(9, id, "").await.is_err());
    let a = e.svc.suspend(9, id, "abuse").await.unwrap();
    assert_eq!(a.release_state, ReleaseState::Suspended);
    assert!(
        e.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(e.svc.gateway_bot(&c.bot_token).await.unwrap().is_none());
    assert!(matches!(
        e.svc.submit_version(OWNER, id, None).await,
        Err(AppsError::Suspended)
    ));
    assert!(e.svc.suspend(9, id, "again").await.is_err());
    let creds = ClientCredentials {
        client_id: id.to_string(),
        secret: Some(c.client_secret.clone()),
    };
    assert!(e.svc.authenticate_client(&creds, true).await.is_err());
    let a = e.svc.unsuspend(9, id, None).await.unwrap();
    assert_eq!(a.release_state, ReleaseState::Development);
    assert!(
        e.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(e.svc.unsuspend(9, id, None).await.is_err());
}

#[tokio::test]
async fn developer_requests_flow() {
    let e = env();
    assert!(matches!(
        e.svc.apply(OTHER, "", "", true).await,
        Err(AppsError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.apply(OTHER, "a bot", "", false).await,
        Err(AppsError::Invalid(_))
    ));
    let r = e
        .svc
        .apply(OTHER, "a music bot", "none", true)
        .await
        .unwrap();
    assert_eq!(r.state, "pending");
    assert!(matches!(
        e.svc.apply(OTHER, "again", "", true).await,
        Err(AppsError::Conflict(_))
    ));
    assert!(!e.svc.developer_status(OTHER).await.unwrap().0);
    assert_eq!(
        e.svc
            .developer_requests("pending", 50, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        e.svc
            .decide_developer_request(9, r.id, false, None)
            .await
            .is_err()
    );
    let done = e
        .svc
        .decide_developer_request(9, r.id, true, None)
        .await
        .unwrap();
    assert_eq!(done.state, "approved");
    assert!(matches!(
        e.svc.decide_developer_request(9, r.id, true, None).await,
        Err(AppsError::Conflict(_))
    ));
    let (is_dev, last) = e.svc.developer_status(OTHER).await.unwrap();
    assert!(is_dev && last.is_some());
    assert!(matches!(
        e.svc.apply(OTHER, "more", "", true).await,
        Err(AppsError::Conflict(_))
    ));
    assert!(e.svc.developer_requests("bogus", 10, None).await.is_err());
}

fn install_req(
    app: &Created,
    installer: i64,
    guild: i64,
    perms: Permissions,
    want: u64,
) -> InstallRequest {
    InstallRequest {
        application_id: app.application.id,
        installer,
        installer_perms: perms,
        guild_id: guild,
        scopes: vec![Scope::Bot, Scope::ApplicationsCommands],
        requested_permissions: want,
    }
}

async fn release(e: &Env, c: &Created) {
    e.svc
        .update_application(OWNER, c.application.id, policy_url())
        .await
        .unwrap();
    let v = e
        .svc
        .submit_version(OWNER, c.application.id, None)
        .await
        .unwrap();
    e.svc
        .decide_version(9, v.id, VersionDecision::Approve, None, &[])
        .await
        .unwrap();
}

#[tokio::test]
async fn installs_follow_the_test_server_rules() {
    let e = env();
    let c = app(&e).await;
    e.repo.add_guild(50, OWNER, &[]);
    let big: Vec<i64> = (100..130).collect();
    e.repo.add_guild(51, OWNER, &big);
    e.repo.add_guild(52, OTHER, &[]);
    let out = e
        .svc
        .install(install_req(&c, OWNER, 50, Permissions::MANAGE_GUILD, 8))
        .await
        .unwrap();
    assert!(out.member_added);
    assert!(e.repo.is_member(50, c.application.bot_user_id));
    assert!(e.svc.is_installed(c.application.id, 50).await.unwrap());
    // reinstalling refreshes without adding the member twice
    let again = e
        .svc
        .install(install_req(&c, OWNER, 50, Permissions::MANAGE_GUILD, 0))
        .await
        .unwrap();
    assert!(!again.member_added);
    assert_eq!(
        e.svc
            .install(install_req(&c, OWNER, 51, Permissions::MANAGE_GUILD, 0))
            .await
            .err(),
        Some(AppsError::NotReleased)
    );
    assert_eq!(
        e.svc
            .install(install_req(&c, OTHER, 52, Permissions::MANAGE_GUILD, 0))
            .await
            .err(),
        Some(AppsError::NotReleased)
    );
    assert_eq!(
        e.svc
            .install(install_req(&c, OWNER, 52, Permissions::SEND_MESSAGES, 0))
            .await
            .err(),
        Some(AppsError::MissingPermissions)
    );
    assert!(
        e.svc
            .install(install_req(&c, OWNER, 999, Permissions::MANAGE_GUILD, 0))
            .await
            .is_err()
    );
    // kicked bot: no longer installed
    e.repo.remove_member(50, c.application.bot_user_id);
    assert!(!e.svc.is_installed(c.application.id, 50).await.unwrap());
}

#[tokio::test]
async fn a_bot_never_gets_more_than_the_installer_holds() {
    let e = env();
    let c = app(&e).await;
    e.svc
        .update_application(
            OWNER,
            c.application.id,
            UpdateInput {
                bot_public: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    release(&e, &c).await;
    e.repo.add_account(3, "third");
    e.repo.add_guild(54, 3, &[OTHER]);
    let want =
        (Permissions::SEND_MESSAGES | Permissions::MANAGE_ROLES | Permissions::ADMINISTRATOR)
            .bits();
    let out = e
        .svc
        .install(install_req(
            &c,
            OTHER,
            54,
            Permissions::MANAGE_GUILD | Permissions::SEND_MESSAGES,
            want,
        ))
        .await
        .unwrap();
    assert_eq!(out.install.permissions, Permissions::SEND_MESSAGES.bits());
    // the guild's owner may hand out anything
    let out = e
        .svc
        .install(install_req(&c, 3, 54, Permissions::empty(), want))
        .await
        .unwrap();
    assert_eq!(out.install.permissions, want);
}

#[tokio::test]
async fn private_bots_install_only_for_their_owner_even_when_released() {
    let e = env();
    let c = app(&e).await;
    release(&e, &c).await;
    e.repo.add_guild(60, OTHER, &[]);
    let r = e
        .svc
        .install(install_req(&c, OTHER, 60, Permissions::MANAGE_GUILD, 0))
        .await;
    assert!(matches!(r, Err(AppsError::Forbidden(_))), "{r:?}");
}

// ----------------------------------------------------------------------------- OAuth2

fn challenge_for(verifier: &str) -> String {
    BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()))
}

async fn oauth_app(e: &Env) -> (Created, ClientCredentials) {
    let c = app(e).await;
    e.svc
        .update_application(
            OWNER,
            c.application.id,
            UpdateInput {
                redirect_uris: Some(vec!["https://client.test/cb".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let creds = ClientCredentials {
        client_id: c.application.id.to_string(),
        secret: Some(c.client_secret.clone()),
    };
    (c, creds)
}

fn params(c: &Created, scope: &str) -> AuthorizeParams {
    AuthorizeParams {
        client_id: c.application.id.to_string(),
        scope: scope.into(),
        redirect_uri: Some("https://client.test/cb".into()),
        response_type: Some("code".into()),
        ..Default::default()
    }
}

const CB: &str = "https://client.test/cb";

#[test]
fn the_test_challenge_matches_the_rfc_vector() {
    let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        challenge_for(v),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert!(pkce_verify(v, &challenge_for(v)));
}

#[tokio::test]
async fn authorize_requests_are_validated_without_redirecting_on_errors() {
    let e = env();
    let (c, _) = oauth_app(&e).await;
    let ok = e
        .svc
        .validate_authorize(&params(&c, "identify guilds"))
        .await
        .unwrap();
    assert_eq!(ok.scopes, vec![Scope::Identify, Scope::Guilds]);
    let cases = [
        (
            "unknown client",
            AuthorizeParams {
                client_id: "12345".into(),
                ..params(&c, "identify")
            },
        ),
        (
            "bad client id",
            AuthorizeParams {
                client_id: "abc".into(),
                ..params(&c, "identify")
            },
        ),
        (
            "unregistered redirect",
            AuthorizeParams {
                redirect_uri: Some("https://evil.test/cb".into()),
                ..params(&c, "identify")
            },
        ),
        (
            "near-miss redirect",
            AuthorizeParams {
                redirect_uri: Some("https://client.test/cb/".into()),
                ..params(&c, "identify")
            },
        ),
        (
            "missing redirect",
            AuthorizeParams {
                redirect_uri: None,
                ..params(&c, "identify")
            },
        ),
        ("unknown scope", params(&c, "identify rpc")),
        ("unsupported scope", params(&c, "guilds.join")),
        ("empty scope", params(&c, "")),
        ("email without identify", params(&c, "email")),
        (
            "implicit grant",
            AuthorizeParams {
                response_type: Some("token".into()),
                ..params(&c, "identify")
            },
        ),
        (
            "plain pkce",
            AuthorizeParams {
                code_challenge: Some("x".repeat(43)),
                code_challenge_method: Some("plain".into()),
                ..params(&c, "identify")
            },
        ),
        (
            "short challenge",
            AuthorizeParams {
                code_challenge: Some("short".into()),
                ..params(&c, "identify")
            },
        ),
        (
            "bad permissions",
            AuthorizeParams {
                permissions: Some("abc".into()),
                ..params(&c, "bot")
            },
        ),
    ];
    for (what, p) in cases {
        assert!(e.svc.validate_authorize(&p).await.is_err(), "{what}");
    }
    let bot = e
        .svc
        .validate_authorize(&AuthorizeParams {
            redirect_uri: None,
            ..params(&c, "bot applications.commands")
        })
        .await
        .unwrap();
    assert!(bot.wants_bot() && !bot.wants_code());
    e.svc.suspend(9, c.application.id, "x").await.unwrap();
    assert_eq!(
        e.svc
            .validate_authorize(&params(&c, "identify"))
            .await
            .err(),
        Some(AppsError::Suspended)
    );
}

async fn code_for(
    e: &Env,
    c: &Created,
    user: i64,
    scope: &str,
    challenge: Option<String>,
) -> String {
    let mut p = params(c, scope);
    p.code_challenge = challenge;
    let v = e.svc.validate_authorize(&p).await.unwrap();
    e.svc.issue_code(user, &v).await.unwrap()
}

#[tokio::test]
async fn authorization_code_flow_with_pkce() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let fresh = || {
        code_for(
            &e,
            &c,
            OTHER,
            "identify email",
            Some(challenge_for(verifier)),
        )
    };
    // wrong verifier, missing verifier, redirect mismatch: all refused
    let code = fresh().await;
    assert_eq!(
        code_of(
            e.svc
                .exchange_code(&creds, &code, CB, Some(&"x".repeat(50)))
                .await
        ),
        Some("invalid_grant")
    );
    let code = fresh().await;
    assert_eq!(
        code_of(e.svc.exchange_code(&creds, &code, CB, None).await),
        Some("invalid_grant")
    );
    let code = fresh().await;
    assert!(
        e.svc
            .exchange_code(&creds, &code, "https://client.test/other", Some(verifier))
            .await
            .is_err()
    );
    // the happy path
    let code = fresh().await;
    let t = e
        .svc
        .exchange_code(&creds, &code, CB, Some(verifier))
        .await
        .unwrap();
    assert_eq!(t.scope, vec!["identify".to_owned(), "email".to_owned()]);
    assert_eq!(t.expires_in, ACCESS_TTL_SECS);
    let (rec, _) = e.svc.bearer(&t.access_token).await.unwrap().unwrap();
    assert_eq!((rec.user_id, rec.application_id), (OTHER, c.application.id));
    // replay: refused, and the first tokens die
    assert_eq!(
        code_of(e.svc.exchange_code(&creds, &code, CB, Some(verifier)).await),
        Some("invalid_grant")
    );
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_none());
}

#[tokio::test]
async fn client_authentication_rules() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let c2 = e.svc.create_application(OTHER, "Second", "").await.unwrap();
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    let bad = ClientCredentials {
        secret: Some("x".repeat(43)),
        ..creds.clone()
    };
    assert_eq!(
        code_of(e.svc.exchange_code(&bad, &code, CB, None).await),
        Some("invalid_client")
    );
    let swapped = ClientCredentials {
        secret: Some(c2.client_secret.clone()),
        ..creds.clone()
    };
    assert_eq!(
        code_of(e.svc.exchange_code(&swapped, &code, CB, None).await),
        Some("invalid_client")
    );
    // no secret and no PKCE challenge on the code
    let public = ClientCredentials {
        secret: None,
        ..creds.clone()
    };
    assert_eq!(
        code_of(e.svc.exchange_code(&public, &code, CB, None).await),
        Some("invalid_client")
    );
    // a code issued to one application never works for another
    let other_creds = ClientCredentials {
        client_id: c2.application.id.to_string(),
        secret: Some(c2.client_secret.clone()),
    };
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    assert_eq!(
        code_of(e.svc.exchange_code(&other_creds, &code, CB, None).await),
        Some("invalid_grant")
    );
    assert!(e.svc.exchange_code(&creds, "junk", CB, None).await.is_err());
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    e.clock.advance_ms(CODE_TTL_SECS * 1000 + 1);
    assert_eq!(
        code_of(e.svc.exchange_code(&creds, &code, CB, None).await),
        Some("invalid_grant")
    );
}

#[tokio::test]
async fn a_public_client_may_use_pkce_without_a_secret() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let verifier = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
    let code = code_for(&e, &c, OTHER, "identify", Some(challenge_for(verifier))).await;
    let public = ClientCredentials {
        secret: None,
        ..creds
    };
    let t = e
        .svc
        .exchange_code(&public, &code, CB, Some(verifier))
        .await
        .unwrap();
    assert_eq!(
        code_of(e.svc.refresh(&public, &t.refresh_token).await),
        Some("invalid_client")
    );
}

#[tokio::test]
async fn refresh_tokens_rotate_and_reuse_revokes_the_grant() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let code = code_for(&e, &c, OTHER, "identify guilds", None).await;
    let t1 = e.svc.exchange_code(&creds, &code, CB, None).await.unwrap();
    let t2 = e.svc.refresh(&creds, &t1.refresh_token).await.unwrap();
    assert_ne!(t1.access_token, t2.access_token);
    assert_eq!(t2.scope, t1.scope);
    assert!(e.svc.bearer(&t1.access_token).await.unwrap().is_none());
    assert!(e.svc.bearer(&t2.access_token).await.unwrap().is_some());
    // the spent refresh token is theft evidence: everything of the grant dies
    assert_eq!(
        code_of(e.svc.refresh(&creds, &t1.refresh_token).await),
        Some("invalid_grant")
    );
    assert!(e.svc.bearer(&t2.access_token).await.unwrap().is_none());
    assert_eq!(
        code_of(e.svc.refresh(&creds, &t2.refresh_token).await),
        Some("invalid_grant")
    );
    // another client cannot use the token, and trying does not revoke the real grant
    let c3 = e.svc.create_application(OTHER, "Third", "").await.unwrap();
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    let t = e.svc.exchange_code(&creds, &code, CB, None).await.unwrap();
    let foreign = ClientCredentials {
        client_id: c3.application.id.to_string(),
        secret: Some(c3.client_secret.clone()),
    };
    assert!(e.svc.refresh(&foreign, &t.refresh_token).await.is_err());
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_some());
    // expiry
    e.clock.advance_ms(ACCESS_TTL_SECS * 1000 + 1);
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_none());
    let fresh = e.svc.refresh(&creds, &t.refresh_token).await.unwrap();
    assert!(e.svc.bearer(&fresh.access_token).await.unwrap().is_some());
    e.clock.advance_ms(REFRESH_TTL_SECS * 1000);
    assert!(e.svc.refresh(&creds, &fresh.refresh_token).await.is_err());
}

#[tokio::test]
async fn revoke_and_introspect_are_scoped_to_the_issuing_client() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    let t = e.svc.exchange_code(&creds, &code, CB, None).await.unwrap();
    let i = e
        .svc
        .introspect(&creds, &t.access_token, None)
        .await
        .unwrap();
    assert!(i.active);
    let d = i.details.unwrap();
    assert_eq!(
        (d.sub, d.client_id, d.username.as_deref()),
        (OTHER, c.application.id, Some("other"))
    );
    let c3 = e.svc.create_application(OTHER, "Third", "").await.unwrap();
    let foreign = ClientCredentials {
        client_id: c3.application.id.to_string(),
        secret: Some(c3.client_secret.clone()),
    };
    assert!(
        !e.svc
            .introspect(&foreign, &t.access_token, None)
            .await
            .unwrap()
            .active
    );
    let public = ClientCredentials {
        secret: None,
        ..creds.clone()
    };
    assert!(
        e.svc
            .introspect(&public, &t.access_token, None)
            .await
            .is_err()
    );
    e.svc
        .revoke_token(&foreign, &t.access_token, None)
        .await
        .unwrap();
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_some());
    e.svc
        .revoke_token(&creds, &t.access_token, Some("access_token"))
        .await
        .unwrap();
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_none());
    assert!(
        !e.svc
            .introspect(&creds, &t.access_token, None)
            .await
            .unwrap()
            .active
    );
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    let t = e.svc.exchange_code(&creds, &code, CB, None).await.unwrap();
    e.svc
        .revoke_token(&creds, &t.refresh_token, Some("refresh_token"))
        .await
        .unwrap();
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_none());
    e.svc.revoke_token(&creds, "whatever", None).await.unwrap();
}

#[tokio::test]
async fn users_can_list_and_revoke_what_they_authorized() {
    let e = env();
    let (c, creds) = oauth_app(&e).await;
    let code = code_for(&e, &c, OTHER, "identify", None).await;
    let t = e.svc.exchange_code(&creds, &code, CB, None).await.unwrap();
    let grants = e.svc.grants_of(OTHER).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert!(e.svc.grants_of(OWNER).await.unwrap().is_empty());
    let gid = grants[0].0.id;
    assert!(e.svc.revoke_grant(OWNER, gid).await.is_err());
    e.svc.revoke_grant(OTHER, gid).await.unwrap();
    assert!(e.svc.bearer(&t.access_token).await.unwrap().is_none());
    assert!(e.svc.grants_of(OTHER).await.unwrap().is_empty());
}

// ----------------------------------------------------------------------------- commands

#[tokio::test]
async fn commands_register_per_scope_and_respect_limits() {
    let e = env();
    let c = app(&e).await;
    let a = c.application.id;
    let (cmd, created) = e
        .svc
        .create_command(a, None, &json!({"name": "ping", "description": "Pong"}))
        .await
        .unwrap();
    assert!(created);
    let (again, created) = e
        .svc
        .create_command(a, None, &json!({"name": "ping", "description": "Pong 2"}))
        .await
        .unwrap();
    assert!(!created && again.id == cmd.id && again.version != cmd.version);
    assert_eq!(e.svc.list_commands(a, None).await.unwrap().len(), 1);
    assert!(matches!(
        e.svc
            .create_command(a, None, &json!({"name": "Bad Name", "description": "x"}))
            .await,
        Err(AppsError::Invalid(_))
    ));
    assert_eq!(
        e.svc
            .create_command(a, Some(50), &json!({"name": "g", "description": "x"}))
            .await
            .err(),
        Some(AppsError::Forbidden("Missing Access".into()))
    );
    e.repo.add_guild(50, OWNER, &[]);
    e.svc
        .install(install_req(&c, OWNER, 50, Permissions::MANAGE_GUILD, 0))
        .await
        .unwrap();
    let (g, _) = e
        .svc
        .create_command(a, Some(50), &json!({"name": "g", "description": "x"}))
        .await
        .unwrap();
    assert_eq!(g.guild_id, Some(50));
    assert_eq!(e.svc.list_commands(a, None).await.unwrap().len(), 1);
    assert_eq!(
        e.svc.get_command(a, Some(50), cmd.id).await.err(),
        Some(AppsError::UnknownCommand)
    );
    let other = e
        .svc
        .create_application(OWNER, "Another", "")
        .await
        .unwrap();
    assert_eq!(
        e.svc
            .get_command(other.application.id, None, cmd.id)
            .await
            .err(),
        Some(AppsError::UnknownCommand)
    );
    let p = e
        .svc
        .patch_command(a, None, cmd.id, &json!({"description": "new"}))
        .await
        .unwrap();
    assert_eq!(p.description, "new");
    e.svc
        .create_command(a, None, &json!({"name": "other", "description": "x"}))
        .await
        .unwrap();
    assert!(
        e.svc
            .patch_command(a, None, cmd.id, &json!({"name": "other"}))
            .await
            .is_err()
    );
    e.svc.delete_command(a, None, cmd.id).await.unwrap();
    assert_eq!(
        e.svc.delete_command(a, None, cmd.id).await.err(),
        Some(AppsError::UnknownCommand)
    );
    for i in 0..15 {
        e.svc
            .create_command(
                a,
                None,
                &json!({"name": format!("User cmd {i}"), "type": 2}),
            )
            .await
            .unwrap();
    }
    assert!(
        e.svc
            .create_command(a, None, &json!({"name": "Too many", "type": 2}))
            .await
            .is_err()
    );
    e.svc
        .create_command(
            a,
            None,
            &json!({"name": "User cmd 3", "type": 2, "nsfw": true}),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn bulk_overwrite_replaces_the_set_and_keeps_ids() {
    let e = env();
    let c = app(&e).await;
    let a = c.application.id;
    let first = e
        .svc
        .bulk_overwrite(
            a,
            None,
            &json!([{"name": "a", "description": "A"}, {"name": "b", "description": "B"}]),
        )
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    let second = e
        .svc
        .bulk_overwrite(
            a,
            None,
            &json!([{"name": "a", "description": "A"}, {"name": "c", "description": "C"}]),
        )
        .await
        .unwrap();
    let a1 = first.iter().find(|x| x.name == "a").unwrap();
    let a2 = second.iter().find(|x| x.name == "a").unwrap();
    assert_eq!((a1.id, a1.version), (a2.id, a2.version));
    assert_eq!(e.svc.list_commands(a, None).await.unwrap().len(), 2);
    for bad in [
        json!([{"name": "x", "description": "d"}, {"name": "x", "description": "d"}]),
        json!([{"name": "BAD", "description": "d"}]),
        json!({"not": "an array"}),
    ] {
        assert!(e.svc.bulk_overwrite(a, None, &bad).await.is_err());
    }
    assert_eq!(e.svc.list_commands(a, None).await.unwrap().len(), 2);
    assert!(
        e.svc
            .bulk_overwrite(a, None, &json!([]))
            .await
            .unwrap()
            .is_empty()
    );
    let many: Vec<_> = (0..101)
        .map(|i| json!({"name": format!("c{i}"), "description": "d"}))
        .collect();
    assert!(e.svc.bulk_overwrite(a, None, &json!(many)).await.is_err());
}

#[tokio::test]
async fn command_permissions_are_stored_per_command() {
    let e = env();
    let c = app(&e).await;
    let a = c.application.id;
    e.repo.add_guild(50, OWNER, &[]);
    e.svc
        .install(install_req(&c, OWNER, 50, Permissions::MANAGE_GUILD, 0))
        .await
        .unwrap();
    let (cmd, _) = e
        .svc
        .create_command(a, None, &json!({"name": "ping", "description": "x"}))
        .await
        .unwrap();
    let body = json!({"permissions": [{"id": "7", "type": 1, "permission": true}, {"id": "8", "type": 2, "permission": false}]});
    let v = e
        .svc
        .set_command_permissions(a, 50, Some(cmd.id), &body)
        .await
        .unwrap();
    assert_eq!(v.permissions.len(), 2);
    assert_eq!(
        e.svc
            .command_permissions(a, 50, Some(cmd.id))
            .await
            .unwrap()
            .permissions
            .len(),
        2
    );
    assert_eq!(e.svc.guild_permissions(a, 50).await.unwrap().len(), 1);
    let d = e
        .svc
        .set_command_permissions(a, 50, None, &json!({"permissions": []}))
        .await
        .unwrap();
    assert_eq!(d.id, a);
    for bad in [
        json!({}),
        json!({"permissions": [{"id": "x", "type": 1, "permission": true}]}),
        json!({"permissions": [{"id": "1", "type": 4, "permission": true}]}),
        json!({"permissions": [{"id": "1", "type": 1, "permission": true}, {"id": "1", "type": 1, "permission": false}]}),
    ] {
        assert!(
            e.svc
                .set_command_permissions(a, 50, Some(cmd.id), &bad)
                .await
                .is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        e.svc
            .set_command_permissions(a, 50, Some(424_242), &body)
            .await
            .err(),
        Some(AppsError::UnknownCommand)
    );
    e.svc.delete_command(a, None, cmd.id).await.unwrap();
    assert!(e.svc.guild_permissions(a, 50).await.unwrap().is_empty());
}

// ----------------------------------------------------------------------------- interactions

fn new_interaction(app: i64) -> NewInteraction {
    NewInteraction {
        application_id: app,
        kind: 2,
        user_id: OTHER,
        guild_id: Some(50),
        channel_id: 70,
        command_id: Some(5),
        source_message_id: None,
        nonce: Some("n1".into()),
    }
}

#[tokio::test]
async fn the_first_response_has_three_seconds_and_happens_once() {
    let e = env();
    let c = app(&e).await;
    let (row, token) = e
        .svc
        .create_interaction(new_interaction(c.application.id))
        .await
        .unwrap();
    assert_eq!(row.state, "created");
    assert_eq!(
        e.svc.interaction_by_token(row.id, "x").await.err(),
        Some(AppsError::InvalidToken)
    );
    assert_eq!(
        e.svc
            .interaction_by_token(row.id, &"A".repeat(43))
            .await
            .err(),
        Some(AppsError::InvalidToken)
    );
    assert_eq!(
        e.svc.interaction_by_token(row.id + 1, &token).await.err(),
        Some(AppsError::UnknownInteraction)
    );
    let live = e.svc.interaction_by_token(row.id, &token).await.unwrap();
    e.svc
        .acknowledge(&live, "responded", 4, false)
        .await
        .unwrap();
    assert_eq!(
        e.svc.acknowledge(&live, "responded", 4, false).await.err(),
        Some(AppsError::AlreadyAcknowledged)
    );
    let fresh = e.svc.interaction_by_token(row.id, &token).await.unwrap();
    assert_eq!(fresh.state, "responded");
    assert_eq!(
        e.svc.acknowledge(&fresh, "deferred", 5, false).await.err(),
        Some(AppsError::AlreadyAcknowledged)
    );

    let (late, _) = e
        .svc
        .create_interaction(new_interaction(c.application.id))
        .await
        .unwrap();
    e.clock.advance_ms(3_001);
    assert_eq!(
        e.svc.acknowledge(&late, "responded", 4, false).await.err(),
        Some(AppsError::UnknownInteraction)
    );
    let (d, dt) = e
        .svc
        .create_interaction(new_interaction(c.application.id))
        .await
        .unwrap();
    e.svc.acknowledge(&d, "deferred", 5, false).await.unwrap();
    e.clock.advance_ms(14 * 60 * 1000);
    assert!(e.svc.interaction_by_token(d.id, &dt).await.is_ok());
    e.clock.advance_ms(2 * 60 * 1000);
    assert_eq!(
        e.svc.interaction_by_token(d.id, &dt).await.err(),
        Some(AppsError::UnknownInteraction)
    );
}

#[tokio::test]
async fn webhook_tokens_resolve_only_for_their_application_and_own_messages() {
    let e = env();
    let c = app(&e).await;
    let other = e
        .svc
        .create_application(OWNER, "Another", "")
        .await
        .unwrap();
    let (row, token) = e
        .svc
        .create_interaction(new_interaction(c.application.id))
        .await
        .unwrap();
    let hit = e
        .svc
        .interaction_for_webhook(c.application.id, &token)
        .await
        .unwrap();
    assert_eq!(hit.map(|r| r.id), Some(row.id));
    assert!(
        e.svc
            .interaction_for_webhook(other.application.id, &token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        e.svc
            .interaction_for_webhook(c.application.id, "junk")
            .await
            .unwrap()
            .is_none()
    );
    e.svc
        .attach_message(&row, 900, false, true, None)
        .await
        .unwrap();
    assert_eq!(e.svc.message_of(&row, 900).await.unwrap().message_id, 900);
    assert!(e.svc.message_of(&row, 901).await.is_err());
    let (row2, _) = e
        .svc
        .create_interaction(new_interaction(c.application.id))
        .await
        .unwrap();
    assert!(e.svc.message_of(&row2, 900).await.is_err());
    assert_eq!(
        e.svc
            .interaction_by_token(row.id, &token)
            .await
            .unwrap()
            .original_message_id,
        Some(900)
    );
    e.clock.advance_ms(15 * 60 * 1000 + 1);
    assert!(
        e.svc
            .interaction_for_webhook(c.application.id, &token)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(e.svc.purge_interactions().await.unwrap(), 0);
    e.clock.advance_ms(24 * 3600 * 1000);
    assert_eq!(e.svc.purge_interactions().await.unwrap(), 2);
}

#[tokio::test]
async fn suspended_applications_receive_no_interactions() {
    let e = env();
    let c = app(&e).await;
    e.svc.suspend(9, c.application.id, "x").await.unwrap();
    assert_eq!(
        e.svc
            .create_interaction(new_interaction(c.application.id))
            .await
            .err(),
        Some(AppsError::Suspended)
    );
}

#[tokio::test]
async fn deleting_an_application_removes_the_bot_and_its_credentials() {
    let e = env();
    let c = app(&e).await;
    e.repo.add_guild(50, OWNER, &[]);
    e.svc
        .install(install_req(&c, OWNER, 50, Permissions::MANAGE_GUILD, 0))
        .await
        .unwrap();
    assert!(e.repo.is_member(50, c.application.bot_user_id));
    let (_, left) = e
        .svc
        .delete_application(OWNER, c.application.id)
        .await
        .unwrap();
    assert_eq!(left, vec![50]);
    assert!(!e.repo.is_member(50, c.application.bot_user_id));
    assert!(
        e.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(e.svc.application(c.application.id).await.is_err());
}

// ---------------------------------------------------------------- testers (G-01, D-204)

#[tokio::test]
async fn accepted_testers_can_install_a_development_app_but_invitees_cannot_yet() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    e.repo.add_guild(60, OTHER, &[]);

    // not a tester: refused
    assert_eq!(
        e.svc
            .install(install_req(&c, OTHER, 60, Permissions::MANAGE_GUILD, 0))
            .await
            .err(),
        Some(AppsError::NotReleased)
    );
    // only the owner invites; yourself and unknown users are refused
    assert!(
        e.svc
            .invite_tester(OTHER, id, TesterTarget::Id(OWNER))
            .await
            .is_err()
    );
    assert!(matches!(
        e.svc
            .invite_tester(OWNER, id, TesterTarget::Id(OWNER))
            .await,
        Err(AppsError::Invalid(_))
    ));
    assert!(
        e.svc
            .invite_tester(OWNER, id, TesterTarget::Username("nobody".into()))
            .await
            .is_err()
    );
    let (t, _) = e
        .svc
        .invite_tester(OWNER, id, TesterTarget::Username("OTHER".into()))
        .await
        .unwrap();
    assert_eq!(t.state, TesterState::Invited);
    // an open invitation grants nothing
    assert!(!e.svc.is_tester(id, OTHER).await.unwrap());
    assert_eq!(
        e.svc
            .install(install_req(&c, OTHER, 60, Permissions::MANAGE_GUILD, 0))
            .await
            .err(),
        Some(AppsError::NotReleased)
    );
    // a second invitation for the same account conflicts
    assert!(matches!(
        e.svc
            .invite_tester(OWNER, id, TesterTarget::Id(OTHER))
            .await,
        Err(AppsError::Conflict(_))
    ));
    // only the invitee may answer, and only once
    assert!(e.svc.respond_tester(OWNER, t.id, true).await.is_err());
    let ans = e.svc.respond_tester(OTHER, t.id, true).await.unwrap();
    assert!(ans.accepted);
    assert!(matches!(
        e.svc.respond_tester(OTHER, t.id, true).await,
        Err(AppsError::Conflict(_))
    ));
    assert!(e.svc.is_tester(id, OTHER).await.unwrap());

    // the tester installs in a small guild they manage, not in a big one, not without MANAGE_GUILD
    assert!(
        e.svc
            .install(install_req(&c, OTHER, 60, Permissions::MANAGE_GUILD, 0))
            .await
            .is_ok()
    );
    let big: Vec<i64> = (100..130).collect();
    e.repo.add_guild(61, OTHER, &big);
    assert_eq!(
        e.svc
            .install(install_req(&c, OTHER, 61, Permissions::MANAGE_GUILD, 0))
            .await
            .err(),
        Some(AppsError::NotReleased)
    );
    e.repo.add_guild(62, OWNER, &[OTHER]);
    assert_eq!(
        e.svc
            .install(install_req(&c, OTHER, 62, Permissions::SEND_MESSAGES, 0))
            .await
            .err(),
        Some(AppsError::MissingPermissions)
    );

    // removal takes the right away again
    e.svc.remove_tester(OWNER, id, OTHER).await.unwrap();
    assert!(!e.svc.is_tester(id, OTHER).await.unwrap());
    assert!(e.svc.remove_tester(OWNER, id, OTHER).await.is_err());
    assert_eq!(e.svc.list_testers(OWNER, id).await.unwrap().len(), 0);
}

#[tokio::test]
async fn declining_deletes_the_invitation_and_the_cap_is_25() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    let (t, _) = e
        .svc
        .invite_tester(OWNER, id, TesterTarget::Id(OTHER))
        .await
        .unwrap();
    let ans = e.svc.respond_tester(OTHER, t.id, false).await.unwrap();
    assert!(!ans.accepted);
    assert!(e.svc.list_testers(OWNER, id).await.unwrap().is_empty());
    assert!(e.svc.respond_tester(OTHER, t.id, true).await.is_err());

    for n in 0..25 {
        let acct = 500 + n;
        e.repo.add_account(acct, &format!("tester{n}"));
        e.svc
            .invite_tester(OWNER, id, TesterTarget::Id(acct))
            .await
            .unwrap();
    }
    e.repo.add_account(600, "one-too-many");
    assert!(matches!(
        e.svc.invite_tester(OWNER, id, TesterTarget::Id(600)).await,
        Err(AppsError::Conflict(_))
    ));
}

// ------------------------------------------------- endpoint auto-disable (G-05, D-186)

#[test]
fn endpoint_health_counts_consecutive_failures_inside_the_window() {
    let mut c = InteractionsConfig::default();
    let t = T0;
    // 24 failures do nothing; a success resets the streak
    for i in 0..24 {
        assert!(!endpoint_health_step(&mut c, false, t + i * 1000));
    }
    assert_eq!(c.failures, 24);
    assert!(!endpoint_health_step(&mut c, true, t + 30_000));
    assert_eq!((c.failures, c.failure_window_start_ms), (0, None));
    // a streak that outlives the 10 minute window restarts
    assert!(!endpoint_health_step(&mut c, false, t));
    assert!(!endpoint_health_step(
        &mut c,
        false,
        t + ENDPOINT_FAILURE_WINDOW_MS + 1
    ));
    assert_eq!(c.failures, 1);
    // the 25th consecutive failure inside the window flips the state, once
    let mut c = InteractionsConfig::default();
    let mut flips = 0;
    for i in 0..30 {
        if endpoint_health_step(&mut c, false, t + i * 1000) {
            flips += 1;
        }
    }
    assert_eq!(flips, 1);
    assert_eq!(c.state, EndpointState::Disabled);
    // a disabled endpoint ignores further outcomes
    assert!(!endpoint_health_step(&mut c, true, t + 99_000));
    assert_eq!(c.state, EndpointState::Disabled);
}

#[tokio::test]
async fn a_failing_endpoint_is_disabled_and_verification_re_enables_it() {
    let e = env();
    let c = app(&e).await;
    let id = c.application.id;
    e.svc
        .set_endpoint(OWNER, id, Some("https://bot.example.com/i".into()))
        .await
        .unwrap();
    assert!(e.svc.endpoint_target(id).await.unwrap().is_some());

    for _ in 0..24 {
        assert!(!e.svc.record_delivery(id, false).await.unwrap());
    }
    // a success in the middle resets the count
    assert!(!e.svc.record_delivery(id, true).await.unwrap());
    for _ in 0..24 {
        assert!(!e.svc.record_delivery(id, false).await.unwrap());
    }
    assert!(e.svc.endpoint_target(id).await.unwrap().is_some());
    assert!(e.svc.record_delivery(id, false).await.unwrap());
    // dispatch falls back to the gateway
    assert!(e.svc.endpoint_target(id).await.unwrap().is_none());
    assert_eq!(
        e.svc.endpoint_info(id).await.unwrap().state,
        EndpointState::Disabled
    );
    // no second flip (no second DM)
    assert!(!e.svc.record_delivery(id, false).await.unwrap());

    assert!(e.svc.reenable_endpoint(OTHER, id).await.is_err());
    let info = e.svc.reenable_endpoint(OWNER, id).await.unwrap();
    assert_eq!(info.state, EndpointState::Enabled);
    assert!(e.svc.endpoint_target(id).await.unwrap().is_some());
    let actions: Vec<String> = e
        .svc
        .audit_log(OWNER, id, 100, None)
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.action)
        .collect();
    assert!(actions.contains(&"app.interactions_endpoint_disabled".to_string()));
    assert!(actions.contains(&"app.interactions_endpoint_reenabled".to_string()));
}
