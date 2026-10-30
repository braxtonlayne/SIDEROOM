//! Per-server feature module endpoints (docs/inventory/09-feature-modules.md) over the
//! settings test environment.

use super::*;
use crate::routes::settings::features::{GuildFeatures, RoleAssigner};
use async_trait::async_trait;
use sr_chat::{ChatError, ChatResult};
use sr_features::testing::FakeActions;
use sr_features::{FeatureCtx, FeatureRegistry, FeatureService, InMemorySettingsStore};

/// Gives roles by editing the in-memory members, refusing like the real backend when
/// the actor lacks `MANAGE_ROLES` (the hierarchy itself is the backend's job and has
/// its own tests).
struct MemoryRoles {
    backend: MemoryBackend,
}

#[async_trait]
impl RoleAssigner for MemoryRoles {
    async fn assign(
        &self,
        actor: UserId,
        _guild: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<()> {
        if actor.get() != OWNER {
            return Err(ChatError::MissingPermissions);
        }
        let mut d = self.backend.data();
        let m = d
            .members
            .iter_mut()
            .find(|m| m.user_id == user)
            .ok_or(ChatError::UnknownMember)?;
        m.role_ids.retain(|r| *r != role);
        if add {
            m.role_ids.push(role);
        }
        Ok(())
    }
}

pub(super) fn wire(
    backend: &MemoryBackend,
    clock: Arc<dyn Clock>,
    bus: Arc<InProcessBus>,
    actions: Arc<FakeActions>,
) -> GuildFeatures {
    let mut registry = FeatureRegistry::new();
    sr_features::modules::register_all(&mut registry).unwrap();
    registry
        .register(sr_features::ModerationReportsFeature)
        .unwrap();
    registry.register(sr_userfeat::LevelingFeature).unwrap();
    let settings = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    GuildFeatures {
        ctx: FeatureCtx {
            bus,
            settings,
            clock,
            actions,
            state: Arc::new(sr_features::state::InMemoryState::new()),
        },
        roles: Arc::new(MemoryRoles {
            backend: backend.clone(),
        }),
    }
}

async fn env_features() -> Env {
    env_with(Opts {
        features: true,
        ..Opts::default()
    })
    .await
}

fn add_role(e: &Env, name: &str, position: i32) -> RoleId {
    let id = RoleId::new(0x5000 + u64::try_from(position).unwrap());
    e.backend.data().roles.push(Role {
        id,
        guild_id: e.guild,
        name: name.into(),
        color: 0,
        secondary_color: None,
        tertiary_color: None,
        hoist: false,
        position,
        permissions: Permissions::empty(),
        mentionable: false,
        managed: false,
        unicode_emoji: None,
        icon_key: None,
    });
    id
}

#[tokio::test]
async fn the_public_compliance_object_names_the_initial_invite_credits() {
    let e = env_features().await;
    let (s, v) = e
        .call(Method::GET, "/sr/compliance/public", None, None)
        .await;
    assert_eq!(s, StatusCode::OK);
    // No auth service is wired in this world, so the default shows.
    assert_eq!(v["invite_initial_credits"], 5);
    assert_eq!(v["minimum_age"], 13);
}

#[tokio::test]
async fn managers_list_modules_with_their_schema() {
    let e = env_features().await;
    let path = e.g("/sr/features");
    assert_eq!(e.get(&path, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&path, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&path, "nobody").await.0, StatusCode::UNAUTHORIZED);

    for who in ["owner", "mod"] {
        let (s, v) = e.get(&path, who).await;
        assert_eq!(s, StatusCode::OK, "{who}");
        let list = v.as_array().unwrap();
        let star = list.iter().find(|f| f["id"] == "lounge.starboard").unwrap();
        assert_eq!(star["enabled"], false);
        assert_eq!(star["platform_disabled"], false);
        assert_eq!(star["settings"]["threshold"], 3);
        assert_eq!(star["schema"]["fields"][0]["key"], "channel");
        // Platform-only modules are not offered to servers.
        assert!(list.iter().all(|f| f["id"] != "moderation.reports"));
    }
}

#[tokio::test]
async fn enabling_and_configuring_a_module() {
    let e = env_features().await;
    let path = e.g("/sr/features/lounge.starboard");
    let (s, v) = e
        .patch(
            &path,
            "mod",
            json!({"enabled": true, "settings": {"channel": e.ch.to_string(), "threshold": 5}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["enabled"], true);
    assert_eq!(v["settings"]["threshold"], 5);
    assert_eq!(v["settings"]["channel"], e.ch.to_string());

    // Persisted, per server.
    let (_, list) = e.get(&e.g("/sr/features"), "owner").await;
    let star = list
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "lounge.starboard")
        .unwrap();
    assert_eq!(star["enabled"], true);

    // Switch off again, settings kept.
    let (_, v) = e.patch(&path, "owner", json!({"enabled": false})).await;
    assert_eq!(v["enabled"], false);
    assert_eq!(v["settings"]["threshold"], 5);

    // Members cannot.
    assert_eq!(
        e.patch(&path, "bob", json!({"enabled": true})).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn bad_settings_and_unknown_modules_are_refused() {
    let e = env_features().await;
    let path = e.g("/sr/features/lounge.starboard");
    // Out of range and unknown keys.
    for bad in [json!({"threshold": 0}), json!({"colour": "red"})] {
        let r = e.patch(&path, "owner", json!({"settings": bad})).await;
        assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    }
    assert_eq!(
        e.patch(&e.g("/sr/features/lounge.nope"), "owner", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // A platform-only module has no per-server settings.
    assert_eq!(
        e.patch(
            &e.g("/sr/features/moderation.reports"),
            "owner",
            json!({"enabled": true})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn settings_may_only_name_this_servers_channels_and_roles() {
    let e = env_features().await;
    // A channel of some other server.
    let foreign = ChannelId::new(0xdead_beef);
    let path = e.g("/sr/features/lounge.starboard");
    let r = e
        .patch(
            &path,
            "owner",
            json!({"settings": {"channel": foreign.to_string()}}),
        )
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    // Nested pickers are checked too (a sticky inside a list of groups).
    let r = e
        .patch(
            &e.g("/sr/features/lounge.sticky"),
            "owner",
            json!({"settings": {"stickies": [{"channel": foreign.to_string(), "content": "x"}]}}),
        )
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);

    // Roles: must exist here, and the manager must be able to assign them.
    let roles = e.g("/sr/features/moderation.auto_roles");
    let r = e
        .patch(&roles, "owner", json!({"settings": {"roles": ["777"]}}))
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    let high = add_role(&e, "admins", 9);
    let low_ok = add_role(&e, "newbies", 0);
    // The mod's top role has position 1 but no MANAGE_ROLES: it may not hand out any role.
    let r = e
        .patch(
            &roles,
            "mod",
            json!({"settings": {"roles": [low_ok.to_string()]}}),
        )
        .await;
    assert_eq!(r.0, StatusCode::FORBIDDEN, "{}", r.1);
    // The owner may name any role.
    for role in [high, low_ok] {
        let r = e
            .patch(
                &roles,
                "owner",
                json!({"settings": {"roles": [role.to_string()]}}),
            )
            .await;
        assert_eq!(r.0, StatusCode::OK, "{}", r.1);
    }
}

#[tokio::test]
async fn temporary_roles_need_the_module_and_manage_roles() {
    let e = env_features().await;
    let base = e.g("/sr/temp-roles");
    let role = add_role(&e, "event", 0);
    let body =
        json!({"user_id": BOB.to_string(), "role_id": role.to_string(), "duration_seconds": 3600});

    // Off by default.
    let r = e.post(&base, "owner", body.clone()).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(900001)));

    e.patch(
        &e.g("/sr/features/moderation.temp_roles"),
        "owner",
        json!({"enabled": true, "settings": {"max_duration": 7200}}),
    )
    .await;

    // The mod holds no MANAGE_ROLES.
    assert_eq!(
        e.post(&base, "mod", body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(e.get(&base, "bob").await.0, StatusCode::FORBIDDEN);

    // Over the configured maximum, zero, and a bad id.
    for secs in [7201, 0] {
        let mut b = body.clone();
        b["duration_seconds"] = json!(secs);
        let r = e.post(&base, "owner", b).await;
        assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    }
    let mut b = body.clone();
    b["user_id"] = json!("x");
    assert_eq!(e.post(&base, "owner", b).await.0, StatusCode::BAD_REQUEST);

    let (s, v) = e.post(&base, "owner", body.clone()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["user_id"], BOB.to_string());
    assert_eq!(v["expires_at"], T0 + 3_600_000);
    assert!(
        e.backend
            .data()
            .members
            .iter()
            .find(|m| m.user_id.get() == BOB)
            .unwrap()
            .role_ids
            .contains(&role)
    );

    let (s, list) = e.get(&base, "owner").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    let one = format!("{base}/{BOB}/{role}");
    assert_eq!(e.delete(&one, "owner").await.0, StatusCode::NO_CONTENT);
    assert!(
        !e.backend
            .data()
            .members
            .iter()
            .find(|m| m.user_id.get() == BOB)
            .unwrap()
            .role_ids
            .contains(&role)
    );
    assert_eq!(e.delete(&one, "owner").await.0, StatusCode::NOT_FOUND);
    assert!(e.get(&base, "owner").await.1.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn slowmode_presets_are_listed_to_channel_managers_only_while_the_module_is_on() {
    let e = env_features().await;
    let path = e.g("/sr/slowmode/presets");
    assert_eq!(e.get(&path, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&path, "nobody").await.0, StatusCode::UNAUTHORIZED);
    let (s, v) = e.get(&path, "mod").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["enabled"], false);
    assert!(v["presets"].as_array().unwrap().is_empty());

    e.patch(
        &e.g("/sr/features/lounge.slowmode_plus"),
        "owner",
        json!({"enabled": true}),
    )
    .await;
    let (_, v) = e.get(&path, "mod").await;
    assert_eq!(v["enabled"], true);
    let ids: Vec<_> = v["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["light", "medium", "strict", "lockdown"]);
}

#[tokio::test]
async fn applying_a_slowmode_preset_checks_the_module_the_permission_and_the_input() {
    let e = env_features().await;
    let path = e.g("/sr/slowmode/apply");
    let body = json!({"preset_id": "medium", "channel_ids": [e.ch.to_string()]});

    // Off by default.
    let r = e.post(&path, "mod", body.clone()).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(900001)));
    e.patch(
        &e.g("/sr/features/lounge.slowmode_plus"),
        "owner",
        json!({"enabled": true}),
    )
    .await;

    // A plain member may not.
    assert_eq!(
        e.post(&path, "bob", body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    // Bad input.
    for bad in [
        json!({"preset_id": "medium", "channel_ids": []}),
        json!({"preset_id": "medium", "channel_ids": [e.ch.to_string()], "minutes": 0}),
        json!({"preset_id": "medium", "channel_ids": [e.ch.to_string()], "minutes": 99999}),
        json!({"preset_id": "nope", "channel_ids": [e.ch.to_string()]}),
        json!({"preset_id": "medium", "channel_ids": ["x"]}),
    ] {
        let r = e.post(&path, "mod", bad).await;
        assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    }
    // A channel of some other server.
    let r = e
        .post(
            &path,
            "mod",
            json!({"preset_id": "medium", "channel_ids": ["12345"]}),
        )
        .await;
    assert_eq!(r.0, StatusCode::NOT_FOUND, "{}", r.1);
    assert!(e.actions.calls().is_empty(), "nothing was changed so far");

    // A channel manager applies it: the slowmode changes and the answer says what it was before.
    let (s, v) = e.post(&path, "mod", body).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["applied"][0]["channel_id"], e.ch.to_string());
    assert_eq!(v["applied"][0]["before"], 5);
    assert_eq!(v["applied"][0]["seconds"], 30);
    assert!(v["applied"][0]["revert_at"].is_null());
    assert_eq!(
        e.actions.calls(),
        vec![sr_features::testing::Call::Slowmode(e.ch, 30)]
    );

    // With minutes the answer carries the revert time.
    let (s, v) = e
        .post(
            &path,
            "owner",
            json!({"preset_id": "strict", "channel_ids": [e.ch.to_string()], "minutes": 15}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["applied"][0]["seconds"], 120);
    assert_eq!(
        v["applied"][0]["revert_at"].as_i64().unwrap(),
        T0 + 15 * 60_000
    );
}

// ---- channel and user scope (LNG-CHN-017, LNG-USET-036) ---------------------------------

impl Env {
    fn ch_features(&self, rest: &str) -> String {
        format!("/channels/{}/sr/features{rest}", self.ch)
    }
}

const QUOTES: &str = "lounge.message_quotes";

#[tokio::test]
async fn channel_managers_see_the_modules_that_have_a_channel_scope() {
    let e = env_features().await;
    let path = e.ch_features("");
    assert_eq!(e.get(&path, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&path, "nobody").await.0, StatusCode::UNAUTHORIZED);
    for who in ["owner", "mod"] {
        let (s, v) = e.get(&path, who).await;
        assert_eq!(s, StatusCode::OK, "{who}: {v}");
        let list = v.as_array().unwrap();
        // Only modules that declare a channel scope are offered.
        let ids: Vec<_> = list.iter().map(|f| f["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["lounge.auto_thread", QUOTES], "{v}");
        let q = list.iter().find(|f| f["id"] == QUOTES).unwrap();
        assert_eq!(q["guild_enabled"], false);
        assert_eq!(q["enabled"], false);
        assert_eq!(q["override_enabled"], Value::Null);
        assert_eq!(q["settings"]["max_quotes"], 2);
        // The form is the channel part of the schema.
        assert_eq!(q["schema"]["fields"][0]["key"], "max_quotes");
    }
}

#[tokio::test]
async fn a_channel_narrows_what_the_server_has_on_and_can_be_reset() {
    let e = env_features().await;
    let path = e.ch_features(&format!("/{QUOTES}"));
    // The channel cannot switch on what the server has off.
    let (s, v) = e.patch(&path, "mod", json!({"enabled": true})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["override_enabled"], true);
    assert_eq!(v["guild_enabled"], false);
    assert_eq!(v["enabled"], false);

    // The server turns it on with four previews; the channel then takes three.
    let (s, v) = e
        .patch(
            &e.g(&format!("/sr/features/{QUOTES}")),
            "owner",
            json!({"enabled": true, "settings": {"max_quotes": 4}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = e.get(&e.ch_features(""), "mod").await;
    assert_eq!(v[1]["enabled"], true);
    assert_eq!(
        v[1]["settings"]["max_quotes"], 4,
        "inherited from the server"
    );

    let (s, v) = e
        .patch(&path, "mod", json!({"settings": {"max_quotes": 3}}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["settings"]["max_quotes"], 3);
    assert_eq!(v["overrides"]["max_quotes"], 3);
    assert_eq!(
        v["inherited"]["max_quotes"], 4,
        "the server value stays the baseline"
    );
    // The server keeps its own value.
    let (_, g) = e.get(&e.g("/sr/features"), "owner").await;
    let g = g
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == QUOTES)
        .unwrap();
    assert_eq!(g["settings"]["max_quotes"], 4);

    // Off in this channel only.
    let (_, v) = e.patch(&path, "mod", json!({"enabled": false})).await;
    assert_eq!(v["enabled"], false);
    assert_eq!(v["guild_enabled"], true);
    assert_eq!(v["override_enabled"], false);
    // Another channel of the server is untouched.
    let other = e
        .get(&format!("/channels/{}/sr/features", e.voice), "mod")
        .await
        .1;
    assert_eq!(other[1]["enabled"], true);

    // Reset: back to following the server.
    assert_eq!(e.delete(&path, "mod").await.0, StatusCode::NO_CONTENT);
    let (_, v) = e.get(&e.ch_features(""), "mod").await;
    assert_eq!(v[1]["override_enabled"], Value::Null);
    assert_eq!(v[1]["overrides"], json!({}));
    assert_eq!(v[1]["enabled"], true);
    assert_eq!(v[1]["inherited"]["max_quotes"], 4);
    assert_eq!(v[1]["settings"]["max_quotes"], 4);
}

#[tokio::test]
async fn channel_settings_are_checked_and_members_cannot_write() {
    let e = env_features().await;
    let path = e.ch_features(&format!("/{QUOTES}"));
    for bad in [json!({"max_quotes": 0}), json!({"colour": "red"})] {
        let r = e.patch(&path, "owner", json!({"settings": bad})).await;
        assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    }
    assert_eq!(
        e.patch(&path, "bob", json!({"enabled": false})).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(e.delete(&path, "bob").await.0, StatusCode::FORBIDDEN);
    // A module without a channel scope, and an unknown one.
    for id in ["lounge.starboard", "lounge.nope", "moderation.reports"] {
        let r = e
            .patch(
                &e.ch_features(&format!("/{id}")),
                "owner",
                json!({"enabled": true}),
            )
            .await;
        assert_eq!(r.0, StatusCode::NOT_FOUND, "{id}: {}", r.1);
    }
    // Not a channel at all.
    assert_eq!(
        e.get("/channels/424242/sr/features", "owner").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn people_read_and_write_only_their_own_module_preferences() {
    let e = env_features().await;
    let leveling = "community.leveling";
    let path = format!("/users/@me/sr/features/{leveling}");
    assert_eq!(
        e.get("/users/@me/sr/features", "nobody").await.0,
        StatusCode::UNAUTHORIZED
    );
    // A bot has no personal settings.
    assert_eq!(
        e.get("/users/@me/sr/features", "owner").await.0,
        StatusCode::FORBIDDEN
    );

    let (s, v) = e
        .as_user(BOB, Method::GET, "/users/@me/sr/features", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 1, "only modules with a personal scope: {v}");
    assert_eq!(list[0]["id"], leveling);
    assert_eq!(list[0]["settings"]["announce_mine"], true);
    // The personal form only carries the personal field, not the server's.
    let keys: Vec<_> = list[0]["schema"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["announce_mine"]);

    let (s, v) = e
        .as_user(
            BOB,
            Method::PATCH,
            &path,
            Some(json!({"settings": {"announce_mine": false}})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["settings"]["announce_mine"], false);
    assert_eq!(v["overrides"]["announce_mine"], false);

    // Someone else is unaffected.
    let (_, v) = e
        .as_user(CAROL, Method::GET, "/users/@me/sr/features", None)
        .await;
    assert_eq!(v[0]["settings"]["announce_mine"], true);

    // A server-level key is not a personal setting, and nothing outside the scope is writable.
    let (s, v) = e
        .as_user(
            BOB,
            Method::PATCH,
            &path,
            Some(json!({"settings": {"announce": false}})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    for id in ["lounge.starboard", "lounge.nope"] {
        let (s, _) = e
            .as_user(
                BOB,
                Method::PATCH,
                &format!("/users/@me/sr/features/{id}"),
                Some(json!({"settings": {}})),
            )
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{id}");
    }

    // Reset.
    assert_eq!(
        e.as_user(BOB, Method::DELETE, &path, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e
        .as_user(BOB, Method::GET, "/users/@me/sr/features", None)
        .await;
    assert_eq!(v[0]["settings"]["announce_mine"], true);
    assert_eq!(v[0]["overrides"], json!({}));
}

#[tokio::test]
async fn the_server_form_does_not_show_or_accept_personal_fields() {
    let e = env_features().await;
    let (_, list) = e.get(&e.g("/sr/features"), "owner").await;
    let lv = list
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "community.leveling")
        .unwrap();
    let keys: Vec<_> = lv["schema"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"announce"));
    assert!(!keys.contains(&"announce_mine"));
    assert!(lv["settings"].get("announce_mine").is_none());
    let r = e
        .patch(
            &e.g("/sr/features/community.leveling"),
            "owner",
            json!({"settings": {"announce_mine": false}}),
        )
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
}

/// PRF-SCR-001: the scrapbook is hidden until the operator enables it at platform scope. Until then the
/// server's module list does not show it and a server cannot configure or switch it on.
#[tokio::test]
async fn the_scrapbook_is_hidden_from_servers_until_the_operator_offers_it() {
    let e = env_features().await;
    let (s, v) = e.get(&e.g("/sr/features"), "owner").await;
    assert_eq!(s, StatusCode::OK);
    let list = v.as_array().unwrap();
    assert!(list.iter().any(|f| f["id"] == "lounge.starboard"));
    assert!(list.iter().all(|f| f["id"] != "lounge.scrapbook"));
    let path = e.g("/sr/features/lounge.scrapbook");
    let (s, _) = e.patch(&path, "owner", json!({"enabled": true})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .patch(&path, "owner", json!({"settings": {"threshold": 2}}))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
