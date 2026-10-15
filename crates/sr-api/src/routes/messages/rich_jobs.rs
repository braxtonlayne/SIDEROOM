//! Background work of the rich message features, called by the job handlers in
//! `sr-server`: link previews (`messages.unfurl`) and poll finalisation
//! (`messages.poll_finalize`, `messages.poll_sweep`). Each function announces its
//! result as `MESSAGE_UPDATE`.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use super::render::event_payload;
use super::rich::MAX_PREVIEW_LINKS;
use sr_chat::message::Embed;
use sr_chat::{Message, message_flags};
use sr_core::{ChannelId, MessageId};
use sr_events::DomainEvent;

/// Result of a preview pass, for logging and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfurlOutcome {
    /// The message is gone, hidden, or not eligible (DM, feature off, no unfurler).
    Skipped,
    /// Previews were computed and equal what the message already had.
    Unchanged,
    /// The message embeds changed and `MESSAGE_UPDATE` went out.
    Updated,
}

async fn announce_update(deps: &MessagesDeps, msg: &Message) -> MsgResult<()> {
    let data = event_payload(deps, msg).await?;
    deps.publish(DomainEvent::MessageEdited {
        guild: msg.guild_id,
        channel: msg.channel_id,
        message_id: msg.id.get(),
        data,
    })
    .await;
    Ok(())
}

/// Re-read `message` and announce it as `MESSAGE_UPDATE` (used by the attachment
/// expiry job, LNG-MSG-022). A message that is gone or hidden is skipped.
pub async fn announce_message_update(
    deps: &MessagesDeps,
    channel: ChannelId,
    message: MessageId,
) -> MsgResult<()> {
    if let Some(msg) = deps
        .repo
        .get_message(channel, message)
        .await?
        .filter(Message::is_visible)
    {
        announce_update(deps, &msg).await?;
    }
    Ok(())
}

/// Recompute the link previews of one message: embeds written by the author or a
/// bot stay, previous server-generated ones are replaced by fresh previews of the
/// links currently in the content. `SUPPRESS_EMBEDS` and `<url>` links yield none.
pub async fn run_unfurl(
    deps: &MessagesDeps,
    channel: ChannelId,
    message: MessageId,
) -> MsgResult<UnfurlOutcome> {
    let (Some(unfurler), Some(store)) = (&deps.rich.unfurler, &deps.rich.store) else {
        return Ok(UnfurlOutcome::Skipped);
    };
    let Some(msg) = deps
        .repo
        .get_message(channel, message)
        .await?
        .filter(Message::is_visible)
    else {
        return Ok(UnfurlOutcome::Skipped);
    };
    // Previews are guild-only: a DM is never fetched on a user's behalf.
    let Some(guild) = msg.guild_id else {
        return Ok(UnfurlOutcome::Skipped);
    };
    if let Some(gate) = &deps.rich.gate
        && !gate.enabled(guild).await
    {
        return Ok(UnfurlOutcome::Skipped);
    }

    let mut next: Vec<Embed> = msg
        .embeds
        .iter()
        .filter(|e| !e.is_unfurled())
        .cloned()
        .collect();
    if msg.flags & message_flags::SUPPRESS_EMBEDS == 0 {
        let links = sr_markdown::extract_preview_links(&sr_markdown::parse(&msg.content));
        for link in links.iter().take(MAX_PREVIEW_LINKS) {
            match unfurler.unfurl(link).await {
                Ok(Some(preview)) => {
                    let embed = serde_json::to_value(&preview)
                        .ok()
                        .and_then(|v| serde_json::from_value::<Embed>(v).ok());
                    if let Some(e) = embed {
                        next.push(e);
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::debug!(error = %e, "link preview skipped"),
            }
        }
    }
    next.truncate(sr_chat::message::MAX_EMBEDS);
    if next == msg.embeds {
        return Ok(UnfurlOutcome::Unchanged);
    }
    store.set_embeds(msg.id, &next).await?;
    let fresh = deps.visible_message(channel, message).await?;
    announce_update(deps, &fresh).await?;
    Ok(UnfurlOutcome::Updated)
}

/// Finalise `message`'s poll if it has run out, announcing the final results with
/// `MESSAGE_UPDATE`. True when this call finalised it.
pub async fn finalize_poll(
    deps: &MessagesDeps,
    channel: ChannelId,
    message: MessageId,
) -> MsgResult<bool> {
    let Some(store) = &deps.rich.store else {
        return Ok(false);
    };
    if !store.finalize_poll(message, deps.now_ms()).await? {
        return Ok(false);
    }
    if let Some(msg) = deps
        .repo
        .get_message(channel, message)
        .await?
        .filter(Message::is_visible)
    {
        announce_update(deps, &msg).await?;
    }
    Ok(true)
}

/// Finalise every poll that is due. Returns how many were finalised.
pub async fn sweep_polls(deps: &MessagesDeps) -> MsgResult<usize> {
    let Some(store) = &deps.rich.store else {
        return Ok(0);
    };
    let mut done = 0;
    for (channel, message) in store.due_polls(deps.now_ms(), 100).await? {
        if finalize_poll(deps, channel, message).await? {
            done += 1;
        }
    }
    Ok(done)
}
