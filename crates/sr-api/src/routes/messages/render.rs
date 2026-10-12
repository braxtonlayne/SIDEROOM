//! Turning domain messages into Discord-shaped objects.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use super::model::MessageObject;
use crate::Principal;
use sr_chat::Message;
use sr_chat::repo::UserSummary;
use sr_core::UserId;
use std::collections::{HashMap, HashSet};

/// The acting user of a request.
pub fn actor(p: &Principal) -> UserId {
    UserId::new(u64::try_from(p.user_id).unwrap_or(0))
}

/// Display fields for every author and mentioned user of `msgs` (open question 157).
async fn lookup_users(
    deps: &MessagesDeps,
    msgs: &[Message],
) -> MsgResult<HashMap<UserId, UserSummary>> {
    let ids: HashSet<UserId> = msgs
        .iter()
        .flat_map(|m| std::iter::once(m.author_id).chain(m.mention_user_ids.iter().copied()))
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<UserId> = ids.into_iter().collect();
    let found = deps.users.users(&ids).await?;
    Ok(found.into_iter().map(|u| (u.id, u)).collect())
}

/// Attachment URLs are signed for the viewer (open question 153).
fn sign_attachments(deps: &MessagesDeps, objects: &mut [MessageObject]) {
    for o in objects {
        for a in &mut o.attachments {
            a.url = deps.sign_attachment_url(&a.url);
        }
    }
}

/// REST view: attachments, the viewer's reactions and user objects included.
pub async fn render_many(
    deps: &MessagesDeps,
    viewer: UserId,
    msgs: &[Message],
) -> MsgResult<Vec<MessageObject>> {
    let ids: Vec<_> = msgs.iter().map(|m| m.id).collect();
    let mut atts = deps.store.attachments_of(&ids).await?;
    let mut reactions = deps.store.reactions_for(&ids, viewer).await?;
    let users = lookup_users(deps, msgs).await?;
    let mut out: Vec<MessageObject> = msgs
        .iter()
        .map(|m| {
            MessageObject::build(
                m,
                &atts.remove(&m.id).unwrap_or_default(),
                &reactions.remove(&m.id).unwrap_or_default(),
                None,
                &users,
            )
        })
        .collect();
    sign_attachments(deps, &mut out);
    Ok(out)
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
    let users = lookup_users(deps, std::slice::from_ref(msg)).await?;
    let mut obj = MessageObject::build(msg, &atts, &[], nonce, &users);
    sign_attachments(deps, std::slice::from_mut(&mut obj));
    Ok(obj.to_value())
}
