//! Postgres [`sr_apps::AppsRepository`] (tables in migrations 0500 to 0505). Multi-row
//! changes run in one transaction; state transitions are guarded `UPDATE`s so concurrent
//! requests cannot both win. Tests: `tests/apps_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow};
use sr_apps::{
    AppsError, AppsResult,
    model::*,
    repo::{
        AccountBrief, AppsRepository, CodeConsume, InstallOutcome, InstallWrite, RefreshLookup,
    },
};
use sr_auth::session::TokenHash;

/// The repository.
#[derive(Clone)]
pub struct PgAppsRepo {
    pool: PgPool,
}

impl PgAppsRepo {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> AppsError {
    AppsError::Storage(e.to_string())
}

fn is_unique(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23505"))
}

fn ms(col: &str, name: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint AS {name}")
}

/// `to_timestamp` of a bound millisecond value.
const TS: &str = "to_timestamp($1::double precision / 1000.0)";

fn hash_of(r: &PgRow, col: &str) -> AppsResult<TokenHash> {
    let b: Vec<u8> = r.try_get(col).map_err(db)?;
    TokenHash::from_bytes(&b).map_err(|e| AppsError::Storage(e.to_string()))
}

fn app_cols(a: &str) -> String {
    format!(
        "{a}.id, {a}.owner_id, {a}.bot_user_id, {a}.name, {a}.description, {a}.icon_key, {a}.tags, \
         {a}.bot_public, {a}.bot_require_code_grant, {a}.redirect_uris, {a}.install_params, \
         {a}.terms_url, {a}.privacy_url, {a}.release_state::text AS release_state, \
         {a}.suspended_reason, {a}.intents_requested, {a}.intents_approved, \
         {a}.client_secret_hash, {a}.client_secret_hint, {}",
        ms(&format!("{a}.created_at"), "created_ms")
    )
}

fn app_from(r: &PgRow) -> AppsResult<Application> {
    let state: String = r.try_get("release_state").map_err(db)?;
    let params: Value = r.try_get("install_params").map_err(db)?;
    let scopes = params["scopes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let perms = params["permissions"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let g = |c: &str| -> AppsResult<i64> { r.try_get(c).map_err(db) };
    Ok(Application {
        id: g("id")?,
        owner_id: g("owner_id")?,
        bot_user_id: g("bot_user_id")?,
        name: r.try_get("name").map_err(db)?,
        description: r.try_get("description").map_err(db)?,
        icon_key: r.try_get("icon_key").map_err(db)?,
        tags: r.try_get("tags").map_err(db)?,
        bot_public: r.try_get("bot_public").map_err(db)?,
        bot_require_code_grant: r.try_get("bot_require_code_grant").map_err(db)?,
        redirect_uris: r.try_get("redirect_uris").map_err(db)?,
        install_scopes: scopes,
        install_permissions: perms,
        terms_url: r.try_get("terms_url").map_err(db)?,
        privacy_url: r.try_get("privacy_url").map_err(db)?,
        release_state: ReleaseState::parse(&state)
            .ok_or_else(|| AppsError::Storage(format!("unknown release state {state}")))?,
        suspended_reason: r.try_get("suspended_reason").map_err(db)?,
        intents_requested: g("intents_requested")? as u64,
        intents_approved: g("intents_approved")? as u64,
        client_secret_hash: hash_of(r, "client_secret_hash")?,
        client_secret_hint: r.try_get("client_secret_hint").map_err(db)?,
        created_at_ms: g("created_ms")?,
    })
}

async fn fetch_app(tx: &mut Transaction<'_, Postgres>, id: i64) -> AppsResult<Application> {
    let sql = format!(
        "SELECT {} FROM applications a WHERE a.id = $1",
        app_cols("a")
    );
    let r = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(AppsError::NotFound("application"))?;
    app_from(&r)
}

async fn audit_in(tx: &mut Transaction<'_, Postgres>, a: &AuditRow) -> AppsResult<()> {
    sqlx::query(
        "INSERT INTO application_audit (id, application_id, actor_id, action, details, created_at) \
         VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
    )
    .bind(a.id)
    .bind(a.application_id)
    .bind(a.actor_id)
    .bind(&a.action)
    .bind(&a.details)
    .bind(a.created_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

fn request_cols_p(p: &str) -> String {
    format!(
        "{p}id, {p}account_id, {p}pitch, {p}prior_work, {p}state::text AS state, {p}reviewer_note, {}, {}",
        ms(&format!("{p}created_at"), "created_ms"),
        ms(&format!("{p}reviewed_at"), "reviewed_ms")
    )
}

fn request_cols() -> String {
    request_cols_p("")
}

fn request_from(r: &PgRow) -> AppsResult<DeveloperRequest> {
    Ok(DeveloperRequest {
        id: r.try_get("id").map_err(db)?,
        account_id: r.try_get("account_id").map_err(db)?,
        pitch: r.try_get("pitch").map_err(db)?,
        prior_work: r.try_get("prior_work").map_err(db)?,
        state: r.try_get("state").map_err(db)?,
        reviewer_note: r.try_get("reviewer_note").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        reviewed_at_ms: r.try_get("reviewed_ms").map_err(db)?,
    })
}

fn version_cols(v: &str) -> String {
    format!(
        "{v}.id AS v_id, {v}.application_id AS v_app, {v}.version, {v}.manifest, \
         {v}.needs_review_reasons, {v}.note, {v}.state::text AS v_state, {v}.reviewer_note AS v_reviewer_note, {}, {}",
        ms(&format!("{v}.created_at"), "v_created_ms"),
        ms(&format!("{v}.decided_at"), "v_decided_ms")
    )
}

fn version_from(r: &PgRow) -> AppsResult<AppVersion> {
    Ok(AppVersion {
        id: r.try_get("v_id").map_err(db)?,
        application_id: r.try_get("v_app").map_err(db)?,
        version: r.try_get("version").map_err(db)?,
        manifest: r.try_get("manifest").map_err(db)?,
        needs_review_reasons: r.try_get("needs_review_reasons").map_err(db)?,
        note: r.try_get("note").map_err(db)?,
        state: r.try_get("v_state").map_err(db)?,
        reviewer_note: r.try_get("v_reviewer_note").map_err(db)?,
        created_at_ms: r.try_get("v_created_ms").map_err(db)?,
        decided_at_ms: r.try_get("v_decided_ms").map_err(db)?,
    })
}

const COMMAND_COLS: &str = "id, application_id, guild_id, kind, name, description, options, \
     name_localizations, description_localizations, default_member_permissions, dm_permission, \
     nsfw, contexts, integration_types, version";

fn command_from(r: &PgRow) -> AppsResult<Command> {
    let perms: Option<i64> = r.try_get("default_member_permissions").map_err(db)?;
    Ok(Command {
        id: r.try_get("id").map_err(db)?,
        application_id: r.try_get("application_id").map_err(db)?,
        guild_id: r.try_get("guild_id").map_err(db)?,
        kind: r.try_get("kind").map_err(db)?,
        name: r.try_get("name").map_err(db)?,
        description: r.try_get("description").map_err(db)?,
        options: r.try_get("options").map_err(db)?,
        name_localizations: r.try_get("name_localizations").map_err(db)?,
        description_localizations: r.try_get("description_localizations").map_err(db)?,
        default_member_permissions: perms.map(|p| p as u64),
        dm_permission: r.try_get("dm_permission").map_err(db)?,
        nsfw: r.try_get("nsfw").map_err(db)?,
        contexts: r.try_get("contexts").map_err(db)?,
        integration_types: r.try_get("integration_types").map_err(db)?,
        version: r.try_get("version").map_err(db)?,
    })
}

fn grant_from(r: &PgRow) -> AppsResult<Grant> {
    Ok(Grant {
        id: r.try_get("grant_id").map_err(db)?,
        application_id: r.try_get("grant_app").map_err(db)?,
        user_id: r.try_get("grant_user").map_err(db)?,
        scopes: r.try_get("grant_scopes").map_err(db)?,
        created_at_ms: r.try_get("grant_created_ms").map_err(db)?,
    })
}

fn grant_cols(g: &str) -> String {
    format!(
        "{g}.id AS grant_id, {g}.application_id AS grant_app, {g}.user_id AS grant_user, \
         {g}.scopes AS grant_scopes, {}",
        ms(&format!("{g}.created_at"), "grant_created_ms")
    )
}

const TOKEN_COLS: &str = "t.id AS tok_id, t.grant_id AS tok_grant, g.application_id AS tok_app, \
     g.user_id AS tok_user, t.scopes AS tok_scopes, t.access_hash, t.refresh_hash, \
     (extract(epoch FROM t.access_expires_at) * 1000)::bigint AS tok_access_ms, \
     (extract(epoch FROM t.refresh_expires_at) * 1000)::bigint AS tok_refresh_ms, \
     (t.refresh_used_at IS NOT NULL) AS tok_used, \
     (t.revoked_at IS NOT NULL OR g.revoked_at IS NOT NULL) AS tok_revoked";

fn token_from(r: &PgRow) -> AppsResult<TokenRecord> {
    Ok(TokenRecord {
        id: r.try_get("tok_id").map_err(db)?,
        grant_id: r.try_get("tok_grant").map_err(db)?,
        application_id: r.try_get("tok_app").map_err(db)?,
        user_id: r.try_get("tok_user").map_err(db)?,
        scopes: r.try_get("tok_scopes").map_err(db)?,
        access_hash: hash_of(r, "access_hash")?,
        refresh_hash: hash_of(r, "refresh_hash")?,
        access_expires_at_ms: r.try_get("tok_access_ms").map_err(db)?,
        refresh_expires_at_ms: r.try_get("tok_refresh_ms").map_err(db)?,
        refresh_used: r.try_get("tok_used").map_err(db)?,
        revoked: r.try_get("tok_revoked").map_err(db)?,
    })
}

const INTERACTION_COLS: &str = "id, application_id, token_hash, kind, user_id, guild_id, channel_id, \
     command_id, source_message_id, nonce, state, response_type, ephemeral, original_message_id, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms";

fn interaction_from(r: &PgRow) -> AppsResult<InteractionRow> {
    let h: Vec<u8> = r.try_get("token_hash").map_err(db)?;
    let token_hash: [u8; 32] = h
        .try_into()
        .map_err(|_| AppsError::Storage("bad token digest length".into()))?;
    Ok(InteractionRow {
        id: r.try_get("id").map_err(db)?,
        application_id: r.try_get("application_id").map_err(db)?,
        kind: r.try_get("kind").map_err(db)?,
        user_id: r.try_get("user_id").map_err(db)?,
        guild_id: r.try_get("guild_id").map_err(db)?,
        channel_id: r.try_get("channel_id").map_err(db)?,
        command_id: r.try_get("command_id").map_err(db)?,
        source_message_id: r.try_get("source_message_id").map_err(db)?,
        nonce: r.try_get("nonce").map_err(db)?,
        state: r.try_get("state").map_err(db)?,
        response_type: r.try_get("response_type").map_err(db)?,
        ephemeral: r.try_get("ephemeral").map_err(db)?,
        original_message_id: r.try_get("original_message_id").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        expires_at_ms: r.try_get("expires_ms").map_err(db)?,
        token_hash,
    })
}

fn bind_command<'q>(
    q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    c: &'q NewCommand,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    q.bind(c.kind)
        .bind(&c.name)
        .bind(&c.description)
        .bind(&c.options)
        .bind(&c.name_localizations)
        .bind(&c.description_localizations)
        .bind(c.default_member_permissions.map(|p| p as i64))
        .bind(c.dm_permission)
        .bind(c.nsfw)
        .bind(&c.contexts)
        .bind(&c.integration_types)
}

#[async_trait]
impl AppsRepository for PgAppsRepo {
    async fn account_brief(&self, account: i64) -> AppsResult<Option<AccountBrief>> {
        let r = sqlx::query(
            "SELECT id, username::text AS username, display_name, is_bot FROM accounts WHERE id = $1",
        )
        .bind(account)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        r.map(|r| {
            Ok(AccountBrief {
                id: r.try_get("id").map_err(db)?,
                username: r.try_get("username").map_err(db)?,
                display_name: r.try_get("display_name").map_err(db)?,
                is_bot: r.try_get("is_bot").map_err(db)?,
            })
        })
        .transpose()
    }

    async fn account_flags(&self, account: i64) -> AppsResult<Option<i64>> {
        sqlx::query_scalar("SELECT flags FROM accounts WHERE id = $1 AND state = 'active'")
            .bind(account)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)
    }

    async fn create_developer_request(
        &self,
        id: i64,
        account: i64,
        pitch: &str,
        prior_work: &str,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest> {
        let sql = format!(
            "INSERT INTO developer_requests (id, account_id, pitch, prior_work, terms_accepted_at, created_at) \
             VALUES ($2, $3, $4, $5, {TS}, {TS}) RETURNING {}",
            request_cols()
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(id)
            .bind(account)
            .bind(pitch)
            .bind(prior_work)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                if is_unique(&e) {
                    AppsError::Conflict("A request is already waiting for review.".into())
                } else {
                    db(e)
                }
            })?;
        request_from(&r)
    }

    async fn latest_developer_request(&self, account: i64) -> AppsResult<Option<DeveloperRequest>> {
        let sql = format!(
            "SELECT {} FROM developer_requests WHERE account_id = $1 ORDER BY id DESC LIMIT 1",
            request_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(request_from)
            .transpose()
    }

    async fn developer_requests(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(DeveloperRequest, AccountBrief)>> {
        let sql = format!(
            "SELECT {}, a.username::text AS a_username, a.display_name AS a_display, a.is_bot AS a_bot \
             FROM developer_requests d JOIN accounts a ON a.id = d.account_id \
             WHERE d.state = $1::developer_request_state AND ($2::bigint IS NULL OR d.id < $2) \
             ORDER BY d.id DESC LIMIT $3",
            request_cols_p("d.")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(state)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| {
                let req = request_from(r)?;
                let brief = AccountBrief {
                    id: req.account_id,
                    username: r.try_get("a_username").map_err(db)?,
                    display_name: r.try_get("a_display").map_err(db)?,
                    is_bot: r.try_get("a_bot").map_err(db)?,
                };
                Ok((req, brief))
            })
            .collect()
    }

    async fn decide_developer_request(
        &self,
        id: i64,
        reviewer: i64,
        approve: bool,
        note: Option<&str>,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let sql = format!(
            "UPDATE developer_requests SET state = $2::developer_request_state, reviewer_id = $3, \
             reviewer_note = $4, reviewed_at = {TS}, updated_at = now() \
             WHERE id = $5 AND state = 'pending' RETURNING {}",
            request_cols()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(if approve { "approved" } else { "rejected" })
            .bind(reviewer)
            .bind(note)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let Some(row) = row else {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM developer_requests WHERE id = $1)",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            return Err(if exists {
                AppsError::Conflict("Already decided.".into())
            } else {
                AppsError::NotFound("request")
            });
        };
        let req = request_from(&row)?;
        if approve {
            sqlx::query("UPDATE accounts SET flags = flags | $2, updated_at = now() WHERE id = $1")
                .bind(req.account_id)
                .bind(FLAG_APP_DEVELOPER)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(req)
    }

    async fn create_application(&self, new: NewApplication) -> AppsResult<Application> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let owner_ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE id = $1 AND state = 'active' AND NOT is_bot)",
        )
        .bind(new.owner_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !owner_ok {
            return Err(AppsError::NotFound("account"));
        }
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(new.bot_person_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        // A well-formed Argon2id hash whose digest is 32 zero bytes: verification does the
        // normal work (same timing as any account) and no password can ever match it.
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, display_name, email, password_hash, state, is_bot) \
             VALUES ($1, $2, $3, $4, $5, '$argon2id$v=19$m=65536,t=3,p=4$c2lkZXJvb21ib3RzYQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', 'active', TRUE)",
        )
        .bind(new.bot_user_id)
        .bind(new.bot_person_id)
        .bind(&new.bot_username)
        .bind(&new.name)
        .bind(format!("bot-{}@bots.invalid", new.bot_user_id))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let sql = format!(
            "INSERT INTO applications (id, owner_id, bot_user_id, name, description, client_secret_hash, \
             client_secret_hint, created_at, updated_at) VALUES ($2, $3, $4, $5, $6, $7, $8, {TS}, {TS})"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(new.now_ms)
            .bind(new.id)
            .bind(new.owner_id)
            .bind(new.bot_user_id)
            .bind(&new.name)
            .bind(&new.description)
            .bind(&new.secret_hash.as_bytes()[..])
            .bind(&new.secret_hint)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let sql = format!(
            "INSERT INTO bot_tokens (id, application_id, user_id, token_hash, created_at) \
             VALUES ($2, $3, $4, $5, {TS})"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(new.now_ms)
            .bind(new.token_id)
            .bind(new.id)
            .bind(new.bot_user_id)
            .bind(&new.token_hash.as_bytes()[..])
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit_in(
            &mut tx,
            &AuditRow {
                id: new.audit_id,
                application_id: new.id,
                actor_id: new.owner_id,
                action: "app.create".into(),
                details: json!({"name": new.name}),
                created_at_ms: new.now_ms,
            },
        )
        .await?;
        let app = fetch_app(&mut tx, new.id).await?;
        tx.commit().await.map_err(db)?;
        Ok(app)
    }

    async fn application(&self, id: i64) -> AppsResult<Option<Application>> {
        let sql = format!(
            "SELECT {} FROM applications a WHERE a.id = $1",
            app_cols("a")
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(app_from)
            .transpose()
    }

    async fn applications_of(&self, owner: i64) -> AppsResult<Vec<Application>> {
        let sql = format!(
            "SELECT {} FROM applications a WHERE a.owner_id = $1 ORDER BY a.id",
            app_cols("a")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(owner)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(app_from).collect()
    }

    async fn update_application(
        &self,
        id: i64,
        p: &AppPatch,
        audit: AuditRow,
    ) -> AppsResult<Application> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let current = fetch_app(&mut tx, id).await?;
        let mut qb = QueryBuilder::<Postgres>::new("UPDATE applications SET updated_at = now()");
        macro_rules! set {
            ($opt:expr, $col:literal) => {
                if let Some(v) = $opt {
                    qb.push(concat!(", ", $col, " = ")).push_bind(v.clone());
                }
            };
        }
        set!(&p.name, "name");
        set!(&p.description, "description");
        set!(&p.icon_key, "icon_key");
        set!(&p.tags, "tags");
        set!(&p.bot_public, "bot_public");
        set!(&p.bot_require_code_grant, "bot_require_code_grant");
        set!(&p.redirect_uris, "redirect_uris");
        set!(&p.terms_url, "terms_url");
        set!(&p.privacy_url, "privacy_url");
        if let Some(v) = p.intents_requested {
            qb.push(", intents_requested = ").push_bind(v as i64);
        }
        if p.install_scopes.is_some() || p.install_permissions.is_some() {
            let scopes = p
                .install_scopes
                .clone()
                .unwrap_or_else(|| current.install_scopes.clone());
            let perms = p.install_permissions.unwrap_or(current.install_permissions);
            qb.push(", install_params = ")
                .push_bind(json!({"scopes": scopes, "permissions": perms.to_string()}));
        }
        qb.push(" WHERE id = ").push_bind(id);
        qb.build().execute(&mut *tx).await.map_err(db)?;
        if let Some(name) = &p.name {
            sqlx::query("UPDATE accounts SET display_name = $2, updated_at = now() WHERE id = $1")
                .bind(current.bot_user_id)
                .bind(name)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        if let Some(icon) = &p.icon_key {
            // The bot account wears the application icon (served from `avatars/<bot id>/`).
            sqlx::query("UPDATE accounts SET avatar_key = $2, updated_at = now() WHERE id = $1")
                .bind(current.bot_user_id)
                .bind(icon)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        audit_in(&mut tx, &audit).await?;
        let app = fetch_app(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(app)
    }

    async fn set_bot_name(&self, app: i64, name: &str, audit: AuditRow) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n = sqlx::query(
            "UPDATE accounts SET display_name = $2, updated_at = now() \
             WHERE id = (SELECT bot_user_id FROM applications WHERE id = $1)",
        )
        .bind(app)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if n == 0 {
            return Err(AppsError::NotFound("application"));
        }
        audit_in(&mut tx, &audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn rotate_secret(
        &self,
        app: i64,
        hash: &TokenHash,
        hint: &str,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let sql = format!(
            "UPDATE applications SET client_secret_hash = $2, client_secret_hint = $3, \
             secret_rotated_at = {TS}, updated_at = now() WHERE id = $4"
        );
        let n = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(&hash.as_bytes()[..])
            .bind(hint)
            .bind(app)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        if n == 0 {
            return Err(AppsError::NotFound("application"));
        }
        audit_in(&mut tx, &audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn reset_bot_token(
        &self,
        app: i64,
        token_id: i64,
        hash: &TokenHash,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let bot: i64 =
            sqlx::query_scalar("SELECT bot_user_id FROM applications WHERE id = $1 FOR UPDATE")
                .bind(app)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or(AppsError::NotFound("application"))?;
        sqlx::query("UPDATE bot_tokens SET revoked_at = now() WHERE application_id = $1 AND revoked_at IS NULL")
            .bind(app)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let sql = format!(
            "INSERT INTO bot_tokens (id, application_id, user_id, token_hash, created_at) \
             VALUES ($2, $3, $4, $5, {TS})"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(token_id)
            .bind(app)
            .bind(bot)
            .bind(&hash.as_bytes()[..])
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit_in(&mut tx, &audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn delete_application(&self, app: i64, audit: AuditRow) -> AppsResult<Vec<i64>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let bot: i64 =
            sqlx::query_scalar("SELECT bot_user_id FROM applications WHERE id = $1 FOR UPDATE")
                .bind(app)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or(AppsError::NotFound("application"))?;
        let guilds: Vec<i64> =
            sqlx::query_scalar("SELECT guild_id FROM guild_members WHERE user_id = $1")
                .bind(bot)
                .fetch_all(&mut *tx)
                .await
                .map_err(db)?;
        // Managed roles go first (member_roles cascade), then the memberships.
        sqlx::query(
            "DELETE FROM roles WHERE id IN \
             (SELECT role_id FROM application_installs WHERE application_id = $1 AND role_id IS NOT NULL)",
        )
        .bind(app)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM guild_members WHERE user_id = $1")
            .bind(bot)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE accounts SET state = 'deleted', state_changed_at = now(), updated_at = now() WHERE id = $1")
            .bind(bot)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit_in(&mut tx, &audit).await?;
        sqlx::query("DELETE FROM applications WHERE id = $1")
            .bind(app)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(guilds)
    }

    async fn bot_by_token_hash(&self, hash: &TokenHash) -> AppsResult<Option<BotRecord>> {
        let sql = format!(
            "SELECT t.id AS tok_id, t.user_id AS tok_user, t.token_hash, \
             (acc.state = 'active') AS acc_active, {} \
             FROM bot_tokens t JOIN applications a ON a.id = t.application_id \
             JOIN accounts acc ON acc.id = t.user_id \
             WHERE t.token_hash = $1 AND t.revoked_at IS NULL",
            app_cols("a")
        );
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hash.as_bytes()[..])
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
        else {
            return Ok(None);
        };
        let token_id: i64 = r.try_get("tok_id").map_err(db)?;
        // Keep `last_used_at` fresh without a write per request.
        let _ = sqlx::query(
            "UPDATE bot_tokens SET last_used_at = now() \
             WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')",
        )
        .bind(token_id)
        .execute(&self.pool)
        .await;
        Ok(Some(BotRecord {
            token_id,
            user_id: r.try_get("tok_user").map_err(db)?,
            application: app_from(&r)?,
            token_hash: hash_of(&r, "token_hash")?,
            account_active: r.try_get("acc_active").map_err(db)?,
        }))
    }

    async fn application_audit(
        &self,
        app: i64,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<AuditRow>> {
        let sql = format!(
            "SELECT id, application_id, actor_id, action, details, {} FROM application_audit \
             WHERE application_id = $1 AND ($2::bigint IS NULL OR id < $2) ORDER BY id DESC LIMIT $3",
            ms("created_at", "created_ms")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(AuditRow {
                    id: r.try_get("id").map_err(db)?,
                    application_id: r.try_get("application_id").map_err(db)?,
                    actor_id: r.try_get("actor_id").map_err(db)?,
                    action: r.try_get("action").map_err(db)?,
                    details: r.try_get("details").map_err(db)?,
                    created_at_ms: r.try_get("created_ms").map_err(db)?,
                })
            })
            .collect()
    }

    async fn record_audit(&self, row: AuditRow) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        audit_in(&mut tx, &row).await?;
        tx.commit().await.map_err(db)
    }

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
    ) -> AppsResult<AppVersion> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT 1 FROM applications WHERE id = $1 FOR UPDATE")
            .bind(app)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(AppsError::NotFound("application"))?;
        let next: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0)::int + 1 FROM application_versions WHERE application_id = $1",
        )
        .bind(app)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let sql = format!(
            "INSERT INTO application_versions (id, application_id, version, manifest, needs_review_reasons, \
             note, submitted_by, created_at) VALUES ($2, $3, $4, $5, $6, $7, $8, {TS}) RETURNING {}",
            version_cols("application_versions")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(id)
            .bind(app)
            .bind(next)
            .bind(&manifest)
            .bind(&reasons)
            .bind(note)
            .bind(by)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                if is_unique(&e) {
                    AppsError::Conflict("A version is already waiting for review.".into())
                } else {
                    db(e)
                }
            })?;
        sqlx::query(
            "UPDATE applications SET release_state = 'in_review', updated_at = now() \
             WHERE id = $1 AND release_state <> 'suspended'",
        )
        .bind(app)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        audit_in(&mut tx, &audit).await?;
        tx.commit().await.map_err(db)?;
        version_from(&row)
    }

    async fn versions(&self, app: i64) -> AppsResult<Vec<AppVersion>> {
        let sql = format!(
            "SELECT {} FROM application_versions v WHERE v.application_id = $1 ORDER BY v.id DESC",
            version_cols("v")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(version_from).collect()
    }

    async fn version(&self, id: i64) -> AppsResult<Option<(AppVersion, Application)>> {
        let sql = format!(
            "SELECT {}, {} FROM application_versions v JOIN applications a ON a.id = v.application_id \
             WHERE v.id = $1",
            version_cols("v"),
            app_cols("a")
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .map(|r| Ok((version_from(&r)?, app_from(&r)?)))
            .transpose()
    }

    async fn versions_by_state(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(AppVersion, Application)>> {
        let sql = format!(
            "SELECT {}, {} FROM application_versions v JOIN applications a ON a.id = v.application_id \
             WHERE v.state = $1::app_version_state AND ($2::bigint IS NULL OR v.id < $2) \
             ORDER BY v.id DESC LIMIT $3",
            version_cols("v"),
            app_cols("a")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(state)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| Ok((version_from(r)?, app_from(r)?)))
            .collect()
    }

    async fn last_approved_manifest(&self, app: i64) -> AppsResult<Option<Value>> {
        sqlx::query_scalar(
            "SELECT manifest FROM application_versions WHERE application_id = $1 AND state = 'approved' \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(app)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn decide_version(
        &self,
        id: i64,
        reviewer: i64,
        state: &str,
        note: Option<&str>,
        approved_intents: Option<u64>,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<(AppVersion, Application)> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let sql = format!(
            "UPDATE application_versions SET state = $2::app_version_state, reviewer_id = $3, \
             reviewer_note = $4, decided_at = {TS} WHERE id = $5 AND state = 'pending' \
             RETURNING application_id"
        );
        let app_id: Option<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(state)
            .bind(reviewer)
            .bind(note)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let Some(app_id) = app_id else {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM application_versions WHERE id = $1)",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            return Err(if exists {
                AppsError::Conflict("Already decided.".into())
            } else {
                AppsError::NotFound("version")
            });
        };
        sqlx::query(
            "UPDATE applications SET updated_at = now(), \
             release_state = CASE WHEN release_state = 'suspended' THEN release_state \
                  WHEN EXISTS (SELECT 1 FROM application_versions WHERE application_id = $1 AND state = 'approved') \
                  THEN 'released'::app_release_state ELSE 'development'::app_release_state END, \
             intents_approved = COALESCE($2, intents_approved) WHERE id = $1",
        )
        .bind(app_id)
        .bind(approved_intents.map(|b| b as i64))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        audit_in(&mut tx, &audit).await?;
        let sql = format!(
            "SELECT {}, {} FROM application_versions v JOIN applications a ON a.id = v.application_id WHERE v.id = $1",
            version_cols("v"),
            app_cols("a")
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let out = (version_from(&r)?, app_from(&r)?);
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn set_suspended(
        &self,
        app: i64,
        suspended: bool,
        reason: Option<&str>,
        audit: AuditRow,
    ) -> AppsResult<Application> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n = if suspended {
            sqlx::query(
                "UPDATE applications SET state_before_suspend = release_state, release_state = 'suspended', \
                 suspended_reason = $2, updated_at = now() WHERE id = $1 AND release_state <> 'suspended'",
            )
            .bind(app)
            .bind(reason)
            .execute(&mut *tx)
            .await
        } else {
            sqlx::query(
                "UPDATE applications SET release_state = COALESCE(state_before_suspend, 'development'), \
                 state_before_suspend = NULL, suspended_reason = NULL, updated_at = now() \
                 WHERE id = $1 AND release_state = 'suspended'",
            )
            .bind(app)
            .execute(&mut *tx)
            .await
        }
        .map_err(db)?
        .rows_affected();
        if n == 0 {
            return Err(AppsError::Conflict(
                "The application is not in the right state.".into(),
            ));
        }
        audit_in(&mut tx, &audit).await?;
        let out = fetch_app(&mut tx, app).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn guild_stats(&self, guild: i64) -> AppsResult<Option<GuildStats>> {
        let r = sqlx::query(
            "SELECT g.owner_id, (SELECT count(*) FROM guild_members m WHERE m.guild_id = g.id) AS members \
             FROM guilds g WHERE g.id = $1",
        )
        .bind(guild)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        r.map(|r| {
            Ok(GuildStats {
                owner_id: r.try_get("owner_id").map_err(db)?,
                member_count: r.try_get("members").map_err(db)?,
            })
        })
        .transpose()
    }

    async fn user_guilds(&self, user: i64) -> AppsResult<Vec<GuildBrief>> {
        let rows = sqlx::query(
            "SELECT g.id, g.name, g.icon_key, g.owner_id, \n             (SELECT count(*) FROM guild_members c WHERE c.guild_id = g.id) AS members \n             FROM guild_members m JOIN guilds g ON g.id = m.guild_id WHERE m.user_id = $1 \n             ORDER BY g.id LIMIT 200",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(GuildBrief {
                    id: r.try_get("id").map_err(db)?,
                    name: r.try_get("name").map_err(db)?,
                    icon_key: r.try_get("icon_key").map_err(db)?,
                    owner_id: r.try_get("owner_id").map_err(db)?,
                    member_count: r.try_get("members").map_err(db)?,
                })
            })
            .collect()
    }

    async fn install(&self, req: InstallWrite) -> AppsResult<InstallOutcome> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT 1 FROM guilds WHERE id = $1 FOR SHARE")
            .bind(req.guild_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(AppsError::NotFound("guild"))?;
        if req.with_bot {
            let banned: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM guild_bans WHERE guild_id = $1 AND user_id = $2)",
            )
            .bind(req.guild_id)
            .bind(req.bot_user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if banned {
                return Err(AppsError::Conflict(
                    "The bot is banned from this server.".into(),
                ));
            }
        }
        let prior_role: Option<i64> = sqlx::query_scalar(
            "SELECT role_id FROM application_installs WHERE application_id = $1 AND guild_id = $2 FOR UPDATE",
        )
        .bind(req.application_id)
        .bind(req.guild_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .flatten();
        let mut member_added = false;
        let mut role: Option<(i64, String, u64)> = None;
        let mut role_created = false;
        let mut role_id = prior_role;
        if req.with_bot {
            member_added = sqlx::query(
                "INSERT INTO guild_members (guild_id, user_id, joined_at) \
                 VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
                 ON CONFLICT (guild_id, user_id) DO NOTHING",
            )
            .bind(req.guild_id)
            .bind(req.bot_user_id)
            .bind(req.now_ms)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected()
                == 1;
            let existing = match prior_role {
                Some(r) => sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM roles WHERE id = $1 AND guild_id = $2",
                )
                .bind(r)
                .bind(req.guild_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?,
                None => None,
            };
            let rid = if let Some(r) = existing {
                sqlx::query("UPDATE roles SET permissions = $2, updated_at = now() WHERE id = $1")
                    .bind(r)
                    .bind(req.permissions as i64)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                r
            } else {
                sqlx::query(
                    "INSERT INTO roles (id, guild_id, name, position, permissions, managed, tags) \
                     VALUES ($1, $2, $3, 1, $4, TRUE, $5)",
                )
                .bind(req.role_id)
                .bind(req.guild_id)
                .bind(&req.bot_name)
                .bind(req.permissions as i64)
                .bind(json!({"bot_id": req.bot_user_id.to_string()}))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
                role_created = true;
                req.role_id
            };
            sqlx::query(
                "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(req.guild_id)
            .bind(req.bot_user_id)
            .bind(rid)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            role_id = Some(rid);
            role = Some((rid, req.bot_name.clone(), req.permissions));
        }
        let sql = format!(
            "INSERT INTO application_installs (application_id, guild_id, installed_by, scopes, permissions, role_id, created_at) \
             VALUES ($2, $3, $4, $5, $6, $7, {TS}) \
             ON CONFLICT (application_id, guild_id) DO UPDATE SET installed_by = EXCLUDED.installed_by, \
             scopes = EXCLUDED.scopes, permissions = EXCLUDED.permissions, \
             role_id = COALESCE(EXCLUDED.role_id, application_installs.role_id), updated_at = now() \
             RETURNING {}",
            ms("created_at", "created_ms")
        );
        let ir = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(req.now_ms)
            .bind(req.application_id)
            .bind(req.guild_id)
            .bind(req.installed_by)
            .bind(&req.scopes)
            .bind(req.permissions as i64)
            .bind(role_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let mut audit_entry = None;
        if member_added {
            sqlx::query(
                "INSERT INTO audit_log_entries (id, guild_id, actor_id, target_id, action, created_at) \
                 VALUES ($1, $2, $3, $4, 28, to_timestamp($5::double precision / 1000.0))",
            )
            .bind(req.guild_audit_id)
            .bind(req.guild_id)
            .bind(req.installed_by)
            .bind(req.bot_user_id)
            .bind(req.now_ms)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            audit_entry = Some(json!({
                "id": req.guild_audit_id.to_string(),
                "guild_id": req.guild_id.to_string(),
                "user_id": req.installed_by.to_string(),
                "target_id": req.bot_user_id.to_string(),
                "action_type": 28,
                "changes": [],
            }));
        }
        audit_in(
            &mut tx,
            &AuditRow {
                id: req.app_audit_id,
                application_id: req.application_id,
                actor_id: req.installed_by,
                action: "app.install".into(),
                details: json!({
                    "guild_id": req.guild_id.to_string(),
                    "scopes": req.scopes,
                    "permissions": req.permissions.to_string(),
                }),
                created_at_ms: req.now_ms,
            },
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(InstallOutcome {
            install: Install {
                application_id: req.application_id,
                guild_id: req.guild_id,
                installed_by: req.installed_by,
                scopes: req.scopes.clone(),
                permissions: req.permissions,
                role_id,
                created_at_ms: ir.try_get("created_ms").map_err(db)?,
            },
            member_added,
            role,
            role_created,
            audit_entry,
        })
    }

    async fn installs(&self, app: i64) -> AppsResult<Vec<Install>> {
        let sql = format!(
            "SELECT application_id, guild_id, installed_by, scopes, permissions, role_id, {} \
             FROM application_installs WHERE application_id = $1 ORDER BY guild_id",
            ms("created_at", "created_ms")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| {
                let p: i64 = r.try_get("permissions").map_err(db)?;
                Ok(Install {
                    application_id: r.try_get("application_id").map_err(db)?,
                    guild_id: r.try_get("guild_id").map_err(db)?,
                    installed_by: r.try_get("installed_by").map_err(db)?,
                    scopes: r.try_get("scopes").map_err(db)?,
                    permissions: p as u64,
                    role_id: r.try_get("role_id").map_err(db)?,
                    created_at_ms: r.try_get("created_ms").map_err(db)?,
                })
            })
            .collect()
    }

    async fn is_installed(&self, app: i64, guild: i64) -> AppsResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM application_installs i JOIN applications a ON a.id = i.application_id \
             WHERE i.application_id = $1 AND i.guild_id = $2 AND (NOT ('bot' = ANY (i.scopes)) OR EXISTS \
             (SELECT 1 FROM guild_members m WHERE m.guild_id = i.guild_id AND m.user_id = a.bot_user_id)))",
        )
        .bind(app)
        .bind(guild)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn commands(&self, app: i64, guild: Option<i64>) -> AppsResult<Vec<Command>> {
        let sql = format!(
            "SELECT {COMMAND_COLS} FROM application_commands WHERE application_id = $1 \
             AND guild_id IS NOT DISTINCT FROM $2 ORDER BY name, kind"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(command_from).collect()
    }

    async fn command(&self, app: i64, id: i64) -> AppsResult<Option<Command>> {
        let sql = format!(
            "SELECT {COMMAND_COLS} FROM application_commands WHERE application_id = $1 AND id = $2"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(command_from)
            .transpose()
    }

    async fn command_by_id(&self, id: i64) -> AppsResult<Option<Command>> {
        let sql = format!("SELECT {COMMAND_COLS} FROM application_commands WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(command_from)
            .transpose()
    }

    async fn upsert_command(
        &self,
        app: i64,
        guild: Option<i64>,
        new_id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<(Command, bool)> {
        let sql = format!(
            "INSERT INTO application_commands (application_id, guild_id, id, version, kind, name, description, \
             options, name_localizations, description_localizations, default_member_permissions, \
             dm_permission, nsfw, contexts, integration_types) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
             ON CONFLICT (application_id, COALESCE(guild_id, 0), kind, name) DO UPDATE SET \
             description = EXCLUDED.description, options = EXCLUDED.options, \
             name_localizations = EXCLUDED.name_localizations, \
             description_localizations = EXCLUDED.description_localizations, \
             default_member_permissions = EXCLUDED.default_member_permissions, \
             dm_permission = EXCLUDED.dm_permission, nsfw = EXCLUDED.nsfw, contexts = EXCLUDED.contexts, \
             integration_types = EXCLUDED.integration_types, version = EXCLUDED.version, updated_at = now() \
             RETURNING {COMMAND_COLS}, (xmax = 0) AS created"
        );
        let q = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(guild)
            .bind(new_id)
            .bind(version);
        let r = bind_command(q, c).fetch_one(&self.pool).await.map_err(db)?;
        Ok((command_from(&r)?, r.try_get("created").map_err(db)?))
    }

    async fn replace_command(
        &self,
        app: i64,
        id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<Command> {
        let sql = format!(
            "UPDATE application_commands SET version = $3, kind = $4, name = $5, description = $6, options = $7, \
             name_localizations = $8, description_localizations = $9, default_member_permissions = $10, \
             dm_permission = $11, nsfw = $12, contexts = $13, integration_types = $14, updated_at = now() \
             WHERE application_id = $1 AND id = $2 RETURNING {COMMAND_COLS}"
        );
        let q = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(id)
            .bind(version);
        let r = bind_command(q, c)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                if is_unique(&e) {
                    AppsError::field(
                        "name",
                        "APPLICATION_COMMAND_DUPLICATE_NAME",
                        "Application command names must be unique.",
                    )
                } else {
                    db(e)
                }
            })?
            .ok_or(AppsError::UnknownCommand)?;
        command_from(&r)
    }

    async fn delete_command(&self, app: i64, id: i64) -> AppsResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n =
            sqlx::query("DELETE FROM application_commands WHERE application_id = $1 AND id = $2")
                .bind(app)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db)?
                .rows_affected();
        sqlx::query("DELETE FROM application_command_permissions WHERE application_id = $1 AND command_id = $2")
            .bind(app)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(n > 0)
    }

    async fn bulk_overwrite(
        &self,
        app: i64,
        guild: Option<i64>,
        ids: Vec<i64>,
        version: i64,
        cmds: &[NewCommand],
    ) -> AppsResult<Vec<Command>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let sql = format!(
            "SELECT {COMMAND_COLS} FROM application_commands WHERE application_id = $1 \
             AND guild_id IS NOT DISTINCT FROM $2 FOR UPDATE"
        );
        let old: Vec<Command> = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(guild)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?
            .iter()
            .map(command_from)
            .collect::<AppsResult<_>>()?;
        let keep: Vec<(i16, &str)> = cmds.iter().map(|c| (c.kind, c.name.as_str())).collect();
        let gone: Vec<i64> = old
            .iter()
            .filter(|o| !keep.contains(&(o.kind, o.name.as_str())))
            .map(|o| o.id)
            .collect();
        if !gone.is_empty() {
            sqlx::query(
                "DELETE FROM application_commands WHERE application_id = $1 AND id = ANY($2)",
            )
            .bind(app)
            .bind(&gone)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            sqlx::query("DELETE FROM application_command_permissions WHERE application_id = $1 AND command_id = ANY($2)")
                .bind(app)
                .bind(&gone)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let mut out = Vec::with_capacity(cmds.len());
        for (i, c) in cmds.iter().enumerate() {
            let prior = old.iter().find(|o| o.kind == c.kind && o.name == c.name);
            let (id, ver) = match prior {
                Some(p) if sr_apps::memory::same_definition(p, c) => (p.id, p.version),
                Some(p) => (p.id, version),
                None => (ids[i], version),
            };
            let sql = format!(
                "INSERT INTO application_commands (application_id, guild_id, id, version, kind, name, description, \
                 options, name_localizations, description_localizations, default_member_permissions, \
                 dm_permission, nsfw, contexts, integration_types) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
                 ON CONFLICT (application_id, COALESCE(guild_id, 0), kind, name) DO UPDATE SET \
                 description = EXCLUDED.description, options = EXCLUDED.options, \
                 name_localizations = EXCLUDED.name_localizations, \
                 description_localizations = EXCLUDED.description_localizations, \
                 default_member_permissions = EXCLUDED.default_member_permissions, \
                 dm_permission = EXCLUDED.dm_permission, nsfw = EXCLUDED.nsfw, contexts = EXCLUDED.contexts, \
                 integration_types = EXCLUDED.integration_types, version = EXCLUDED.version, updated_at = now() \
                 RETURNING {COMMAND_COLS}"
            );
            let q = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(app)
                .bind(guild)
                .bind(id)
                .bind(ver);
            let r = bind_command(q, c).fetch_one(&mut *tx).await.map_err(db)?;
            out.push(command_from(&r)?);
        }
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
    ) -> AppsResult<Vec<CommandPermission>> {
        let rows = sqlx::query(
            "SELECT target_id, target_type, allowed FROM application_command_permissions \
             WHERE application_id = $1 AND guild_id = $2 AND command_id = $3 ORDER BY target_type, target_id",
        )
        .bind(app)
        .bind(guild)
        .bind(command_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(perm_from).collect()
    }

    async fn guild_command_permissions(
        &self,
        app: i64,
        guild: i64,
    ) -> AppsResult<Vec<(i64, Vec<CommandPermission>)>> {
        let rows = sqlx::query(
            "SELECT command_id, target_id, target_type, allowed FROM application_command_permissions \
             WHERE application_id = $1 AND guild_id = $2 ORDER BY command_id, target_type, target_id",
        )
        .bind(app)
        .bind(guild)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out: Vec<(i64, Vec<CommandPermission>)> = Vec::new();
        for r in &rows {
            let cid: i64 = r.try_get("command_id").map_err(db)?;
            let p = perm_from(r)?;
            match out.last_mut() {
                Some((last, v)) if *last == cid => v.push(p),
                _ => out.push((cid, vec![p])),
            }
        }
        Ok(out)
    }

    async fn set_command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
        perms: &[CommandPermission],
    ) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "DELETE FROM application_command_permissions WHERE application_id = $1 AND guild_id = $2 AND command_id = $3",
        )
        .bind(app)
        .bind(guild)
        .bind(command_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for p in perms {
            sqlx::query(
                "INSERT INTO application_command_permissions (application_id, guild_id, command_id, target_id, target_type, allowed) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(app)
            .bind(guild)
            .bind(command_id)
            .bind(p.target_id)
            .bind(p.target_type)
            .bind(p.allowed)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn upsert_grant(
        &self,
        new_id: i64,
        app: i64,
        user: i64,
        scopes: &[String],
        now_ms: i64,
    ) -> AppsResult<Grant> {
        let sql = format!(
            "INSERT INTO oauth2_grants (id, application_id, user_id, scopes, created_at) \
             VALUES ($2, $3, $4, $5, {TS}) \
             ON CONFLICT (application_id, user_id) WHERE revoked_at IS NULL DO UPDATE SET \
             scopes = (SELECT array_agg(DISTINCT s ORDER BY s) FROM unnest(oauth2_grants.scopes || EXCLUDED.scopes) AS s), \
             updated_at = now() \
             RETURNING {}",
            grant_cols("oauth2_grants")
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(new_id)
            .bind(app)
            .bind(user)
            .bind(scopes)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        grant_from(&r)
    }

    async fn store_code(
        &self,
        hash: &TokenHash,
        grant_id: i64,
        redirect_uri: &str,
        scopes: &[String],
        challenge: Option<&str>,
        expires_ms: i64,
    ) -> AppsResult<()> {
        sqlx::query(
            "INSERT INTO oauth2_codes (code_hash, grant_id, redirect_uri, scopes, code_challenge, expires_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
        )
        .bind(&hash.as_bytes()[..])
        .bind(grant_id)
        .bind(redirect_uri)
        .bind(scopes)
        .bind(challenge)
        .bind(expires_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn consume_code(&self, hash: &TokenHash) -> AppsResult<CodeConsume> {
        let sql = format!(
            "UPDATE oauth2_codes c SET used_at = now() FROM oauth2_grants g \
             WHERE c.code_hash = $1 AND c.used_at IS NULL AND g.id = c.grant_id AND g.revoked_at IS NULL \
             RETURNING c.redirect_uri, c.scopes AS code_scopes, c.code_challenge, {}, {}",
            ms("c.expires_at", "code_expires_ms"),
            grant_cols("g")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hash.as_bytes()[..])
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        if let Some(r) = row {
            return Ok(CodeConsume::Fresh(CodeRecord {
                code_hash: *hash,
                grant: grant_from(&r)?,
                redirect_uri: r.try_get("redirect_uri").map_err(db)?,
                scopes: r.try_get("code_scopes").map_err(db)?,
                code_challenge: r.try_get("code_challenge").map_err(db)?,
                expires_at_ms: r.try_get("code_expires_ms").map_err(db)?,
                used: true,
            }));
        }
        let grant: Option<i64> =
            sqlx::query_scalar("SELECT grant_id FROM oauth2_codes WHERE code_hash = $1")
                .bind(&hash.as_bytes()[..])
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        Ok(grant.map_or(CodeConsume::Unknown, CodeConsume::Replayed))
    }

    async fn insert_token(
        &self,
        rec: &TokenRecord,
        code: Option<&TokenHash>,
        now_ms: i64,
    ) -> AppsResult<()> {
        sqlx::query(
            "INSERT INTO oauth2_tokens (id, grant_id, code_hash, access_hash, refresh_hash, scopes, \
             access_expires_at, refresh_expires_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, \
             to_timestamp($7::double precision / 1000.0), to_timestamp($8::double precision / 1000.0), \
             to_timestamp($9::double precision / 1000.0))",
        )
        .bind(rec.id)
        .bind(rec.grant_id)
        .bind(code.map(|c| c.as_bytes().to_vec()))
        .bind(&rec.access_hash.as_bytes()[..])
        .bind(&rec.refresh_hash.as_bytes()[..])
        .bind(&rec.scopes)
        .bind(rec.access_expires_at_ms)
        .bind(rec.refresh_expires_at_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn token_by_access(&self, hash: &TokenHash) -> AppsResult<Option<TokenRecord>> {
        let sql = format!(
            "SELECT {TOKEN_COLS} FROM oauth2_tokens t JOIN oauth2_grants g ON g.id = t.grant_id \
             WHERE t.access_hash = $1 AND t.revoked_at IS NULL AND g.revoked_at IS NULL"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hash.as_bytes()[..])
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(token_from)
            .transpose()
    }

    async fn token_by_refresh(&self, hash: &TokenHash) -> AppsResult<RefreshLookup> {
        let sql = format!(
            "SELECT {TOKEN_COLS} FROM oauth2_tokens t JOIN oauth2_grants g ON g.id = t.grant_id \
             WHERE t.refresh_hash = $1"
        );
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hash.as_bytes()[..])
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
        else {
            return Ok(RefreshLookup::Unknown);
        };
        let rec = token_from(&r)?;
        Ok(if rec.refresh_used || rec.revoked {
            RefreshLookup::Spent(rec)
        } else {
            RefreshLookup::Live(rec)
        })
    }

    async fn spend_refresh(&self, token_id: i64) -> AppsResult<bool> {
        Ok(sqlx::query(
            "UPDATE oauth2_tokens SET refresh_used_at = now() WHERE id = $1 AND refresh_used_at IS NULL",
        )
        .bind(token_id)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected()
            == 1)
    }

    async fn revoke_token(&self, token_id: i64) -> AppsResult<()> {
        sqlx::query(
            "UPDATE oauth2_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(token_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn revoke_grant(&self, grant_id: i64) -> AppsResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("UPDATE oauth2_grants SET revoked_at = now(), updated_at = now() WHERE id = $1 AND revoked_at IS NULL")
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE oauth2_tokens SET revoked_at = now() WHERE grant_id = $1 AND revoked_at IS NULL")
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn revoke_code_tokens(&self, grant_id: i64, code: &TokenHash) -> AppsResult<()> {
        sqlx::query(
            "UPDATE oauth2_tokens SET revoked_at = now() WHERE grant_id = $1 AND code_hash = $2 AND revoked_at IS NULL",
        )
        .bind(grant_id)
        .bind(&code.as_bytes()[..])
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn grants_of(&self, user: i64) -> AppsResult<Vec<(Grant, Application)>> {
        let sql = format!(
            "SELECT {}, {} FROM oauth2_grants g JOIN applications a ON a.id = g.application_id \
             WHERE g.user_id = $1 AND g.revoked_at IS NULL ORDER BY g.id",
            grant_cols("g"),
            app_cols("a")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| Ok((grant_from(r)?, app_from(r)?)))
            .collect()
    }

    async fn grant_of(&self, user: i64, grant_id: i64) -> AppsResult<Option<Grant>> {
        let sql = format!(
            "SELECT {} FROM oauth2_grants g WHERE g.user_id = $1 AND g.id = $2 AND g.revoked_at IS NULL",
            grant_cols("g")
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .bind(grant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(grant_from)
            .transpose()
    }

    async fn insert_interaction(&self, row: &InteractionRow) -> AppsResult<()> {
        sqlx::query(
            "INSERT INTO interactions (id, application_id, token_hash, kind, user_id, guild_id, channel_id, \
             command_id, source_message_id, nonce, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, \
             $8, $9, $10, to_timestamp($11::double precision / 1000.0), to_timestamp($12::double precision / 1000.0))",
        )
        .bind(row.id)
        .bind(row.application_id)
        .bind(&row.token_hash[..])
        .bind(row.kind)
        .bind(row.user_id)
        .bind(row.guild_id)
        .bind(row.channel_id)
        .bind(row.command_id)
        .bind(row.source_message_id)
        .bind(&row.nonce)
        .bind(row.created_at_ms)
        .bind(row.expires_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn interaction(&self, id: i64) -> AppsResult<Option<InteractionRow>> {
        let sql = format!("SELECT {INTERACTION_COLS} FROM interactions WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(interaction_from)
            .transpose()
    }

    async fn interaction_by_hash(&self, hash: &TokenHash) -> AppsResult<Option<InteractionRow>> {
        let sql = format!("SELECT {INTERACTION_COLS} FROM interactions WHERE token_hash = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hash.as_bytes()[..])
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(interaction_from)
            .transpose()
    }

    async fn ack_interaction(
        &self,
        id: i64,
        state: &str,
        response_type: i16,
        ephemeral: bool,
        now_ms: i64,
    ) -> AppsResult<bool> {
        Ok(sqlx::query(
            "UPDATE interactions SET state = $2, response_type = $3, ephemeral = $4, \
             responded_at = to_timestamp($5::double precision / 1000.0) WHERE id = $1 AND state = 'created'",
        )
        .bind(id)
        .bind(state)
        .bind(response_type)
        .bind(ephemeral)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected()
            == 1)
    }

    async fn set_original_message(&self, id: i64, message_id: i64) -> AppsResult<()> {
        sqlx::query("UPDATE interactions SET original_message_id = $2 WHERE id = $1")
            .bind(id)
            .bind(message_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn add_interaction_message(&self, m: &InteractionMessage) -> AppsResult<()> {
        sqlx::query(
            "INSERT INTO interaction_messages (message_id, interaction_id, ephemeral, is_original, payload) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (message_id) DO NOTHING",
        )
        .bind(m.message_id)
        .bind(m.interaction_id)
        .bind(m.ephemeral)
        .bind(m.is_original)
        .bind(&m.payload)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn interaction_message(&self, message_id: i64) -> AppsResult<Option<InteractionMessage>> {
        let r = sqlx::query(
            "SELECT message_id, interaction_id, ephemeral, is_original, payload FROM interaction_messages \
             WHERE message_id = $1",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        r.map(|r| {
            Ok(InteractionMessage {
                message_id: r.try_get("message_id").map_err(db)?,
                interaction_id: r.try_get("interaction_id").map_err(db)?,
                ephemeral: r.try_get("ephemeral").map_err(db)?,
                is_original: r.try_get("is_original").map_err(db)?,
                payload: r.try_get("payload").map_err(db)?,
            })
        })
        .transpose()
    }

    async fn set_ephemeral_payload(&self, message_id: i64, payload: &Value) -> AppsResult<()> {
        sqlx::query(
            "UPDATE interaction_messages SET payload = $2 WHERE message_id = $1 AND ephemeral",
        )
        .bind(message_id)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_interaction_message(&self, message_id: i64) -> AppsResult<()> {
        sqlx::query("DELETE FROM interaction_messages WHERE message_id = $1")
            .bind(message_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn purge_interactions(&self, before_ms: i64) -> AppsResult<u64> {
        Ok(sqlx::query("DELETE FROM interactions WHERE expires_at < to_timestamp($1::double precision / 1000.0)")
            .bind(before_ms)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected())
    }
}

fn perm_from(r: &PgRow) -> AppsResult<CommandPermission> {
    Ok(CommandPermission {
        target_id: r.try_get("target_id").map_err(db)?,
        target_type: r.try_get("target_type").map_err(db)?,
        allowed: r.try_get("allowed").map_err(db)?,
    })
}
