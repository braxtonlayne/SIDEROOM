//! The call record (VOX-SIG-008): when a DM call ends, a `CALL` system message goes into the
//! conversation ("X started a call that lasted 4 minutes", or "missed call" for members who
//! never joined).
//!
//! The record is stored as a system message of type 3 whose `content` holds the call
//! details as JSON; [`super::model::MessageObject`] turns that into Discord's `call` object
//! (`{participants, ended_timestamp}`) and an empty `content`, so clients see the shape
//! they expect and the stored row stays a plain text column.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use super::render::event_payload;
use sr_chat::{Message, MessageType, Visibility};
use sr_core::{ChannelId, MessageId, UserId};
use sr_events::DomainEvent;

/// What `content` of a `CALL` message holds.
pub fn call_content(participants: &[UserId], started_ms: i64, ended_ms: i64) -> String {
    serde_json::json!({
        "participants": participants.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "started_ms": started_ms,
        "ended_ms": ended_ms,
    })
    .to_string()
}

/// The Discord `call` object for a stored `CALL` message, or `None` for content that is
/// not call details.
pub fn call_object(content: &str) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let participants = v.get("participants")?.clone();
    let ended = v.get("ended_ms")?.as_i64()?;
    Some(serde_json::json!({
        "participants": participants,
        "ended_timestamp": super::model::iso(ended),
    }))
}

/// Write the record of a finished call into `channel` and announce it like any message.
pub async fn post_call_record(
    deps: &MessagesDeps,
    channel: ChannelId,
    initiator: UserId,
    participants: &[UserId],
    started_ms: i64,
    ended_ms: i64,
) -> MsgResult<()> {
    let guild = deps
        .repo
        .get_channel(channel)
        .await?
        .and_then(|c| c.guild_id);
    let msg = Message {
        id: MessageId::from(deps.next_id()),
        channel_id: channel,
        guild_id: guild,
        author_id: initiator,
        kind: MessageType::Call,
        content: call_content(participants, started_ms, ended_ms),
        flags: 0,
        edited_at_ms: None,
        reference: None,
        attachment_ids: Vec::new(),
        embeds: Vec::new(),
        components: Vec::new(),
        sticker_ids: Vec::new(),
        poll: None,
        mention_everyone: false,
        mention_user_ids: Vec::new(),
        mention_role_ids: Vec::new(),
        mention_channel_ids: Vec::new(),
        pinned: false,
        visibility: Visibility::Visible,
        deleted_at_ms: None,
        purge_after_ms: None,
        hold_id: None,
    };
    deps.repo.insert_message(&msg).await?;
    let data = event_payload(deps, &msg).await?;
    deps.publish(DomainEvent::MessageSent {
        guild,
        channel,
        message_id: msg.id.get(),
        author: initiator,
        data,
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_round_trips_into_a_call_object() {
        let c = call_content(&[UserId::new(1), UserId::new(2)], 1_000, 61_000);
        let obj = call_object(&c).unwrap();
        assert_eq!(obj["participants"], serde_json::json!(["1", "2"]));
        assert_eq!(obj["ended_timestamp"], "1970-01-01T00:01:01.000000+00:00");
    }

    #[test]
    fn other_content_is_not_a_call() {
        assert!(call_object("hello").is_none());
        assert!(call_object("{}").is_none());
        assert!(call_object("{\"participants\":[]}").is_none());
    }
}
