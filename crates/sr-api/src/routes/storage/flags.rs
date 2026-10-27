//! The BYOS switches in front of the storage routes (BYOS-FLG-001).
//!
//! * `platform.byos` off at platform scope: every mount route answers 403 `SR_FEATURE_DISABLED`, except
//!   the reads of the instance bucket (mount 1: list, metadata, presign), because the beta's media lives
//!   there.
//! * `guild_mounts` off for a server (or for the whole platform): connecting a new bucket to that server
//!   (`POST /guilds/{id}/storage/mounts`) answers 403 `SR_FEATURE_DISABLED`. Buckets already connected
//!   keep working and can be disconnected.
//!
//! The gate is a route layer, so a route added to the storage router is covered without remembering to
//! ask. It decides nothing about who may do what: authorization stays in each handler
//! (`sr_storage::scopes`), and no flag here can loosen it (the IMMUTABLE RULE).

use axum::{
    extract::{MatchedPath, Request, State},
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use sr_core::{ApiError, ErrorCode};
use sr_features::{BYOS_ID, ByosSettings, FeatureService, ScopeRef};
use sr_storage::mount_policy::{EndpointAllow, MemberStanding, WhoMayMount};
use sr_storage::mounts::INSTANCE_MOUNT_ID;

use crate::{
    AppState,
    routes::{
        admin::AdminDeps,
        messages::error::{MsgError, MsgResult},
        settings::features::GuildFeatures,
    },
};

fn unversioned(path: &str) -> &str {
    path.strip_prefix("/api/v10")
        .or_else(|| path.strip_prefix("/api/v1"))
        .unwrap_or(path)
}

/// Whether a route template belongs to the storage surface.
pub fn is_storage_route(template: &str) -> bool {
    template.starts_with("/users/@me/storage")
        || template.starts_with("/users/@me/shelves")
        || template.starts_with("/storage/mounts")
        || (template.starts_with("/guilds/{guild_id}/storage/"))
}

/// A read of the instance bucket, which the kill switch leaves working.
pub fn is_instance_read(method: &Method, actual_path: &str) -> bool {
    let prefix = format!("/storage/mounts/{INSTANCE_MOUNT_ID}/");
    let Some(rest) = actual_path.strip_prefix(&prefix) else {
        return false;
    };
    (matches!(*method, Method::GET | Method::HEAD)
        && (rest == "objects" || rest.starts_with("objects/")))
        || (*method == Method::POST && rest == "presign")
}

/// What the gate decides for one request, given the switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Refuse,
}

/// The pure decision: `platform_on` is `platform.byos`, `guild_mounts` the combined server choice (or
/// `None` when the path names no server).
pub fn decide(
    method: &Method,
    template: &str,
    actual_path: &str,
    platform_on: bool,
    guild_mounts: Option<bool>,
) -> Verdict {
    if !is_storage_route(template) {
        return Verdict::Pass;
    }
    if !platform_on {
        return if is_instance_read(method, actual_path) {
            Verdict::Pass
        } else {
            Verdict::Refuse
        };
    }
    if *method == Method::POST
        && template == "/guilds/{guild_id}/storage/mounts"
        && guild_mounts == Some(false)
    {
        return Verdict::Refuse;
    }
    Verdict::Pass
}

/// The guild id in `/guilds/{id}/storage/...`.
fn guild_of(actual_path: &str) -> Option<i64> {
    let rest = actual_path.strip_prefix("/guilds/")?;
    rest.split('/')
        .next()?
        .parse::<i64>()
        .ok()
        .filter(|g| *g > 0)
}

/// The route layer.
pub async fn byos_gate(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(template) = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| unversioned(m.as_str()).to_owned())
    else {
        return next.run(req).await;
    };
    if !is_storage_route(&template) {
        return next.run(req).await;
    }
    let Some(features) = state.extension::<GuildFeatures>() else {
        return next.run(req).await;
    };
    let svc = &features.ctx.settings;
    let actual = unversioned(req.uri().path()).to_owned();
    let platform_on = svc
        .is_enabled(BYOS_ID, &ScopeRef::Platform)
        .await
        .unwrap_or(true);
    let guild_mounts = if platform_on && template == "/guilds/{guild_id}/storage/mounts" {
        match guild_of(&actual) {
            Some(g) => Some(guild_mounts_allowed(svc, g).await),
            None => None,
        }
    } else {
        None
    };
    match decide(req.method(), &template, &actual, platform_on, guild_mounts) {
        Verdict::Pass => next.run(req).await,
        Verdict::Refuse => crate::HttpError::from(ErrorCode::SrFeatureDisabled).into_response(),
    }
}

/// Whether the platform and the server both allow the server to connect buckets. The server's own
/// switch on `platform.byos` (a guild-scope off) also counts.
pub async fn guild_mounts_allowed(svc: &FeatureService, guild: i64) -> bool {
    let scope = ScopeRef::Guild {
        guild: sr_events::GuildId::new(guild as u64),
    };
    if !svc.is_enabled(BYOS_ID, &scope).await.unwrap_or(true) {
        return false;
    }
    let read = |s: ScopeRef| async move {
        svc.effective_settings(BYOS_ID, &s)
            .await
            .map(|v| ByosSettings::from_values(&v))
            .unwrap_or_default()
    };
    let combined = ByosSettings::combine(read(ScopeRef::Platform).await, read(scope).await);
    combined.guild_mounts
}

/// The operator's mount policy for one request: who may connect a bucket and where (BYOS-FLG-001).
pub(crate) struct MountPolicy {
    pub(crate) who: WhoMayMount,
    pub(crate) allow: EndpointAllow,
}

impl MountPolicy {
    /// Read `byos.who_may_mount` and `byos.endpoint_allow`. Without the admin service (unit tests) the
    /// defaults apply; a failed read of the settings fails closed to staff only.
    pub(crate) async fn load(admin: Option<&AdminDeps>) -> Self {
        let Some(admin) = admin else {
            return Self {
                who: WhoMayMount::All,
                allow: EndpointAllow::default(),
            };
        };
        let who = match admin
            .service
            .setting_value_text(sr_admin::settings::BYOS_WHO_MAY_MOUNT)
            .await
        {
            Ok(v) => WhoMayMount::parse(&v),
            Err(e) => {
                tracing::error!(error = %e, "byos.who_may_mount unreadable; staff only");
                WhoMayMount::Staff
            }
        };
        let allow = admin
            .service
            .setting_value_text(sr_admin::settings::BYOS_ENDPOINT_ALLOW)
            .await
            .map(|v| EndpointAllow::parse(&v))
            .unwrap_or_default();
        Self { who, allow }
    }

    /// 403 unless `user` may connect a bucket of their own under `byos.who_may_mount`.
    pub(crate) async fn may_connect(
        &self,
        standing: Option<&dyn MemberStanding>,
        user: i64,
    ) -> MsgResult<()> {
        if self.who.is_open() {
            return Ok(());
        }
        let refused = || {
            MsgError::Api(ApiError::with_message(
                ErrorCode::MissingPermissions,
                "Connecting a bucket is limited to patrons or staff on this server.",
            ))
        };
        let Some(standing) = standing else {
            return Err(refused());
        };
        let (patron, staff) = match (
            standing.is_patron(user).await,
            standing.is_staff(user).await,
        ) {
            (Ok(p), Ok(s)) => (p, s),
            (a, b) => {
                tracing::error!(?a, ?b, "member standing unreadable; refusing");
                return Err(refused());
            }
        };
        if self.who.admits(patron, staff) {
            Ok(())
        } else {
            Err(refused())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G_MOUNTS: &str = "/guilds/{guild_id}/storage/mounts";

    #[test]
    fn storage_routes_are_recognised() {
        for t in [
            "/users/@me/storage/mounts",
            "/users/@me/shelves/summary",
            "/storage/mounts/{mount_id}/objects",
            "/guilds/{guild_id}/storage/grants",
            G_MOUNTS,
        ] {
            assert!(is_storage_route(t), "{t}");
        }
        for t in ["/users/@me", "/guilds/{guild_id}/channels", "/cdn/x"] {
            assert!(!is_storage_route(t), "{t}");
        }
    }

    #[test]
    fn the_kill_switch_refuses_every_mount_route_but_instance_reads() {
        let get = Method::GET;
        let post = Method::POST;
        // Refused: the member's own mounts, a foreign mount's objects, uploads, shares, grants.
        assert_eq!(
            decide(
                &get,
                "/users/@me/storage/mounts",
                "/users/@me/storage/mounts",
                false,
                None
            ),
            Verdict::Refuse
        );
        assert_eq!(
            decide(
                &get,
                "/storage/mounts/{mount_id}/objects",
                "/storage/mounts/99/objects",
                false,
                None
            ),
            Verdict::Refuse
        );
        assert_eq!(
            decide(
                &post,
                "/users/@me/storage/mounts",
                "/users/@me/storage/mounts",
                false,
                None
            ),
            Verdict::Refuse
        );
        assert_eq!(
            decide(
                &Method::PUT,
                "/guilds/{guild_id}/storage/grants",
                "/guilds/5/storage/grants",
                false,
                None
            ),
            Verdict::Refuse
        );
        // Passed: reads of mount 1 (list, metadata, presign).
        assert_eq!(
            decide(
                &get,
                "/storage/mounts/{mount_id}/objects",
                "/storage/mounts/1/objects",
                false,
                None
            ),
            Verdict::Pass
        );
        assert_eq!(
            decide(
                &get,
                "/storage/mounts/{mount_id}/objects/{*key}",
                "/storage/mounts/1/objects/a/b.mp4",
                false,
                None
            ),
            Verdict::Pass
        );
        assert_eq!(
            decide(
                &post,
                "/storage/mounts/{mount_id}/presign",
                "/storage/mounts/1/presign",
                false,
                None
            ),
            Verdict::Pass
        );
        // Mount 11 or 10 is not mount 1, and writes to mount 1 stay refused.
        assert_eq!(
            decide(
                &get,
                "/storage/mounts/{mount_id}/objects",
                "/storage/mounts/11/objects",
                false,
                None
            ),
            Verdict::Refuse
        );
        assert_eq!(
            decide(
                &Method::DELETE,
                "/storage/mounts/{mount_id}/objects/{*key}",
                "/storage/mounts/1/objects/x",
                false,
                None
            ),
            Verdict::Refuse
        );
    }

    #[test]
    fn other_routes_are_never_touched() {
        assert_eq!(
            decide(&Method::GET, "/users/@me", "/users/@me", false, None),
            Verdict::Pass
        );
    }

    #[test]
    fn a_server_without_guild_mounts_cannot_connect_a_new_bucket_only() {
        let off = Some(false);
        assert_eq!(
            decide(
                &Method::POST,
                G_MOUNTS,
                "/guilds/5/storage/mounts",
                true,
                off
            ),
            Verdict::Refuse
        );
        assert_eq!(
            decide(
                &Method::GET,
                G_MOUNTS,
                "/guilds/5/storage/mounts",
                true,
                off
            ),
            Verdict::Pass
        );
        assert_eq!(
            decide(
                &Method::DELETE,
                "/guilds/{guild_id}/storage/mounts/{mount_id}",
                "/guilds/5/storage/mounts/9",
                true,
                off
            ),
            Verdict::Pass
        );
        assert_eq!(
            decide(
                &Method::POST,
                G_MOUNTS,
                "/guilds/5/storage/mounts",
                true,
                Some(true)
            ),
            Verdict::Pass
        );
    }

    #[test]
    fn guild_ids_are_read_from_the_path() {
        assert_eq!(guild_of("/guilds/123/storage/mounts"), Some(123));
        assert_eq!(guild_of("/guilds/x/storage/mounts"), None);
        assert_eq!(guild_of("/users/@me"), None);
    }
}
