//! FTR-RSS-001 RSS/Atom poster.
//!
//! * [`parse_feed`] reads RSS 2.0, RSS 1.0 (RDF) and Atom 1.0 with `quick-xml`. Hostile input is bounded (size,
//!   depth, event count, field length, entries) and DTDs are refused, so there is no entity expansion.
//! * [`fetch_feed`] fetches through `sr_net::SafeClient`, the one SSRF-safe path for URLs that people supply:
//!   scheme, credentials, deny list, every resolved address and every redirect hop are checked.
//! * The first successful fetch of a feed only records what is already there (so adding a feed does not flood the
//!   channel); after that each new entry is posted once, newest at most [`MAX_POSTS_PER_RUN`] per run.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use sr_net::{NetError, Policy, SafeClient};
use url::Url;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::RSS_ID};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};

/// Largest feed document read.
pub const MAX_FEED_BYTES: usize = 1024 * 1024;
const MAX_EVENTS: usize = 200_000;
const MAX_DEPTH: usize = 16;
const MAX_FIELD_CHARS: usize = 20_000;
/// Entries read from one document (newest feeds list newest first).
pub const MAX_ENTRIES: usize = 100;
/// Posts per feed per run; the rest are marked seen.
pub const MAX_POSTS_PER_RUN: usize = 5;
/// Shortest and longest poll interval, seconds.
pub const MIN_INTERVAL_SECS: i32 = 300;
pub const MAX_INTERVAL_SECS: i32 = 86_400;
const SUMMARY_CHARS: usize = 300;
const MAX_KEYWORDS: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// `guid` / `id`, else the link, else the title: what makes an entry "the same" next time.
    pub id: String,
    pub title: String,
    pub link: Option<String>,
    /// Plain text, at most 300 characters.
    pub summary: String,
    pub published: Option<String>,
    pub author: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error("not a feed: {0}")]
    Parse(String),
    #[error(transparent)]
    Net(#[from] NetError),
}

fn bad(m: impl Into<String>) -> FeedError {
    FeedError::Parse(m.into())
}

/// Strip tags and decode the few entities feeds use, collapse whitespace and clip.
pub fn plain_text(html: &str, max: usize) -> String {
    let mut out = String::with_capacity(html.len().min(max * 2));
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        let mut cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
        cut.push('\u{2026}');
        cut
    }
}

fn http_url(v: &str) -> Option<String> {
    let t = v.trim();
    let u = Url::parse(t).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| t.chars().take(2048).collect())
}

fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref().eq_ignore_ascii_case(name))
        .map(|a| a.value.as_ref().to_owned())
}

/// What the parser is collecting text for.
struct Collect {
    field: &'static str,
    depth: usize,
    buf: String,
}

/// The field a child element of an entry feeds, by its qualified name.
fn field_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "title" => "title",
        "link" => "link",
        "guid" | "id" => "id",
        "description" | "summary" => "summary",
        "content" | "content:encoded" => "content",
        "pubdate" | "published" | "dc:date" => "published",
        "updated" => "updated",
        "dc:creator" => "creator",
        "author" => "author",
        _ => return None,
    })
}

/// Parse an RSS or Atom document.
pub fn parse_feed(bytes: &[u8]) -> Result<Feed, FeedError> {
    if bytes.len() > MAX_FEED_BYTES {
        return Err(bad("document is too large"));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let src = std::str::from_utf8(bytes).map_err(|_| bad("only UTF-8 is supported"))?;
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut feed = Feed::default();
    let mut cur: Option<Entry> = None;
    let mut entry_depth = 0usize;
    let mut depth = 0usize;
    let mut events = 0usize;
    let mut collect: Option<Collect> = None;
    let mut saw_root = false;
    // Per-entry scratch: fields that have fallbacks.
    let (mut content, mut updated, mut creator, mut author_text, mut summary) = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );

    loop {
        events += 1;
        if events > MAX_EVENTS {
            return Err(bad("too many elements"));
        }
        let ev = reader.read_event().map_err(|e| bad(e.to_string()))?;
        match ev {
            Event::Start(e) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(bad("nested too deeply"));
                }
                let name = e.name().as_ref().to_ascii_lowercase();
                if depth == 1 {
                    if !matches!(name.as_str(), "rss" | "feed" | "rdf:rdf") {
                        return Err(bad("the root element is not rss, feed or rdf"));
                    }
                    saw_root = true;
                }
                if cur.is_none() && matches!(name.as_str(), "item" | "entry") {
                    cur = Some(Entry::default());
                    entry_depth = depth;
                    content.clear();
                    updated.clear();
                    creator.clear();
                    author_text.clear();
                    summary.clear();
                } else if let Some(entry) = &mut cur {
                    if depth == entry_depth + 1 {
                        entry_child(entry, &name, &e);
                        if let Some(field) = field_of(&name) {
                            // Atom `<link href=.../>` has no text; the attribute was read above.
                            collect = Some(Collect {
                                field,
                                depth,
                                buf: String::new(),
                            });
                        }
                    } else if depth == entry_depth + 2 && name == "name" {
                        collect = Some(Collect {
                            field: "authorname",
                            depth,
                            buf: String::new(),
                        });
                    }
                } else if name == "title" && feed.title.is_empty() && depth <= 3 {
                    collect = Some(Collect {
                        field: "feedtitle",
                        depth,
                        buf: String::new(),
                    });
                }
            }
            Event::Empty(e) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if depth == 0 {
                    return Err(bad("empty document"));
                }
                if let Some(entry) = &mut cur
                    && depth == entry_depth
                {
                    entry_child(entry, &name, &e);
                }
            }
            Event::Text(t) => {
                if let Some(c) = &mut collect
                    && c.buf.chars().count() < MAX_FIELD_CHARS
                {
                    c.buf.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(c) = &mut collect
                    && c.buf.chars().count() < MAX_FIELD_CHARS
                {
                    c.buf.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(c) = &mut collect {
                    let name: &str = &r;
                    let piece = match name {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => r.resolve_char_ref().ok().flatten(),
                    };
                    if let Some(ch) = piece {
                        c.buf.push(ch);
                    }
                }
            }
            Event::DocType(_) => return Err(bad("DTDs are not allowed")),
            Event::End(_) => {
                if collect.as_ref().is_some_and(|c| c.depth == depth)
                    && let Some(c) = collect.take()
                {
                    {
                        let v = c.buf.trim().to_owned();
                        match (c.field, cur.as_mut()) {
                            ("feedtitle", None) => feed.title = plain_text(&v, 200),
                            ("title", Some(e)) => e.title = plain_text(&v, 200),
                            ("link", Some(e)) => {
                                // Text links (RSS); an Atom href was set when the element opened.
                                if e.link.is_none() {
                                    e.link = http_url(&v);
                                }
                            }
                            ("id", Some(e)) => e.id = v.chars().take(500).collect(),
                            ("summary", Some(_)) => summary = v,
                            ("content", Some(_)) => content = v,
                            ("published", Some(e)) => {
                                e.published = Some(v).filter(|s| !s.is_empty())
                            }
                            ("updated", Some(_)) => updated = v,
                            ("creator", Some(_)) => creator = v,
                            ("author", Some(_)) => author_text = v,
                            ("authorname", Some(_)) => author_text = v,
                            _ => {}
                        }
                    }
                }
                if cur.is_some()
                    && depth == entry_depth
                    && let Some(mut entry) = cur.take()
                {
                    {
                        let body = if summary.trim().is_empty() {
                            &content
                        } else {
                            &summary
                        };
                        entry.summary = plain_text(body, SUMMARY_CHARS);
                        if entry.published.is_none() && !updated.trim().is_empty() {
                            entry.published = Some(updated.trim().to_owned());
                        }
                        let who = [&creator, &author_text]
                            .into_iter()
                            .map(|s| plain_text(s, 100))
                            .find(|s| !s.is_empty());
                        entry.author = who;
                        if entry.id.is_empty() {
                            entry.id = entry.link.clone().unwrap_or_else(|| entry.title.clone());
                        }
                        if !entry.id.is_empty() && feed.entries.len() < MAX_ENTRIES {
                            feed.entries.push(entry);
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(bad("no feed found"));
    }
    Ok(feed)
}

/// Attributes that carry a link or an image: Atom `<link href>`, `<enclosure url>`, `media:thumbnail`,
/// `media:content`.
fn entry_child(entry: &mut Entry, name: &str, e: &BytesStart<'_>) {
    match name {
        "link" => {
            if let Some(href) = attr(e, "href") {
                let rel = attr(e, "rel").unwrap_or_else(|| "alternate".into());
                if rel == "alternate" && entry.link.is_none() {
                    entry.link = http_url(&href);
                }
            }
        }
        "enclosure" => {
            let is_image = attr(e, "type").is_some_and(|t| t.starts_with("image/"));
            if is_image && entry.image.is_none() {
                entry.image = attr(e, "url").and_then(|u| http_url(&u));
            }
        }
        "media:thumbnail" => {
            if entry.image.is_none() {
                entry.image = attr(e, "url").and_then(|u| http_url(&u));
            }
        }
        "media:content" => {
            let image = attr(e, "medium").is_some_and(|m| m == "image")
                || attr(e, "type").is_some_and(|t| t.starts_with("image/"));
            if image && entry.image.is_none() {
                entry.image = attr(e, "url").and_then(|u| http_url(&u));
            }
        }
        _ => {}
    }
}

/// Parse and check a feed URL against the SSRF policy (syntax only; the fetch checks the addresses).
pub fn check_feed_url(policy: &Policy, raw: &str) -> UfResult<Url> {
    let url = Url::parse(raw.trim()).map_err(|_| invalid("url is not a valid web address"))?;
    policy
        .check_url(&url)
        .map_err(|e| invalid(format!("that address cannot be used: {e}")))?;
    Ok(url)
}

/// Fetch and parse a feed through the SSRF-safe client.
pub async fn fetch_feed(client: &SafeClient, url: &Url) -> Result<Feed, FeedError> {
    let resp = client
        .get(
            url,
            "application/atom+xml, application/rss+xml, application/xml;q=0.9, text/xml;q=0.8, */*;q=0.1",
            MAX_FEED_BYTES,
        )
        .await?;
    parse_feed(&resp.body)
}

/// Stable short hash of an entry id.
pub fn entry_hash(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Keyword filters: an entry passes when it matches any `include` word (or there are none) and no `exclude`
/// word. Matching is case-insensitive over title and summary.
pub fn passes_filters(entry: &Entry, include: &[String], exclude: &[String]) -> bool {
    let hay = format!("{} {}", entry.title, entry.summary).to_lowercase();
    let hit = |w: &String| hay.contains(&w.to_lowercase());
    (include.is_empty() || include.iter().any(hit)) && !exclude.iter().any(hit)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct RssFeed {
    pub id: String,
    pub channel_id: String,
    pub url: String,
    pub title: String,
    pub include_keywords: Vec<String>,
    pub exclude_keywords: Vec<String>,
    pub poll_interval_secs: i32,
    pub enabled: bool,
    pub last_checked_at: Option<String>,
    /// `pending`, `ok` or the last error.
    pub last_status: String,
    pub failures: i32,
    pub created_at: String,
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, channel_id::text AS channel_id, url, title, include_keywords, exclude_keywords, \
             poll_interval_secs, enabled, ",
            iso!("last_checked_at"),
            " AS last_checked_at, last_status, failures, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RssFeedInput {
    pub url: String,
    /// Channel snowflake the entries are posted to.
    pub channel_id: String,
    #[serde(default)]
    pub include_keywords: Vec<String>,
    #[serde(default)]
    pub exclude_keywords: Vec<String>,
    /// Seconds between checks (300 to 86400). Defaults to 1800.
    #[serde(default)]
    pub poll_interval_secs: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct RssFeedPatch {
    pub channel_id: Option<String>,
    pub include_keywords: Option<Vec<String>>,
    pub exclude_keywords: Option<Vec<String>>,
    pub poll_interval_secs: Option<i32>,
    pub enabled: Option<bool>,
}

/// A feed that is due for a check.
#[derive(Debug, Clone, FromRow)]
pub struct DueFeed {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub url: String,
    pub title: String,
    pub include_keywords: Vec<String>,
    pub exclude_keywords: Vec<String>,
    pub poll_interval_secs: i32,
    pub seeded: bool,
    pub failures: i32,
}

pub fn clean_keywords(words: &[String]) -> UfResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let w = w.trim();
        if w.is_empty() {
            continue;
        }
        if w.chars().count() > 50 {
            return Err(invalid("keywords must be 50 characters or fewer"));
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(w)) {
            out.push(w.to_owned());
        }
    }
    if out.len() > MAX_KEYWORDS {
        return Err(invalid(format!("at most {MAX_KEYWORDS} keywords")));
    }
    Ok(out)
}

fn clean_interval(secs: Option<i32>) -> UfResult<i32> {
    let v = secs.unwrap_or(1800);
    if !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&v) {
        return Err(invalid(format!(
            "poll_interval_secs must be {MIN_INTERVAL_SECS} to {MAX_INTERVAL_SECS}"
        )));
    }
    Ok(v)
}

/// Delay before the next check after `failures` failures in a row: the interval, doubled each time, up to a day.
pub fn backoff_secs(interval: i32, failures: i32) -> i64 {
    let shift = u32::try_from(failures.clamp(0, 10)).unwrap_or(10);
    (i64::from(interval) << shift).min(i64::from(MAX_INTERVAL_SECS))
}

impl UserFeatures {
    pub async fn list_feeds(&self, guild: i64) -> UfResult<Vec<RssFeed>> {
        self.require(RSS_ID, Some(guild)).await?;
        Ok(sqlx::query_as::<_, RssFeed>(concat!(
            "SELECT ",
            cols!(),
            " FROM rss_feeds WHERE guild_id = $1 ORDER BY id"
        ))
        .bind(guild)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Add a feed. `url` must already have passed [`check_feed_url`].
    pub async fn add_feed(
        &self,
        guild: i64,
        channel: i64,
        by: i64,
        url: &Url,
        input: &RssFeedInput,
    ) -> UfResult<RssFeed> {
        self.require(RSS_ID, Some(guild)).await?;
        let cfg = self.settings(RSS_ID, Some(guild)).await?;
        let include = clean_keywords(&input.include_keywords)?;
        let exclude = clean_keywords(&input.exclude_keywords)?;
        let interval = clean_interval(input.poll_interval_secs)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rss_feeds WHERE guild_id = $1")
            .bind(guild)
            .fetch_one(&self.pool)
            .await?;
        let max = cfg.int("max_feeds", 10);
        if count >= max {
            return Err(UfError::Conflict(format!(
                "A server can follow {max} feeds."
            )));
        }
        let inserted = sqlx::query_as::<_, RssFeed>(concat!(
            "INSERT INTO rss_feeds (id, guild_id, channel_id, url, include_keywords, exclude_keywords, \
             poll_interval_secs, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING ",
            cols!()
        ))
        .bind(self.next_id())
        .bind(guild)
        .bind(channel)
        .bind(url.as_str())
        .bind(&include)
        .bind(&exclude)
        .bind(interval)
        .bind(by)
        .fetch_one(&self.pool)
        .await;
        match inserted {
            Ok(f) => Ok(f),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UfError::Conflict(
                "That feed is already posted to that channel.".into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn patch_feed(
        &self,
        guild: i64,
        id: i64,
        channel: Option<i64>,
        patch: &RssFeedPatch,
    ) -> UfResult<RssFeed> {
        let include = patch
            .include_keywords
            .as_deref()
            .map(clean_keywords)
            .transpose()?;
        let exclude = patch
            .exclude_keywords
            .as_deref()
            .map(clean_keywords)
            .transpose()?;
        let interval = patch
            .poll_interval_secs
            .map(|v| clean_interval(Some(v)))
            .transpose()?;
        sqlx::query_as::<_, RssFeed>(concat!(
            "UPDATE rss_feeds SET channel_id = COALESCE($3, channel_id), \
             include_keywords = COALESCE($4, include_keywords), exclude_keywords = COALESCE($5, exclude_keywords), \
             poll_interval_secs = COALESCE($6, poll_interval_secs), enabled = COALESCE($7, enabled), \
             failures = CASE WHEN $7 IS TRUE THEN 0 ELSE failures END, \
             next_check_at = CASE WHEN $7 IS TRUE THEN now() ELSE next_check_at END \
             WHERE guild_id = $1 AND id = $2 RETURNING ",
            cols!()
        ))
        .bind(guild)
        .bind(id)
        .bind(channel)
        .bind(include)
        .bind(exclude)
        .bind(interval)
        .bind(patch.enabled)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("feed"))
    }

    pub async fn delete_feed(&self, guild: i64, id: i64) -> UfResult<()> {
        let n = sqlx::query("DELETE FROM rss_feeds WHERE guild_id = $1 AND id = $2")
            .bind(guild)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 1 {
            Ok(())
        } else {
            Err(UfError::NotFound("feed"))
        }
    }

    /// Feeds whose check is due, in servers that still run the module.
    pub async fn due_feeds(&self, limit: i64) -> UfResult<Vec<DueFeed>> {
        Ok(sqlx::query_as::<_, DueFeed>(
            "SELECT id, guild_id, channel_id, url, title, include_keywords, exclude_keywords, \
                    poll_interval_secs, seeded, failures \
             FROM rss_feeds WHERE enabled AND next_check_at <= now() ORDER BY next_check_at LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Record a check: `error` is `None` for a good fetch.
    pub async fn settle_feed(
        &self,
        feed: &DueFeed,
        title: Option<&str>,
        error: Option<&str>,
    ) -> UfResult<()> {
        match error {
            None => {
                sqlx::query(
                    "UPDATE rss_feeds SET last_checked_at = now(), last_status = 'ok', failures = 0, seeded = TRUE, \
                     title = COALESCE($3, title), \
                     next_check_at = now() + make_interval(secs => $2::double precision) WHERE id = $1",
                )
                .bind(feed.id)
                .bind(f64::from(feed.poll_interval_secs))
                .bind(title)
                .execute(&self.pool)
                .await?;
            }
            Some(e) => {
                let failures = feed.failures.saturating_add(1);
                sqlx::query(
                    "UPDATE rss_feeds SET last_checked_at = now(), last_status = $3, failures = $4, \
                     next_check_at = now() + make_interval(secs => $2::double precision) WHERE id = $1",
                )
                .bind(feed.id)
                .bind(backoff_secs(feed.poll_interval_secs, failures) as f64)
                .bind(e.chars().take(200).collect::<String>())
                .bind(failures)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Record `entries` as seen and return the ones that were new, in the order given.
    pub async fn mark_seen(&self, feed: i64, entries: &[Entry]) -> UfResult<Vec<Entry>> {
        let mut fresh = Vec::new();
        for e in entries {
            let inserted = sqlx::query(
                "INSERT INTO rss_seen (feed_id, entry_hash) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(feed)
            .bind(entry_hash(&e.id))
            .execute(&self.pool)
            .await?
            .rows_affected();
            if inserted == 1 {
                fresh.push(e.clone());
            }
        }
        sqlx::query(
            "DELETE FROM rss_seen WHERE feed_id = $1 AND seen_at < now() - interval '90 days'",
        )
        .bind(feed)
        .execute(&self.pool)
        .await?;
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:media="http://search.yahoo.com/mrss/" xmlns:content="http://purl.org/rss/1.0/modules/content/">
  <channel>
    <title>Example &amp; Co</title>
    <link>https://example.test/</link>
    <item>
      <title>Chapter 12 is out</title>
      <link>https://example.test/c/12</link>
      <guid isPermaLink="false">post-12</guid>
      <description>&lt;p&gt;The &lt;b&gt;big&lt;/b&gt; one &amp;amp; more.&lt;/p&gt;</description>
      <pubDate>Mon, 06 Oct 2025 10:00:00 GMT</pubDate>
      <dc:creator>Ayu</dc:creator>
      <media:thumbnail url="https://example.test/t.png"/>
    </item>
    <item>
      <title>Second</title>
      <link>javascript:alert(1)</link>
      <enclosure url="https://example.test/e.jpg" type="image/jpeg" length="1"/>
      <content:encoded><![CDATA[<p>From content</p>]]></content:encoded>
    </item>
  </channel>
</rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Blog</title>
  <entry>
    <title>Hello</title>
    <link rel="self" href="https://example.test/self"/>
    <link rel="alternate" href="https://example.test/hello"/>
    <id>urn:uuid:1</id>
    <updated>2025-10-06T10:00:00Z</updated>
    <summary>Short</summary>
    <author><name>Bo</name><email>bo@example.test</email></author>
  </entry>
</feed>"#;

    #[test]
    fn parses_rss() {
        let f = parse_feed(RSS.as_bytes()).unwrap();
        assert_eq!(f.title, "Example & Co");
        assert_eq!(f.entries.len(), 2);
        let a = &f.entries[0];
        assert_eq!(a.id, "post-12");
        assert_eq!(a.title, "Chapter 12 is out");
        assert_eq!(a.link.as_deref(), Some("https://example.test/c/12"));
        assert_eq!(a.summary, "The big one & more.");
        assert_eq!(a.author.as_deref(), Some("Ayu"));
        assert_eq!(a.image.as_deref(), Some("https://example.test/t.png"));
        assert_eq!(
            a.published.as_deref(),
            Some("Mon, 06 Oct 2025 10:00:00 GMT")
        );
        let b = &f.entries[1];
        // A non-http link is dropped, the title becomes the id, content fills in the summary.
        assert_eq!(b.link, None);
        assert_eq!(b.id, "Second");
        assert_eq!(b.summary, "From content");
        assert_eq!(b.image.as_deref(), Some("https://example.test/e.jpg"));
    }

    #[test]
    fn parses_atom() {
        let f = parse_feed(ATOM.as_bytes()).unwrap();
        assert_eq!(f.title, "Atom Blog");
        let e = &f.entries[0];
        assert_eq!(e.id, "urn:uuid:1");
        assert_eq!(e.link.as_deref(), Some("https://example.test/hello"));
        assert_eq!(e.author.as_deref(), Some("Bo"));
        assert_eq!(e.published.as_deref(), Some("2025-10-06T10:00:00Z"));
        assert_eq!(e.summary, "Short");
    }

    #[test]
    fn refuses_hostile_or_wrong_documents() {
        assert!(parse_feed(b"<html><body>hi</body></html>").is_err());
        assert!(parse_feed(b"").is_err());
        assert!(parse_feed(b"\xff\xfe").is_err());
        let dtd = r#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY a "aaaa">]><rss><channel/></rss>"#;
        assert!(parse_feed(dtd.as_bytes()).is_err());
        let deep = format!("<rss>{}{}</rss>", "<a>".repeat(40), "</a>".repeat(40));
        assert!(parse_feed(deep.as_bytes()).is_err());
        let big = vec![b'x'; MAX_FEED_BYTES + 1];
        assert!(parse_feed(&big).is_err());
    }

    #[test]
    fn entry_count_is_capped() {
        let items: String = (0..150)
            .map(|n| format!("<item><title>t{n}</title><guid>g{n}</guid></item>"))
            .collect();
        let doc = format!("<rss><channel>{items}</channel></rss>");
        assert_eq!(
            parse_feed(doc.as_bytes()).unwrap().entries.len(),
            MAX_ENTRIES
        );
    }

    #[test]
    fn filters_match_title_or_summary() {
        let e = Entry {
            title: "Big Release".into(),
            summary: "includes a Sale".into(),
            ..Entry::default()
        };
        let w = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(passes_filters(&e, &[], &[]));
        assert!(passes_filters(&e, &w(&["release"]), &[]));
        assert!(passes_filters(&e, &w(&["nope", "SALE"]), &[]));
        assert!(!passes_filters(&e, &w(&["nope"]), &[]));
        assert!(!passes_filters(&e, &[], &w(&["sale"])));
        assert!(!passes_filters(&e, &w(&["release"]), &w(&["sale"])));
    }

    #[test]
    fn hashes_and_backoff() {
        assert_eq!(entry_hash("a"), entry_hash("a"));
        assert_ne!(entry_hash("a"), entry_hash("b"));
        assert_eq!(entry_hash("a").len(), 32);
        assert_eq!(backoff_secs(1800, 0), 1800);
        assert_eq!(backoff_secs(1800, 1), 3600);
        assert_eq!(backoff_secs(1800, 10), 86_400);
    }

    #[test]
    fn keywords_are_cleaned() {
        let w = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            clean_keywords(&w(&[" a ", "A", "", "b"])).unwrap(),
            vec!["a", "b"]
        );
        assert!(clean_keywords(&[String::from("x").repeat(51)]).is_err());
        let many: Vec<String> = (0..21).map(|n| format!("k{n}")).collect();
        assert!(clean_keywords(&many).is_err());
    }

    #[test]
    fn urls_go_through_the_policy() {
        let p = Policy::default();
        assert!(check_feed_url(&p, "https://example.test/feed.xml").is_ok());
        assert!(check_feed_url(&p, "ftp://example.test/feed.xml").is_err());
        assert!(check_feed_url(&p, "http://127.0.0.1/feed").is_err());
        assert!(check_feed_url(&p, "http://user:pw@example.test/feed").is_err());
        assert!(check_feed_url(&p, "not a url").is_err());
    }

    #[test]
    fn plain_text_strips_and_clips() {
        assert_eq!(plain_text("<p>a&nbsp;b</p><br/>c", 100), "a b c");
        let long = "word ".repeat(200);
        let t = plain_text(&long, 20);
        assert_eq!(t.chars().count(), 20);
        assert!(t.ends_with('\u{2026}'));
    }
}
