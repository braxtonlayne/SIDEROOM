//! Turning domain messages into Discord-shaped objects.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use super::model::MessageObject;
use crate::Principal;
use sr_chat::Message;
use sr_core::UserId;

/// The acting user of a request.
pub fn actor(p: &Principal) -> UserId {
    UserId::new(u64::try_from(p.user_id).unwrap_or(0))
}

/// REST view: attachments and the viewer's reactions included.
pub async fn render_many(
    deps: &MessagesDeps,
    viewer: UserId,
    msgs: &[Message],
) -> MsgResult<Vec<MessageObject>> {
    let ids: Vec<_> = msgs.iter().map(|m| m.id).collect();
    let mut atts = deps.store.attachments_of(&ids).await?;
    let mut reactions = deps.store.reactions_for(&ids, viewer).await?;
    Ok(msgs
        .iter()
        .map(|m| {
            MessageObject::build(
                m,
                &atts.remove(&m.id).unwrap_or_default(),
                &reactions.remove(&m.id).unwrap_or_default(),
                None,
            )
        })
        .collect())
}

pub async fn render_one(
    deps: &MessagesDeps,
    viewer: UserId,
    msg: &Message,
) -> MsgResult<MessageObject> {
    let mut v = render_many(deps, viewer, std::slice::from_ref(msg)).await?;
    Ok(v.remove(0))
}

/// Gateway payload: no per-viewer reaction state, but the author's nonce so the
/// sending client can reconcile its pending message.
pub async fn event_payload(deps: &MessagesDeps, msg: &Message) -> MsgResult<serde_json::Value> {
    let atts = deps
        .store
        .attachments_of(&[msg.id])
        .await?
        .remove(&msg.id)
        .unwrap_or_default();
    let nonce = deps.store.nonce_of(msg.id).await?;
    Ok(MessageObject::build(msg, &atts, &[], nonce).to_value())
}
