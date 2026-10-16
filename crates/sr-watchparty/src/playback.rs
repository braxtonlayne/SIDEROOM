//! Playback commands, media changes, the queue and the buffering rule (WP-BE-003, 005, 006, 007).

use serde_json::json;
use sqlx::PgConnection;

use crate::error::{PartyError, PartyResult};
use crate::model::{
    Action, ChangeMediaInput, CommandInput, PartyView, PlaybackView, QueueAddInput, QueueMoveInput,
    Viewer,
};
use crate::ports::Resolved;
use crate::rules::{self, BufferDecision, BufferReport};
use crate::service::{Notice, PartyService, clamp_start};
use crate::state::{Command, MAX_RATE_MILLI, MIN_RATE_MILLI, Playback};
use crate::store::{self, MemberRow, PartyRow};

impl PartyRow {
    pub(crate) fn set_playback(&mut self, p: &Playback, wait_until_ms: Option<i64>) {
        self.playing = p.playing;
        self.position_ms = p.position_ms;
        self.rate_milli = p.rate_milli;
        self.at_ms = p.at_ms;
        self.version = p.version;
        self.auto_paused = p.auto_paused;
        self.buffer_wait_until_ms = wait_until_ms;
    }
}

fn parse_command(input: &CommandInput) -> PartyResult<(Command, &'static str)> {
    Ok(match input.action {
        Action::Play => (Command::Play, "play"),
        Action::Pause => (Command::Pause, "pause"),
        Action::Ended => (Command::Ended, "ended"),
        Action::Seek => {
            let p = input
                .position_ms
                .ok_or_else(|| PartyError::Invalid("seek needs position_ms.".to_owned()))?;
            (Command::Seek(p), "seek")
        }
        Action::Rate => {
            let r = input
                .rate
                .filter(|r| r.is_finite())
                .ok_or_else(|| PartyError::Invalid("rate needs a number.".to_owned()))?;
            let milli = (r * 1000.0).round();
            if !(f64::from(MIN_RATE_MILLI)..=f64::from(MAX_RATE_MILLI)).contains(&milli) {
                return Err(PartyError::Invalid(
                    "Speed must be between 0.5x and 2x.".to_owned(),
                ));
            }
            // In range, so the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            (Command::Rate(milli as i32), "rate")
        }
    })
}

impl PartyService {
    /// Apply a playback command (WP-BE-003). Controllers per the party's shared-control setting; an `ended`
    /// report is accepted from any member and verified against the expected position.
    pub async fn command(
        &self,
        v: Viewer,
        id: i64,
        input: CommandInput,
    ) -> PartyResult<PlaybackView> {
        let (cmd, reason) = parse_command(&input)?;
        if cmd != Command::Ended {
            self.limit(v.user_id, "command", 5_000, 12)?;
        }
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        if cmd != Command::Ended && !rules::allowed(row.shared_control, standing) {
            return Err(PartyError::Forbidden);
        }
        if row.is_idle() {
            return Err(PartyError::Conflict(
                "Pick something to watch first.".to_owned(),
            ));
        }
        let now = self.now();
        let state = row.playback();
        state.check_version(input.expected_version, now)?;
        let applied = match state.apply(cmd, now) {
            Ok(a) => a,
            // A stale `ended` (someone already advanced) is not an error worth a 400.
            Err(crate::state::StateError::NotAtEnd) if cmd == Command::Ended => {
                crate::state::Applied {
                    state,
                    changed: false,
                }
            }
            Err(e) => return Err(e.into()),
        };
        if applied.changed {
            store::save_playback(&mut tx, id, &applied.state, None, now).await?;
            row.set_playback(&applied.state, None);
            notices.push(Notice::Playback {
                party: id,
                reason,
                by: Some(v.user_id),
            });
            if cmd == Command::Ended {
                self.advance_queue(&mut tx, &mut row, true, Some(v.user_id), &mut notices)
                    .await?;
            }
        }
        tx.commit().await?;
        self.flush(notices).await;
        Ok(PlaybackView::from_state(&row.playback(), self.now()))
    }

    /// Change what is playing (spec section 4, controllers). The title must be published and playable and
    /// the actor must pass the age gate; for a restricted title, members who have not confirmed 18+ are
    /// removed from the party (D-184).
    pub async fn change_media(
        &self,
        v: Viewer,
        id: i64,
        input: ChangeMediaInput,
    ) -> PartyResult<PartyView> {
        self.limit(v.user_id, "command", 5_000, 12)?;
        let media = self
            .resolve_media(
                v,
                input.title_id,
                input.tv_channel_id,
                input.episode_id,
                input.variant_id,
            )
            .await?;
        let start = clamp_start(input.start_ms, media.duration_ms, media.live)?;
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::allowed(row.shared_control, standing) {
            return Err(PartyError::Forbidden);
        }
        self.apply_media(
            &mut tx,
            &mut row,
            &media,
            start,
            false,
            Some(v.user_id),
            &mut notices,
        )
        .await?;
        tx.commit().await?;
        self.flush(notices).await;
        self.get(v, id).await
    }

    /// Replace the media of `row` and reset the playback (version continues). Restricted media removes the
    /// members who have not confirmed their age.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn apply_media(
        &self,
        c: &mut PgConnection,
        row: &mut PartyRow,
        media: &Resolved,
        start_ms: i64,
        autoplay: bool,
        actor: Option<i64>,
        notices: &mut Vec<Notice>,
    ) -> PartyResult<()> {
        let now = self.now();
        let mut state = Playback::new(now, start_ms, media.duration_ms, media.live);
        state.version = row.version + 1;
        state.playing = media.live || autoplay;
        sqlx::query(
            "UPDATE watch_parties SET title_id = $2, episode_id = $3, variant_id = $4, asset_id = $5, \
             title_name = $6, episode_label = $7, duration_ms = $8, age_restricted = $9, live = $10 WHERE id = $1",
        )
        .bind(row.id)
        .bind(media.title_id)
        .bind(media.episode_id)
        .bind(media.variant_id)
        .bind(media.asset_id)
        .bind(&media.title)
        .bind(&media.episode_label)
        .bind(media.duration_ms)
        .bind(media.age_restricted)
        .bind(media.live)
        .execute(&mut *c)
        .await?;
        store::save_playback(c, row.id, &state, None, now).await?;
        sqlx::query(
            "UPDATE watch_party_members SET buffering_since_ms = NULL, ready = FALSE WHERE party_id = $1",
        )
        .bind(row.id)
        .execute(&mut *c)
        .await?;
        row.title_id = media.title_id;
        row.episode_id = media.episode_id;
        row.variant_id = media.variant_id;
        row.asset_id = media.asset_id;
        row.title_name = media.title.clone();
        row.episode_label = media.episode_label.clone();
        row.duration_ms = media.duration_ms;
        row.age_restricted = media.age_restricted;
        row.live = media.live;
        row.set_playback(&state, None);
        self.audit(
            c,
            row.id,
            actor,
            "media_changed",
            None,
            json!({"title_id": media.title_id.to_string(), "episode_id": media.episode_id.to_string()}),
        )
        .await?;
        if media.age_restricted {
            for m in store::members(c, row.id).await? {
                if !self.inner.age.confirmed(m.user_id).await {
                    self.remove_member(c, row, m.user_id, "participant_left", notices)
                        .await?;
                    notices.push(Notice::Removed {
                        user: m.user_id,
                        party: row.id,
                        reason: "age_restricted",
                    });
                }
            }
        }
        notices.push(Notice::Updated {
            party: row.id,
            kind: "media",
            card: true,
            also: vec![],
        });
        notices.push(Notice::Playback {
            party: row.id,
            reason: "media",
            by: actor,
        });
        Ok(())
    }

    // ---- queue (WP-BE-006) -------------------------------------------------------------------------

    /// Add a title or episode to the queue (per `queue_policy`).
    pub async fn queue_add(
        &self,
        v: Viewer,
        id: i64,
        input: QueueAddInput,
    ) -> PartyResult<PartyView> {
        self.limit(v.user_id, "queue", 60_000, 30)?;
        let media = self
            .resolve(v, input.title_id, input.episode_id, input.variant_id)
            .await?;
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::allowed(row.queue_policy, standing) {
            return Err(PartyError::Forbidden);
        }
        if media.live {
            return Err(PartyError::Invalid(
                "A live channel cannot be queued.".to_owned(),
            ));
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM watch_party_queue WHERE party_id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= rules::MAX_QUEUE {
            return Err(PartyError::Conflict("The queue is full.".to_owned()));
        }
        sqlx::query(
            "INSERT INTO watch_party_queue (id, party_id, position, title_id, episode_id, variant_id, asset_id, \
             title_name, episode_label, duration_ms, age_restricted, added_by, added_at_ms) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(self.next_id())
        .bind(id)
        .bind(i32::try_from(count).unwrap_or(i32::MAX))
        .bind(media.title_id)
        .bind(media.episode_id)
        .bind(media.variant_id)
        .bind(media.asset_id)
        .bind(&media.title)
        .bind(&media.episode_label)
        .bind(media.duration_ms)
        .bind(media.age_restricted)
        .bind(v.user_id)
        .bind(self.now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "queue",
            card: false,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    /// Remove a queue entry (per `queue_policy`; the person who added it may always remove it).
    pub async fn queue_remove(&self, v: Viewer, id: i64, item: i64) -> PartyResult<PartyView> {
        self.limit(v.user_id, "queue", 60_000, 30)?;
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        let owner: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT added_by FROM watch_party_queue WHERE id = $1 AND party_id = $2",
        )
        .bind(item)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let owner = owner.ok_or(PartyError::NotFound("queue item"))?;
        if !(rules::allowed(row.queue_policy, standing) || owner == Some(v.user_id)) {
            return Err(PartyError::Forbidden);
        }
        sqlx::query("DELETE FROM watch_party_queue WHERE id = $1")
            .bind(item)
            .execute(&mut *tx)
            .await?;
        renumber(&mut tx, id, None).await?;
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "queue",
            card: false,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    /// Reorder: move an entry to a new index (controllers).
    pub async fn queue_move(
        &self,
        v: Viewer,
        id: i64,
        item: i64,
        input: QueueMoveInput,
    ) -> PartyResult<PartyView> {
        self.limit(v.user_id, "queue", 60_000, 30)?;
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::allowed(row.shared_control, standing) {
            return Err(PartyError::Forbidden);
        }
        let to = usize::try_from(input.position)
            .map_err(|_| PartyError::Invalid("position must not be negative.".to_owned()))?;
        if !renumber(&mut tx, id, Some((item, to))).await? {
            return Err(PartyError::NotFound("queue item"));
        }
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "queue",
            card: false,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    /// Skip to the next queue entry (controllers).
    pub async fn next(&self, v: Viewer, id: i64) -> PartyResult<PartyView> {
        self.limit(v.user_id, "command", 5_000, 12)?;
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, &row, mem).await?;
        if !standing.member {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::allowed(row.shared_control, standing) {
            return Err(PartyError::Forbidden);
        }
        if !self
            .advance_queue(&mut tx, &mut row, true, Some(v.user_id), &mut notices)
            .await?
        {
            return Err(PartyError::Conflict("The queue is empty.".to_owned()));
        }
        tx.commit().await?;
        self.flush(notices).await;
        self.get(v, id).await
    }

    /// Start the first playable queue entry. Entries that are no longer playable are dropped. `false`:
    /// nothing to play. The entry was admitted by someone who passed the age gate; members who did not are
    /// removed by [`Self::apply_media`].
    pub(crate) async fn advance_queue(
        &self,
        c: &mut PgConnection,
        row: &mut PartyRow,
        autoplay: bool,
        actor: Option<i64>,
        notices: &mut Vec<Notice>,
    ) -> PartyResult<bool> {
        loop {
            let q = store::queue(c, row.id).await?;
            let Some(first) = q.first() else {
                return Ok(false);
            };
            sqlx::query("DELETE FROM watch_party_queue WHERE id = $1")
                .bind(first.id)
                .execute(&mut *c)
                .await?;
            renumber(c, row.id, None).await?;
            let resolved = self
                .inner
                .play
                .resolve(
                    true,
                    first.media.title_id,
                    (first.media.episode_id != 0).then_some(first.media.episode_id),
                    first.media.variant_id,
                )
                .await;
            let Ok(media) = resolved else {
                continue;
            };
            self.apply_media(c, row, &media, 0, autoplay, actor, notices)
                .await?;
            notices.push(Notice::Updated {
                party: row.id,
                kind: "queue",
                card: false,
                also: vec![],
            });
            return Ok(true);
        }
    }

    // ---- buffering (WP-BE-005) ---------------------------------------------------------------------

    /// Run the buffering rule over the current reports and apply what it decides.
    pub(crate) async fn rebuffer(
        &self,
        c: &mut PgConnection,
        row: &mut PartyRow,
        members: &[MemberRow],
        notices: &mut Vec<Notice>,
    ) -> PartyResult<()> {
        let now = self.now();
        let reports: Vec<BufferReport> = members
            .iter()
            .map(|m| BufferReport {
                user: m.user_id,
                since_ms: m.buffering_since_ms,
            })
            .collect();
        let decision = rules::decide_buffering(
            row.auto_pause_buffering,
            row.playing,
            row.auto_paused,
            row.buffer_wait_until_ms,
            &reports,
            now,
            i64::from(row.max_wait_ms),
        );
        match decision {
            BufferDecision::Nothing => {}
            BufferDecision::Pause { wait_until_ms, .. } => {
                let applied = row.playback().apply(Command::AutoPause, now)?;
                if applied.changed {
                    store::save_playback(c, row.id, &applied.state, Some(wait_until_ms), now)
                        .await?;
                    row.set_playback(&applied.state, Some(wait_until_ms));
                    notices.push(Notice::Playback {
                        party: row.id,
                        reason: "buffering",
                        by: None,
                    });
                    notices.push(Notice::ArmTimer {
                        party: row.id,
                        delay_ms: i64::from(row.max_wait_ms) + 100,
                    });
                }
            }
            BufferDecision::Resume | BufferDecision::ContinueWithout { .. } => {
                let reason = if let BufferDecision::ContinueWithout { users } = &decision {
                    sqlx::query(
                        "UPDATE watch_party_members SET buffering_since_ms = NULL \
                         WHERE party_id = $1 AND user_id = ANY($2)",
                    )
                    .bind(row.id)
                    .bind(users)
                    .execute(&mut *c)
                    .await?;
                    notices.push(Notice::Updated {
                        party: row.id,
                        kind: "participant_updated",
                        card: false,
                        also: vec![],
                    });
                    "continued"
                } else {
                    "resumed"
                };
                let applied = row.playback().apply(Command::AutoResume, now)?;
                if applied.changed {
                    store::save_playback(c, row.id, &applied.state, None, now).await?;
                    row.set_playback(&applied.state, None);
                    notices.push(Notice::Playback {
                        party: row.id,
                        reason,
                        by: None,
                    });
                }
            }
        }
        Ok(())
    }

    /// Timer entry point: re-run the rule for one party (a wait may have run out).
    pub async fn tick(&self, id: i64) -> PartyResult<()> {
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let Some(mut row) = store::get_party(&mut tx, id, true).await? else {
            return Ok(());
        };
        if row.ended_at_ms.is_some() {
            return Ok(());
        }
        let members = store::members(&mut tx, id).await?;
        self.rebuffer(&mut tx, &mut row, &members, &mut notices)
            .await?;
        tx.commit().await?;
        self.flush(notices).await;
        Ok(())
    }
}

/// Rewrite the `position` of a party's queue to 0..n in order, optionally moving `(item, to)` first.
/// `false` when the item to move is not in the queue.
async fn renumber(c: &mut PgConnection, party: i64, mv: Option<(i64, usize)>) -> PartyResult<bool> {
    let mut ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM watch_party_queue WHERE party_id = $1 ORDER BY position, id",
    )
    .bind(party)
    .fetch_all(&mut *c)
    .await?;
    if let Some((item, to)) = mv
        && !rules::move_item(&mut ids, item, to)
    {
        return Ok(false);
    }
    let positions: Vec<i32> = (0..ids.len())
        .map(|i| i32::try_from(i).unwrap_or(i32::MAX))
        .collect();
    sqlx::query(
        "UPDATE watch_party_queue q SET position = x.pos FROM unnest($1::bigint[], $2::int[]) AS x(id, pos) \
         WHERE q.id = x.id",
    )
    .bind(&ids)
    .bind(&positions)
    .execute(&mut *c)
    .await?;
    Ok(true)
}
