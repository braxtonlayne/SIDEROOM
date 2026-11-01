//! Profile vouches (MOD-VCH-001, wave 44-B "Heavy Crown"). The logic is `sr-vouch`; this is the HTTP layer and
//! the one decorator that applies the accountability multiplier to AutoMod timeouts.
//!
//! * `GET /users/{user_id}/vouches`: the vouch panel of a profile (count, tier, multiplier, whether I vouched).
//! * `PUT /users/{user_id}/vouch`: vouch for a member (idempotent; 1 per pair; never for yourself).
//! * `DELETE /users/{user_id}/vouch`: take my vouch back (204 whether or not there was one).
//!
//! These are *profile* vouches. The Arcade's hash-pinned build vouches (`/activities/{id}/vouch`, wave 42) are a
//! different table and different routes. Reading and vouching follow the profile's privacy rules exactly: a
//! member who may not see the page, or who is blocked either way, gets the same 404 as a missing member. Taking a
//! vouch back needs no visibility (a blocked voucher can still withdraw) and answers 204 without a body, so it
//! leaks nothing.

use crate::auth::SessionOnly;
use crate::error::{ApiResult, HttpError};
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::settings::MemberTimeouter;
use crate::state::AppState;
use axum::{Json, extract::Path, extract::State, http::StatusCode};
use serde::Serialize;
use sr_core::{ApiError, ErrorCode, GuildId, UserId};
use sr_packages::profile_privacy::Standing;
use sr_packages::service::PackageService;
use sr_vouch::{AccountabilityMultiplier, VouchError, VouchSummary, Vouches, tiers};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the vouch routes.
pub struct VouchDeps {
    pub vouches: Vouches,
    /// For the profile privacy rules (who may see this member's page).
    pub packages: PackageService,
}

/// The router, relative to `/api/v1`.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::{get, put};
    axum::Router::new()
        .route("/users/{user_id}/vouches", get(get_vouches))
        .route(
            "/users/{user_id}/vouch",
            put(put_vouch).delete(delete_vouch),
        )
}

/// The vouch tier of a member.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProfileVouchTier {
    /// 0 to 49 vouches.
    Citizen,
    /// 50 to 499 vouches.
    Pillar,
    /// 500 or more vouches.
    Elder,
}

impl From<tiers::Tier> for ProfileVouchTier {
    fn from(t: tiers::Tier) -> Self {
        match t {
            tiers::Tier::Citizen => Self::Citizen,
            tiers::Tier::Pillar => Self::Pillar,
            tiers::Tier::Elder => Self::Elder,
        }
    }
}

/// The vouch panel of a profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProfileVouchSummary {
    /// Vouches from members in good standing.
    pub count: i64,
    pub tier: ProfileVouchTier,
    /// The accountability multiplier of the tier: automated timeouts, holds and strike weights are scaled by it
    /// when this member breaks a rule. 1.0, 1.5 or 3.0.
    pub multiplier: f32,
    /// First count of the Pillar tier (50).
    pub pillar_min: i64,
    /// First count of the Elder tier (500).
    pub elder_min: i64,
    /// The multiplier of the Pillar tier (1.5).
    pub pillar_multiplier: f32,
    /// The multiplier of the Elder tier (3.0).
    pub elder_multiplier: f32,
    /// I have an active vouch for this member.
    pub viewer_vouched: bool,
    /// I may vouch for this member now.
    pub can_vouch: bool,
    /// This is my own profile.
    pub is_self: bool,
}

impl ProfileVouchSummary {
    fn new(s: VouchSummary, is_self: bool) -> Self {
        Self {
            count: s.count,
            tier: s.tier.into(),
            multiplier: s.multiplier,
            pillar_min: tiers::PILLAR_MIN,
            elder_min: tiers::ELDER_MIN,
            pillar_multiplier: tiers::PILLAR_MULTIPLIER,
            elder_multiplier: tiers::ELDER_MULTIPLIER,
            viewer_vouched: s.viewer_vouched,
            can_vouch: s.can_vouch,
            is_self,
        }
    }
}

fn not_found() -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, "Unknown profile").into()
}

fn internal(e: &dyn std::fmt::Display) -> HttpError {
    tracing::error!(error = %e, "vouch failure");
    ApiError::new(ErrorCode::SrInternal).into()
}

fn vch(e: VouchError) -> HttpError {
    match e {
        VouchError::SelfVouch => ApiError::invalid_form()
            .field(
                "user_id",
                "INVALID_VALUE",
                "You cannot vouch for yourself.".to_owned(),
            )
            .into(),
        VouchError::NotFound => not_found(),
        VouchError::NotAllowed => {
            ApiError::with_message(ErrorCode::MissingPermissions, "You cannot vouch right now.")
                .into()
        }
        VouchError::RateLimited(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        VouchError::Db(e) => internal(&e),
    }
}

fn target_id(raw: &str, me: i64) -> ApiResult<i64> {
    if raw == "@me" {
        return Ok(me);
    }
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(not_found)
}

/// The same gate as the profile page: either side blocked the other, or the page's tier is above the viewer's
/// rank, is a 404. The owner always sees their own.
async fn require_visible(
    state: &AppState,
    deps: &VouchDeps,
    viewer: i64,
    owner: i64,
) -> ApiResult<()> {
    let Standing::Viewer(rank) = deps
        .packages
        .standing_of(Some(viewer), owner)
        .await
        .map_err(|e| internal(&e))?
    else {
        return Err(not_found());
    };
    let (_, stored) = deps
        .packages
        .privacy_row(owner)
        .await
        .map_err(|e| internal(&e))?;
    let policy = stored.effective(crate::routes::packages::public_possible(state).await);
    if !policy.page_visible(rank) {
        return Err(not_found());
    }
    Ok(())
}

/// The vouch panel of a member's profile: count, tier, multiplier and whether I vouched.
#[utoipa::path(
    get, path = "/users/{user_id}/vouches", tag = "vouches",
    params(("user_id" = String, Path, description = "User snowflake, or @me")),
    responses((status = 200, body = ProfileVouchSummary), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_vouches(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<VouchDeps>,
    Path(user): Path<String>,
) -> ApiResult<Json<ProfileVouchSummary>> {
    let me = p.user_id;
    let target = target_id(&user, me)?;
    require_visible(&state, &deps, me, target).await?;
    let s = deps.vouches.summary(me, target).await.map_err(vch)?;
    Ok(Json(ProfileVouchSummary::new(s, me == target)))
}

/// Vouch for a member on their profile. One vouch per pair; vouching again changes nothing. 400 for yourself,
/// 403 when you cannot vouch (an account under three days old, or one that may not invite), 404 when the member
/// is missing, blocked either way or the page is not visible to you, 409 past 20 new vouches a day.
#[utoipa::path(
    put, path = "/users/{user_id}/vouch", tag = "vouches",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = ProfileVouchSummary), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_vouch(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<VouchDeps>,
    Path(user): Path<String>,
) -> ApiResult<Json<ProfileVouchSummary>> {
    let me = p.user_id;
    let target = target_id(&user, me)?;
    if me != target {
        require_visible(&state, &deps, me, target).await?;
    }
    deps.vouches.vouch(me, target).await.map_err(vch)?;
    let s = deps.vouches.summary(me, target).await.map_err(vch)?;
    Ok(Json(ProfileVouchSummary::new(s, me == target)))
}

/// Take my vouch for a member back. 204 whether or not there was one.
#[utoipa::path(
    delete, path = "/users/{user_id}/vouch", tag = "vouches",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_vouch(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<VouchDeps>,
    Path(user): Path<String>,
) -> ApiResult<StatusCode> {
    let me = p.user_id;
    let target = target_id(&user, me)?;
    deps.vouches.revoke(me, target).await.map_err(vch)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The accountability multiplier on AutoMod timeouts (MOD-VCH-001). Wraps the wired [`MemberTimeouter`]: when a
/// vouched member trips an AutoMod timeout rule, the timeout runs `multiplier` times as long (1.5x for a Pillar,
/// 3.0x for an Elder, at most 28 days). It only ever lengthens a timeout, never touches a member who nobody
/// vouched for, and a failed lookup leaves the timeout as the rule set it.
pub struct WeightedMemberTimeouter {
    inner: Arc<dyn MemberTimeouter>,
    multiplier: Arc<dyn AccountabilityMultiplier>,
}

impl WeightedMemberTimeouter {
    pub fn new(
        inner: Arc<dyn MemberTimeouter>,
        multiplier: Arc<dyn AccountabilityMultiplier>,
    ) -> Self {
        Self { inner, multiplier }
    }
}

/// `until_ms` stretched by `multiplier` as seen from `now_ms`. A timeout already in the past, or a multiplier
/// of 1.0, is returned unchanged.
pub fn scale_until_ms(now_ms: i64, until_ms: i64, multiplier: f32) -> i64 {
    let span_ms = until_ms.saturating_sub(now_ms);
    if span_ms <= 0 || multiplier <= 1.0 {
        return until_ms;
    }
    let secs = (span_ms as u64).div_ceil(1000);
    let scaled = tiers::scale_timeout_secs(secs, multiplier);
    now_ms.saturating_add((scaled as i64).saturating_mul(1000))
}

#[async_trait::async_trait]
impl MemberTimeouter for WeightedMemberTimeouter {
    async fn timeout_member(
        &self,
        guild: GuildId,
        user: UserId,
        until_ms: i64,
    ) -> Result<Option<i64>, ApiError> {
        let m = self
            .multiplier
            .strike_weight_multiplier(user.get() as i64)
            .await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.inner
            .timeout_member(guild, user, scale_until_ms(now, until_ms, m))
            .await
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(get_vouches, put_vouch, delete_vouch),
    components(schemas(ProfileVouchSummary, ProfileVouchTier))
)]
pub struct VouchApiDoc;

/// The OpenAPI document fragment for the vouch endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    VouchApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn until_is_stretched_from_now_and_never_shortened() {
        let now = 1_000_000;
        // a 10 minute timeout becomes 15 and 30 minutes
        assert_eq!(scale_until_ms(now, now + 600_000, 1.0), now + 600_000);
        assert_eq!(scale_until_ms(now, now + 600_000, 1.5), now + 900_000);
        assert_eq!(scale_until_ms(now, now + 600_000, 3.0), now + 1_800_000);
        // 28 days stays 28 days
        let max = 28 * 24 * 3600 * 1000;
        assert_eq!(scale_until_ms(now, now + max, 3.0), now + max);
        // in the past, or a multiplier below one, changes nothing
        assert_eq!(scale_until_ms(now, now - 5, 3.0), now - 5);
        assert_eq!(scale_until_ms(now, now + 600_000, 0.2), now + 600_000);
    }

    struct Fixed(f32);
    #[async_trait::async_trait]
    impl AccountabilityMultiplier for Fixed {
        async fn strike_weight_multiplier(&self, _: i64) -> f32 {
            self.0
        }
    }

    #[derive(Default)]
    struct Rec(Mutex<Vec<i64>>);
    #[async_trait::async_trait]
    impl MemberTimeouter for Rec {
        async fn timeout_member(
            &self,
            _: GuildId,
            _: UserId,
            until: i64,
        ) -> Result<Option<i64>, ApiError> {
            self.0.lock().unwrap().push(until);
            Ok(Some(until))
        }
    }

    #[tokio::test]
    async fn the_decorator_lengthens_the_timeout_the_inner_one_applies() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        for (m, factor) in [(1.0_f32, 1_i64), (1.5, 2), (3.0, 3)] {
            let rec = Arc::new(Rec::default());
            let w = WeightedMemberTimeouter::new(rec.clone(), Arc::new(Fixed(m)));
            let asked = now + 600_000;
            w.timeout_member(GuildId::new(1), UserId::new(2), asked)
                .await
                .unwrap();
            let got = rec.0.lock().unwrap()[0] - now;
            let want = match factor {
                1 => 600_000,
                2 => 900_000,
                _ => 1_800_000,
            };
            // `now` moves a few ms between the test's clock and the decorator's
            assert!((got - want).abs() < 2_000, "x{m}: got {got}, want {want}");
        }
    }
}
