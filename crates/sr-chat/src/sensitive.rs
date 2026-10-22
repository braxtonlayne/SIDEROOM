//! Per-viewer sensitive-media handling (D-230, H-33). One function decides what a viewer's
//! `sensitive_media` settings do to the attachments of one message; the REST decorators and the
//! gateway's per-session serialisation both call it, so a message looks the same on both paths.
//!
//! Nothing scans: the two signals are what the platform already knows.
//! * an image or video attachment in a DM from someone who is not a friend (`others`);
//! * an image or video attachment in an age-restricted channel (`servers`).
//!
//! `friends` has no signal until a scanner hook exists. `blur` sets the spoiler flag (a
//! `SPOILER_` file name, how the client reads it) and `block` strips the URL and sets
//! `sr_filtered: true`; nothing is deleted. The author and bots are never filtered.

use crate::social::safety::{MediaFilter, SensitiveMedia};
use serde_json::{Value, json};

/// The viewer's relation to the author of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// The viewer wrote it, or the author is a bot: never filtered.
    Exempt,
    Friend,
    Other,
}

/// What the message sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelCtx {
    /// A DM or group DM.
    pub dm: bool,
    /// The channel (or a thread's parent) is age-restricted.
    pub age_restricted: bool,
}

/// An attachment as seen by [`apply`]; implemented for the REST object and for a JSON value.
pub trait Attachment {
    /// `Content-Type`, when known.
    fn content_type(&self) -> Option<&str>;
    fn filename(&self) -> &str;
    /// Set the spoiler flag for this viewer.
    fn blur(&mut self);
    /// Withhold the file: no URL, `sr_filtered: true`.
    fn block(&mut self);
}

/// An image or video, by declared type and then by extension.
pub fn is_image_or_video(content_type: Option<&str>, filename: &str) -> bool {
    if let Some(ct) = content_type {
        let ct = ct.to_ascii_lowercase();
        if ct.starts_with("image/") || ct.starts_with("video/") {
            return true;
        }
        if ct != "application/octet-stream" {
            return false;
        }
    }
    let ext = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "mp4" | "webm" | "mov" | "mkv"
    )
}

/// The setting that acts on a message in `channel` from someone `relation` away, if a signal fires.
pub fn setting_for(
    prefs: &SensitiveMedia,
    relation: Relation,
    channel: &ChannelCtx,
) -> Option<MediaFilter> {
    if relation == Relation::Exempt {
        return None;
    }
    let f = if channel.dm {
        match relation {
            Relation::Other => prefs.others,
            // `friends` has no signal yet (D-230).
            _ => return None,
        }
    } else if channel.age_restricted {
        prefs.servers
    } else {
        return None;
    };
    (f != MediaFilter::Show).then_some(f)
}

/// Apply the viewer's settings to the attachments of one message. Returns whether anything changed.
pub fn apply<A: Attachment>(
    prefs: &SensitiveMedia,
    relation: Relation,
    channel: &ChannelCtx,
    attachments: &mut [A],
) -> bool {
    let Some(filter) = setting_for(prefs, relation, channel) else {
        return false;
    };
    let mut changed = false;
    for a in attachments {
        if !is_image_or_video(a.content_type(), a.filename()) {
            continue;
        }
        match filter {
            MediaFilter::Blur => a.blur(),
            MediaFilter::Block => a.block(),
            MediaFilter::Show => continue,
        }
        changed = true;
    }
    changed
}

/// The spoiler name of a file (`SPOILER_` once).
pub fn spoiler_name(name: &str) -> String {
    if name.starts_with("SPOILER_") {
        name.to_owned()
    } else {
        format!("SPOILER_{name}")
    }
}

/// JSON attachments (the gateway payloads).
impl Attachment for Value {
    fn content_type(&self) -> Option<&str> {
        self.get("content_type").and_then(Value::as_str)
    }
    fn filename(&self) -> &str {
        self.get("filename").and_then(Value::as_str).unwrap_or("")
    }
    fn blur(&mut self) {
        let name = spoiler_name(self.filename());
        if let Some(o) = self.as_object_mut() {
            o.insert("filename".into(), json!(name));
        }
    }
    fn block(&mut self) {
        if let Some(o) = self.as_object_mut() {
            o.insert("url".into(), json!(""));
            o.remove("proxy_url");
            o.insert("sr_filtered".into(), json!(true));
        }
    }
}

/// Filter the `attachments` array of a message payload in place.
pub fn apply_to_message(
    prefs: &SensitiveMedia,
    relation: Relation,
    channel: &ChannelCtx,
    message: &mut Value,
) -> bool {
    match message.get_mut("attachments").and_then(Value::as_array_mut) {
        Some(list) => apply(prefs, relation, channel, list),
        None => false,
    }
}

/// MOD-AGE-004 (spec 7, "Previews"): whether the embeds and link previews of a message must stay behind when it is
/// copied from `source_restricted` to a place with `destination_restricted`. Only an age-restricted place sits
/// behind the age gate, so a copy into a place without it (a followed announcement channel, a forward) carries the
/// text but no previews. A copy into another age-restricted place keeps them, and nothing is stripped when the
/// source is not restricted.
pub const fn embeds_stay_behind(source_restricted: bool, destination_restricted: bool) -> bool {
    source_restricted && !destination_restricted
}

#[cfg(test)]
mod tests {
    use super::*;
    use MediaFilter::{Block, Blur, Show};

    fn prefs(friends: MediaFilter, others: MediaFilter, servers: MediaFilter) -> SensitiveMedia {
        SensitiveMedia {
            friends,
            others,
            servers,
        }
    }
    fn msg() -> Value {
        json!({"attachments": [
            {"id": "1", "filename": "cat.png", "content_type": "image/png", "url": "/cdn/a"},
            {"id": "2", "filename": "notes.txt", "content_type": "text/plain", "url": "/cdn/b"},
            {"id": "3", "filename": "clip.bin", "content_type": "application/octet-stream", "url": "/cdn/c"},
            {"id": "4", "filename": "clip.mp4", "url": "/cdn/d"},
        ]})
    }
    const DM: ChannelCtx = ChannelCtx {
        dm: true,
        age_restricted: false,
    };
    const NSFW: ChannelCtx = ChannelCtx {
        dm: false,
        age_restricted: true,
    };
    const PLAIN: ChannelCtx = ChannelCtx {
        dm: false,
        age_restricted: false,
    };

    #[test]
    fn embeds_stay_behind_only_when_the_gate_does_not_follow() {
        assert!(embeds_stay_behind(true, false));
        assert!(!embeds_stay_behind(true, true));
        assert!(!embeds_stay_behind(false, false));
        assert!(!embeds_stay_behind(false, true));
    }

    #[test]
    fn every_setting_times_every_signal() {
        let cases = [
            (
                "others blur, stranger DM",
                prefs(Show, Blur, Show),
                Relation::Other,
                DM,
                Some(Blur),
            ),
            (
                "others block, stranger DM",
                prefs(Show, Block, Show),
                Relation::Other,
                DM,
                Some(Block),
            ),
            (
                "others show",
                prefs(Show, Show, Show),
                Relation::Other,
                DM,
                None,
            ),
            (
                "friend DM has no signal",
                prefs(Block, Block, Block),
                Relation::Friend,
                DM,
                None,
            ),
            (
                "servers blur, nsfw channel",
                prefs(Show, Show, Blur),
                Relation::Other,
                NSFW,
                Some(Blur),
            ),
            (
                "servers block, nsfw channel",
                prefs(Show, Show, Block),
                Relation::Friend,
                NSFW,
                Some(Block),
            ),
            (
                "servers show",
                prefs(Show, Block, Show),
                Relation::Other,
                NSFW,
                None,
            ),
            (
                "ordinary channel has no signal",
                prefs(Block, Block, Block),
                Relation::Other,
                PLAIN,
                None,
            ),
            (
                "author never filtered in a DM",
                prefs(Block, Block, Block),
                Relation::Exempt,
                DM,
                None,
            ),
            (
                "author never filtered in nsfw",
                prefs(Block, Block, Block),
                Relation::Exempt,
                NSFW,
                None,
            ),
        ];
        for (name, p, rel, ch, expect) in cases {
            assert_eq!(setting_for(&p, rel, &ch), expect, "{name}");
        }
    }

    #[test]
    fn blur_sets_the_spoiler_flag_and_block_strips_the_url_for_media_only() {
        let mut m = msg();
        assert!(apply_to_message(
            &prefs(Show, Blur, Show),
            Relation::Other,
            &DM,
            &mut m
        ));
        let a = m["attachments"].as_array().unwrap();
        assert_eq!(a[0]["filename"], "SPOILER_cat.png");
        assert_eq!(a[1]["filename"], "notes.txt", "documents are left alone");
        assert_eq!(a[2]["filename"], "clip.bin", "unknown bytes are not media");
        assert_eq!(
            a[3]["filename"], "SPOILER_clip.mp4",
            "an extension counts without a type"
        );
        assert_eq!(a[0]["url"], "/cdn/a");

        let mut m = msg();
        assert!(apply_to_message(
            &prefs(Show, Block, Show),
            Relation::Other,
            &DM,
            &mut m
        ));
        let a = m["attachments"].as_array().unwrap();
        assert_eq!(
            (a[0]["url"].as_str(), a[0]["sr_filtered"].as_bool()),
            (Some(""), Some(true))
        );
        assert_eq!(a[1]["url"], "/cdn/b");
        assert!(a[1].get("sr_filtered").is_none());
        // Nothing is deleted: the attachment is still listed with its id.
        assert_eq!(a[0]["id"], "1");
    }

    #[test]
    fn a_message_without_attachments_is_untouched() {
        let mut m = json!({"content": "hi"});
        assert!(!apply_to_message(
            &prefs(Block, Block, Block),
            Relation::Other,
            &DM,
            &mut m
        ));
        assert_eq!(m, json!({"content": "hi"}));
    }
}
