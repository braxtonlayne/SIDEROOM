//! The three-way decision and the authority table (design sections 2.2, 2.3 and 11.4).
//!
//! For one field of one mapped object the reconciler holds SIDE ROOM's current value, Discord's current value
//! and the value both agreed on last ([`FieldState`]). [`decide`] turns those into what to do. It is pure: the
//! callers read the sides, call it, and perform the [`Decision`]. Values are compared as each side expresses
//! them; `to_discord` converts a SIDE ROOM value into Discord's expression for the one comparison that crosses
//! sides (a channel name becomes Discord's lowercase-with-hyphens form, and so on).

use serde_json::Value;

use super::types::{FieldState, ObjectKind};

/// Who owns a field (design section 2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Either side may change it; the other follows.
    TwoWay,
    /// SIDE ROOM owns it. A Discord edit is reverted, logged and offered as a proposal; never applied here.
    SideRoom,
    /// SIDE ROOM only and never sent (SIDE ROOM-only bits and channel properties).
    SideRoomPrivate,
}

/// The authority of `field` on an object of `kind` (whole_guild mode). Unknown fields default to SIDE ROOM's
/// authority: a field nobody classified can never be written from Discord.
pub fn field_authority(kind: ObjectKind, field: &str) -> Authority {
    match (kind, field) {
        // Roles.
        (
            ObjectKind::Role,
            "name" | "color" | "hoist" | "mentionable" | "unicode_emoji" | "icon",
        ) => Authority::TwoWay,
        // Role order is an authorization input (section 2.3), so it is SIDE ROOM's.
        (ObjectKind::Role | ObjectKind::RoleOrder, "position" | "order") => Authority::SideRoom,
        (ObjectKind::Role, "permissions") => Authority::SideRoom,
        // Overwrites are SIDE ROOM's, for roles and for `@everyone`.
        (ObjectKind::Overwrites, _) => Authority::SideRoom,
        // SIDE ROOM-only channel properties never leave.
        (
            ObjectKind::Channel,
            "watch_party_room" | "music_allowed" | "music_bitrate_kbps" | "live_tv" | "shelves",
        ) => Authority::SideRoomPrivate,
        (ObjectKind::Channel | ObjectKind::ChannelOrder, "permission_overwrites") => {
            Authority::SideRoom
        }
        (
            ObjectKind::Channel | ObjectKind::ChannelOrder,
            "name"
            | "topic"
            | "nsfw"
            | "rate_limit_per_user"
            | "parent_id"
            | "position"
            | "bitrate"
            | "user_limit"
            | "video_quality_mode"
            | "default_auto_archive_duration"
            | "default_thread_rate_limit"
            | "default_reaction_emoji"
            | "default_sort_order"
            | "default_forum_layout"
            | "available_tags",
        ) => Authority::TwoWay,
        (
            ObjectKind::Thread,
            "name"
            | "archived"
            | "locked"
            | "auto_archive_duration"
            | "rate_limit_per_user"
            | "applied_tags",
        ) => Authority::TwoWay,
        (ObjectKind::ForumTags, _) => Authority::TwoWay,
        _ => Authority::SideRoom,
    }
}

/// The three values of one field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldView<'a> {
    pub sr_now: &'a Value,
    pub discord_now: &'a Value,
    /// The last agreed state; `None` when the pair was never compared (the first look).
    pub synced: Option<&'a FieldState>,
}

/// What to do about one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Neither side moved (or Discord's value is an echo of the last agreed one): nothing to do.
    Nothing,
    /// Only SIDE ROOM changed: push SIDE ROOM's value.
    PushToDiscord,
    /// Only Discord changed a two-way field: apply it here as the bridge actor.
    ApplyToSr,
    /// Only Discord changed a SIDE ROOM-authoritative field: put SIDE ROOM's value back, log a conflict and
    /// offer a proposal.
    Revert,
    /// Both changed to the same value: record it as the new agreed state.
    Converged,
    /// Both changed to different values: SIDE ROOM wins (push it), and a `structure_conflict` row offers
    /// "Take Discord's".
    Conflict,
}

impl Decision {
    /// The decision changes something on Discord.
    pub const fn writes_discord(&self) -> bool {
        matches!(self, Self::PushToDiscord | Self::Revert | Self::Conflict)
    }

    /// The decision changes something on SIDE ROOM. Only [`Decision::ApplyToSr`] ever does, and it is only
    /// produced for [`Authority::TwoWay`] fields.
    pub const fn writes_sr(&self) -> bool {
        matches!(self, Self::ApplyToSr)
    }

    /// The decision needs a `bridge_conflicts` row (BRG-SYN-002 writes them).
    pub const fn is_conflict(&self) -> bool {
        matches!(self, Self::Revert | Self::Conflict)
    }
}

/// Decide one field. `to_discord` expresses a SIDE ROOM value the way Discord would hold it.
///
/// A [`Authority::SideRoomPrivate`] field is never compared: nothing is sent and nothing comes back.
pub fn decide(
    authority: Authority,
    view: &FieldView<'_>,
    to_discord: impl Fn(&Value) -> Value,
) -> Decision {
    if authority == Authority::SideRoomPrivate {
        return Decision::Nothing;
    }
    let Some(synced) = view.synced else {
        // First look. If the sides already agree there is nothing to do; otherwise SIDE ROOM's value wins for a
        // SIDE ROOM-authoritative field and, for a two-way field, the seed has already picked a side (plan), so
        // a later disagreement is a plain SIDE ROOM push.
        return if to_discord(view.sr_now) == *view.discord_now {
            Decision::Converged
        } else {
            Decision::PushToDiscord
        };
    };
    let sr_changed = view.sr_now != &synced.sr;
    let discord_changed = view.discord_now != &synced.discord;
    match (sr_changed, discord_changed) {
        (false, false) => Decision::Nothing,
        (true, false) => Decision::PushToDiscord,
        (false, true) => match authority {
            Authority::TwoWay => Decision::ApplyToSr,
            Authority::SideRoom => Decision::Revert,
            Authority::SideRoomPrivate => Decision::Nothing,
        },
        (true, true) => {
            if to_discord(view.sr_now) == *view.discord_now {
                Decision::Converged
            } else {
                Decision::Conflict
            }
        }
    }
}

/// Discord's rule for a text channel name, applied to a SIDE ROOM name for comparison only: lowercase, spaces to
/// hyphens, trimmed. (Voice channels and categories keep their case on Discord; see [`discord_name_for`].)
pub fn discord_text_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .flat_map(char::to_lowercase)
        .collect()
}

/// The name Discord would hold for a SIDE ROOM channel of type `kind`: text-like types are normalized, voice,
/// stage and category names keep their form.
pub fn discord_name_for(kind: i16, name: &str) -> String {
    match kind {
        0 | 5 | 10 | 11 | 12 | 15 | 16 => discord_text_name(name),
        _ => name.trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn st(sr: Value, d: Value) -> FieldState {
        FieldState { sr, discord: d }
    }

    fn id(v: &Value) -> Value {
        v.clone()
    }

    fn run(auth: Authority, sr: Value, d: Value, synced: Option<FieldState>) -> Decision {
        decide(
            auth,
            &FieldView {
                sr_now: &sr,
                discord_now: &d,
                synced: synced.as_ref(),
            },
            id,
        )
    }

    #[test]
    fn the_three_way_table() {
        let s = || Some(st(json!("a"), json!("a")));
        use Authority::*;
        assert_eq!(run(TwoWay, json!("a"), json!("a"), s()), Decision::Nothing);
        assert_eq!(
            run(TwoWay, json!("b"), json!("a"), s()),
            Decision::PushToDiscord
        );
        assert_eq!(
            run(TwoWay, json!("a"), json!("b"), s()),
            Decision::ApplyToSr
        );
        assert_eq!(run(SideRoom, json!("a"), json!("b"), s()), Decision::Revert);
        assert_eq!(
            run(TwoWay, json!("b"), json!("b"), s()),
            Decision::Converged
        );
        assert_eq!(run(TwoWay, json!("b"), json!("c"), s()), Decision::Conflict);
        assert_eq!(
            run(SideRoom, json!("b"), json!("c"), s()),
            Decision::Conflict
        );
    }

    #[test]
    fn sr_wins_both_changed_and_discord_never_writes_a_sideroom_authoritative_field() {
        let s = || Some(st(json!(1), json!(1)));
        for d in [json!(0), json!(2), json!(null)] {
            let r = run(Authority::SideRoom, json!(1), d, s());
            assert!(!r.writes_sr(), "{r:?}");
            assert!(r.writes_discord());
        }
        // Only a two-way field is ever applied here.
        for auth in [Authority::SideRoom, Authority::SideRoomPrivate] {
            for sr in [json!(1), json!(2)] {
                for d in [json!(1), json!(3)] {
                    assert!(!run(auth, sr.clone(), d, s()).writes_sr());
                }
            }
        }
    }

    #[test]
    fn an_echo_of_the_agreed_state_does_nothing() {
        // Discord shows the value the bridge itself pushed (its own expression), SIDE ROOM is unchanged.
        let s = Some(st(json!("General Chat"), json!("general-chat")));
        let d = decide(
            Authority::TwoWay,
            &FieldView {
                sr_now: &json!("General Chat"),
                discord_now: &json!("general-chat"),
                synced: s.as_ref(),
            },
            |v| json!(discord_text_name(v.as_str().unwrap())),
        );
        assert_eq!(d, Decision::Nothing);
    }

    #[test]
    fn first_look_agrees_or_sideroom_pushes() {
        assert_eq!(
            run(Authority::TwoWay, json!("a"), json!("a"), None),
            Decision::Converged
        );
        assert_eq!(
            run(Authority::SideRoom, json!("a"), json!("z"), None),
            Decision::PushToDiscord
        );
    }

    #[test]
    fn private_fields_are_never_compared() {
        assert_eq!(
            run(Authority::SideRoomPrivate, json!(1), json!(2), None),
            Decision::Nothing
        );
    }

    #[test]
    fn authority_table() {
        use ObjectKind::*;
        assert_eq!(field_authority(Role, "name"), Authority::TwoWay);
        assert_eq!(field_authority(Role, "permissions"), Authority::SideRoom);
        assert_eq!(field_authority(Role, "position"), Authority::SideRoom);
        assert_eq!(field_authority(RoleOrder, "order"), Authority::SideRoom);
        assert_eq!(field_authority(Overwrites, "allow"), Authority::SideRoom);
        assert_eq!(field_authority(Channel, "topic"), Authority::TwoWay);
        assert_eq!(field_authority(Channel, "parent_id"), Authority::TwoWay);
        assert_eq!(
            field_authority(Channel, "permission_overwrites"),
            Authority::SideRoom
        );
        assert_eq!(
            field_authority(Channel, "watch_party_room"),
            Authority::SideRoomPrivate
        );
        assert_eq!(field_authority(Thread, "archived"), Authority::TwoWay);
        // A field nobody classified is SIDE ROOM's.
        assert_eq!(
            field_authority(Channel, "brand_new_field"),
            Authority::SideRoom
        );
        assert_eq!(
            field_authority(Role, "brand_new_field"),
            Authority::SideRoom
        );
    }

    #[test]
    fn discord_names() {
        assert_eq!(discord_text_name("  General Chat "), "general-chat");
        assert_eq!(discord_name_for(0, "Off Topic"), "off-topic");
        assert_eq!(discord_name_for(2, "Game Night "), "Game Night");
        assert_eq!(discord_name_for(4, "Text Rooms"), "Text Rooms");
    }
}
