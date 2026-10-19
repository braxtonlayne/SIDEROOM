//! Application commands: registration, limits, bulk overwrite, permissions v2
//! (DEV-BOT-003).

use super::AppsService;
use crate::{
    commands::{self, check_limit},
    error::{AppsError, AppsResult, FieldProblem},
    model::*,
};
use serde_json::Value;

/// Overwrites of one command (or the application default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionsView {
    /// Command id, or the application id for the application default.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// Guild.
    pub guild_id: i64,
    /// Entries.
    pub permissions: Vec<CommandPermission>,
}

fn problems(p: Vec<FieldProblem>) -> AppsError {
    AppsError::Invalid(p)
}

impl AppsService {
    /// A guild scope is only usable once the application is installed there.
    async fn scope_ok(&self, app: i64, guild: Option<i64>) -> AppsResult<()> {
        if let Some(g) = guild
            && !self.repo.is_installed(app, g).await?
        {
            return Err(AppsError::Forbidden("Missing Access".into()));
        }
        Ok(())
    }

    /// Commands of a scope.
    pub async fn list_commands(&self, app: i64, guild: Option<i64>) -> AppsResult<Vec<Command>> {
        self.scope_ok(app, guild).await?;
        self.repo.commands(app, guild).await
    }

    /// One command of a scope.
    pub async fn get_command(&self, app: i64, guild: Option<i64>, id: i64) -> AppsResult<Command> {
        self.scope_ok(app, guild).await?;
        self.repo
            .command(app, id)
            .await?
            .filter(|c| c.guild_id == guild)
            .ok_or(AppsError::UnknownCommand)
    }

    /// Create a command, or update the one with the same type and name. The bool is "created".
    pub async fn create_command(
        &self,
        app: i64,
        guild: Option<i64>,
        body: &Value,
    ) -> AppsResult<(Command, bool)> {
        self.scope_ok(app, guild).await?;
        let new = commands::validate_command(body).map_err(problems)?;
        let existing = self.repo.commands(app, guild).await?;
        let replaces = existing
            .iter()
            .any(|c| c.kind == new.kind && c.name == new.name);
        if !replaces {
            let kinds: Vec<i16> = existing.iter().map(|c| c.kind).collect();
            check_limit(&kinds, new.kind)
                .map_err(|m| AppsError::field("name", "MAX_APPLICATION_COMMANDS", m))?;
        }
        self.repo
            .upsert_command(app, guild, self.next_id(), self.next_id(), &new)
            .await
    }

    /// Modify a command.
    pub async fn patch_command(
        &self,
        app: i64,
        guild: Option<i64>,
        id: i64,
        body: &Value,
    ) -> AppsResult<Command> {
        let current = self.get_command(app, guild, id).await?;
        let new = commands::validate_patch(&current, body).map_err(problems)?;
        if new.name != current.name {
            let clash = self
                .repo
                .commands(app, guild)
                .await?
                .into_iter()
                .any(|c| c.id != id && c.kind == new.kind && c.name == new.name);
            if clash {
                return Err(AppsError::field(
                    "name",
                    "APPLICATION_COMMAND_DUPLICATE_NAME",
                    "Application command names must be unique.",
                ));
            }
        }
        self.repo
            .replace_command(app, id, self.next_id(), &new)
            .await
    }

    /// Delete a command.
    pub async fn delete_command(&self, app: i64, guild: Option<i64>, id: i64) -> AppsResult<()> {
        let c = self.get_command(app, guild, id).await?;
        if self.repo.delete_command(app, c.id).await? {
            Ok(())
        } else {
            Err(AppsError::UnknownCommand)
        }
    }

    /// Replace a scope's whole command set.
    pub async fn bulk_overwrite(
        &self,
        app: i64,
        guild: Option<i64>,
        body: &Value,
    ) -> AppsResult<Vec<Command>> {
        self.scope_ok(app, guild).await?;
        let Some(list) = body.as_array() else {
            return Err(AppsError::field(
                "",
                "MODEL_TYPE_CONVERT",
                "Body must be an array of commands.",
            ));
        };
        let mut all = Vec::with_capacity(list.len());
        let mut errs = Vec::new();
        for (i, raw) in list.iter().enumerate() {
            match commands::validate_command(raw) {
                Ok(c) => all.push(c),
                Err(ps) => errs.extend(ps.into_iter().map(|mut p| {
                    p.path = if p.path.is_empty() {
                        i.to_string()
                    } else {
                        format!("{i}.{}", p.path)
                    };
                    p
                })),
            }
        }
        if !errs.is_empty() {
            return Err(AppsError::Invalid(errs));
        }
        for (i, c) in all.iter().enumerate() {
            if all[..i]
                .iter()
                .any(|o| o.kind == c.kind && o.name == c.name)
            {
                return Err(AppsError::field(
                    &format!("{i}.name"),
                    "APPLICATION_COMMAND_DUPLICATE_NAME",
                    "Application command names must be unique.",
                ));
            }
        }
        for kind in 1..=3i16 {
            let kinds: Vec<i16> = all.iter().map(|c| c.kind).collect();
            let n = kinds.iter().filter(|k| **k == kind).count();
            if n > 0 {
                let mut without_last = kinds.clone();
                if let Some(pos) = without_last.iter().rposition(|k| *k == kind) {
                    without_last.remove(pos);
                }
                if n > 0 && check_limit(&without_last, kind).is_err() {
                    return Err(AppsError::field(
                        "",
                        "MAX_APPLICATION_COMMANDS",
                        "Too many commands of one type.",
                    ));
                }
            }
        }
        let ids: Vec<i64> = (0..all.len()).map(|_| self.next_id()).collect();
        self.repo
            .bulk_overwrite(app, guild, ids, self.next_id(), &all)
            .await
    }

    // ------------------------------------------------------------ permissions v2

    /// One command's overwrites, or the application default when `command` is `None`.
    pub async fn command_permissions(
        &self,
        app: i64,
        guild: i64,
        command: Option<i64>,
    ) -> AppsResult<PermissionsView> {
        self.scope_ok(app, Some(guild)).await?;
        let id = match command {
            Some(c) => {
                let cmd = self
                    .repo
                    .command(app, c)
                    .await?
                    .ok_or(AppsError::UnknownCommand)?;
                if cmd.guild_id.is_some_and(|g| g != guild) {
                    return Err(AppsError::UnknownCommand);
                }
                c
            }
            None => app,
        };
        Ok(PermissionsView {
            id,
            application_id: app,
            guild_id: guild,
            permissions: self.repo.command_permissions(app, guild, id).await?,
        })
    }

    /// Every command's overwrites in a guild.
    pub async fn guild_permissions(
        &self,
        app: i64,
        guild: i64,
    ) -> AppsResult<Vec<PermissionsView>> {
        self.scope_ok(app, Some(guild)).await?;
        Ok(self
            .repo
            .guild_command_permissions(app, guild)
            .await?
            .into_iter()
            .map(|(id, permissions)| PermissionsView {
                id,
                application_id: app,
                guild_id: guild,
                permissions,
            })
            .collect())
    }

    /// Replace one command's overwrites (the caller has already checked `MANAGE_GUILD`).
    pub async fn set_command_permissions(
        &self,
        app: i64,
        guild: i64,
        command: Option<i64>,
        body: &Value,
    ) -> AppsResult<PermissionsView> {
        let view = self.command_permissions(app, guild, command).await?;
        let Some(list) = body.get("permissions").and_then(Value::as_array) else {
            return Err(AppsError::field(
                "permissions",
                "BASE_TYPE_REQUIRED",
                "This field is required",
            ));
        };
        if list.len() > 100 {
            return Err(AppsError::field(
                "permissions",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 100 or fewer in length.",
            ));
        }
        let mut out: Vec<CommandPermission> = Vec::new();
        for (i, e) in list.iter().enumerate() {
            let id = e
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0);
            let ty = e
                .get("type")
                .and_then(Value::as_i64)
                .filter(|t| (1..=3).contains(t));
            let allowed = e.get("permission").and_then(Value::as_bool);
            let (Some(id), Some(ty), Some(allowed)) = (id, ty, allowed) else {
                return Err(AppsError::field(
                    &format!("permissions.{i}"),
                    "BASE_TYPE_REQUIRED",
                    "Each entry needs id, type (1 role, 2 user, 3 channel) and permission.",
                ));
            };
            if out
                .iter()
                .any(|o| o.target_id == id && o.target_type == ty as i16)
            {
                return Err(AppsError::field(
                    &format!("permissions.{i}.id"),
                    "BASE_TYPE_UNIQUE",
                    "Duplicate entry.",
                ));
            }
            out.push(CommandPermission {
                target_id: id,
                target_type: ty as i16,
                allowed,
            });
        }
        self.repo
            .set_command_permissions(app, guild, view.id, &out)
            .await?;
        Ok(PermissionsView {
            permissions: out,
            ..view
        })
    }
}

/// One installed application with the commands it offers in a guild.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    /// The application (live ones only).
    pub application: Application,
    /// Global commands (when the install has `applications.commands`) then the guild's own.
    pub commands: Vec<Command>,
}

impl AppsService {
    /// What a member can pick in the command picker: for every live application installed in
    /// `guild`, its global commands (if the install carries `applications.commands`) and its
    /// guild commands. The caller filters by what the member may use.
    pub async fn command_index(&self, guild: i64) -> AppsResult<Vec<IndexEntry>> {
        let mut out = Vec::new();
        for install in self.repo.guild_installs(guild).await? {
            let Some(application) = self.repo.application(install.application_id).await? else {
                continue;
            };
            if !application.is_live() {
                continue;
            }
            let mut commands = Vec::new();
            if install.scopes.iter().any(|s| s == "applications.commands") {
                commands.extend(self.repo.commands(application.id, None).await?);
            }
            commands.extend(self.repo.commands(application.id, Some(guild)).await?);
            commands.retain(|c| c.guild_offered());
            if !commands.is_empty() {
                out.push(IndexEntry {
                    application,
                    commands,
                });
            }
        }
        Ok(out)
    }
}
