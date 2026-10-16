//! OAuth2, installs, commands and interactions through the real router.

use super::tests::{BOB, CAROL, CB, Env, OWNER, Who, code_of, env, json_of, next_named, no_event};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sr_chat::settings::repo::AuditRepository;
use sr_core::Permissions;
use std::collections::HashMap;

fn challenge_for(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    data_encoding::BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()))
}

fn basic(id: &str, secret: &str) -> (&'static str, String) {
    (
        "authorization",
        format!(
            "Basic {}",
            data_encoding::BASE64.encode(format!("{id}:{secret}").as_bytes())
        ),
    )
}

fn form(pairs: &[(&str, &str)]) -> Vec<u8> {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
        .into_bytes()
}

fn query_of(location: &str) -> HashMap<String, String> {
    url::Url::parse(location)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

impl Env {
    async fn token_form(
        &self,
        extra: &[(&str, String)],
        pairs: &[(&str, &str)],
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        let (s, h, b) = self
            .raw(
                Who::Anon,
                Method::POST,
                "/oauth2/token",
                Some("application/x-www-form-urlencoded"),
                form(pairs),
                extra,
            )
            .await;
        (s, h, json_of(&b))
    }

    /// Authorize as `user` and exchange the code; returns the token response.
    async fn grant(&self, user: i64, app: (&str, &str), scope: &str) -> Value {
        let (s, v) = self
            .post(
                Who::Session(user),
                "/oauth2/authorize",
                json!({
                    "client_id": app.0, "scope": scope, "redirect_uri": CB, "response_type": "code",
                    "state": "st", "code_challenge": challenge_for(VERIFIER),
                    "code_challenge_method": "S256", "authorize": true,
                }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let q = query_of(v["location"].as_str().unwrap());
        assert_eq!(q["state"], "st");
        let (s, _, t) = self
            .token_form(
                &[basic(app.0, app.1)],
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &q["code"]),
                    ("redirect_uri", CB),
                    ("code_verifier", VERIFIER),
                ],
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{t}");
        t
    }
}

// ======================================================================= OAuth2

#[tokio::test]
async fn authorization_code_flow_end_to_end() {
    let e = env().await;
    let (id, _, _, secret) = e.new_app().await;
    e.configure(&id).await;
    let q = format!(
        "/oauth2/authorize?client_id={id}&scope=identify%20guilds&redirect_uri={}&state=st&response_type=code",
        url::form_urlencoded::byte_serialize(CB.as_bytes()).collect::<String>()
    );
    let (s, v) = e.get(Who::Session(BOB), &q).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["application"]["name"], "Helper");
    assert!(
        v["application"].get("redirect_uris").is_none(),
        "owner-only fields stay hidden"
    );
    assert_eq!(v["scopes"][0]["name"], "identify");
    assert_eq!(v["user"]["id"], BOB.to_string());
    assert_eq!(v["guilds"], json!([]));
    assert_eq!(v["already_authorized"], false);
    assert_eq!(e.get(Who::Anon, &q).await.0, StatusCode::UNAUTHORIZED);

    let t = e.grant(BOB, (&id, &secret), "identify guilds").await;
    assert_eq!(t["token_type"], "Bearer");
    assert_eq!(t["scope"], "identify guilds");
    let access = t["access_token"].as_str().unwrap().to_owned();
    let refresh = t["refresh_token"].as_str().unwrap().to_owned();

    // the bearer works where it is allowed...
    let (s, v) = e.get(Who::Bearer(&access), "/users/@me").await;
    assert_eq!(
        (s, v["id"].clone(), v["auth"].clone()),
        (StatusCode::OK, json!(BOB.to_string()), json!("bearer"))
    );
    assert!(v.get("email").is_none(), "no email without the email scope");
    let (s, v) = e.get(Who::Bearer(&access), "/oauth2/@me").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["application"]["id"], json!(id));
    assert_eq!(v["scopes"], json!(["identify", "guilds"]));
    assert_eq!(v["user"]["id"], BOB.to_string());
    // ...and nowhere else
    let ch = e.ch;
    for (m, p, b) in [
        (
            Method::POST,
            format!("/channels/{ch}/messages"),
            Some(json!({"content": "hi"})),
        ),
        (Method::GET, format!("/channels/{ch}/messages"), None),
        (Method::GET, "/applications".to_owned(), None),
        (
            Method::POST,
            "/applications".to_owned(),
            Some(json!({"name": "mine"})),
        ),
        (
            Method::POST,
            "/oauth2/authorize".to_owned(),
            Some(json!({"client_id": id, "scope": "identify", "authorize": true})),
        ),
        (Method::GET, "/oauth2/tokens".to_owned(), None),
        (Method::GET, "/developers/status".to_owned(), None),
    ] {
        let (s, v) = e.call(Who::Bearer(&access), m.clone(), &p, b).await;
        assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001), "{m} {p}");
    }
    let (s, _) = e.get(Who::Bearer("not-a-token"), "/users/@me").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // the user sees and can revoke the grant
    let (_, v) = e.get(Who::Session(BOB), "/oauth2/tokens").await;
    assert_eq!(v[0]["application"]["name"], "Helper");
    assert_eq!(v[0]["scopes"], json!(["identify", "guilds"]));
    let gid = v[0]["id"].as_str().unwrap().to_owned();
    let (s, _) = e
        .delete(Who::Session(CAROL), &format!("/oauth2/tokens/{gid}"), None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "someone else's grant");

    // introspection is for the issuing client only
    let intro = |id: String, secret: String, token: String| {
        let e = &e;
        async move {
            let (s, _, b) = e
                .raw(
                    Who::Anon,
                    Method::POST,
                    "/oauth2/introspect",
                    Some("application/x-www-form-urlencoded"),
                    form(&[
                        ("token", &token),
                        ("client_id", &id),
                        ("client_secret", &secret),
                    ]),
                    &[],
                )
                .await;
            (s, json_of(&b))
        }
    };
    let (s, v) = intro(id.clone(), secret.clone(), access.clone()).await;
    assert_eq!(
        (s, v["active"].clone(), v["scope"].clone()),
        (StatusCode::OK, json!(true), json!("identify guilds"))
    );
    let (s, v) = intro(id.clone(), "x".repeat(43), access.clone()).await;
    assert_eq!(
        (s, v["error"].clone()),
        (StatusCode::UNAUTHORIZED, json!("invalid_client"))
    );
    let other = e.svc.create_application(OWNER, "Other", "").await.unwrap();
    let (_, v) = intro(
        other.application.id.to_string(),
        other.client_secret.clone(),
        access.clone(),
    )
    .await;
    assert_eq!(v["active"], false);

    // refresh rotates; the old access token dies and reuse revokes everything
    let refresh_with = |tok: String| {
        let (id, secret) = (id.clone(), secret.clone());
        let e = &e;
        async move {
            let (s, _, v) = e
                .token_form(
                    &[basic(&id, &secret)],
                    &[("grant_type", "refresh_token"), ("refresh_token", &tok)],
                )
                .await;
            (s, v)
        }
    };
    let (s, t2) = refresh_with(refresh.clone()).await;
    assert_eq!(s, StatusCode::OK, "{t2}");
    assert_eq!(
        e.get(Who::Bearer(&access), "/users/@me").await.0,
        StatusCode::UNAUTHORIZED
    );
    let access2 = t2["access_token"].as_str().unwrap().to_owned();
    assert_eq!(
        e.get(Who::Bearer(&access2), "/users/@me").await.0,
        StatusCode::OK
    );
    let (s, v) = refresh_with(refresh).await;
    assert_eq!(
        (s, v["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_grant"))
    );
    assert_eq!(
        e.get(Who::Bearer(&access2), "/users/@me").await.0,
        StatusCode::UNAUTHORIZED
    );

    // revocation
    let t3 = e.grant(BOB, (&id, &secret), "identify").await;
    let a3 = t3["access_token"].as_str().unwrap();
    let (s, _, _) = e
        .raw(
            Who::Anon,
            Method::POST,
            "/oauth2/token/revoke",
            Some("application/x-www-form-urlencoded"),
            form(&[
                ("token", a3),
                ("client_id", &id),
                ("client_secret", &secret),
            ]),
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        e.get(Who::Bearer(a3), "/users/@me").await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn authorize_never_redirects_on_a_bad_client_or_redirect_uri() {
    let e = env().await;
    let (id, _, _, _) = e.new_app().await;
    e.configure(&id).await;
    for body in [
        json!({"client_id": id, "scope": "identify", "redirect_uri": "https://evil.test/cb", "authorize": true}),
        json!({"client_id": id, "scope": "identify", "redirect_uri": "https://client.test/cb/", "authorize": false}),
        json!({"client_id": "999", "scope": "identify", "redirect_uri": CB, "authorize": true}),
        json!({"client_id": id, "scope": "identify rpc", "redirect_uri": CB, "authorize": true}),
        json!({"client_id": id, "scope": "identify", "authorize": true}),
        json!({"client_id": id, "scope": "identify", "redirect_uri": CB, "response_type": "token", "authorize": true}),
        json!({"client_id": id, "scope": "identify", "redirect_uri": CB, "code_challenge": "x", "authorize": true}),
    ] {
        let (s, h, b) = e
            .raw(
                Who::Session(BOB),
                Method::POST,
                "/oauth2/authorize",
                Some("application/json"),
                body.to_string().into_bytes(),
                &[],
            )
            .await;
        let v = json_of(&b);
        assert_eq!(
            (s, code_of(&v)),
            (StatusCode::BAD_REQUEST, 50035),
            "{body}: {v}"
        );
        assert!(h.get("location").is_none(), "{body}");
        assert!(v.get("location").is_none(), "{body}");
    }
    let (s, v) = e
        .post(
            Who::Session(BOB),
            "/oauth2/authorize",
            json!({"client_id": id, "scope": "identify", "redirect_uri": CB, "state": "abc", "authorize": false}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let q = query_of(v["location"].as_str().unwrap());
    assert_eq!(
        (q["error"].as_str(), q["state"].as_str()),
        ("access_denied", "abc")
    );
    assert!(!q.contains_key("code"));
}

#[tokio::test]
async fn token_endpoint_error_handling() {
    let e = env().await;
    let (id, _, _, secret) = e.new_app().await;
    e.configure(&id).await;
    let (s, v) = e
        .post(
            Who::Session(BOB),
            "/oauth2/authorize",
            json!({"client_id": id, "scope": "identify", "redirect_uri": CB,
                   "code_challenge": challenge_for(VERIFIER), "authorize": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let code = query_of(v["location"].as_str().unwrap())["code"].clone();
    let good = basic(&id, &secret);
    let ex = |code: &str, verifier: Option<&str>| {
        let mut p = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", code.to_owned()),
            ("redirect_uri", CB.to_owned()),
        ];
        if let Some(v) = verifier {
            p.push(("code_verifier", v.to_owned()));
        }
        p
    };
    let call = |extra: Vec<(&'static str, String)>, p: Vec<(&'static str, String)>| {
        let e = &e;
        async move {
            let pairs: Vec<(&str, &str)> = p.iter().map(|(k, v)| (*k, v.as_str())).collect();
            e.token_form(&extra, &pairs).await
        }
    };
    let (s, h, v) = call(vec![basic(&id, &"y".repeat(43))], ex(&code, Some(VERIFIER))).await;
    assert_eq!(
        (s, v["error"].clone()),
        (StatusCode::UNAUTHORIZED, json!("invalid_client"))
    );
    assert!(h.get("www-authenticate").is_some());
    assert_eq!(h["cache-control"], "no-store");
    // a wrong verifier burns the code
    let (s, _, v) = call(vec![good.clone()], ex(&code, Some(&"v".repeat(50)))).await;
    assert_eq!(
        (s, v["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_grant"))
    );
    let (s, _, v) = call(vec![good.clone()], ex(&code, Some(VERIFIER))).await;
    assert_eq!(
        (s, v["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_grant"))
    );
    // malformed requests
    for (extra, pairs, err) in [
        (
            vec![good.clone()],
            vec![("grant_type", "client_credentials".to_owned())],
            "unsupported_grant_type",
        ),
        (vec![good.clone()], vec![], "invalid_request"),
        (
            vec![good.clone()],
            vec![("grant_type", "authorization_code".to_owned())],
            "invalid_request",
        ),
        (
            vec![],
            vec![("grant_type", "authorization_code".to_owned())],
            "invalid_request",
        ),
        (
            vec![good.clone()],
            vec![
                ("grant_type", "refresh_token".to_owned()),
                ("client_id", id.clone()),
                ("refresh_token", "x".to_owned()),
            ],
            "invalid_request",
        ),
    ] {
        let (s, _, v) = call(extra, pairs).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some(err)),
            "{v}"
        );
    }
    // JSON bodies work as well
    let (s, _, b) = e
        .raw(
            Who::Anon,
            Method::POST,
            "/oauth2/token",
            Some("application/json"),
            json!({"grant_type": "authorization_code", "code": "junk", "redirect_uri": CB,
                   "client_id": id, "client_secret": secret})
            .to_string()
            .into_bytes(),
            &[],
        )
        .await;
    assert_eq!(
        (s, json_of(&b)["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_grant"))
    );
    let (s, _) = e.get(Who::Session(BOB), "/oauth2/token").await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
}

// ======================================================================= installs

#[tokio::test]
async fn installs_need_manage_guild_and_respect_the_release_rules() {
    let e = env().await;
    let (id, bot, _, _) = e.new_app().await;
    e.configure(&id).await;
    let guild = e.guild.to_string();
    let authz = |user: i64, perms: String| {
        let (e, id, guild) = (&e, id.clone(), guild.clone());
        async move {
            e.post(
                Who::Session(user),
                "/oauth2/authorize",
                json!({"client_id": id, "scope": "bot", "guild_id": guild, "permissions": perms, "authorize": true}),
            )
            .await
        }
    };
    let (s, v) = authz(BOB, "0".into()).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let (s, v) = authz(CAROL, "0".into()).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001));
    let (s, _) = e
        .post(
            Who::Session(OWNER),
            "/oauth2/authorize",
            json!({"client_id": id, "scope": "bot", "authorize": true}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a server is required");
    let (_, v) = e
        .get(
            Who::Session(OWNER),
            &format!("/oauth2/authorize?client_id={id}&scope=bot&permissions=8"),
        )
        .await;
    assert_eq!(v["guilds"][0]["can_install"], true);
    assert_eq!(v["bot"]["bot"], true);
    let (_, v) = e
        .get(
            Who::Session(BOB),
            &format!("/oauth2/authorize?client_id={id}&scope=bot"),
        )
        .await;
    assert_eq!(v["guilds"][0]["can_install"], false);
    assert_eq!(v["guilds"][0]["reason"], "missing_permission");

    let (s, v) = authz(OWNER, Permissions::SEND_MESSAGES.bits().to_string()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["location"], "https://side.test/oauth2/authorized");
    assert_eq!(v["already_installed"], false);
    assert!(e.apps.is_member(e.guild.get() as i64, bot.parse().unwrap()));
    let (s, v) = authz(OWNER, "0".into()).await;
    assert_eq!(
        (s, v["already_installed"].clone()),
        (StatusCode::OK, json!(true))
    );
    let (_, v) = e
        .get(Who::Session(OWNER), &format!("/applications/{id}/installs"))
        .await;
    assert_eq!(v[0]["guild_id"], guild);

    // a private bot is its owner's alone
    e.patch(
        Who::Session(OWNER),
        &format!("/applications/{id}"),
        json!({"bot_public": false}),
    )
    .await;
    let (s, v) = e
        .post(
            Who::Session(BOB),
            "/oauth2/authorize",
            json!({"client_id": id, "scope": "bot", "guild_id": guild, "authorize": true}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
}

// ======================================================================= commands

#[tokio::test]
async fn commands_are_registered_by_the_bot_and_validated() {
    let e = env().await;
    let (id, bot, token, _) = e.new_app().await;
    e.configure(&id).await;
    let base = format!("/applications/{id}/commands");
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &base,
            json!({"name": "ping", "description": "Pong"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["application_id"], json!(id));
    assert_eq!(v["type"], 1);
    let cid = v["id"].as_str().unwrap().to_owned();
    let (s, _) = e
        .post(
            Who::Bot(&token),
            &base,
            json!({"name": "ping", "description": "Pong 2"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "same name updates");
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &base,
            json!({"name": "ok", "description": "d", "options": [{"type": 3, "name": "Bad Name", "description": "d"}]}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    assert!(
        v["errors"]["options"]["0"]["name"]["_errors"][0]["code"].is_string(),
        "{v}"
    );
    let (s, v) = e.get(Who::Bot(&token), &format!("{base}/{cid}")).await;
    assert_eq!((s, v["name"].clone()), (StatusCode::OK, json!("ping")));
    let (s, v) = e
        .patch(
            Who::Bot(&token),
            &format!("{base}/{cid}"),
            json!({"description": "changed"}),
        )
        .await;
    assert_eq!(
        (s, v["description"].clone()),
        (StatusCode::OK, json!("changed"))
    );
    let (s, v) = e.get(Who::Bot(&token), &format!("{base}/999")).await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10063));
    let (s, v) = e
        .put(
            Who::Bot(&token),
            &base,
            json!([{"name": "ping", "description": "changed"}, {"name": "Report", "type": 3}]),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!(
        e.get(Who::Bot(&token), &base)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let gbase = format!("/applications/{id}/guilds/{}/commands", e.guild);
    let (s, v) = e.get(Who::Bot(&token), &gbase).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001));
    e.install(&id, &bot, 0).await;
    let (s, _) = e
        .post(
            Who::Bot(&token),
            &gbase,
            json!({"name": "g", "description": "guild only"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(
        e.get(Who::Bot(&token), &gbase)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (s, _) = e
        .delete(Who::Bot(&token), &format!("{base}/{cid}"), None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .delete(Who::Bot(&token), &format!("{base}/{cid}"), None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &base,
            json!({"name": "x", "description": "x"}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 20002));
}

#[tokio::test]
async fn command_permissions_are_written_by_people_who_manage_the_server() {
    let e = env().await;
    let (id, bot, token, secret) = e.new_app().await;
    e.configure(&id).await;
    e.install(&id, &bot, 0).await;
    let (_, v) = e
        .post(
            Who::Bot(&token),
            &format!("/applications/{id}/commands"),
            json!({"name": "ping", "description": "x"}),
        )
        .await;
    let cid = v["id"].as_str().unwrap().to_owned();
    let path = format!(
        "/applications/{id}/guilds/{}/commands/{cid}/permissions",
        e.guild
    );
    let body = json!({"permissions": [{"id": BOB.to_string(), "type": 2, "permission": false}]});
    let (s, _) = e.put(Who::Bot(&token), &path, body.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "bot tokens are refused");
    // A member who neither owns the application nor manages the server is refused.
    let (s, v) = e.put(Who::Session(BOB), &path, body.clone()).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let (s, _) = e.put(Who::Session(CAROL), &path, body.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "not even a member");
    // The application owner (who also owns the server) is accepted; the change is audited (121).
    let (s, v) = e.put(Who::Session(OWNER), &path, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let log = e
        .settings
        .list_audit(&sr_chat::settings::audit::AuditQuery::new(e.guild))
        .await
        .unwrap();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].action.code(), 121);
    assert_eq!(log[0].actor_id.get(), OWNER as u64);
    assert_eq!(log[0].target_id, cid.parse::<u64>().ok());
    // Reading is open to the same people.
    let (s, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!(
        (s, v["permissions"].as_array().map(Vec::len)),
        (StatusCode::OK, Some(1))
    );
    let (s, _) = e.get(Who::Session(BOB), &path).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let t = e.grant(OWNER, (&id, &secret), "identify").await;
    let (s, v) = e
        .put(
            Who::Bearer(t["access_token"].as_str().unwrap()),
            &path,
            body.clone(),
        )
        .await;
    assert_eq!(
        (s, code_of(&v)),
        (StatusCode::FORBIDDEN, 50001),
        "missing scope"
    );
    let tb = e
        .grant(
            BOB,
            (&id, &secret),
            "applications.commands.permissions.update",
        )
        .await;
    let (s, v) = e
        .put(
            Who::Bearer(tb["access_token"].as_str().unwrap()),
            &path,
            body.clone(),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let to = e
        .grant(
            OWNER,
            (&id, &secret),
            "applications.commands.permissions.update",
        )
        .await;
    let (s, v) = e
        .put(
            Who::Bearer(to["access_token"].as_str().unwrap()),
            &path,
            body.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["permissions"][0]["id"], BOB.to_string());
    // a token issued to another application is not accepted
    let other = e.svc.create_application(OWNER, "Other", "").await.unwrap();
    e.svc
        .update_application(
            OWNER,
            other.application.id,
            sr_apps::service::UpdateInput {
                redirect_uris: Some(vec![CB.into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let tx = e
        .grant(
            OWNER,
            (&other.application.id.to_string(), &other.client_secret),
            "applications.commands.permissions.update",
        )
        .await;
    let (s, _) = e
        .put(
            Who::Bearer(tx["access_token"].as_str().unwrap()),
            &path,
            body,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = e.get(Who::Bot(&token), &path).await;
    assert_eq!(
        (s, v["permissions"].as_array().map(Vec::len)),
        (StatusCode::OK, Some(1))
    );
    let (s, v) = e
        .get(
            Who::Bot(&token),
            &format!("/applications/{id}/guilds/{}/commands/permissions", e.guild),
        )
        .await;
    assert_eq!((s, v.as_array().map(Vec::len)), (StatusCode::OK, Some(1)));
}

// ======================================================================= interactions

struct Fx {
    e: Env,
    app: String,
    bot: String,
    token: String,
    cmd: String,
}

async fn fx() -> Fx {
    fx_with(Permissions::SEND_MESSAGES.bits() | Permissions::VIEW_CHANNEL.bits()).await
}

/// A fixture whose bot was installed with exactly `bot_perms`.
async fn fx_with(bot_perms: u64) -> Fx {
    let e = env().await;
    let (app, bot, token, _) = e.new_app().await;
    e.configure(&app).await;
    e.install(&app, &bot, bot_perms).await;
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &format!("/applications/{app}/commands"),
            json!({"name": "echo", "description": "Echo", "options": [
                {"type": 3, "name": "text", "description": "What", "required": true},
                {"type": 6, "name": "who", "description": "Who"},
            ]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let cmd = v["id"].as_str().unwrap().to_owned();
    Fx {
        e,
        app,
        bot,
        token,
        cmd,
    }
}

impl Fx {
    fn invoke_body(&self, text: &str) -> Value {
        json!({
            "type": 2, "application_id": self.app, "channel_id": self.e.ch.to_string(),
            "nonce": "n-1",
            "data": {"id": self.cmd, "options": [{"type": 3, "name": "text", "value": text}]},
        })
    }

    async fn invoke(&self, user: i64) -> (StatusCode, Value) {
        self.e
            .post(
                Who::Session(user),
                "/interactions",
                self.invoke_body("hello"),
            )
            .await
    }

    async fn callback(&self, id: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.e
            .post(
                Who::Anon,
                &format!("/interactions/{id}/{token}/callback"),
                body,
            )
            .await
    }
}

#[tokio::test]
async fn a_member_invokes_a_command_and_the_bot_receives_it() {
    let f = fx().await;
    let mut bot_events = f.e.user_events(f.bot.parse().unwrap());
    let (s, v) = f.invoke(BOB).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(ev["type"], 2);
    assert_eq!(ev["application_id"], json!(f.app));
    assert_eq!(ev["data"]["name"], "echo");
    assert_eq!(
        ev["data"]["options"][0],
        json!({"name": "text", "type": 3, "value": "hello"})
    );
    assert_eq!(ev["member"]["user"]["id"], BOB.to_string());
    assert_eq!(ev["channel_id"], f.e.ch.to_string());
    assert_eq!(ev["guild_id"], f.e.guild.to_string());
    assert!(ev["token"].as_str().unwrap().len() == 43);
    assert!(ev["member"]["permissions"].is_string());
    assert!(ev["app_permissions"].is_string());
    assert_eq!(ev["version"], 1);
}

#[tokio::test]
async fn invoking_is_refused_without_access_install_permission_or_a_valid_command() {
    let f = fx().await;
    let e = &f.e;
    // not signed in, or a bot
    assert_eq!(
        e.post(Who::Anon, "/interactions", f.invoke_body("x"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (s, v) = e
        .post(Who::Bot(&f.token), "/interactions", f.invoke_body("x"))
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 20001));
    // someone outside the server cannot see the channel
    let (s, v) = f.invoke(CAROL).await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10003));
    // an application that is not installed
    let other = e.svc.create_application(OWNER, "Other", "").await.unwrap();
    let mut body = f.invoke_body("x");
    body["application_id"] = json!(other.application.id.to_string());
    let (s, v) = e.post(Who::Session(BOB), "/interactions", body).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001), "{v}");
    // unknown, malformed and foreign commands
    let mut body = f.invoke_body("x");
    body["data"]["id"] = json!("424242");
    let (s, v) = e.post(Who::Session(BOB), "/interactions", body).await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10063));
    let mut body = f.invoke_body("x");
    body["data"]["id"] = json!("nope");
    assert_eq!(
        e.post(Who::Session(BOB), "/interactions", body).await.0,
        StatusCode::BAD_REQUEST
    );
    let (_, foreign) = e
        .svc_command(&other.application.id.to_string(), "mine")
        .await;
    let mut body = f.invoke_body("x");
    body["data"]["id"] = json!(foreign);
    let (s, v) = e.post(Who::Session(BOB), "/interactions", body).await;
    assert_eq!(
        (s, code_of(&v)),
        (StatusCode::NOT_FOUND, 10063),
        "another application's command"
    );
    // option validation against the registered schema
    for data in [
        json!({"id": f.cmd, "options": []}),
        json!({"id": f.cmd, "options": [{"type": 4, "name": "text", "value": 3}]}),
        json!({"id": f.cmd, "options": [{"type": 3, "name": "nope", "value": "x"}]}),
        json!({"id": f.cmd, "options": [{"type": 3, "name": "text", "value": "x"}, {"type": 6, "name": "who", "value": "999"}]}),
    ] {
        let mut body = f.invoke_body("x");
        body["data"] = data.clone();
        let (s, v) = e.post(Who::Session(BOB), "/interactions", body).await;
        assert_eq!(
            (s, code_of(&v)),
            (StatusCode::BAD_REQUEST, 50035),
            "{data}: {v}"
        );
    }
    // type and channel checks
    let mut body = f.invoke_body("x");
    body["type"] = json!(7);
    assert_eq!(
        e.post(Who::Session(BOB), "/interactions", body).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut body = f.invoke_body("x");
    body["channel_id"] = json!("123456");
    assert_eq!(
        e.post(Who::Session(BOB), "/interactions", body).await.0,
        StatusCode::NOT_FOUND
    );
    // a suspended application receives nothing
    e.svc
        .suspend(9, f.app.parse().unwrap(), "abuse")
        .await
        .unwrap();
    let (s, v) = f.invoke(BOB).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 900121), "{v}");
}

impl Env {
    /// Register a command for another application directly through the service.
    async fn svc_command(&self, app: &str, name: &str) -> (String, String) {
        let (c, _) = self
            .svc
            .create_command(
                app.parse().unwrap(),
                None,
                &json!({"name": name, "description": "x"}),
            )
            .await
            .unwrap();
        (app.to_owned(), c.id.to_string())
    }
}

#[tokio::test]
async fn command_permissions_gate_who_can_invoke() {
    let f = fx().await;
    let e = &f.e;
    let app: i64 = f.app.parse().unwrap();
    // a command that needs BAN_MEMBERS by default
    let (_, v) = e
        .post(
            Who::Bot(&f.token),
            &format!("/applications/{}/commands", f.app),
            json!({"name": "ban", "description": "d", "default_member_permissions": Permissions::BAN_MEMBERS.bits().to_string()}),
        )
        .await;
    let ban = v["id"].as_str().unwrap().to_owned();
    let body = json!({"type": 2, "application_id": f.app, "channel_id": e.ch.to_string(), "data": {"id": ban}});
    let (s, v) = e
        .post(Who::Session(BOB), "/interactions", body.clone())
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    assert_eq!(
        e.post(Who::Session(OWNER), "/interactions", body.clone())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    // an explicit overwrite for the user beats the default requirement
    e.svc
        .set_command_permissions(
            app,
            e.guild.get() as i64,
            Some(ban.parse().unwrap()),
            &json!({"permissions": [{"id": BOB.to_string(), "type": 2, "permission": true}]}),
        )
        .await
        .unwrap();
    assert_eq!(
        e.post(Who::Session(BOB), "/interactions", body.clone())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    // and a deny for the user blocks a command everyone could use
    e.svc
        .set_command_permissions(
            app,
            e.guild.get() as i64,
            Some(f.cmd.parse().unwrap()),
            &json!({"permissions": [{"id": BOB.to_string(), "type": 2, "permission": false}]}),
        )
        .await
        .unwrap();
    let (s, v) = f.invoke(BOB).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    // the owner (an administrator) is never locked out
    assert_eq!(f.invoke(OWNER).await.0, StatusCode::NO_CONTENT);
}

async fn start(f: &Fx, user: i64) -> (String, String) {
    let mut bot_events = f.e.user_events(f.bot.parse().unwrap());
    let (s, v) = f.invoke(user).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    (
        ev["id"].as_str().unwrap().to_owned(),
        ev["token"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn the_first_response_is_checked_timed_and_single() {
    let f = fx().await;
    let (id, token) = start(&f, BOB).await;
    // wrong token, unknown id, bad type
    let (s, v) = f
        .callback(
            &id,
            &"A".repeat(43),
            json!({"type": 4, "data": {"content": "x"}}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::UNAUTHORIZED, 50027));
    let (s, v) = f
        .callback("123", &token, json!({"type": 4, "data": {"content": "x"}}))
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10062));
    for bad in [
        json!({"type": 7, "data": {"content": "x"}}),
        json!({"type": 8, "data": {"choices": []}}),
        json!({"type": 12}),
        json!({"type": 4, "data": {}}),
        json!({"type": 4, "data": {"content": "x".repeat(2001)}}),
    ] {
        let (s, v) = f.callback(&id, &token, bad.clone()).await;
        assert_eq!(
            (s, code_of(&v)),
            (StatusCode::BAD_REQUEST, 50035),
            "{bad}: {v}"
        );
    }
    // the answer posts a real message from the bot and confirms to the invoker
    let mut mine = f.e.user_events(BOB);
    let (s, v) = f
        .callback(
            &id,
            &token,
            json!({"type": 4, "data": {"content": "hello back"}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ok = next_named(&mut mine, "INTERACTION_SUCCESS").await;
    assert_eq!(
        (ok["id"].clone(), ok["nonce"].clone()),
        (json!(id), json!("n-1"))
    );
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    assert_eq!(msgs[0]["content"], "hello back");
    assert_eq!(msgs[0]["author"]["id"], f.bot);
    // a second answer is 40060
    let (s, v) = f
        .callback(
            &id,
            &token,
            json!({"type": 4, "data": {"content": "again"}}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 40060));
    // after three seconds there is no first response any more
    let (id2, token2) = start(&f, BOB).await;
    f.e.clock.advance_ms(3_001);
    let (s, v) = f
        .callback(
            &id2,
            &token2,
            json!({"type": 4, "data": {"content": "late"}}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10062));
    // a token is good for fifteen minutes at most
    let (id3, token3) = start(&f, BOB).await;
    f.callback(&id3, &token3, json!({"type": 5})).await;
    f.e.clock.advance_ms(14 * 60 * 1000);
    let (s, _) =
        f.e.patch(
            Who::Anon,
            &format!("/webhooks/{}/{token3}/messages/@original", f.app),
            json!({"content": "slow"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    f.e.clock.advance_ms(2 * 60 * 1000);
    let (s, _) =
        f.e.patch(
            Who::Anon,
            &format!("/webhooks/{}/{token3}/messages/@original", f.app),
            json!({"content": "too slow"}),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn silence_is_reported_to_the_invoker() {
    let f = fx().await;
    let mut mine = f.e.user_events(BOB);
    let (id, _) = start(&f, BOB).await;
    let ev = next_named(&mut mine, "INTERACTION_FAILURE").await;
    assert_eq!(
        (ev["id"].clone(), ev["reason_code"].clone()),
        (json!(id), json!(2))
    );
    // an answered interaction produces no failure
    let (id2, token2) = start(&f, BOB).await;
    assert_eq!(
        f.callback(&id2, &token2, json!({"type": 5})).await.0,
        StatusCode::NO_CONTENT
    );
    no_event(&mut mine, "INTERACTION_FAILURE").await;
}

#[tokio::test]
async fn ephemeral_responses_reach_only_the_invoker_and_are_never_stored() {
    let f = fx().await;
    let (id, token) = start(&f, BOB).await;
    let mut bob = f.e.user_events(BOB);
    let mut owner = f.e.user_events(OWNER);
    let (s, _) = f
        .callback(
            &id,
            &token,
            json!({"type": 4, "data": {"content": "secret", "flags": 64}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let m = next_named(&mut bob, "MESSAGE_CREATE").await;
    assert_eq!(
        (m["content"].clone(), m["flags"].clone()),
        (json!("secret"), json!(64))
    );
    assert_eq!(m["author"]["id"], f.bot);
    no_event(&mut owner, "MESSAGE_CREATE").await;
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    assert_eq!(
        msgs.as_array().unwrap().len(),
        0,
        "nothing was stored in the channel"
    );
    // the bot can read, edit and delete its ephemeral reply through the token
    let at = format!("/webhooks/{}/{token}/messages/@original", f.app);
    let (s, v) = f.e.get(Who::Anon, &at).await;
    assert_eq!((s, v["content"].clone()), (StatusCode::OK, json!("secret")));
    let (s, v) =
        f.e.patch(
            Who::Anon,
            &at,
            json!({"content": "edited", "embeds": [{"title": "t"}]}),
        )
        .await;
    assert_eq!((s, v["content"].clone()), (StatusCode::OK, json!("edited")));
    let upd = next_named(&mut bob, "MESSAGE_UPDATE").await;
    assert_eq!(upd["embeds"][0]["title"], "t");
    // follow-ups can be ephemeral too
    let (s, v) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{token}?wait=true", f.app),
            json!({"content": "more", "flags": 64}),
        )
        .await;
    assert_eq!((s, v["flags"].clone()), (StatusCode::OK, json!(64)));
    next_named(&mut bob, "MESSAGE_CREATE").await;
    let (s, _) = f.e.delete(Who::Anon, &at, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    next_named(&mut bob, "MESSAGE_DELETE").await;
    assert_eq!(f.e.get(Who::Anon, &at).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deferred_responses_followups_and_edits() {
    let f = fx().await;
    let (id, token) = start(&f, BOB).await;
    let mut bob = f.e.user_events(BOB);
    // a follow-up before any acknowledgement is refused
    let (s, _) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{token}", f.app),
            json!({"content": "early"}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = f.callback(&id, &token, json!({"type": 5})).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let ok = next_named(&mut bob, "INTERACTION_SUCCESS").await;
    assert_eq!(ok["deferred"], true);
    // the first edit of @original creates the message
    let at = format!("/webhooks/{}/{token}/messages/@original", f.app);
    assert_eq!(f.e.get(Who::Anon, &at).await.0, StatusCode::NOT_FOUND);
    let (s, v) =
        f.e.patch(Who::Anon, &at, json!({"content": "done thinking"}))
            .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], "done thinking");
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    assert_eq!(msgs.as_array().unwrap().len(), 1);
    // later edits change the content; embeds of a posted message cannot be replaced yet
    let (s, v) =
        f.e.patch(Who::Anon, &at, json!({"content": "edited"}))
            .await;
    assert_eq!((s, v["content"].clone()), (StatusCode::OK, json!("edited")));
    let (s, v) =
        f.e.patch(Who::Anon, &at, json!({"embeds": [{"title": "x"}]}))
            .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    // a follow-up posts a second message; only the interaction's own messages are reachable
    let (s, v) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{token}?wait=true", f.app),
            json!({"content": "extra"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let extra = v["id"].as_str().unwrap().to_owned();
    let (s, _) =
        f.e.delete(
            Who::Anon,
            &format!("/webhooks/{}/{token}/messages/{extra}", f.app),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, other_token) = {
        let (i, t) = start(&f, BOB).await;
        f.callback(&i, &t, json!({"type": 4, "data": {"content": "mine"}}))
            .await;
        (i, t)
    };
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    let first = msgs
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["content"] == "edited")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (s, v) =
        f.e.delete(
            Who::Anon,
            &format!("/webhooks/{}/{other_token}/messages/{first}", f.app),
            None,
        )
        .await;
    assert_eq!(
        (s, code_of(&v)),
        (StatusCode::NOT_FOUND, 10008),
        "another interaction's message"
    );
    // the token is bound to its application
    let other =
        f.e.svc
            .create_application(OWNER, "Other", "")
            .await
            .unwrap();
    let (s, _) =
        f.e.patch(
            Who::Anon,
            &format!(
                "/webhooks/{}/{token}/messages/@original",
                other.application.id
            ),
            json!({"content": "hijack"}),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // a webhook route with a non-interaction token falls through
    let (s, _) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{}", f.app, "B".repeat(43)),
            json!({"content": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn modals_autocomplete_and_components() {
    let f = fx().await;
    let mut bob = f.e.user_events(BOB);
    // modal
    let (id, token) = start(&f, BOB).await;
    let modal = json!({"custom_id": "m1", "title": "Tell me", "components": [{"type": 1, "components": [
        {"type": 4, "custom_id": "name", "style": 1, "label": "Name"}]}]});
    assert_eq!(
        f.callback(
            &id,
            &token,
            json!({"type": 9, "data": {"custom_id": "", "title": "t", "components": []}})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.callback(&id, &token, json!({"type": 9, "data": modal}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let ev = next_named(&mut bob, "INTERACTION_MODAL_CREATE").await;
    assert_eq!(
        (ev["custom_id"].clone(), ev["title"].clone()),
        (json!("m1"), json!("Tell me"))
    );
    // submitting it is another interaction
    let mut bot_events = f.e.user_events(f.bot.parse().unwrap());
    let (s, v) =
        f.e.post(
            Who::Session(BOB),
            "/interactions",
            json!({"type": 5, "application_id": f.app, "channel_id": f.e.ch.to_string(),
                   "data": {"custom_id": "m1", "components": [{"type": 1, "components": [
                       {"type": 4, "custom_id": "name", "value": "Bob"}]}]}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(ev["type"], 5);
    assert_eq!(ev["data"]["components"][0]["components"][0]["value"], "Bob");
    // autocomplete
    let (_, v) = f
        .e
        .post(
            Who::Bot(&f.token),
            &format!("/applications/{}/commands", f.app),
            json!({"name": "find", "description": "d", "options": [
                {"type": 3, "name": "q", "description": "d", "required": true, "autocomplete": true}]}),
        )
        .await;
    let find = v["id"].as_str().unwrap().to_owned();
    let (s, v) = f
        .e
        .post(
            Who::Session(BOB),
            "/interactions",
            json!({"type": 4, "application_id": f.app, "channel_id": f.e.ch.to_string(),
                   "data": {"id": find, "options": [{"type": 3, "name": "q", "value": "he", "focused": true}]}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(ev["type"], 4);
    assert_eq!(ev["data"]["options"][0]["focused"], true);
    let (iid, itoken) = (
        ev["id"].as_str().unwrap().to_owned(),
        ev["token"].as_str().unwrap().to_owned(),
    );
    assert_eq!(
        f.callback(&iid, &itoken, json!({"type": 4, "data": {"content": "x"}}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.callback(
            &iid,
            &itoken,
            json!({"type": 8, "data": {"choices": [{"name": "Hello", "value": "hello"}]}})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let ev = next_named(&mut bob, "SR_INTERACTION_AUTOCOMPLETE").await;
    assert_eq!(ev["choices"][0]["value"], "hello");

    // components: the bot posts a message with a button, the member presses it
    let (s, v) =
        f.e.post(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
            json!({"content": "Pick", "components": [{"type": 1, "components": [
                {"type": 2, "style": 1, "label": "Go", "custom_id": "go"}]}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let mid = v["id"].as_str().unwrap().to_owned();
    let press = |custom: &str, message: &str| {
        json!({"type": 3, "application_id": f.app, "channel_id": f.e.ch.to_string(), "message_id": message,
               "data": {"custom_id": custom, "component_type": 2}})
    };
    let (s, v) =
        f.e.post(Who::Session(BOB), "/interactions", press("go", &mid))
            .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(
        (ev["type"].clone(), ev["data"]["custom_id"].clone()),
        (json!(3), json!("go"))
    );
    assert_eq!(ev["message"]["id"], json!(mid));
    // update-message edits the content of the pressed message
    let (iid, itoken) = (
        ev["id"].as_str().unwrap().to_owned(),
        ev["token"].as_str().unwrap().to_owned(),
    );
    assert_eq!(
        f.callback(
            &iid,
            &itoken,
            json!({"type": 7, "data": {"content": "Picked"}})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, m) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages/{mid}", f.e.ch),
        )
        .await;
    assert_eq!(m["content"], "Picked");
    // a component that is not on the message, and a message that is not the bot's
    let (s, v) =
        f.e.post(Who::Session(BOB), "/interactions", press("forged", &mid))
            .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    let (_, theirs) =
        f.e.post(
            Who::Session(BOB),
            &format!("/channels/{}/messages", f.e.ch),
            json!({"content": "mine"}),
        )
        .await;
    let (s, v) =
        f.e.post(
            Who::Session(BOB),
            "/interactions",
            press("go", theirs["id"].as_str().unwrap()),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035), "{v}");
}

#[tokio::test]
async fn responses_with_secrets_are_not_cacheable() {
    let e = env().await;
    let (s, h, _) = e
        .raw(
            Who::Session(OWNER),
            Method::POST,
            "/applications",
            Some("application/json"),
            json!({"name": "Cache Test"}).to_string().into_bytes(),
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(h["cache-control"], "no-store");
    let (id, ..) = e.new_app().await;
    for path in [
        format!("/applications/{id}/reset-secret"),
        format!("/applications/{id}/bot/reset-token"),
    ] {
        let (s, h, _) = e
            .raw(
                Who::Session(OWNER),
                Method::POST,
                &path,
                Some("application/json"),
                json!({"password": "correct"}).to_string().into_bytes(),
                &[],
            )
            .await;
        assert_eq!(
            (s, h["cache-control"].to_str().unwrap()),
            (StatusCode::OK, "no-store"),
            "{path}"
        );
    }
}

// ======================================================================= web client helpers

#[tokio::test]
async fn a_component_press_finds_its_application_from_the_message_author() {
    let f = fx().await;
    let mut bot_events = f.e.user_events(f.bot.parse().unwrap());
    let (s, v) =
        f.e.post(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
            json!({"content": "Pick", "components": [{"type": 1, "components": [
                {"type": 2, "style": 1, "label": "Go", "custom_id": "go"}]}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let mid = v["id"].as_str().unwrap().to_owned();
    // no application_id at all: the web client does not know it
    let (s, v) =
        f.e.post(
            Who::Session(BOB),
            "/interactions",
            json!({"type": 3, "channel_id": f.e.ch.to_string(), "message_id": mid,
                   "data": {"custom_id": "go", "component_type": 2}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(ev["application_id"], json!(f.app));
    assert_eq!(ev["data"]["custom_id"], "go");
    // a message that is a person's, not an application's, is refused
    let (_, theirs) =
        f.e.post(
            Who::Session(BOB),
            &format!("/channels/{}/messages", f.e.ch),
            json!({"content": "mine"}),
        )
        .await;
    let (s, v) =
        f.e.post(
            Who::Session(BOB),
            "/interactions",
            json!({"type": 3, "channel_id": f.e.ch.to_string(), "message_id": theirs["id"],
                   "data": {"custom_id": "go", "component_type": 2}}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035), "{v}");
    // and without a message id there is nothing to resolve
    let (s, _) =
        f.e.post(
            Who::Session(BOB),
            "/interactions",
            json!({"type": 3, "channel_id": f.e.ch.to_string(),
                   "data": {"custom_id": "go", "component_type": 2}}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_command_index_lists_what_the_caller_may_use() {
    let f = fx().await;
    let path = format!("/channels/{}/application-commands", f.e.ch);
    // a command only administrators may use (default_member_permissions = ADMINISTRATOR)
    let (s, v) =
        f.e.post(
            Who::Bot(&f.token),
            &format!("/applications/{}/commands", f.app),
            json!({"name": "secret", "description": "Admins only", "default_member_permissions": "8"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");

    let (s, v) = f.e.get(Who::Session(BOB), &path).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["applications"][0]["id"], json!(f.app));
    assert_eq!(v["applications"][0]["bot_id"], json!(f.bot));
    let names: Vec<&str> = v["application_commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["echo"], "bob cannot see the admin command");
    assert_eq!(v["application_commands"][0]["application_id"], json!(f.app));
    assert_eq!(v["application_commands"][0]["options"][0]["name"], "text");

    // the owner of the server is an administrator
    let (_, v) = f.e.get(Who::Session(OWNER), &path).await;
    assert_eq!(v["application_commands"].as_array().unwrap().len(), 2);

    // signed out, not a member, a bot
    assert_eq!(f.e.get(Who::Anon, &path).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        f.e.get(Who::Session(CAROL), &path).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.e.get(Who::Bot(&f.token), &path).await.0,
        StatusCode::FORBIDDEN
    );
    // an application that is not installed offers nothing
    let (other, _, _, _) = f.e.new_app().await;
    let _ = other;
    let (_, v) = f.e.get(Who::Session(BOB), &path).await;
    assert_eq!(v["applications"].as_array().unwrap().len(), 1);

    // a suspended application disappears
    let (st, _) =
        f.e.post(
            Who::Session(super::tests::STAFF),
            &format!("/admin/developers/apps/{}/suspend", f.app),
            json!({"reason": "abuse"}),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (_, v) = f.e.get(Who::Session(BOB), &path).await;
    assert_eq!(v["application_commands"], json!([]));
}

#[tokio::test]
async fn gateway_bot_recommends_one_shard_per_thousand_servers() {
    let f = fx().await;
    let (s, v) = f.e.get(Who::Bot(&f.token), "/gateway/bot").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["shards"], 1);
    assert_eq!(v["session_start_limit"]["max_concurrency"], 1);
}

// ---------------------------------------------------------- SEND_MESSAGES (G-03, D-206)

#[tokio::test]
async fn channel_visible_responses_need_send_messages_but_ephemeral_ones_do_not() {
    // a bot that can see the channel but is denied Send Messages in it
    let f = fx().await;
    f.e.backend
        .data()
        .overwrites
        .entry(f.e.ch)
        .or_default()
        .push(sr_core::permissions::Overwrite {
            id: f.bot.parse::<u64>().unwrap().into(),
            kind: sr_core::permissions::OverwriteKind::Member,
            allow: Permissions::empty(),
            deny: Permissions::SEND_MESSAGES,
        });
    let mut bob = f.e.user_events(BOB);

    // a channel-visible response is refused with 50013 naming the channel
    let (id, token) = start(&f, BOB).await;
    let (s, v) = f
        .callback(&id, &token, json!({"type": 4, "data": {"content": "hi"}}))
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let msg = v["message"].as_str().unwrap();
    assert!(msg.contains("Send Messages") && msg.contains('#'), "{msg}");
    next_named(&mut bob, "INTERACTION_FAILURE").await;
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    assert_eq!(msgs.as_array().unwrap().len(), 0);

    // an ephemeral response skips the check
    let (id, token) = start(&f, BOB).await;
    let (s, v) = f
        .callback(
            &id,
            &token,
            json!({"type": 4, "data": {"content": "psst", "flags": 64}}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let m = next_named(&mut bob, "MESSAGE_CREATE").await;
    assert_eq!(m["content"], "psst");

    // a deferred response is fine; the check applies when the visible follow-up resolves it
    let (id, token) = start(&f, BOB).await;
    assert_eq!(
        f.callback(&id, &token, json!({"type": 5})).await.0,
        StatusCode::NO_CONTENT
    );
    let (s, v) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{token}", f.app),
            json!({"content": "done"}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let (s, _) =
        f.e.post(
            Who::Anon,
            &format!("/webhooks/{}/{token}", f.app),
            json!({"content": "done", "flags": 64}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn channel_visible_responses_work_when_the_bot_may_send() {
    let f = fx().await;
    let (id, token) = start(&f, BOB).await;
    let (s, v) = f
        .callback(&id, &token, json!({"type": 4, "data": {"content": "hi"}}))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let (_, msgs) =
        f.e.get(
            Who::Bot(&f.token),
            &format!("/channels/{}/messages", f.e.ch),
        )
        .await;
    assert_eq!(msgs.as_array().unwrap().len(), 1);
}

// ============================================================ attachment options (DEV-BOT-008)

#[tokio::test]
async fn an_attachment_option_resolves_to_the_invokers_own_staged_upload() {
    use bytes::Bytes;
    use sr_storage::ObjectStorage;
    let f = fx().await;
    let (s, v) =
        f.e.post(
            Who::Bot(&f.token),
            &format!("/applications/{}/commands", f.app),
            json!({"name": "scan", "description": "Scan a file", "options": [
                {"type": 11, "name": "file", "description": "The file", "required": true}]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let cmd = v["id"].as_str().unwrap().to_owned();

    // Bob stages an upload in the channel (slot, PUT, no message yet).
    let stage = |who: i64, name: &'static str| {
        let f = &f;
        async move {
            let (s, v) =
                f.e.post(
                    Who::Session(who),
                    &format!("/channels/{}/attachments", f.e.ch),
                    json!({"files": [{"filename": name, "file_size": 5}]}),
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{v}");
            let slot = v["attachments"][0].clone();
            let key = format!(
                "chat/{}/{}/{name}",
                f.e.ch,
                slot["upload_id"].as_str().unwrap()
            );
            f.e.storage
                .put(&key, Bytes::from_static(b"hello"))
                .await
                .unwrap();
            slot["upload_id"].as_str().unwrap().to_owned()
        }
    };
    let bobs = stage(BOB, "notes.txt").await;
    let owners = stage(OWNER, "other.txt").await;

    let invoke = |who: i64, id: String| {
        let f = &f;
        let cmd = cmd.clone();
        async move {
            f.e.post(
                Who::Session(who),
                "/interactions",
                json!({"type": 2, "application_id": f.app, "channel_id": f.e.ch.to_string(),
                       "data": {"id": cmd, "options": [{"type": 11, "name": "file", "value": id}]}}),
            )
            .await
        }
    };

    // Someone else's upload, or an id that is no upload, is refused.
    for bad in [owners.clone(), "123456789".to_owned()] {
        let (s, v) = invoke(BOB, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    }

    let mut bot_events = f.e.user_events(f.bot.parse().unwrap());
    let (s, v) = invoke(BOB, bobs.clone()).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ev = next_named(&mut bot_events, "INTERACTION_CREATE").await;
    assert_eq!(
        ev["data"]["options"][0],
        json!({"name": "file", "type": 11, "value": bobs})
    );
    let a = &ev["data"]["resolved"]["attachments"][bobs.as_str()];
    assert_eq!(a["filename"], "notes.txt");
    assert_eq!(a["size"], 5);
    assert!(
        a["content_type"].is_string(),
        "sniffed at resolve time: {a}"
    );
    assert!(
        a["url"].as_str().unwrap().starts_with("/cdn/attachments/"),
        "{a}"
    );
}

// ======================================================================= integrations (J-05)

#[tokio::test]
async fn the_integrations_list_shows_installed_applications_and_managers_remove_them() {
    let e = env().await;
    let (id, bot, _token, _) = e.new_app().await;
    e.configure(&id).await;
    e.install(&id, &bot, 0).await;
    let path = format!("/guilds/{}/integrations", e.guild);

    // Needs MANAGE_GUILD: a plain member and an outsider are refused.
    let (s, v) = e.get(Who::Session(BOB), &path).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50013), "{v}");
    let (s, v) = e.get(Who::Session(CAROL), &path).await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001), "{v}");

    let (s, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 1, "{v}");
    let one = &list[0];
    assert_eq!(one["id"], id);
    assert_eq!(one["type"], "discord");
    assert_eq!(one["enabled"], true);
    assert_eq!(one["application"]["id"], id);
    assert_eq!(one["application"]["bot"]["id"], bot);
    assert_eq!(one["user"]["id"], OWNER.to_string());
    assert!(
        one["scopes"].as_array().unwrap().iter().any(|s| s == "bot"),
        "{one}"
    );

    // Removing needs the same permission; a second removal is unknown.
    let item = format!("{path}/{id}");
    let (s, _) = e.delete(Who::Session(BOB), &item, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = e.delete(Who::Session(OWNER), &item, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!(v.as_array().map(Vec::len), Some(0), "{v}");
    let (s, v) = e.delete(Who::Session(OWNER), &item, None).await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10005), "{v}");
    let (s, v) = e
        .delete(Who::Session(OWNER), &format!("{path}/12345"), None)
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10005), "{v}");

    // INTEGRATION_DELETE (82) names the application.
    let log = e
        .settings
        .list_audit(&sr_chat::settings::audit::AuditQuery::new(e.guild))
        .await
        .unwrap();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].action.code(), 82);
    assert_eq!(log[0].actor_id.get(), OWNER as u64);
    assert_eq!(log[0].target_id, id.parse::<u64>().ok());
}
