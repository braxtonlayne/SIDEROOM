//! Postgres access: rows and the SQL helpers the service composes inside its transactions. Every helper
//! takes a `&mut PgConnection` (a pool connection or a transaction).

use sqlx::{PgConnection, Row, postgres::PgRow};

use crate::error::PartyResult;
use crate::model::{
    Binding, ControlMode, MediaView, ParticipantView, PartyView, PlaybackView, QueueItemView,
    SettingsView, YouView,
};
use crate::state::Playback;

/// One `watch_parties` row.
#[derive(Debug, Clone)]
pub struct PartyRow {
    pub id: i64,
    pub host_id: i64,
    pub binding: Binding,
    pub guild_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub invite_code: Option<String>,
    pub title_id: i64,
    pub episode_id: i64,
    pub variant_id: Option<i64>,
    pub asset_id: i64,
    pub title_name: String,
    pub episode_label: Option<String>,
    pub duration_ms: Option<i64>,
    pub age_restricted: bool,
    pub live: bool,
    pub playing: bool,
    pub position_ms: i64,
    pub rate_milli: i32,
    pub at_ms: i64,
    pub version: i64,
    pub auto_paused: bool,
    pub buffer_wait_until_ms: Option<i64>,
    pub shared_control: ControlMode,
    pub queue_policy: ControlMode,
    pub auto_pause_buffering: bool,
    pub max_wait_ms: i32,
    pub persistent: bool,
    pub max_participants: i32,
    pub created_at_ms: i64,
    pub empty_since_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

fn bad(what: &str) -> sqlx::Error {
    sqlx::Error::Decode(format!("unexpected {what} in watch_parties").into())
}

impl PartyRow {
    pub fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        let binding: String = r.try_get("binding")?;
        let shared: String = r.try_get("shared_control")?;
        let queue: String = r.try_get("queue_policy")?;
        Ok(Self {
            id: r.try_get("id")?,
            host_id: r.try_get("host_id")?,
            binding: Binding::parse(&binding).ok_or_else(|| bad("binding"))?,
            guild_id: r.try_get("guild_id")?,
            channel_id: r.try_get("channel_id")?,
            invite_code: r.try_get("invite_code")?,
            // An empty room (D-227) stores NULL; in memory it is 0, which no real title or channel has.
            title_id: r.try_get::<Option<i64>, _>("title_id")?.unwrap_or(0),
            episode_id: r.try_get("episode_id")?,
            variant_id: r.try_get("variant_id")?,
            asset_id: r.try_get::<Option<i64>, _>("asset_id")?.unwrap_or(0),
            title_name: r.try_get("title_name")?,
            episode_label: r.try_get("episode_label")?,
            duration_ms: r.try_get("duration_ms")?,
            age_restricted: r.try_get("age_restricted")?,
            live: r.try_get("live")?,
            playing: r.try_get("playing")?,
            position_ms: r.try_get("position_ms")?,
            rate_milli: r.try_get("rate_milli")?,
            at_ms: r.try_get("at_ms")?,
            version: r.try_get("version")?,
            auto_paused: r.try_get("auto_paused")?,
            buffer_wait_until_ms: r.try_get("buffer_wait_until_ms")?,
            shared_control: ControlMode::parse(&shared).ok_or_else(|| bad("shared_control"))?,
            queue_policy: ControlMode::parse(&queue).ok_or_else(|| bad("queue_policy"))?,
            auto_pause_buffering: r.try_get("auto_pause_buffering")?,
            max_wait_ms: r.try_get("max_wait_ms")?,
            persistent: r.try_get("persistent")?,
            max_participants: r.try_get("max_participants")?,
            created_at_ms: r.try_get("created_at_ms")?,
            empty_since_ms: r.try_get("empty_since_ms")?,
            ended_at_ms: r.try_get("ended_at_ms")?,
        })
    }

    pub fn playback(&self) -> Playback {
        Playback {
            playing: self.playing,
            position_ms: self.position_ms,
            rate_milli: self.rate_milli,
            at_ms: self.at_ms,
            version: self.version,
            auto_paused: self.auto_paused,
            duration_ms: self.duration_ms,
            live: self.live,
        }
    }

    /// A party without media: an empty watch party room waiting for its host to pick something.
    pub fn is_idle(&self) -> bool {
        self.title_id == 0
    }

    /// What is playing; `None` for an idle room.
    pub fn media(&self) -> Option<MediaView> {
        if self.is_idle() {
            return None;
        }
        Some(MediaView {
            title_id: self.title_id,
            episode_id: self.episode_id,
            variant_id: self.variant_id,
            asset_id: self.asset_id,
            title: self.title_name.clone(),
            episode_label: self.episode_label.clone(),
            duration_ms: self.duration_ms,
            age_restricted: self.age_restricted,
            live: self.live,
        })
    }

    pub fn settings(&self) -> SettingsView {
        SettingsView {
            shared_control: self.shared_control,
            queue_policy: self.queue_policy,
            auto_pause_buffering: self.auto_pause_buffering,
            max_wait_ms: self.max_wait_ms,
            persistent: self.persistent,
            max_participants: self.max_participants,
        }
    }
}

/// One `watch_party_members` row.
#[derive(Debug, Clone)]
pub struct MemberRow {
    pub user_id: i64,
    pub joined_at_ms: i64,
    pub is_controller: bool,
    pub ready: bool,
    pub buffering_since_ms: Option<i64>,
    pub last_seen_ms: i64,
    pub rtt_ms: Option<i32>,
}

impl MemberRow {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            user_id: r.try_get("user_id")?,
            joined_at_ms: r.try_get("joined_at_ms")?,
            is_controller: r.try_get("is_controller")?,
            ready: r.try_get("ready")?,
            buffering_since_ms: r.try_get("buffering_since_ms")?,
            last_seen_ms: r.try_get("last_seen_ms")?,
            rtt_ms: r.try_get("rtt_ms")?,
        })
    }
}

pub async fn get_party(c: &mut PgConnection, id: i64, lock: bool) -> PartyResult<Option<PartyRow>> {
    let sql = if lock {
        "SELECT * FROM watch_parties WHERE id = $1 FOR UPDATE"
    } else {
        "SELECT * FROM watch_parties WHERE id = $1"
    };
    let row = sqlx::query(sql).bind(id).fetch_optional(c).await?;
    Ok(row.as_ref().map(PartyRow::from_row).transpose()?)
}

pub async fn members(c: &mut PgConnection, party: i64) -> PartyResult<Vec<MemberRow>> {
    let rows = sqlx::query(
        "SELECT * FROM watch_party_members WHERE party_id = $1 ORDER BY joined_at_ms, user_id",
    )
    .bind(party)
    .fetch_all(c)
    .await?;
    Ok(rows
        .iter()
        .map(MemberRow::from_row)
        .collect::<Result<_, _>>()?)
}

pub async fn queue(c: &mut PgConnection, party: i64) -> PartyResult<Vec<QueueItemView>> {
    let rows = sqlx::query(
        "SELECT id, position, title_id, episode_id, variant_id, asset_id, title_name, episode_label, \
         duration_ms, age_restricted, added_by, added_at_ms \
         FROM watch_party_queue WHERE party_id = $1 ORDER BY position, id",
    )
    .bind(party)
    .fetch_all(c)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(QueueItemView {
                id: r.try_get("id")?,
                position: r.try_get("position")?,
                media: MediaView {
                    title_id: r.try_get("title_id")?,
                    episode_id: r.try_get("episode_id")?,
                    variant_id: r.try_get("variant_id")?,
                    asset_id: r.try_get("asset_id")?,
                    title: r.try_get("title_name")?,
                    episode_label: r.try_get("episode_label")?,
                    duration_ms: r.try_get("duration_ms")?,
                    age_restricted: r.try_get("age_restricted")?,
                    live: false,
                },
                added_by: r.try_get("added_by")?,
                added_at_ms: r.try_get("added_at_ms")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(Into::into)
}

/// Usernames of `ids`: `(id, username, display_name)`.
pub async fn names(
    c: &mut PgConnection,
    ids: &[i64],
) -> PartyResult<Vec<(i64, String, Option<String>)>> {
    let rows = sqlx::query(
        "SELECT id, username::text AS username, display_name FROM accounts WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(c)
    .await?;
    rows.iter()
        .map(|r| {
            Ok((
                r.try_get("id")?,
                r.try_get("username")?,
                r.try_get("display_name")?,
            ))
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(Into::into)
}

/// Build the wire object. `now_ms` stamps the playback; `you` is filled by the caller for a viewer.
pub async fn build_view(
    c: &mut PgConnection,
    row: &PartyRow,
    now_ms: i64,
) -> PartyResult<PartyView> {
    let members = members(c, row.id).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let names = names(c, &ids).await?;
    let participants = members
        .iter()
        .map(|m| {
            let (username, display_name) = names
                .iter()
                .find(|(id, _, _)| *id == m.user_id)
                .map(|(_, u, d)| (u.clone(), d.clone()))
                .unwrap_or_default();
            ParticipantView {
                user_id: m.user_id,
                username,
                display_name,
                joined_at_ms: m.joined_at_ms,
                is_host: m.user_id == row.host_id,
                is_controller: m.is_controller,
                ready: m.ready,
                buffering: m.buffering_since_ms.is_some(),
                rtt_ms: m.rtt_ms,
            }
        })
        .collect();
    let queue = queue(c, row.id).await?;
    Ok(PartyView {
        id: row.id,
        host_id: row.host_id,
        binding: row.binding,
        guild_id: row.guild_id,
        channel_id: row.channel_id,
        invite_code: None,
        media: row.media(),
        playback: PlaybackView::from_state(&row.playback(), now_ms),
        settings: row.settings(),
        participants,
        queue,
        created_at_ms: row.created_at_ms,
        ended: row.ended_at_ms.is_some(),
        you: None::<YouView>,
    })
}

pub struct AuditNew<'a> {
    pub id: i64,
    pub party: i64,
    pub actor: Option<i64>,
    pub action: &'a str,
    pub target: Option<i64>,
    pub detail: serde_json::Value,
    pub at_ms: i64,
}

pub async fn audit(c: &mut PgConnection, a: AuditNew<'_>) -> PartyResult<()> {
    sqlx::query(
        "INSERT INTO watch_party_audit (id, party_id, actor_id, action, target_id, detail, at_ms) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(a.id)
    .bind(a.party)
    .bind(a.actor)
    .bind(a.action)
    .bind(a.target)
    .bind(a.detail)
    .bind(a.at_ms)
    .execute(c)
    .await?;
    Ok(())
}

/// Persist the playback columns of `row` (state, version, auto-pause flag and deadline).
pub async fn save_playback(
    c: &mut PgConnection,
    party: i64,
    p: &Playback,
    wait_until_ms: Option<i64>,
    now_ms: i64,
) -> PartyResult<()> {
    sqlx::query(
        "UPDATE watch_parties SET playing = $2, position_ms = $3, rate_milli = $4, at_ms = $5, \
         version = $6, auto_paused = $7, buffer_wait_until_ms = $8, updated_at_ms = $9 WHERE id = $1",
    )
    .bind(party)
    .bind(p.playing)
    .bind(p.position_ms)
    .bind(p.rate_milli)
    .bind(p.at_ms)
    .bind(p.version)
    .bind(p.auto_paused)
    .bind(wait_until_ms)
    .bind(now_ms)
    .execute(c)
    .await?;
    Ok(())
}
