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
