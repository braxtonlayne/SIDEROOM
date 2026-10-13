//! Create-message inputs beyond plain text: polls, stickers, voice messages,
//! forwards and `allowed_mentions` validation (LNG-MSG-002/007/010/019).

use super::deps::{Access, MessagesDeps};
use super::error::{MsgResult, invalid, parse_id};
use super::model::{AllowedMentionsBody, AttachmentRef, MessageReferenceIn};
use super::store::Attachment;
use data_encoding::BASE64;
use sr_chat::message::{ForwardSnapshot, Poll, SnapshotAttachment};
use sr_chat::poll_rules::{self, PollCreate};
use sr_chat::{ForwardRequest, Message};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId, UserId};

/// Stickers per message (Discord).
pub const MAX_STICKERS: usize = 3;
/// Longest voice message.
pub const MAX_VOICE_SECS: f64 = 120.0;
/// Largest voice-message waveform, in decoded bytes.
pub const MAX_WAVEFORM_BYTES: usize = 256;
/// Largest voice-message file. Two minutes of speech codec output is far below this;
/// the cap bounds what a client can pass off as a voice message.
pub const MAX_VOICE_BYTES: u64 = 8 * 1024 * 1024;

/// The `poll` field of a create request, validated into a stored poll.
pub fn plan_poll(
    deps: &MessagesDeps,
    raw: Option<&serde_json::Value>,
) -> MsgResult<Option<Poll>> {
    let Some(raw) = raw else { return Ok(None) };
    deps.rich_store("poll")?;
    let create: PollCreate = serde_json::from_value(raw.clone())
        .map_err(|e| invalid("poll", "INVALID_POLL", &format!("Invalid poll: {e}")))?;
    Ok(Some(poll_rules::plan_poll(create, deps.now_ms())?))
}

/// Sticker ids of a create request: at most three, distinct, existing, available,
/// and either standard or from the channel's own guild.
pub async fn resolve_stickers(
    deps: &MessagesDeps,
    channel_guild: Option<sr_core::GuildId>,
    raw: Option<&Vec<String>>,
) -> MsgResult<Vec<u64>> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Ok(Vec::new());
    };
    if raw.len() > MAX_STICKERS {
        return Err(invalid(
            "sticker_ids",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 3 or fewer in length.",
        ));
    }
    let mut ids = Vec::with_capacity(raw.len());
    for s in raw {
        let id = s
            .parse::<u64>()
            .ok()
            .filter(|n| *n != 0)
            .ok_or_else(|| invalid("sticker_ids", "NUMBER_TYPE_COERCE", "Value is not snowflake."))?;
        if ids.contains(&id) {
            return Err(invalid("sticker_ids", "DUPLICATE", "Duplicate sticker."));
        }
        ids.push(id);
    }
    let found = deps.rich_store("sticker_ids")?.stickers(&ids).await?;
    for id in &ids {
        let ok = found.iter().any(|s| {
            s.id == *id && s.available && (s.guild_id.is_none() || s.guild_id == channel_guild)
        });
        if !ok {
            return Err(invalid(
                "sticker_ids",
                "UNKNOWN_STICKER",
                "Unknown sticker, or it cannot be used in this channel.",
            ));
        }
    }
    Ok(ids)
}

/// Copy the client-declared voice metadata onto the upload rows.
pub fn apply_voice_meta(uploads: &mut [(Attachment, Option<String>)], refs: &[&AttachmentRef]) {
    for ((att, _), r) in uploads.iter_mut().zip(refs) {
        if let Some(d) = r.duration_secs.filter(|d| d.is_finite() && *d > 0.0) {
            att.duration_ms = Some((d * 1000.0).round().min(f64::from(u32::MAX)) as u32);
        }
        if r.waveform.is_some() {
            att.waveform.clone_from(&r.waveform);
        }
    }
}

/// A voice message is one `audio/ogg` file of at most two minutes with a waveform.
/// (Text, embeds, stickers and polls are rejected by the chat rules.)
pub fn validate_voice(uploads: &[(Attachment, Option<String>)]) -> MsgResult<()> {
    let [(att, _)] = uploads else {
        return Err(invalid(
            "flags",
            "VOICE_MESSAGE_SHAPE",
            "A voice message is exactly one audio attachment.",
        ));
    };
    if att.content_type.as_deref() != Some("audio/ogg") {
        return Err(invalid(
            "attachments",
            "VOICE_MESSAGE_TYPE",
            "A voice message must be an audio/ogg file.",
        ));
    }
    if att.size > MAX_VOICE_BYTES {
        return Err(invalid(
            "attachments",
            "VOICE_MESSAGE_SIZE",
            "The voice message file is too large.",
        ));
    }
    match att.duration_ms {
        Some(ms) if f64::from(ms) <= MAX_VOICE_SECS * 1000.0 => {}
        Some(_) => {
            return Err(invalid(
                "attachments.0.duration_secs",
                "VOICE_MESSAGE_DURATION",
                "A voice message can be at most 120 seconds long.",
            ));
        }
        None => {
            return Err(invalid(
                "attachments.0.duration_secs",
                "BASE_TYPE_REQUIRED",
                "A voice message needs its duration.",
            ));
        }
    }
    let wave = att
        .waveform
        .as_deref()
        .ok_or_else(|| invalid("attachments.0.waveform", "BASE_TYPE_REQUIRED", "A voice message needs a waveform."))?;
    match BASE64.decode(wave.as_bytes()) {
        Ok(b) if !b.is_empty() && b.len() <= MAX_WAVEFORM_BYTES => Ok(()),
        _ => Err(invalid(
            "attachments.0.waveform",
            "VOICE_MESSAGE_WAVEFORM",
            "The waveform must be base64 of 1 to 256 bytes.",
        )),
    }
}

/// Validate `allowed_mentions` beyond field syntax (Discord rules): a kind in
/// `parse` excludes explicit ids of that kind, ids are unique and capped at 100.
pub fn validate_allowed_mentions(b: &AllowedMentionsBody) -> MsgResult<()> {
    for (kind, ids) in [("users", &b.users), ("roles", &b.roles)] {
        if b.parse.iter().any(|p| p == kind) && !ids.is_empty() {
            return Err(invalid(
                &format!("allowed_mentions.{kind}"),
                "MESSAGE_ALLOWED_MENTIONS_PARSE_EXCLUSIVE",
                &format!("Cannot list {kind} ids when `{kind}` is in parse."),
            ));
        }
        if ids.len() > 100 {
            return Err(invalid(
                &format!("allowed_mentions.{kind}"),
                "BASE_TYPE_MAX_LENGTH",
                "Must be 100 or fewer in length.",
            ));
        }
        let mut seen: Vec<&String> = Vec::with_capacity(ids.len());
        for id in ids {
            if seen.contains(&id) {
                return Err(invalid(
                    &format!("allowed_mentions.{kind}"),
                    "MESSAGE_ALLOWED_MENTIONS_DUPLICATES",
                    "Duplicate ids.",
                ));
            }
            seen.push(id);
        }
    }
    Ok(())
}

/// The copy a forward keeps of `src` (a forward of a forward keeps the original copy).
pub fn snapshot_of(src: &Message, atts: &[Attachment]) -> ForwardSnapshot {
    if let Some(f) = src.reference.as_ref().and_then(|r| r.forward.clone()) {
        return f;
    }
    ForwardSnapshot {
        source_guild_id: src.guild_id.map(sr_core::GuildId::get),
        kind: src.kind.code(),
        content: src.content.clone(),
        embeds: src.embeds.clone(),
        attachments: atts
            .iter()
            .map(|a| SnapshotAttachment {
                id: a.id.get(),
                channel_id: a.channel_id.get(),
                filename: a.filename.clone(),
                size: a.size,
                content_type: a.content_type.clone(),
                description: a.description.clone(),
                duration_ms: a.duration_ms,
                waveform: a.waveform.clone(),
            })
            .collect(),
        sticker_ids: src.sticker_ids.clone(),
        flags: src.flags
            & !(sr_chat::message_flags::IS_CROSSPOST | sr_chat::message_flags::CROSSPOSTED),
        timestamp_ms: src.id.snowflake().unix_ms(),
        edited_timestamp_ms: src.edited_at_ms,
    }
}

/// Resolve `message_reference.type = 1`: the forwarder must be able to read the
/// source message. The target channel's own send permission is checked by the chat
/// rules as for any message.
pub async fn plan_forward(
    deps: &MessagesDeps,
    forwarder: UserId,
    r: &MessageReferenceIn,
) -> MsgResult<ForwardRequest> {
    let src_channel: ChannelId = r
        .channel_id
        .as_deref()
        .ok_or_else(|| {
            invalid(
                "message_reference.channel_id",
                "BASE_TYPE_REQUIRED",
                "A forward needs the source channel.",
            )
        })
        .and_then(|c| parse_id(c, ErrorCode::UnknownChannel))?;
    let src_id: MessageId = parse_id(&r.message_id, ErrorCode::UnknownMessage)?;
    let acc: Access = deps.access(forwarder, src_channel).await?;
    acc.require(Permissions::READ_MESSAGE_HISTORY)?;
    let src = deps.visible_message(src_channel, src_id).await?;
    let atts = deps
        .store
        .attachments_of(&[src_id])
        .await?
        .remove(&src_id)
        .unwrap_or_default();
    Ok(ForwardRequest {
        source_message: src_id,
        source_channel: src_channel,
        snapshot: snapshot_of(&src, &atts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::repo::UploadState;
    use sr_core::{AttachmentId, UserId};

    fn voice(ct: Option<&str>, ms: Option<u32>, wave: Option<&str>, size: u64) -> Attachment {
        Attachment {
            id: AttachmentId::new(1),
            channel_id: ChannelId::new(2),
            uploader: UserId::new(3),
            filename: "voice-message.ogg".into(),
            size,
            key: "k".into(),
            declared_content_type: None,
            content_type: ct.map(str::to_owned),
            description: None,
            spoiler: false,
            state: UploadState::Ready,
            message_id: None,
            expires_at_ms: None,
            created_at_ms: 0,
            duration_ms: ms,
            waveform: wave.map(str::to_owned),
        }
    }

    fn check(a: Attachment) -> bool {
        validate_voice(&[(a, None)]).is_ok()
    }

    #[test]
    fn voice_message_validation_table() {
        let wave = BASE64.encode(&[1u8, 2, 3, 200]);
        assert!(check(voice(Some("audio/ogg"), Some(5_000), Some(&wave), 1000)));
        assert!(check(voice(Some("audio/ogg"), Some(120_000), Some(&wave), 1000)));
        // Wrong type, too long, missing pieces, bad waveform, oversized.
        assert!(!check(voice(Some("audio/mpeg"), Some(5_000), Some(&wave), 1000)));
        assert!(!check(voice(None, Some(5_000), Some(&wave), 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(120_001), Some(&wave), 1000)));
        assert!(!check(voice(Some("audio/ogg"), None, Some(&wave), 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(5_000), None, 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(5_000), Some("!!not base64"), 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(5_000), Some(&BASE64.encode(&[0u8; 257])), 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(5_000), Some(""), 1000)));
        assert!(!check(voice(Some("audio/ogg"), Some(5_000), Some(&wave), MAX_VOICE_BYTES + 1)));
        // Not exactly one attachment.
        let a = voice(Some("audio/ogg"), Some(5_000), Some(&wave), 1000);
        assert!(validate_voice(&[]).is_err());
        assert!(validate_voice(&[(a.clone(), None), (a, None)]).is_err());
    }

    #[test]
    fn voice_meta_is_copied_from_the_request() {
        let r = AttachmentRef {
            id: None,
            uploaded_filename: None,
            description: None,
            duration_secs: Some(2.5),
            waveform: Some("AAEC".into()),
        };
        let mut ups = vec![(voice(None, None, None, 1), None)];
        apply_voice_meta(&mut ups, &[&r]);
        assert_eq!(ups[0].0.duration_ms, Some(2500));
        assert_eq!(ups[0].0.waveform.as_deref(), Some("AAEC"));
        let bad = AttachmentRef {
            duration_secs: Some(f64::NAN),
            ..r
        };
        let mut ups = vec![(voice(None, None, None, 1), None)];
        apply_voice_meta(&mut ups, &[&bad]);
        assert_eq!(ups[0].0.duration_ms, None);
    }

    #[test]
    fn allowed_mentions_rules() {
        let b = |parse: &[&str], users: &[&str], roles: &[&str]| AllowedMentionsBody {
            parse: parse.iter().map(|s| (*s).to_owned()).collect(),
            users: users.iter().map(|s| (*s).to_owned()).collect(),
            roles: roles.iter().map(|s| (*s).to_owned()).collect(),
            replied_user: false,
        };
        assert!(validate_allowed_mentions(&b(&["users"], &[], &[])).is_ok());
        assert!(validate_allowed_mentions(&b(&[], &["1", "2"], &["3"])).is_ok());
        assert!(validate_allowed_mentions(&b(&["users"], &["1"], &[])).is_err());
        assert!(validate_allowed_mentions(&b(&["roles"], &[], &["1"])).is_err());
        assert!(validate_allowed_mentions(&b(&[], &["1", "1"], &[])).is_err());
        let many: Vec<String> = (1..=101).map(|n| n.to_string()).collect();
        let big = AllowedMentionsBody {
            users: many,
            ..AllowedMentionsBody::default()
        };
        assert!(validate_allowed_mentions(&big).is_err());
    }
}
