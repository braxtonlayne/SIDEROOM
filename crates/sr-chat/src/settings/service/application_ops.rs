//! Membership applications (LNG-SSET-042).

use sr_core::permissions::Permissions;
use sr_core::{GuildId, UserId};

use super::super::applications::{self, Answer, Application, ApplicationForm, ApplicationStatus};
use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::{SettingsService, require};

impl<R: SettingsRepository> SettingsService<R> {
    /// The guild form (disabled and empty until saved).
    pub async fn application_form(&self, guild: GuildId) -> SettingsResult<ApplicationForm> {
        Ok(self.repo.get_application_form(guild).await?)
    }

    /// Save the form and the "apply to join" switch (`MANAGE_GUILD`).
    pub async fn save_application_form(
        &self,
        perms: Permissions,
        actor: UserId,
        form: ApplicationForm,
    ) -> SettingsResult<ApplicationForm> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let form = applications::validate_form(form, || self.new_id())?;
        let old = self.repo.get_application_form(form.guild_id).await?;
        self.repo.put_application_form(&form).await?;
        if old != form {
            let mut changes = vec![AuditChange::new(
                "applications_enabled",
                Some(old.enabled.into()),
                Some(form.enabled.into()),
            )];
            if old.questions != form.questions {
                changes.push(AuditChange::new(
                    "applications_questions",
                    Some(old.questions.len().into()),
                    Some(form.questions.len().into()),
                ));
            }
            self.record_audit(
                form.guild_id,
                actor,
                AuditAction::GuildUpdate,
                Some(form.guild_id.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(form)
    }

    /// Submit an application. The caller says whether the user already is a member.
    pub async fn submit_application(
        &self,
        guild: GuildId,
        user: UserId,
        answers: &[Answer],
        is_member: bool,
    ) -> SettingsResult<Application> {
        let form = self.repo.get_application_form(guild).await?;
        if !form.enabled {
            return Err(SettingsError::invalid(
                "guild_id",
                "APPLICATIONS_CLOSED",
                "This server is not taking applications.",
            ));
        }
        if is_member {
            return Err(SettingsError::invalid(
                "guild_id",
                "ALREADY_MEMBER",
                "You are already a member of this server.",
            ));
        }
        if self.repo.is_banned(guild, user).await? {
            return Err(SettingsError::Banned);
        }
        let answers = applications::validate_answers(&form, answers)?;
        let app = Application {
            id: self.new_id(),
            guild_id: guild,
            user_id: user,
            answers,
            status: ApplicationStatus::Pending,
            created_at_ms: self.now(),
            reviewed_by: None,
            reviewed_at_ms: None,
            rejection_reason: None,
        };
        if !self.repo.insert_application(&app).await? {
            return Err(SettingsError::invalid(
                "application",
                "APPLICATION_PENDING",
                "You already have an application waiting for review.",
            ));
        }
        Ok(app)
    }

    /// The caller's latest application to the guild.
    pub async fn my_application(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> SettingsResult<Option<Application>> {
        Ok(self.repo.latest_application(guild, user).await?)
    }

    /// The review queue, newest first (`MANAGE_GUILD`).
    pub async fn list_applications(
        &self,
        perms: Permissions,
        guild: GuildId,
        status: Option<ApplicationStatus>,
        before: Option<u64>,
        limit: usize,
    ) -> SettingsResult<Vec<Application>> {
        require(perms, Permissions::MANAGE_GUILD)?;
        Ok(self
            .repo
            .list_applications(guild, status, before, limit.clamp(1, 100))
            .await?)
    }

    /// Approve (adds the member) or reject a pending application (`MANAGE_GUILD`).
    pub async fn decide_application(
        &self,
        perms: Permissions,
        guild: GuildId,
        id: u64,
        approve: bool,
        reviewer: UserId,
        reason: Option<&str>,
    ) -> SettingsResult<Application> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let reason = applications::normalize_rejection_reason(reason)?;
        let app = self
            .repo
            .decide_application(guild, id, approve, reviewer, self.now(), reason.as_deref())
            .await?
            .ok_or(SettingsError::UnknownApplication)?;
        // G-33: the decision lands in the guild audit log; the applicant is the target.
        let entry = super::super::audit::AuditEntry::new(
            self.ids.generate().get(),
            guild,
            reviewer,
            AuditAction::ApplicationDecision,
            Some(app.user_id.get()),
            reason.as_deref(),
            self.now(),
        )?
        .with_options(serde_json::json!({
            "status": app.status.as_str(),
            "application_id": app.id.to_string(),
        }));
        self.store_audit(&entry).await?;
        Ok(app)
    }
}
