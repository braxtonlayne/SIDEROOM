//! The providers of account connections (LNG-USET-024): where to send the user, what to
//! ask for, and how to read the answer. Everything here is pure; the HTTP lives in
//! `connections.rs` and goes through the SSRF-safe `sr_net` client.
//!
//! Verified connections prove the account with the provider once and keep only the public
//! identity (id, name, link) and a few public numbers. No access token is stored.
//! Providers without a usable OAuth flow (Xbox, TikTok, Roblox and others) are *link-only*:
//! the member types a name and the connection stays unverified.

use serde_json::{Map, Value, json};

/// How the user proves the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Authorization code grant.
    OAuth2 {
        auth_url: &'static str,
        token_url: &'static str,
        scopes: &'static [&'static str],
        /// Send a PKCE S256 challenge.
        pkce: bool,
        /// The token endpoint wants the client id and secret as HTTP basic credentials.
        basic_auth: bool,
        /// Constant extra query parameters on the authorize URL.
        extra: &'static [(&'static str, &'static str)],
    },
    /// OpenID 2.0 against Steam.
    SteamOpenId,
}

/// A provider that can verify an account.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    pub flow: Flow,
    /// The user-info endpoint called with the access token.
    pub api_url: &'static str,
    /// The API also wants the OAuth client id as a `Client-Id` header (Twitch).
    pub client_id_header: bool,
}

/// Providers with a verified flow, in the order the settings page lists them.
pub const OAUTH: &[Spec] = &[
    Spec {
        id: "github",
        name: "GitHub",
        flow: Flow::OAuth2 {
            auth_url: "https://github.com/login/oauth/authorize",
            token_url: "https://github.com/login/oauth/access_token",
            scopes: &["read:user"],
            pkce: true,
            basic_auth: false,
            extra: &[],
        },
        api_url: "https://api.github.com/user",
        client_id_header: false,
    },
    Spec {
        id: "discord",
        name: "Discord",
        flow: Flow::OAuth2 {
            auth_url: "https://discord.com/oauth2/authorize",
            token_url: "https://discord.com/api/oauth2/token",
            scopes: &["identify"],
            pkce: true,
            basic_auth: false,
            extra: &[("prompt", "none")],
        },
        api_url: "https://discord.com/api/users/@me",
        client_id_header: false,
    },
    Spec {
        id: "spotify",
        name: "Spotify",
        flow: Flow::OAuth2 {
            auth_url: "https://accounts.spotify.com/authorize",
            token_url: "https://accounts.spotify.com/api/token",
            scopes: &[],
            pkce: true,
            basic_auth: true,
            extra: &[],
        },
        api_url: "https://api.spotify.com/v1/me",
        client_id_header: false,
    },
    Spec {
        id: "twitch",
        name: "Twitch",
        flow: Flow::OAuth2 {
            auth_url: "https://id.twitch.tv/oauth2/authorize",
            token_url: "https://id.twitch.tv/oauth2/token",
            scopes: &[],
            pkce: false,
            basic_auth: false,
            extra: &[],
        },
        api_url: "https://api.twitch.tv/helix/users",
        client_id_header: true,
    },
    Spec {
        id: "youtube",
        name: "YouTube",
        flow: Flow::OAuth2 {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            scopes: &["https://www.googleapis.com/auth/youtube.readonly"],
            pkce: true,
            basic_auth: false,
            extra: &[("access_type", "online")],
        },
        api_url: "https://www.googleapis.com/youtube/v3/channels?part=snippet%2Cstatistics&mine=true",
        client_id_header: false,
    },
    Spec {
        id: "reddit",
        name: "Reddit",
        flow: Flow::OAuth2 {
            auth_url: "https://www.reddit.com/api/v1/authorize",
            token_url: "https://www.reddit.com/api/v1/access_token",
            scopes: &["identity"],
            pkce: false,
            basic_auth: true,
            extra: &[("duration", "temporary")],
        },
        api_url: "https://oauth.reddit.com/api/v1/me",
        client_id_header: false,
    },
    Spec {
        id: "steam",
        name: "Steam",
        flow: Flow::SteamOpenId,
        api_url: "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/",
        client_id_header: false,
    },
];

/// Providers a member can only link by hand (unverified): `(id, title, needs a URL)`.
pub const LINK_ONLY: &[(&str, &str, bool)] = &[
    ("website", "Website", true),
    ("xbox", "Xbox", false),
    ("playstation", "PlayStation", false),
    ("nintendo", "Nintendo", false),
    ("roblox", "Roblox", false),
    ("tiktok", "TikTok", false),
    ("instagram", "Instagram", false),
    ("twitter", "X (Twitter)", false),
    ("mastodon", "Mastodon", false),
    ("bluesky", "Bluesky", false),
    ("battlenet", "Battle.net", false),
    ("epic-games", "Epic Games", false),
];

/// The verified provider named `id`.
pub fn oauth_spec(id: &str) -> Option<&'static Spec> {
    OAUTH.iter().find(|s| s.id == id)
}

/// The link-only provider named `id`.
pub fn link_spec(id: &str) -> Option<&'static (&'static str, &'static str, bool)> {
    LINK_ONLY.iter().find(|s| s.0 == id)
}

/// What a provider told us about the account.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub external_id: String,
    pub name: String,
    pub url: Option<String>,
    /// Public numbers only (`{"followers": 12}`).
    pub stats: Value,
}

fn text(v: &Value, max: usize) -> Option<String> {
    let s = match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!s.is_empty() && !s.chars().any(char::is_control)).then(|| s.chars().take(max).collect())
}

fn num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
    .filter(|n| *n >= 0)
}

/// Keep only the numbers that are present.
fn stats(pairs: &[(&str, Option<i64>)]) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        if let Some(v) = v {
            m.insert((*k).to_owned(), json!(v));
        }
    }
    Value::Object(m)
}

/// A handle that is safe to put in a URL path.
fn handle(s: &str) -> Option<&str> {
    (!s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
    .then_some(s)
}

/// Read the user-info document of `provider` (the body of [`Spec::api_url`]).
pub fn parse_identity(provider: &str, body: &Value) -> Option<Identity> {
    match provider {
        "github" => {
            let login = text(&body["login"], 64)?;
            Some(Identity {
                external_id: text(&body["id"], 128)?,
                url: handle(&login).map(|h| format!("https://github.com/{h}")),
                name: login,
                stats: stats(&[
                    ("public_repos", num(&body["public_repos"])),
                    ("followers", num(&body["followers"])),
                    ("following", num(&body["following"])),
                ]),
            })
        }
        "discord" => {
            let id = text(&body["id"], 32)?;
            let name = text(&body["global_name"], 100).or_else(|| text(&body["username"], 100))?;
            let valid = id.bytes().all(|b| b.is_ascii_digit());
            Some(Identity {
                url: valid.then(|| format!("https://discord.com/users/{id}")),
                external_id: id,
                name,
                stats: json!({}),
            })
        }
        "spotify" => {
            let id = text(&body["id"], 128)?;
            let name = text(&body["display_name"], 100).unwrap_or_else(|| id.clone());
            Some(Identity {
                url: handle(&id).map(|h| format!("https://open.spotify.com/user/{h}")),
                external_id: id,
                name,
                stats: stats(&[("followers", num(&body["followers"]["total"]))]),
            })
        }
        "twitch" => {
            let u = body["data"].get(0)?;
            let login = text(&u["login"], 64)?;
            let name = text(&u["display_name"], 100).unwrap_or_else(|| login.clone());
            Some(Identity {
                external_id: text(&u["id"], 128)?,
                url: handle(&login).map(|h| format!("https://www.twitch.tv/{h}")),
                name,
                stats: json!({}),
            })
        }
        "youtube" => {
            let c = body["items"].get(0)?;
            let id = text(&c["id"], 128)?;
            let hidden = c["statistics"]["hiddenSubscriberCount"] == json!(true);
            Some(Identity {
                url: handle(&id).map(|h| format!("https://www.youtube.com/channel/{h}")),
                name: text(&c["snippet"]["title"], 100).unwrap_or_else(|| id.clone()),
                stats: stats(&[
                    (
                        "subscribers",
                        if hidden {
                            None
                        } else {
                            num(&c["statistics"]["subscriberCount"])
                        },
                    ),
                    ("videos", num(&c["statistics"]["videoCount"])),
                ]),
                external_id: id,
            })
        }
        "reddit" => {
            let name = text(&body["name"], 64)?;
            Some(Identity {
                external_id: text(&body["id"], 128)?,
                url: handle(&name).map(|h| format!("https://www.reddit.com/user/{h}")),
                stats: stats(&[("karma", num(&body["total_karma"]))]),
                name,
            })
        }
        _ => None,
    }
}

/// The public endpoint that refreshes the stats of a stored connection without a token, for
/// the providers that have one (`github`, `reddit`).
pub fn refresh_url(provider: &str, external_id: &str, name: &str) -> Option<String> {
    match provider {
        "github" if external_id.bytes().all(|b| b.is_ascii_digit()) => {
            Some(format!("https://api.github.com/user/{external_id}"))
        }
        "reddit" => handle(name).map(|h| format!("https://www.reddit.com/user/{h}/about.json")),
        _ => None,
    }
}

/// Read the stats from the body of [`refresh_url`].
pub fn parse_refresh(provider: &str, body: &Value) -> Option<Value> {
    match provider {
        "github" => parse_identity("github", body).map(|i| i.stats),
        "reddit" => Some(stats(&[("karma", num(&body["data"]["total_karma"]))])),
        _ => None,
    }
}

// ---- Steam OpenID 2.0 ---------------------------------------------------------------------

/// Steam's OpenID endpoint.
pub const STEAM_OPENID: &str = "https://steamcommunity.com/openid/login";

/// Query parameters that send the browser to Steam. `return_to` must carry our `state`.
pub fn steam_redirect_params(realm: &str, return_to: &str) -> Vec<(String, String)> {
    let ns = "http://specs.openid.net/auth/2.0";
    vec![
        ("openid.ns".into(), ns.into()),
        ("openid.mode".into(), "checkid_setup".into()),
        ("openid.return_to".into(), return_to.into()),
        ("openid.realm".into(), realm.into()),
        ("openid.identity".into(), format!("{ns}/identifier_select")),
        (
            "openid.claimed_id".into(),
            format!("{ns}/identifier_select"),
        ),
    ]
}

/// The 17 digit Steam id in an OpenID `claimed_id`.
pub fn steam_id(claimed_id: &str) -> Option<String> {
    let id = claimed_id.strip_prefix("https://steamcommunity.com/openid/id/")?;
    (id.len() == 17 && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.to_owned())
}

/// What the callback of a Steam login must satisfy before Steam is asked to confirm the
/// signature: Steam signed it, for our `return_to`, with an identity that is a Steam id.
/// Returns the Steam id and the form for the `check_authentication` POST.
pub fn steam_check(
    params: &[(String, String)],
    expected_return_to: &str,
) -> Option<(String, Vec<(String, String)>)> {
    let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    if get("openid.mode")? != "id_res"
        || get("openid.op_endpoint")? != STEAM_OPENID
        || get("openid.return_to")? != expected_return_to
    {
        return None;
    }
    let id = steam_id(get("openid.claimed_id")?)?;
    // The signed list must cover the fields we rely on.
    let signed: Vec<&str> = get("openid.signed")?.split(',').collect();
    for needed in [
        "claimed_id",
        "identity",
        "return_to",
        "op_endpoint",
        "response_nonce",
    ] {
        if !signed.contains(&needed) {
            return None;
        }
    }
    let mut form: Vec<(String, String)> = params
        .iter()
        .filter(|(k, _)| k.starts_with("openid.") && k != "openid.mode")
        .cloned()
        .collect();
    form.push(("openid.mode".into(), "check_authentication".into()));
    Some((id, form))
}

/// Steam answers a `check_authentication` with an `is_valid:true` line.
pub fn steam_is_valid(body: &str) -> bool {
    body.lines().any(|l| l.trim() == "is_valid:true")
}

/// The Steam Web API `GetPlayerSummaries` document: persona name and public link.
pub fn parse_steam_summary(steam_id: &str, body: &Value) -> Identity {
    let p = &body["response"]["players"][0];
    let name = text(&p["personaname"], 100).unwrap_or_else(|| steam_id.to_owned());
    Identity {
        external_id: steam_id.to_owned(),
        name,
        url: Some(format!("https://steamcommunity.com/profiles/{steam_id}")),
        stats: json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_identity_uses_a_constructed_link_and_public_numbers() {
        let i = parse_identity(
            "github",
            &json!({"id": 583231, "login": "octocat", "html_url": "javascript:alert(1)",
                    "public_repos": 8, "followers": 3938, "following": 9, "email": "x@y.z"}),
        )
        .unwrap();
        assert_eq!(i.external_id, "583231");
        assert_eq!(i.name, "octocat");
        assert_eq!(i.url.as_deref(), Some("https://github.com/octocat"));
        assert_eq!(
            i.stats,
            json!({"public_repos": 8, "followers": 3938, "following": 9})
        );
    }

    #[test]
    fn odd_handles_get_no_link() {
        let i = parse_identity("github", &json!({"id": 1, "login": "a/b?c"})).unwrap();
        assert!(i.url.is_none());
        assert!(
            parse_identity("github", &json!({"login": "x"})).is_none(),
            "no id"
        );
        assert!(parse_identity("github", &json!({"id": 1, "login": "\u{7}"})).is_none());
    }

    #[test]
    fn each_provider_document_is_read() {
        let d = parse_identity(
            "discord",
            &json!({"id": "80351110224678912", "username": "nelly", "global_name": "Nelly"}),
        )
        .unwrap();
        assert_eq!(
            (d.name.as_str(), d.url.as_deref()),
            ("Nelly", Some("https://discord.com/users/80351110224678912"))
        );

        let s = parse_identity(
            "spotify",
            &json!({"id": "wizzler", "display_name": null, "followers": {"total": 12}}),
        )
        .unwrap();
        assert_eq!(s.name, "wizzler");
        assert_eq!(s.stats, json!({"followers": 12}));

        let t = parse_identity("twitch", &json!({"data": [{"id": "141981764", "login": "twitchdev", "display_name": "TwitchDev"}]})).unwrap();
        assert_eq!(
            (t.external_id.as_str(), t.name.as_str()),
            ("141981764", "TwitchDev")
        );
        assert_eq!(t.url.as_deref(), Some("https://www.twitch.tv/twitchdev"));
        assert!(parse_identity("twitch", &json!({"data": []})).is_none());

        let y = parse_identity("youtube", &json!({"items": [{"id": "UC123", "snippet": {"title": "My Channel"},
            "statistics": {"subscriberCount": "1500", "videoCount": "42", "hiddenSubscriberCount": false}}]})).unwrap();
        assert_eq!(y.stats, json!({"subscribers": 1500, "videos": 42}));
        let hidden = parse_identity("youtube", &json!({"items": [{"id": "UC1", "snippet": {"title": "T"},
            "statistics": {"subscriberCount": "5", "videoCount": "1", "hiddenSubscriberCount": true}}]})).unwrap();
        assert_eq!(
            hidden.stats,
            json!({"videos": 1}),
            "hidden subscriber counts stay hidden"
        );

        let r = parse_identity(
            "reddit",
            &json!({"id": "abc1", "name": "spez", "total_karma": 100}),
        )
        .unwrap();
        assert_eq!(r.url.as_deref(), Some("https://www.reddit.com/user/spez"));
        assert_eq!(r.stats, json!({"karma": 100}));
        assert!(parse_identity("nope", &json!({})).is_none());
    }

    #[test]
    fn refresh_urls_only_exist_for_token_free_providers() {
        assert_eq!(
            refresh_url("github", "583231", "octocat").as_deref(),
            Some("https://api.github.com/user/583231")
        );
        assert!(refresh_url("github", "../x", "octocat").is_none());
        assert_eq!(
            refresh_url("reddit", "abc", "spez").as_deref(),
            Some("https://www.reddit.com/user/spez/about.json")
        );
        assert!(refresh_url("reddit", "abc", "a b").is_none());
        assert!(refresh_url("spotify", "x", "y").is_none());
        assert_eq!(
            parse_refresh("reddit", &json!({"data": {"total_karma": 7}})),
            Some(json!({"karma": 7}))
        );
        assert_eq!(
            parse_refresh("github", &json!({"id": 1, "login": "a", "followers": 2})),
            Some(json!({"followers": 2}))
        );
    }

    fn steam_params(id_url: &str) -> Vec<(String, String)> {
        [
            ("openid.ns", "http://specs.openid.net/auth/2.0"),
            ("openid.mode", "id_res"),
            ("openid.op_endpoint", STEAM_OPENID),
            ("openid.claimed_id", id_url),
            ("openid.identity", id_url),
            (
                "openid.return_to",
                "https://room.test/api/v1/connections/steam/callback?state=S",
            ),
            ("openid.response_nonce", "2026-10-15T10:00:00Zabc"),
            ("openid.assoc_handle", "1234567890"),
            (
                "openid.signed",
                "signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle",
            ),
            ("openid.sig", "c2ln"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
    }

    const RETURN_TO: &str = "https://room.test/api/v1/connections/steam/callback?state=S";

    #[test]
    fn steam_callback_is_checked_before_steam_is_asked() {
        let p = steam_params("https://steamcommunity.com/openid/id/76561197960435530");
        let (id, form) = steam_check(&p, RETURN_TO).unwrap();
        assert_eq!(id, "76561197960435530");
        assert!(form.contains(&("openid.mode".into(), "check_authentication".into())));
        assert_eq!(form.iter().filter(|(k, _)| k == "openid.mode").count(), 1);
        assert!(form.iter().any(|(k, _)| k == "openid.sig"));

        // Another return_to, another provider, a non-Steam identity, unsigned fields: all refused.
        assert!(steam_check(&p, "https://evil.test/cb?state=S").is_none());
        assert!(
            steam_check(
                &steam_params("https://evil.test/openid/id/76561197960435530"),
                RETURN_TO
            )
            .is_none()
        );
        assert!(
            steam_check(
                &steam_params("https://steamcommunity.com/openid/id/123"),
                RETURN_TO
            )
            .is_none()
        );
        let mut p2 = p.clone();
        p2.iter_mut()
            .find(|(k, _)| k == "openid.op_endpoint")
            .unwrap()
            .1 = "https://evil.test/login".into();
        assert!(steam_check(&p2, RETURN_TO).is_none());
        let mut p3 = p.clone();
        p3.iter_mut().find(|(k, _)| k == "openid.signed").unwrap().1 = "signed,return_to".into();
        assert!(steam_check(&p3, RETURN_TO).is_none());
        let mut p4 = p;
        p4.iter_mut().find(|(k, _)| k == "openid.mode").unwrap().1 = "cancel".into();
        assert!(steam_check(&p4, RETURN_TO).is_none());
    }

    #[test]
    fn steam_validity_and_summary() {
        assert!(steam_is_valid(
            "ns:http://specs.openid.net/auth/2.0\nis_valid:true\n"
        ));
        assert!(!steam_is_valid(
            "ns:http://specs.openid.net/auth/2.0\nis_valid:false\n"
        ));
        assert!(!steam_is_valid("is_valid:truely"));
        let i = parse_steam_summary(
            "7656",
            &json!({"response": {"players": [{"personaname": "Gabe"}]}}),
        );
        assert_eq!(i.name, "Gabe");
        let anon = parse_steam_summary("7656", &json!({}));
        assert_eq!(anon.name, "7656");
        assert_eq!(
            anon.url.as_deref(),
            Some("https://steamcommunity.com/profiles/7656")
        );
    }

    #[test]
    fn the_registry_is_consistent() {
        for s in OAUTH {
            assert!(oauth_spec(s.id).is_some());
            assert!(
                link_spec(s.id).is_none(),
                "{} is either verified or link-only",
                s.id
            );
            if let Flow::OAuth2 {
                auth_url,
                token_url,
                ..
            } = s.flow
            {
                assert!(auth_url.starts_with("https://") && token_url.starts_with("https://"));
            }
        }
        assert!(link_spec("roblox").is_some() && oauth_spec("roblox").is_none());
    }
}
