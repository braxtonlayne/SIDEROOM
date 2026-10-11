//! `GET /guilds/{id}/messages/search` and `GET /channels/{id}/messages/search`
//! (LNG-SRC-003).
//!
//! The index never returns a channel outside the caller's allow-list. For guilds
//! the list comes from the 60-second [`ViewableChannelCache`](sr_search::ViewableChannelCache)
//! and drops NSFW channels unless the account confirmed its age; a single channel
//! is checked directly. Hits are loaded from the message store, so a message
//! deleted since indexing never appears.
//!
//! Parameters follow Discord (`content`, `author_id`, `mentions`, `has`,
//! `channel_id`, `pinned`, `author_type`, `sort_by`, `sort_order`, `limit`,
//! `offset`); list-valued ones take commas. `q` is the raw search-box syntax
//! (`from:`, `in:`, `has:`, `before:`, `during:`, `after:`, `pinned:`...).

use super::deps::SearchDeps;
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiQuery, MsgError, MsgResult, invalid, parse_id};
use crate::routes::messages::model::MessageObject;
use crate::routes::messages::render::{actor, render_many};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_chat::Message;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId};
use sr_search::{Access, SearchError, SearchRequest, Sort};
use utoipa::ToSchema;

pub const DEFAULT_LIMIT: usize = 25;
pub const MAX_LIMIT: usize = 25;
/// Discord caps the offset at 9975 (399 pages of 25).
pub const MAX_OFFSET: usize = 9975;

#[derive(Debug, Default, Deserialize)]
pub struct SearchQuery {
    /// Raw search-box text with inline filters.
    pub q: Option<String>,
    /// Words to find (Discord name for the text part).
    pub content: Option<String>,
    /// Comma-separated author ids.
    pub author_id: Option<String>,
    /// Comma-separated mentioned user ids.
    pub mentions: Option<String>,
    /// Comma-separated: `link`, `embed`, `file`, `image`, `video`, `sound`, `sticker`, `poll`.
    pub has: Option<String>,
    /// Comma-separated channel ids (guild search).
    pub channel_id: Option<String>,
    pub pinned: Option<bool>,
    /// Comma-separated: `user`, `bot`, `webhook`.
    pub author_type: Option<String>,
    /// `timestamp` (default) or `relevance`.
    pub sort_by: Option<String>,
    /// `desc` (default) or `asc`; applies to `timestamp`.
    pub sort_order: Option<String>,
    /// 1 to 25, default 25.
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchSnippet {
    pub text: String,
    /// Byte ranges into `text`.
    pub highlights: Vec<(usize, usize)>,
}

/// Discord search response: each result is a one-message group (no context
/// lines), flagged with `hit: true`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchResponse {
    pub total_results: usize,
    #[schema(value_type = Vec<Vec<MessageObject>>)]
    pub messages: Vec<Vec<Value>>,
    pub doing_deep_historical_index: bool,
}

fn csv(raw: &Option<String>) -> impl Iterator<Item = &str> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The search-box string for `q`: its inline filters plus the Discord-style
/// parameters. Values are validated by the query parser.
pub fn build_query(q: &SearchQuery) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(q.q.iter().cloned());
    parts.extend(q.content.iter().cloned());
    parts.extend(csv(&q.author_id).map(|v| format!("from:{v}")));
    parts.extend(csv(&q.mentions).map(|v| format!("mentions:{v}")));
    parts.extend(csv(&q.has).map(|v| format!("has:{v}")));
    parts.extend(csv(&q.channel_id).map(|v| format!("in:{v}")));
    parts.extend(csv(&q.author_type).map(|v| format!("authorType:{v}")));
    if let Some(p) = q.pinned {
        parts.push(format!("pinned:{p}"));
    }
    parts.join(" ")
}

fn sort_of(q: &SearchQuery) -> MsgResult<Sort> {
    let asc = match q.sort_order.as_deref() {
        None | Some("desc") => false,
        Some("asc") => true,
        Some(_) => {
            return Err(invalid(
                "sort_order",
                "ENUM_TYPE_BAD_VALUE",
                "Must be asc or desc.",
            ));
        }
    };
    match q.sort_by.as_deref() {
        None | Some("timestamp") => Ok(if asc { Sort::Oldest } else { Sort::Newest }),
        Some("relevance") => Ok(Sort::Relevance),
        Some(_) => Err(invalid(
            "sort_by",
            "ENUM_TYPE_BAD_VALUE",
            "Must be timestamp or relevance.",
        )),
    }
}

fn search_error(e: SearchError) -> MsgError {
    match e {
        SearchError::Query(q) => invalid("query", "SEARCH_QUERY_INVALID", &q.to_string()),
        SearchError::Index(m) => {
            tracing::error!(error = %m, "search index failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Run `req` and load the hit messages in rank order.
async fn run(
    search: &SearchDeps,
    messages: &MessagesDeps,
    viewer: sr_core::UserId,
    q: &SearchQuery,
    mut req: SearchRequest,
) -> MsgResult<SearchResponse> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(invalid(
            "limit",
            "NUMBER_TYPE_MAX",
            "Must be between 1 and 25.",
        ));
    }
    let offset = q.offset.unwrap_or(0);
    if offset > MAX_OFFSET {
        return Err(invalid(
            "offset",
            "NUMBER_TYPE_MAX",
            "Must be 9975 or less.",
        ));
    }
    let query = build_query(q);
    if query.trim().is_empty() {
        return Err(invalid(
            "content",
            "BASE_TYPE_REQUIRED",
            "Add search text or a filter.",
        ));
    }
    req.query = query;
    req.sort = sort_of(q)?;
    req.limit = limit;
    req.offset = offset;
    let page = search.index.search(&req).map_err(search_error)?;

    let mut loaded: Vec<(Message, sr_search::Snippet)> = Vec::with_capacity(page.hits.len());
    for hit in page.hits {
        if let Some(m) = messages
            .repo
            .get_message(hit.channel_id, hit.message_id)
            .await?
            .filter(Message::is_visible)
        {
            loaded.push((m, hit.snippet));
        }
    }
    let msgs: Vec<Message> = loaded.iter().map(|(m, _)| m.clone()).collect();
    let objects = render_many(messages, viewer, &msgs).await?;
    let groups = objects
        .into_iter()
        .zip(loaded)
        .map(|(obj, (_, snippet))| {
            let mut v = obj.to_value();
            v["hit"] = json!(true);
            v["sr_snippet"] = json!({"text": snippet.text, "highlights": snippet.highlights});
            vec![v]
        })
        .collect();
    Ok(SearchResponse {
        total_results: page.total,
        messages: groups,
        doing_deep_historical_index: false,
    })
}

/// Search a guild's messages across every channel the caller can read.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/messages/search",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("q" = Option<String>, Query, description = "Search-box syntax with inline filters"),
        ("content" = Option<String>, Query, description = "Text to find"),
        ("author_id" = Option<String>, Query, description = "Comma-separated user ids"),
        ("mentions" = Option<String>, Query, description = "Comma-separated user ids"),
        ("has" = Option<String>, Query, description = "link, embed, file, image, video, sound, sticker, poll"),
        ("channel_id" = Option<String>, Query, description = "Comma-separated channel ids"),
        ("pinned" = Option<bool>, Query, description = "Only pinned or only unpinned"),
        ("author_type" = Option<String>, Query, description = "user, bot, webhook"),
        ("sort_by" = Option<String>, Query, description = "timestamp (default) or relevance"),
        ("sort_order" = Option<String>, Query, description = "desc (default) or asc"),
        ("limit" = Option<usize>, Query, description = "1-25, default 25"),
        ("offset" = Option<usize>, Query, description = "0-9975"),
    ),
    responses(
        (status = 200, body = SearchResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn search_guild(
    p: Principal,
    Ext(search): Ext<SearchDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(guild_id): Path<String>,
    ApiQuery(q): ApiQuery<SearchQuery>,
) -> MsgResult<Json<SearchResponse>> {
    let user = actor(&p);
    let guild_id: GuildId = parse_id(&guild_id, ErrorCode::UnknownGuild)?;
    let channels = search
        .searchable_channels(&*messages.repo, user, guild_id, messages.now_ms())
        .await?;
    let mut req = SearchRequest::new(String::new(), Access::Channels(channels));
    req.guild_id = Some(guild_id);
    Ok(Json(run(&search, &messages, user, &q, req).await?))
}

/// Search one channel (or DM) the caller can read.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/messages/search",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("q" = Option<String>, Query, description = "Search-box syntax with inline filters"),
        ("content" = Option<String>, Query, description = "Text to find"),
        ("author_id" = Option<String>, Query, description = "Comma-separated user ids"),
        ("mentions" = Option<String>, Query, description = "Comma-separated user ids"),
        ("has" = Option<String>, Query, description = "link, embed, file, image, video, sound, sticker, poll"),
        ("pinned" = Option<bool>, Query, description = "Only pinned or only unpinned"),
        ("author_type" = Option<String>, Query, description = "user, bot, webhook"),
        ("sort_by" = Option<String>, Query, description = "timestamp (default) or relevance"),
        ("sort_order" = Option<String>, Query, description = "desc (default) or asc"),
        ("limit" = Option<usize>, Query, description = "1-25, default 25"),
        ("offset" = Option<usize>, Query, description = "0-9975"),
    ),
    responses(
        (status = 200, body = SearchResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn search_channel(
    p: Principal,
    Ext(search): Ext<SearchDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<SearchQuery>,
) -> MsgResult<Json<SearchResponse>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let acc = messages.access(user, channel_id).await?;
    acc.require(Permissions::READ_MESSAGE_HISTORY)?;
    // Age-restricted channels are closed to accounts that did not confirm 18+.
    let allowed = !acc.channel.nsfw || search.age_confirmed(user).await;
    let channels = if allowed { vec![channel_id] } else { vec![] };
    let mut req = SearchRequest::new(String::new(), Access::Channels(channels));
    req.guild_id = acc.channel.guild_id;
    Ok(Json(run(&search, &messages, user, &q, req).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_query_string() {
        let q = SearchQuery {
            content: Some("hello world".into()),
            author_id: Some("1, 2".into()),
            has: Some("image,link".into()),
            channel_id: Some("9".into()),
            pinned: Some(true),
            author_type: Some("bot".into()),
            mentions: Some("5".into()),
            ..Default::default()
        };
        assert_eq!(
            build_query(&q),
            "hello world from:1 from:2 mentions:5 has:image has:link in:9 authorType:bot pinned:true"
        );
        let raw = SearchQuery {
            q: Some("from:3 \"exact\"".into()),
            ..Default::default()
        };
        assert_eq!(build_query(&raw), "from:3 \"exact\"");
        assert_eq!(build_query(&SearchQuery::default()), "");
    }

    #[test]
    fn sorting() {
        let s = |by: Option<&str>, order: Option<&str>| {
            sort_of(&SearchQuery {
                sort_by: by.map(Into::into),
                sort_order: order.map(Into::into),
                ..Default::default()
            })
        };
        assert_eq!(s(None, None).unwrap(), Sort::Newest);
        assert_eq!(s(Some("timestamp"), Some("asc")).unwrap(), Sort::Oldest);
        assert_eq!(s(Some("relevance"), None).unwrap(), Sort::Relevance);
        assert!(s(Some("popularity"), None).is_err());
        assert!(s(None, Some("up")).is_err());
    }
}
