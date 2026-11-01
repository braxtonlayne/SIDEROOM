//! The database side of guest links (ARC-GST-001): minting and revoking lobby tokens, guest sessions, and the checks
//! the guest socket makes. A child module of `service` so it shares the pool without widening its visibility.
//!
//! Every token (lobby or guest) is random, shown once and stored as a SHA-256 digest. A link works while it is not
//! revoked, not expired and its instance is running; a guest session works while its own expiry has not passed and
//! its link works.

use sqlx::Row;

use super::ActivityService;
use crate::error::{ActivityError, ActivityResult};
use crate::guest::{
    self, GUEST_TOKEN_PREFIX, GuestJoined, GuestLinkCreated, LINK_TTL_SECS, MAX_GUESTS_PER_LOBBY,
    MAX_LINKS_PER_INSTANCE, SESSION_TTL_SECS, Seating,
};

/// A timestamp column as RFC 3339 UTC text.
fn rfc3339(col: &str) -> String {
    format!("to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')")
}

/// A link that works now: not revoked, not expired, its instance running. `l` is the link, `i` the instance.
const LINK_LIVE: &str = "l.revoked_at IS NULL AND l.expires_at > now() AND i.ended_at IS NULL";

impl ActivityService {
    /// 64 random hex digits from Postgres' strong random source (two UUIDv4s).
    async fn random_hex(&self) -> ActivityResult<String> {
        Ok(sqlx::query_scalar(
            "SELECT replace(gen_random_uuid()::text || gen_random_uuid()::text, '-', '')",
        )
        .fetch_one(self.pool())
        .await?)
    }

    /// Mint a guest link for the running instance `instance`. Needs a current participant and a game whose
    /// reviewed bundle declares `guest_play` (a development build runs with every active flag). The token is
    /// returned once; only its digest is kept. At most [`MAX_LINKS_PER_INSTANCE`] links work at a time.
    pub async fn create_guest_link(
        &self,
        user: i64,
        instance: i64,
    ) -> ActivityResult<GuestLinkCreated> {
        let joined: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM activity_participants p \
             JOIN activity_instances i ON i.id = p.instance_id \
             WHERE p.instance_id = $1 AND p.user_id = $2 AND p.left_at IS NULL AND i.ended_at IS NULL)",
        )
        .bind(instance)
        .bind(user)
        .fetch_one(self.pool())
        .await?;
        if !joined {
            return Err(ActivityError::NotFound("activity"));
        }
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let view = self.view(&row).await?;
        if !view.capabilities.iter().any(|c| c == "guest_play") {
            return Err(ActivityError::Conflict(
                "This game does not allow guests.".to_owned(),
            ));
        }
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_guest_links l JOIN activity_instances i ON i.id = l.instance_id \
             WHERE l.instance_id = $1 AND l.revoked_at IS NULL AND l.expires_at > now() AND i.ended_at IS NULL",
        )
        .bind(instance)
        .fetch_one(self.pool())
        .await?;
        if live >= MAX_LINKS_PER_INSTANCE {
            return Err(ActivityError::Conflict(
                "This lobby already has the most guest links it can have. Revoke one first.".to_owned(),
            ));
        }
        let token = self.random_hex().await?;
        let sql = format!(
            "INSERT INTO activity_guest_links (id, instance_id, created_by, lobby_sha256, expires_at) \
             VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) RETURNING {} AS expires",
            rfc3339("expires_at")
        );
        let expires: String = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind((self.inner.ids)())
            .bind(instance)
            .bind(user)
            .bind(guest::digest(&token))
            .bind(LINK_TTL_SECS as f64)
            .fetch_one(self.pool())
            .await?;
        Ok(GuestLinkCreated {
            lobby_token: token,
            expires_at: expires,
        })
    }

    /// Revoke a guest link. The instance's starter (the lobby host) and the member who minted the link may.
    /// Another participant gets `Forbidden`; anybody else, and an unknown link, `NotFound`. Revoking drops the
    /// guest sessions of the link; the guest socket disconnects them on its next check.
    pub async fn revoke_guest_link(
        &self,
        user: i64,
        instance: i64,
        lobby_token: &str,
    ) -> ActivityResult<()> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let starter: i64 = row.try_get("started_by")?;
        let participant: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM activity_participants \
             WHERE instance_id = $1 AND user_id = $2 AND left_at IS NULL)",
        )
        .bind(instance)
        .bind(user)
        .fetch_one(self.pool())
        .await?;
        if !participant {
            return Err(ActivityError::NotFound("activity"));
        }
        if !guest::is_lobby_token(lobby_token) {
            return Err(ActivityError::NotFound("guest link"));
        }
        let link = sqlx::query(
            "SELECT id, created_by FROM activity_guest_links \
             WHERE instance_id = $1 AND lobby_sha256 = $2",
        )
        .bind(instance)
        .bind(guest::digest(lobby_token))
        .fetch_optional(self.pool())
        .await?
        .ok_or(ActivityError::NotFound("guest link"))?;
        let link_id: i64 = link.try_get("id")?;
        let creator: i64 = link.try_get("created_by")?;
        if user != starter && user != creator {
            return Err(ActivityError::Forbidden);
        }
        sqlx::query(
            "UPDATE activity_guest_links SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(link_id)
        .execute(self.pool())
        .await?;
        sqlx::query("DELETE FROM activity_guest_sessions WHERE link_id = $1")
            .bind(link_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A person without an account joins through a lobby token: a guest session with a random callsign. Every
    /// way the link can be dead (unknown, malformed, expired, revoked, the lobby ended, the game no longer
    /// allows guests) answers the same `NotFound`, so tokens cannot be probed for their state.
    pub async fn guest_join(&self, lobby_token: &str) -> ActivityResult<GuestJoined> {
        let gone = || ActivityError::NotFound("guest link");
        if !guest::is_lobby_token(lobby_token) {
            return Err(gone());
        }
        let sql = format!(
            "SELECT l.id AS link_id, i.id AS instance_id FROM activity_guest_links l \
             JOIN activity_instances i ON i.id = l.instance_id \
             WHERE l.lobby_sha256 = $1 AND {LINK_LIVE}"
        );
        let link = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guest::digest(lobby_token))
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(gone)?;
        let link_id: i64 = link.try_get("link_id")?;
        let instance: i64 = link.try_get("instance_id")?;
        let row = self.running_by_id(instance).await?.ok_or_else(gone)?;
        let view = self.view(&row).await?;
        if !view.capabilities.iter().any(|c| c == "guest_play") {
            return Err(gone());
        }
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_guest_sessions WHERE link_id = $1 AND expires_at > now()",
        )
        .bind(link_id)
        .fetch_one(self.pool())
        .await?;
        if live >= MAX_GUESTS_PER_LOBBY {
            return Err(ActivityError::Conflict("This lobby is full.".to_owned()));
        }
        let hex = self.random_hex().await?;
        let callsign = {
            let b = hex.as_bytes();
            let nib = |i: usize| (b[i] as char).to_digit(16).unwrap_or(0) as u8;
            guest::callsign([
                nib(0) << 4 | nib(1),
                nib(2) << 4 | nib(3),
                nib(4) << 4 | nib(5),
                nib(6) << 4 | nib(7),
            ])
        };
        let token_hex = self.random_hex().await?;
        let token = format!("{GUEST_TOKEN_PREFIX}{token_hex}");
        let sql = format!(
            "INSERT INTO activity_guest_sessions (token_sha256, link_id, callsign, expires_at) \
             SELECT $1, l.id, $3, LEAST(now() + make_interval(secs => $4), l.expires_at) \
             FROM activity_guest_links l WHERE l.id = $2 RETURNING {} AS expires",
            rfc3339("expires_at")
        );
        let expires: String = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(guest::digest(&token))
            .bind(link_id)
            .bind(&callsign)
            .bind(SESSION_TTL_SECS as f64)
            .fetch_one(self.pool())
            .await?;
        Ok(GuestJoined {
            guest_token: token,
            callsign,
            expires_at: expires,
            topic: guest::topic_of(lobby_token),
            socket_path: guest::SOCKET_PATH.to_owned(),
            application_id: view.application_id,
            application_name: view.application_name,
        })
    }

    /// The seat a guest token holds, if the session and its link both work now.
    pub async fn guest_seating(&self, guest_token: &str) -> ActivityResult<Option<Seating>> {
        if !guest::is_guest_token(guest_token) {
            return Ok(None);
        }
        let sql = format!(
            "SELECT l.lobby_sha256, s.callsign FROM activity_guest_sessions s \
             JOIN activity_guest_links l ON l.id = s.link_id \
             JOIN activity_instances i ON i.id = l.instance_id \
             WHERE s.token_sha256 = $1 AND s.expires_at > now() AND {LINK_LIVE}"
        );
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guest::digest(guest_token))
            .fetch_optional(self.pool())
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(Seating {
            lobby: lobby_of(&r)?,
            member: false,
            callsign: r.try_get("callsign")?,
        }))
    }

    /// The seat a signed-in member holds in the lobby named by `lobby_token`: they must be a current participant
    /// of the instance the link belongs to, and the link must work now. The member's name never leaves: their
    /// seat is shown as `Host` or `Player`.
    pub async fn member_seating(
        &self,
        user: i64,
        lobby_token: &str,
    ) -> ActivityResult<Option<Seating>> {
        if !guest::is_lobby_token(lobby_token) {
            return Ok(None);
        }
        let sql = format!(
            "SELECT l.lobby_sha256, (i.started_by = $2) AS host FROM activity_guest_links l \
             JOIN activity_instances i ON i.id = l.instance_id \
             JOIN activity_participants p ON p.instance_id = i.id AND p.user_id = $2 AND p.left_at IS NULL \
             WHERE l.lobby_sha256 = $1 AND {LINK_LIVE}"
        );
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guest::digest(lobby_token))
            .bind(user)
            .fetch_optional(self.pool())
            .await?
        else {
            return Ok(None);
        };
        let host: bool = r.try_get("host")?;
        Ok(Some(Seating {
            lobby: lobby_of(&r)?,
            member: true,
            callsign: if host { "Host" } else { "Player" }.to_owned(),
        }))
    }
}

fn lobby_of(r: &sqlx::postgres::PgRow) -> ActivityResult<[u8; 32]> {
    let v: Vec<u8> = r.try_get("lobby_sha256")?;
    v.try_into()
        .map_err(|_| ActivityError::Storage("lobby digest has the wrong length".to_owned()))
}
