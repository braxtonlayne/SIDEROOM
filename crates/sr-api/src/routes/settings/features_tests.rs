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
    calls: Arc<std::sync::Mutex<Vec<(u64, u64, u64, bool)>>>,
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
        self.calls
            .lock()
            .unwrap()
            .push((actor.get(), user.get(), role.get(), add));
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
            actions: Arc::new(FakeActions::new()),
            state: Arc::new(sr_features::state::InMemoryState::new()),
        },
        roles: Arc::new(MemoryRoles {
            backend: backend.clone(),
            calls: Arc::default(),
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
