//! Storage port of the developer platform. `sr-db` implements it over Postgres,
//! [`crate::memory::MemoryAppsRepo`] over RAM for tests. Every method that changes several
//! rows is one transaction in the Postgres implementation.

use crate::{error::AppsResult, model::*};
use async_trait::async_trait;
use serde_json::Value;
use sr_auth::session::TokenHash;

/// The few account fields staff lists and bot profiles need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBrief {
    /// Account id.
    pub id: i64,
    /// Username.
    pub username: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Bot account?
    pub is_bot: bool,
}

/// Everything an install writes.
#[derive(Debug, Clone)]
pub struct InstallWrite {
    /// Application.
    pub application_id: i64,
    /// Its bot account.
    pub bot_user_id: i64,
    /// Bot display name (the managed role is named after it).
    pub bot_name: String,
    /// Guild.
    pub guild_id: i64,
    /// Installer.
    pub installed_by: i64,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Granted permission bits.
    pub permissions: u64,
    /// Add the bot as a member (scope `bot`).
    pub with_bot: bool,
    /// Id for the managed role.
    pub role_id: i64,
    /// Id for the guild audit entry.
    pub guild_audit_id: i64,
    /// Id for the application audit row.
    pub app_audit_id: i64,
    /// Now.
    pub now_ms: i64,
}

/// What an install changed.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// The install row.
    pub install: Install,
    /// The bot joined the guild just now.
    pub member_added: bool,
    /// The managed role created or refreshed (id, name, permissions).
    pub role: Option<(i64, String, u64)>,
    /// The role was created by this install (else an existing one was updated).
    pub role_created: bool,
    /// The guild audit entry written (Discord shape), when one was.
    pub audit_entry: Option<Value>,
}

/// Result of redeeming an authorization code.
#[derive(Debug, Clone)]
pub enum CodeConsume {
    /// First use; the code is now spent.
    Fresh(CodeRecord),
    /// The code was already used (replay): the caller revokes what it produced.
    Replayed(i64),
    /// Never issued.
    Unknown,
}

/// Outcome of a refresh-token lookup.
#[derive(Debug, Clone)]
pub enum RefreshLookup {
    /// A live, unused refresh token.
    Live(TokenRecord),
    /// Known but already rotated (reuse) or revoked.
    Spent(TokenRecord),
    /// Never issued.
    Unknown,
}

/// The storage port.
#[async_trait]
pub trait AppsRepository: Send + Sync {
    // ----- accounts and developer requests
    /// The account's brief (None if unknown).
    async fn account_brief(&self, account: i64) -> AppsResult<Option<AccountBrief>>;
    /// `accounts.flags`.
    async fn account_flags(&self, account: i64) -> AppsResult<Option<i64>>;
    /// New pending request. `Conflict` when one is pending already.
    async fn create_developer_request(
        &self,
        id: i64,
        account: i64,
        pitch: &str,
        prior_work: &str,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest>;
    /// The account's newest request.
    async fn latest_developer_request(&self, account: i64) -> AppsResult<Option<DeveloperRequest>>;
    /// Requests by state, newest first.
    async fn developer_requests(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(DeveloperRequest, AccountBrief)>>;
    /// Decide a pending request; approval sets `APP_DEVELOPER`. `Conflict` if not pending.
    async fn decide_developer_request(
        &self,
        id: i64,
        reviewer: i64,
        approve: bool,
        note: Option<&str>,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest>;

    // ----- applications
    /// Create the bot account, the application, its first token and the audit row.
    async fn create_application(&self, new: NewApplication) -> AppsResult<Application>;
    /// One application.
    async fn application(&self, id: i64) -> AppsResult<Option<Application>>;
    /// The application whose bot account is `bot_user`.
    async fn application_by_bot(&self, bot_user: i64) -> AppsResult<Option<Application>>;
    /// The owner's applications, oldest first.
    async fn applications_of(&self, owner: i64) -> AppsResult<Vec<Application>>;
    /// Apply a validated patch (renaming also renames the bot's display name).
    async fn update_application(
        &self,
        id: i64,
        patch: &AppPatch,
        audit: AuditRow,
    ) -> AppsResult<Application>;
    /// Rename the bot account's display name.
    async fn set_bot_name(&self, app: i64, name: &str, audit: AuditRow) -> AppsResult<()>;

    /// Interactions endpoint URL and signing seed of an application.
    async fn interactions_config(&self, app: i64) -> AppsResult<InteractionsConfig>;

    /// Set the endpoint URL (`Some(None)` clears) and/or store a signing seed. An existing
    /// seed is never replaced by `seed_if_absent`.
    async fn set_interactions_config(
        &self,
        app: i64,
        endpoint_url: Option<Option<String>>,
        seed_if_absent: Option<crate::signing::SigningSeed>,
        audit: Option<AuditRow>,
    ) -> AppsResult<()>;
    /// Replace the client secret.
    async fn rotate_secret(
        &self,
        app: i64,
        hash: &TokenHash,
        hint: &str,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()>;
    /// Revoke the live bot token and store a new one.
    async fn reset_bot_token(
        &self,
        app: i64,
        token_id: i64,
        hash: &TokenHash,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()>;
    /// Delete: remove the bot from every guild, deactivate the bot account, delete the
    /// application (cascading tokens, commands, grants). Returns the guilds the bot left.
    async fn delete_application(&self, app: i64, audit: AuditRow) -> AppsResult<Vec<i64>>;
    /// Resolve a bot token digest.
    async fn bot_by_token_hash(&self, hash: &TokenHash) -> AppsResult<Option<BotRecord>>;
    /// Audit trail, newest first.
    async fn application_audit(
        &self,
        app: i64,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<AuditRow>>;
    /// Append an audit row.
    async fn record_audit(&self, row: AuditRow) -> AppsResult<()>;

    // ----- release review
    /// Submit a version and move the application to `in_review`.
    #[allow(clippy::too_many_arguments)]
    async fn create_version(
        &self,
        id: i64,
        app: i64,
        manifest: Value,
        reasons: Vec<String>,
        note: Option<&str>,
        by: i64,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<AppVersion>;
    /// Versions of an application, newest first.
    async fn versions(&self, app: i64) -> AppsResult<Vec<AppVersion>>;
    /// One version with its application.
    async fn version(&self, id: i64) -> AppsResult<Option<(AppVersion, Application)>>;
    /// Versions by state, newest first.
    async fn versions_by_state(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(AppVersion, Application)>>;
    /// The newest approved manifest, for the diff.
    async fn last_approved_manifest(&self, app: i64) -> AppsResult<Option<Value>>;
    /// Move a pending version to `approved`, `rejected`, `changes_requested` or `withdrawn`
    /// and update the application's release state (and approved intents on approval).
    #[allow(clippy::too_many_arguments)]
    async fn decide_version(
        &self,
        id: i64,
        reviewer: i64,
        state: &str,
        note: Option<&str>,
        approved_intents: Option<u64>,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<(AppVersion, Application)>;
    /// Suspend or restore.
    async fn set_suspended(
        &self,
        app: i64,
        suspended: bool,
        reason: Option<&str>,
        audit: AuditRow,
    ) -> AppsResult<Application>;

    // ----- installs
    /// Owner and member count of a guild.
    async fn guild_stats(&self, guild: i64) -> AppsResult<Option<GuildStats>>;
    /// Guilds the user is a member of (at most 200), for the consent screen.
    async fn user_guilds(&self, user: i64) -> AppsResult<Vec<GuildBrief>>;
    /// Record an install and, with `bot`, add the member and its managed role.
    async fn install(&self, req: InstallWrite) -> AppsResult<InstallOutcome>;
    /// Installs of an application.
    async fn installs(&self, app: i64) -> AppsResult<Vec<Install>>;
    /// Installs in one guild.
    async fn guild_installs(&self, guild: i64) -> AppsResult<Vec<Install>>;
    /// Is the application installed (and its bot still a member when it needs to be)?
    async fn is_installed(&self, app: i64, guild: i64) -> AppsResult<bool>;

    // ----- commands
    /// Commands of one scope (`None` = global), by name.
    async fn commands(&self, app: i64, guild: Option<i64>) -> AppsResult<Vec<Command>>;
    /// One command of any scope.
    async fn command(&self, app: i64, id: i64) -> AppsResult<Option<Command>>;
    /// A command by id regardless of the application (interactions look it up this way).
    async fn command_by_id(&self, id: i64) -> AppsResult<Option<Command>>;
    /// Create, or replace the one with the same kind and name. The bool is "created".
    async fn upsert_command(
        &self,
        app: i64,
        guild: Option<i64>,
        new_id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<(Command, bool)>;
    /// Replace an existing command by id with a validated body.
    async fn replace_command(
        &self,
        app: i64,
        id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<Command>;
    /// Delete a command and its permission rows.
    async fn delete_command(&self, app: i64, id: i64) -> AppsResult<bool>;
    /// Replace the whole set of a scope in one transaction. `ids` supplies fresh ids for
    /// names that did not exist; existing names keep their id. Returns the new set.
    async fn bulk_overwrite(
        &self,
        app: i64,
        guild: Option<i64>,
        ids: Vec<i64>,
        version: i64,
        cmds: &[NewCommand],
    ) -> AppsResult<Vec<Command>>;
    /// Overwrites of one command (`command_id == app` for the application default).
    async fn command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
    ) -> AppsResult<Vec<CommandPermission>>;
    /// Every command's overwrites in a guild.
    async fn guild_command_permissions(
        &self,
        app: i64,
        guild: i64,
    ) -> AppsResult<Vec<(i64, Vec<CommandPermission>)>>;
    /// Replace the overwrites of one command.
    async fn set_command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
        perms: &[CommandPermission],
    ) -> AppsResult<()>;

    // ----- OAuth2
    /// Create or refresh the live grant of a user for an application (scopes are merged).
    async fn upsert_grant(
        &self,
        new_id: i64,
        app: i64,
        user: i64,
        scopes: &[String],
        now_ms: i64,
    ) -> AppsResult<Grant>;
    /// Store an authorization code digest.
    async fn store_code(
        &self,
        hash: &TokenHash,
        grant_id: i64,
        redirect_uri: &str,
        scopes: &[String],
        challenge: Option<&str>,
        expires_ms: i64,
    ) -> AppsResult<()>;
    /// Atomically spend a code.
    async fn consume_code(&self, hash: &TokenHash) -> AppsResult<CodeConsume>;
    /// Store a token pair (optionally remembering the code it came from).
    async fn insert_token(
        &self,
        rec: &TokenRecord,
        code: Option<&TokenHash>,
        now_ms: i64,
    ) -> AppsResult<()>;
    /// Resolve an access token digest (None when revoked or the grant is revoked).
    async fn token_by_access(&self, hash: &TokenHash) -> AppsResult<Option<TokenRecord>>;
    /// Resolve a refresh token digest.
    async fn token_by_refresh(&self, hash: &TokenHash) -> AppsResult<RefreshLookup>;
    /// Spend a refresh token (compare-and-set; `false` when already spent).
    async fn spend_refresh(&self, token_id: i64) -> AppsResult<bool>;
    /// Revoke one pair.
    async fn revoke_token(&self, token_id: i64) -> AppsResult<()>;
    /// Revoke a grant and every token of it.
    async fn revoke_grant(&self, grant_id: i64) -> AppsResult<()>;
    /// Revoke the tokens minted from a code (replay).
    async fn revoke_code_tokens(&self, grant_id: i64, code: &TokenHash) -> AppsResult<()>;
    /// A user's live grants with their applications.
    async fn grants_of(&self, user: i64) -> AppsResult<Vec<(Grant, Application)>>;
    /// One live grant of a user.
    async fn grant_of(&self, user: i64, grant_id: i64) -> AppsResult<Option<Grant>>;

    // ----- interactions
    /// Store a new interaction (token digest inside).
    async fn insert_interaction(&self, row: &InteractionRow) -> AppsResult<()>;
    /// One interaction.
    async fn interaction(&self, id: i64) -> AppsResult<Option<InteractionRow>>;
    /// The interaction whose token has this digest.
    async fn interaction_by_hash(&self, hash: &TokenHash) -> AppsResult<Option<InteractionRow>>;
    /// `created -> state` exactly once (compare-and-set). `false` when it already moved.
    async fn ack_interaction(
        &self,
        id: i64,
        state: &str,
        response_type: i16,
        ephemeral: bool,
        now_ms: i64,
    ) -> AppsResult<bool>;
    /// Remember the original response message.
    async fn set_original_message(&self, id: i64, message_id: i64) -> AppsResult<()>;
    /// Map a message to its interaction.
    async fn add_interaction_message(&self, m: &InteractionMessage) -> AppsResult<()>;
    /// The mapping of a message.
    async fn interaction_message(&self, message_id: i64) -> AppsResult<Option<InteractionMessage>>;
    /// Replace the stored payload of an ephemeral message.
    async fn set_ephemeral_payload(&self, message_id: i64, payload: &Value) -> AppsResult<()>;
    /// Forget a message mapping.
    async fn delete_interaction_message(&self, message_id: i64) -> AppsResult<()>;
    /// Drop interactions that expired before `before_ms`.
    async fn purge_interactions(&self, before_ms: i64) -> AppsResult<u64>;
}
