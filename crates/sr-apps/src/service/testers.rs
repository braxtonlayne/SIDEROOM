//! Application testers (G-01, D-204): the owner invites up to 25 accounts; an accepted tester
//! may install a `development` application in a small guild they manage, like the owner.

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult},
    model::{Application, MAX_TESTERS, Tester, TesterState},
};
use serde_json::json;

/// How long a decline blocks a new invitation (G-84).
pub const TESTER_COOLDOWN_MS: i64 = 24 * 60 * 60 * 1000;

/// Who to invite: by account id or by username.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TesterTarget {
    /// Account id.
    Id(i64),
    /// Username (case-insensitive).
    Username(String),
}

/// The answer to an invitation.
#[derive(Debug, Clone)]
pub struct TesterAnswer {
    /// The application the invitation was for.
    pub application: Application,
    /// `true` when the invitation was accepted, `false` when it was declined.
    pub accepted: bool,
}

impl AppsService {
    /// Invite an account to test an owned application. The invitee is told through the
    /// system DM the API layer sends; nothing is granted until they accept.
    pub async fn invite_tester(
        &self,
        owner: i64,
        id: i64,
        target: TesterTarget,
    ) -> AppsResult<(Tester, Application)> {
        let app = self.owned(owner, id).await?;
        let account = match target {
            TesterTarget::Id(a) => self.repo.account_brief(a).await?,
            TesterTarget::Username(u) => self.repo.account_by_username(u.trim()).await?,
        }
        .filter(|a| !a.is_bot)
        .ok_or(AppsError::NotFound("user"))?;
        if account.id == owner {
            return Err(AppsError::field(
                "user",
                "TESTER_SELF",
                "You own this application already.",
            ));
        }
        // G-84: someone who declined stays out of reach for a day.
        let since = self.now_ms() - TESTER_COOLDOWN_MS;
        if self.repo.declined_since(app.id, account.id, since).await? {
            return Err(AppsError::TesterRecentlyDeclined);
        }
        let t = Tester {
            id: self.next_id(),
            application_id: app.id,
            account_id: account.id,
            state: TesterState::Invited,
            invited_by: owner,
            created_at_ms: self.now_ms(),
        };
        let audit = self.audit(
            app.id,
            owner,
            "app.tester_invited",
            json!({"account_id": account.id.to_string()}),
        );
        let t = self.repo.invite_tester(&t, audit).await?;
        Ok((t, app))
    }

    /// Testers of an owned application with their account briefs.
    pub async fn list_testers(
        &self,
        owner: i64,
        id: i64,
    ) -> AppsResult<Vec<(Tester, Option<crate::repo::AccountBrief>)>> {
        let app = self.owned(owner, id).await?;
        let mut out = Vec::new();
        for t in self.repo.testers(app.id).await? {
            let brief = self.repo.account_brief(t.account_id).await?;
            out.push((t, brief));
        }
        Ok(out)
    }

    /// Remove a tester (or withdraw an open invitation). Owner only.
    pub async fn remove_tester(&self, owner: i64, id: i64, account: i64) -> AppsResult<()> {
        let app = self.owned(owner, id).await?;
        let audit = self.audit(
            app.id,
            owner,
            "app.tester_removed",
            json!({"account_id": account.to_string()}),
        );
        if self.repo.remove_tester(app.id, account, audit).await? {
            Ok(())
        } else {
            Err(AppsError::NotFound("tester"))
        }
    }

    /// The invitee answers. Only the invited account may answer, once.
    pub async fn respond_tester(
        &self,
        user: i64,
        invite_id: i64,
        accept: bool,
    ) -> AppsResult<TesterAnswer> {
        let t = self
            .repo
            .tester_by_id(invite_id)
            .await?
            .filter(|t| t.account_id == user)
            .ok_or(AppsError::NotFound("invitation"))?;
        if t.state != TesterState::Invited {
            return Err(AppsError::Conflict(
                "You answered this invitation already.".into(),
            ));
        }
        let application = self.application(t.application_id).await?;
        let details = json!({"account_id": user.to_string()});
        if accept {
            let audit = self.audit(t.application_id, user, "app.tester_accepted", details);
            self.repo.accept_tester(t.id, audit).await?;
        } else {
            let audit = self.audit(t.application_id, user, "app.tester_declined", details);
            self.repo
                .decline_tester(t.application_id, user, self.now_ms(), audit)
                .await?;
        }
        Ok(TesterAnswer {
            application,
            accepted: accept,
        })
    }

    /// Is the account an accepted tester of the application?
    pub async fn is_tester(&self, app: i64, account: i64) -> AppsResult<bool> {
        Ok(self
            .repo
            .tester_of(app, account)
            .await?
            .is_some_and(|t| t.state == TesterState::Accepted))
    }

    /// Forget declines older than the cooldown (the `apps.housekeeping` job).
    pub async fn purge_tester_declines(&self) -> AppsResult<u64> {
        self.repo
            .purge_tester_declines(self.now_ms() - TESTER_COOLDOWN_MS)
            .await
    }

    /// Testers one application may have.
    pub const fn max_testers() -> i64 {
        MAX_TESTERS
    }
}
