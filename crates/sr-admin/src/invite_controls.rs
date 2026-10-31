//! OPS-INV-001: the invite controls of Admin > Invites. "Lock the Doors" (`invites.locked`), the quotas and what
//! the `invites.prune` job did.
//!
//! The lock is a platform setting like any other, with one difference: switching it on or off is an audited
//! admin action of its own (`invites.lock`, `invites.unlock`), with a required reason, so the log tells a later
//! reader who closed the doors and why. The prune job never deletes `invite_graph` rows (docs/09 keeps the graph
//! forever); it revokes unused codes and writes the display mark read here.

use crate::{
    error::AdminResult,
    model::{AccessKind, Caller, id},
    service::AdminService,
    settings,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

/// Most inactive roots one answer lists.
pub const MAX_INACTIVE_ROOTS: i64 = 200;

/// What the last `invites.prune` pass did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PruneRun {
    pub ran_at_ms: i64,
    /// Unused invite codes the pass revoked.
    pub codes_revoked: i64,
    /// Subtrees newly marked inactive.
    pub roots_marked: i64,
    /// Marks lifted because the root activated since.
    pub roots_cleared: i64,
    /// Redeemed edges in the invite graph after the pass; the same number as before it.
    pub graph_rows: i64,
}

/// The root of an invite subtree whose first member never activated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InactiveRoot {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    /// The account that issued the invite the root used, when it still exists.
    #[serde(with = "crate::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub invited_by_account_id: Option<i64>,
    pub marked_at_ms: i64,
    /// The root and everyone below it in the invite graph.
    pub subtree_size: i64,
}

/// The controls card of Admin > Invites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InviteControls {
    /// Lock the Doors: no registration succeeds while this is on.
    pub locked: bool,
    /// Credits a new account starts with.
    pub default_credits: i64,
    /// Most unused codes one account may hold at once.
    pub max_outstanding: i64,
    /// Days after which an unused code is revoked.
    pub unused_code_days: i64,
    pub last_prune: Option<PruneRun>,
    /// Subtrees currently marked inactive.
    pub inactive_roots: i64,
}

/// The inactive subtrees (newest mark first, at most [`MAX_INACTIVE_ROOTS`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InactiveRoots {
    pub items: Vec<InactiveRoot>,
    /// How many are marked in all.
    pub total: i64,
}

impl AdminService {
    async fn invite_controls_view(&self) -> AdminResult<InviteControls> {
        let (_, total) = self.repo().inactive_invite_roots(1).await?;
        Ok(InviteControls {
            locked: self.setting_i64(settings::INVITES_LOCKED).await? == 1,
            default_credits: self.setting_i64(settings::INVITES_DEFAULT_CREDITS).await?,
            max_outstanding: self.setting_i64(settings::INVITES_MAX_OUTSTANDING).await?,
            unused_code_days: self.setting_i64(settings::INVITES_UNUSED_CODE_DAYS).await?,
            last_prune: self.repo().last_prune_run().await?,
            inactive_roots: total,
        })
    }

    /// The lock, the quotas and the last prune pass (`MANAGE_INVITES`).
    pub async fn invite_controls(&self, c: &Caller) -> AdminResult<InviteControls> {
        self.require(c, P::MANAGE_INVITES, "invites.controls")
            .await?;
        self.invite_controls_view().await
    }

    /// Lock the Doors (`locked`) or open them again. Needs `MANAGE_FEATURES`, like every platform setting, and a
    /// reason; the change is one audited admin action (`invites.lock` or `invites.unlock`). Setting the value it
    /// already has changes nothing and writes nothing.
    pub async fn set_invites_locked(
        &self,
        c: &Caller,
        locked: bool,
        reason: Option<&str>,
    ) -> AdminResult<InviteControls> {
        let action = if locked {
            "invites.lock"
        } else {
            "invites.unlock"
        };
        self.require(c, P::MANAGE_FEATURES, action).await?;
        let reason = Self::reason(reason)?;
        let was = self.setting_i64(settings::INVITES_LOCKED).await? == 1;
        if was != locked {
            let audit = self.entry(
                c,
                AccessKind::AdminAction,
                action,
                None,
                &reason,
                json!({ "setting": settings::INVITES_LOCKED, "locked": locked }),
            );
            self.repo()
                .put_settings(
                    &[(settings::INVITES_LOCKED.to_owned(), json!(locked))],
                    c.user_id,
                    &audit,
                )
                .await?;
        }
        self.invite_controls_view().await
    }

    /// The subtrees marked inactive by the prune job (`MANAGE_INVITES`).
    pub async fn inactive_invite_roots(
        &self,
        c: &Caller,
        limit: Option<i64>,
    ) -> AdminResult<InactiveRoots> {
        self.require(c, P::MANAGE_INVITES, "invites.inactive")
            .await?;
        let limit = limit.unwrap_or(50).clamp(1, MAX_INACTIVE_ROOTS);
        let (items, total) = self.repo().inactive_invite_roots(limit).await?;
        Ok(InactiveRoots { items, total })
    }

    /// Used by the generic settings patch: a change to the lock through it is audited as a lock too.
    pub(crate) async fn audit_lock_change(
        &self,
        c: &Caller,
        was: bool,
        now: bool,
        reason: &str,
    ) -> AdminResult<()> {
        if was == now {
            return Ok(());
        }
        let action = if now {
            "invites.lock"
        } else {
            "invites.unlock"
        };
        let e = self.entry(
            c,
            AccessKind::AdminAction,
            action,
            None,
            reason,
            json!({ "setting": settings::INVITES_LOCKED, "locked": now, "via": "settings.patch" }),
        );
        self.repo().append_log(&e).await.map(|_| ())
    }
}
