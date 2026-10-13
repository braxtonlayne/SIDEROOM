//! Request and response shapes of the thread endpoints (Discord field names).

use crate::routes::guilds::dto::{ChannelObject, ThreadMemberObject};
use crate::routes::messages::model::{CreateMessageBody, MessageObject};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `POST /channels/{id}/messages/{id}/threads`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartFromMessageBody {
    pub name: String,
    /// Minutes of inactivity before archiving: 60, 1440, 4320 or 10080.
    pub auto_archive_duration: Option<u32>,
    /// Slowmode in seconds.
    pub rate_limit_per_user: Option<u32>,
}

/// `POST /channels/{id}/threads`. In a forum or media channel `message` is required
/// and becomes the post's first message.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartThreadBody {
    pub name: String,
    pub auto_archive_duration: Option<u32>,
    pub rate_limit_per_user: Option<u32>,
    /// 10 announcement, 11 public (default) or 12 private thread.
    #[serde(rename = "type")]
    pub kind: Option<i16>,
    /// Private threads: whether members may invite others.
    pub invitable: Option<bool>,
    /// Forum and media channels: the starter message (no attachments yet).
    pub message: Option<CreateMessageBody>,
    /// Forum and media channels: tag ids, at most 5.
    pub applied_tags: Option<Vec<String>>,
}

/// A thread channel object; for forum posts also the starter `message`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadResponse {
    #[serde(flatten)]
    pub channel: ChannelObject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<MessageObject>,
}

/// Thread listings: the threads plus the caller's memberships among them.
#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadList {
    pub threads: Vec<ChannelObject>,
    pub members: Vec<ThreadMemberObject>,
    pub has_more: bool,
}

/// `before` is the ISO8601 archive timestamp of the last thread of the previous page.
#[derive(Debug, Default, Deserialize)]
pub struct ArchivedQuery {
    pub before: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MembersQuery {
    pub after: Option<String>,
    pub limit: Option<usize>,
    /// Accepted for Discord compatibility; member objects are always user ids.
    pub with_member: Option<bool>,
}
