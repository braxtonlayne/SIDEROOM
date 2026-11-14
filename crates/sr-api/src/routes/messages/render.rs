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

/// Attachment URLs are signed for the viewer (open question 153); third-party embed
/// images get a `proxy_url` (D-176).
fn sign_attachments(deps: &MessagesDeps, objects: &mut [MessageObject]) {
    for o in objects {
        for a in &mut o.attachments {
            a.url = deps.sign_attachment_url(&a.url);
        }
        deps.proxy_embed_images(&mut o.embeds);
    }
}

/// Attach the thread started from each message (a thread shares its starter's id).
///
/// With `preview_users` (the REST views, never a gateway payload: one payload goes to every viewer) each thread
/// also carries `sr_last_message` (D-689): its latest not-deleted message, read for the whole page in ONE query.
/// Only the threads under these messages are asked about, and the viewer already read the channel they sit in. A
/// thread started from a message is a public one, so nothing the viewer could not read is previewed; a private
/// thread (which no message starts) is previewed only inside itself, where its own starter message sits.
async fn attach_threads(
    deps: &MessagesDeps,
    msgs: &[Message],
    out: &mut [MessageObject],
    preview_users: Option<&HashMap<UserId, UserSummary>>,
) {
    let ids: Vec<sr_core::ChannelId> = msgs
        .iter()
        .filter(|m| m.guild_id.is_some())
        .map(|m| sr_core::ChannelId::new(m.id.get()))
        .collect();
    if ids.is_empty() {
        return;
    }
    let threads = match deps.repo.get_threads(&ids).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "looking up message threads failed");
            return;
        }
    };
    let last = match preview_users {
        Some(_) => thread_previews(deps, msgs, &threads).await,
        None => HashMap::new(),
    };
    let mut users = preview_users.cloned().unwrap_or_default();
    let missing: Vec<UserId> = last
        .values()
        .map(|g| g.author_id)
        .filter(|u| !users.contains_key(u))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if !missing.is_empty() {
        match deps.users.users(&missing).await {
            Ok(found) => users.extend(found.into_iter().map(|u| (u.id, u))),
            Err(e) => tracing::warn!(error = %e, "looking up thread preview authors failed"),
        }
    }
    for t in threads {
        let Some(o) = out.iter_mut().find(|o| o.id == t.id.to_string()) else {
            continue;
        };
        o.thread =
            serde_json::to_value(crate::routes::guilds::dto::ChannelObject::new(&t, &[])).ok();
        if let (Some(g), Some(serde_json::Value::Object(obj))) =
            (last.get(&t.id), o.thread.as_mut())
        {
            obj.insert(
                "sr_last_message".to_owned(),
                serde_json::to_value(super::model::ThreadLastMessage::new(g, &users))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }
}

/// The latest message of each thread of the page that may be previewed (see [`attach_threads`]).
async fn thread_previews(
    deps: &MessagesDeps,
    msgs: &[Message],
    threads: &[sr_chat::Channel],
) -> HashMap<sr_core::ChannelId, sr_chat::preview::MessageGist> {
    let ask: Vec<sr_core::ChannelId> = threads
        .iter()
        .filter(|t| {
            t.kind != sr_chat::ChannelType::PrivateThread
                || msgs.iter().any(|m| m.channel_id == t.id)
        })
        .map(|t| t.id)
        .collect();
    if ask.is_empty() {
        return HashMap::new();
    }
    match deps.store.last_messages(&ask).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "reading thread previews failed");
            HashMap::new()
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
    // PAR-FU-002: the likely-spam flag is the recipient's view; the author never gets it.
    for (o, m) in out.iter_mut().zip(msgs) {
        o.flags = sr_chat::message_flags::for_viewer(o.flags, m.author_id, viewer);
    }
    super::rich_model::decorate(deps, Some(viewer), msgs, &mut out).await?;
    attach_threads(deps, msgs, &mut out, Some(&users)).await;
    sign_attachments(deps, &mut out);
    // D-230: the viewer's sensitive-media settings, after signing so a blocked URL stays empty.
    deps.apply_sensitive(viewer, msgs, &mut out).await;
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
    let mut objs = vec![MessageObject::build(msg, &atts, &[], nonce, &users)];
    super::rich_model::decorate(deps, None, std::slice::from_ref(msg), &mut objs).await?;
    sign_attachments(deps, &mut objs);
    attach_threads(deps, std::slice::from_ref(msg), &mut objs, None).await;
    Ok(objs[0].to_value())
}

/// A message just created, rendered once for both of its uses: the author's REST answer and the gateway
/// payload (RDY-PERF-002: rendering it twice read attachments, nonce, users, webhook authors and threads twice
/// for every message sent). A new message has no reactions, and without a poll the viewer changes nothing in
/// `decorate`, so the two differ only by the author's sensitive-media view, applied last. A message with a
/// poll is still rendered twice (the REST view carries `me_voted`).
///
/// `nonce` is the one the request carried (the handler stored it), so it is not read back. Nothing else is read
/// that cannot be there yet: attachments only when the message has some, and no thread lookup (a thread
/// started from the message gets its own `THREAD_CREATE`; Discord's create answer has no `thread` either).
pub async fn render_created(
    deps: &MessagesDeps,
    author: UserId,
    msg: &Message,
    nonce: Option<String>,
) -> MsgResult<(MessageObject, serde_json::Value)> {
    if msg.poll.is_some() {
        let data = event_payload(deps, msg).await?;
        return Ok((render_one(deps, author, msg).await?, data));
    }
    let atts = if msg.attachment_ids.is_empty() {
        Vec::new()
    } else {
        deps.store
            .attachments_of(&[msg.id])
            .await?
            .remove(&msg.id)
            .unwrap_or_default()
    };
    let users = lookup_users(deps, std::slice::from_ref(msg)).await?;
    let mut objs = vec![MessageObject::build(msg, &atts, &[], nonce, &users)];
    super::rich_model::decorate(deps, None, std::slice::from_ref(msg), &mut objs).await?;
    sign_attachments(deps, &mut objs);
    let data = objs[0].to_value();
    // The broadcast keeps the flag (the recipient needs it; the gateway masks it for the author); the author's
    // own REST response does not carry it.
    objs[0].flags = sr_chat::message_flags::for_viewer(objs[0].flags, msg.author_id, author);
    deps.apply_sensitive(author, std::slice::from_ref(msg), &mut objs)
        .await;
    Ok((objs.remove(0), data))
}
