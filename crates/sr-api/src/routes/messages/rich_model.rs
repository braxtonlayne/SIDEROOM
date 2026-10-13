//! Discord-shaped JSON for the rich parts of a message: the poll with its live
//! results, `sticker_items` and the `message_snapshots` of forwards.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use super::model::{AttachmentObject, MessageObject, iso};
use serde_json::{Value, json};
use sr_chat::Message;
use sr_chat::message::{ForwardSnapshot, Poll};
use sr_chat::rich_repo::{PollTally, StickerRecord};
use sr_core::UserId;
use std::collections::HashMap;

/// The `poll` object: definition, expiry and `results`. `viewer_known` fills
/// `me_voted`; gateway payloads have no viewer.
pub fn poll_value(p: &Poll, tally: Option<&PollTally>, viewer_known: bool) -> Value {
    let default = PollTally::default();
    let t = tally.unwrap_or(&default);
    let media = |m: &sr_chat::message::PollMedia| {
        let mut v = json!({});
        if let Some(text) = &m.text {
            v["text"] = json!(text);
        }
        if let Some(e) = &m.emoji {
            v["emoji"] = json!(e);
        }
        v
    };
    json!({
        "question": media(&p.question),
        "answers": p.answers.iter().map(|a| json!({
            "answer_id": a.answer_id,
            "poll_media": media(&a.poll_media),
        })).collect::<Vec<_>>(),
        "expiry": p.expiry_ms.map(iso),
        "allow_multiselect": p.allow_multiselect,
        "layout_type": p.layout_type.unwrap_or(1),
        "results": {
            "is_finalized": p.finalized,
            "answer_counts": p.answers.iter().map(|a| json!({
                "id": a.answer_id,
                "count": t.count(a.answer_id),
                "me_voted": viewer_known && t.mine.contains(&a.answer_id),
            })).collect::<Vec<_>>(),
        },
    })
}

fn sticker_item(s: &StickerRecord) -> Value {
    json!({"id": s.id.to_string(), "name": s.name, "format_type": s.format_type})
}

/// One `message_snapshots` entry for a forward.
fn snapshot_value(deps: &MessagesDeps, f: &ForwardSnapshot) -> Value {
    let attachments: Vec<Value> = f
        .attachments
        .iter()
        .map(|a| {
            let url = crate::routes::cdn::cdn_path(
                "attachments",
                &[&a.channel_id.to_string(), &a.id.to_string(), &a.filename],
            );
            let o = AttachmentObject {
                id: a.id.to_string(),
                filename: a.filename.clone(),
                size: a.size,
                url: deps.sign_attachment_url(&url),
                content_type: a.content_type.clone(),
                description: a.description.clone(),
                expires_at: None,
                duration_secs: a.duration_ms.map(|d| f64::from(d) / 1000.0),
                waveform: a.waveform.clone(),
            };
            serde_json::to_value(o).unwrap_or(Value::Null)
        })
        .collect();
    json!({"message": {
        "type": f.kind,
        "content": f.content,
        "embeds": f.embeds,
        "attachments": attachments,
        "timestamp": iso(f.timestamp_ms),
        "edited_timestamp": f.edited_timestamp_ms.map(iso),
        "flags": f.flags,
        "mentions": [],
        "mention_roles": [],
    }})
}

/// Fill the poll, sticker and forward parts of already built objects. `viewer` is
/// `None` for gateway payloads.
pub async fn decorate(
    deps: &MessagesDeps,
    viewer: Option<UserId>,
    msgs: &[Message],
    objs: &mut [MessageObject],
) -> MsgResult<()> {
    let poll_ids: Vec<_> = msgs
        .iter()
        .filter(|m| m.poll.is_some())
        .map(|m| m.id)
        .collect();
    let mut sticker_ids: Vec<u64> = msgs.iter().flat_map(|m| m.sticker_ids.clone()).collect();
    sticker_ids.sort_unstable();
    sticker_ids.dedup();
    let (tallies, stickers): (HashMap<_, _>, HashMap<u64, StickerRecord>) = match &deps.rich.store {
        Some(store) => (
            if poll_ids.is_empty() {
                HashMap::new()
            } else {
                store.poll_tallies(&poll_ids, viewer).await?
            },
            if sticker_ids.is_empty() {
                HashMap::new()
            } else {
                store
                    .stickers(&sticker_ids)
                    .await?
                    .into_iter()
                    .map(|s| (s.id, s))
                    .collect()
            },
        ),
        None => (HashMap::new(), HashMap::new()),
    };
    for (m, o) in msgs.iter().zip(objs.iter_mut()) {
        if let Some(p) = &m.poll {
            o.poll = Some(poll_value(p, tallies.get(&m.id), viewer.is_some()));
        }
        o.sticker_items = m
            .sticker_ids
            .iter()
            .filter_map(|id| stickers.get(id))
            .map(sticker_item)
            .collect();
        if let Some(f) = m.reference.as_ref().and_then(|r| r.forward.as_ref()) {
            o.message_snapshots = vec![snapshot_value(deps, f)];
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::message::{PollAnswer, PollMedia};

    #[test]
    fn poll_json_has_results_and_expiry() {
        let p = Poll {
            question: PollMedia {
                text: Some("q".into()),
                emoji: None,
            },
            answers: vec![
                PollAnswer {
                    answer_id: 1,
                    poll_media: PollMedia {
                        text: Some("a".into()),
                        emoji: None,
                    },
                },
                PollAnswer {
                    answer_id: 2,
                    poll_media: PollMedia::default(),
                },
            ],
            allow_multiselect: false,
            duration_hours: Some(1),
            layout_type: Some(1),
            expiry_ms: Some(0),
            finalized: true,
        };
        let mut t = PollTally::default();
        t.counts.insert(1, 3);
        t.mine.insert(1);
        let v = poll_value(&p, Some(&t), true);
        assert_eq!(v["results"]["is_finalized"], true);
        assert_eq!(v["results"]["answer_counts"][0]["count"], 3);
        assert_eq!(v["results"]["answer_counts"][0]["me_voted"], true);
        assert_eq!(v["results"]["answer_counts"][1]["count"], 0);
        assert_eq!(v["expiry"], "1970-01-01T00:00:00.000000+00:00");
        assert!(v.get("expiry_ms").is_none());
        let g = poll_value(&p, Some(&t), false);
        assert_eq!(g["results"]["answer_counts"][0]["me_voted"], false);
    }
}
