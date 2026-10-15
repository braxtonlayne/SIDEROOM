//! The scheduler: one pass that dispatches everything that is due. `sr-server` runs it from a recurring job every
//! few seconds (`userfeat.tick`), so a restart loses nothing: due rows simply wait in the database.
//!
//! Each kind of work has its own cadence: scheduled messages and reminders every pass, RSS feeds as they come
//! due, the library feed once a minute, birthdays every five minutes. Failures of one kind never stop another.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sr_net::SafeClient;
use url::Url;

use crate::UserFeatures;
use crate::config::render_template;
use crate::features::{BIRTHDAYS_ID, LIBRARY_FEED_ID, RSS_ID, SCHEDULED_ID};
use crate::poster::{Card, Poster, SendFailure};
use crate::rss::{self, Entry, MAX_POSTS_PER_RUN};

/// How long a delivery is retried (transient failures) before the row is given up.
const GIVE_UP_AFTER_MS: i64 = 3_600_000;
const RETRY_AFTER_MS: i64 = 60_000;
const LIBRARY_EVERY: Duration = Duration::from_secs(60);
const BIRTHDAYS_EVERY: Duration = Duration::from_secs(300);
const BATCH: i64 = 25;
const ORANGE: u32 = 0xEE802F;
const PINK: u32 = 0xE91E63;

/// What one pass did (for logs and tests).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickReport {
    pub scheduled_sent: usize,
    pub scheduled_failed: usize,
    pub reminders_sent: usize,
    pub reminders_failed: usize,
    pub feeds_checked: usize,
    pub feed_posts: usize,
    pub library_posts: usize,
    pub birthdays: usize,
    pub roles_removed: usize,
}

pub struct Ticker {
    uf: UserFeatures,
    poster: Arc<dyn Poster>,
    http: Option<Arc<SafeClient>>,
    last: Mutex<HashMap<&'static str, Instant>>,
}

impl Ticker {
    pub fn new(uf: UserFeatures, poster: Arc<dyn Poster>, http: Option<Arc<SafeClient>>) -> Self {
        Self {
            uf,
            poster,
            http,
            last: Mutex::new(HashMap::new()),
        }
    }

    fn due(&self, name: &'static str, every: Duration, force: bool) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let go = force
            || last
                .get(name)
                .is_none_or(|t| now.duration_since(*t) >= every);
        if go {
            last.insert(name, now);
        }
        go
    }

    /// One pass. `all` ignores the cadences (tests, and the first pass after a boot).
    pub async fn run_once(&self, all: bool) -> TickReport {
        let mut r = TickReport::default();
        log("scheduled messages", self.scheduled(&mut r).await);
        log("reminders", self.reminders(&mut r).await);
        log("rss", self.feeds(&mut r).await);
        if self.due("library", LIBRARY_EVERY, all) {
            log("library feed", self.library(&mut r).await);
        }
        if self.due("birthdays", BIRTHDAYS_EVERY, all) {
            log("birthdays", self.birthdays(&mut r).await);
        }
        r
    }

    async fn scheduled(&self, r: &mut TickReport) -> crate::UfResult<()> {
        for row in self.uf.due_scheduled(BATCH).await? {
            if !self.uf.enabled(SCHEDULED_ID, row.guild_id).await? {
                self.fail_scheduled(&row, "Scheduled messages are turned off in this server.", r)
                    .await?;
                continue;
            }
            match self
                .poster
                .send_as(row.author_id, row.channel_id, &row.content)
                .await
            {
                Ok(message) => {
                    if self
                        .uf
                        .settle_scheduled(row.id, Some(message), None)
                        .await?
                    {
                        r.scheduled_sent += 1;
                    }
                }
                Err(SendFailure::Permanent(why)) => self.fail_scheduled(&row, &why, r).await?,
                Err(SendFailure::Retry(why)) => {
                    if row.late_ms > GIVE_UP_AFTER_MS {
                        self.fail_scheduled(&row, &why, r).await?;
                    } else {
                        self.uf.postpone_scheduled(row.id, RETRY_AFTER_MS).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn fail_scheduled(
        &self,
        row: &crate::scheduled::DueScheduled,
        why: &str,
        r: &mut TickReport,
    ) -> crate::UfResult<()> {
        if self.uf.settle_scheduled(row.id, None, Some(why)).await? {
            r.scheduled_failed += 1;
            let preview: String = row.content.chars().take(120).collect();
            let card = Card {
                title: Some("A scheduled message was not sent".into()),
                description: Some(format!("{why}\n\n> {preview}")),
                color: 0xE74C3C,
                ..Card::default()
            };
            // Best effort: the row already says what happened.
            let _ = self.poster.dm(row.author_id, card).await;
        }
        Ok(())
    }

    async fn reminders(&self, r: &mut TickReport) -> crate::UfResult<()> {
        for row in self.uf.due_reminders(50).await? {
            let link = row
                .message_id
                .zip(row.channel_id)
                .map(|(m, c)| match row.guild_id {
                    Some(g) => format!("/channels/{g}/{c}/{m}"),
                    None => format!("/channels/@me/{c}/{m}"),
                });
            let card = Card {
                title: Some("Reminder".into()),
                description: Some(if row.note.trim().is_empty() {
                    if link.is_some() {
                        "You asked to be reminded about a message.".to_owned()
                    } else {
                        "You asked to be reminded.".to_owned()
                    }
                } else {
                    row.note.clone()
                }),
                url: link,
                color: 0x3498DB,
                ..Card::default()
            };
            match self.poster.dm(row.user_id, card).await {
                Ok(_) => {
                    self.uf.settle_reminder(row.id, true).await?;
                    r.reminders_sent += 1;
                }
                Err(e) if row.late_ms > GIVE_UP_AFTER_MS => {
                    tracing::warn!(reminder = row.id, error = %e, "reminder could not be delivered");
                    self.uf.settle_reminder(row.id, false).await?;
                    r.reminders_failed += 1;
                }
                Err(_) => self.uf.postpone_reminder(row.id, RETRY_AFTER_MS).await?,
            }
        }
        Ok(())
    }

    async fn feeds(&self, r: &mut TickReport) -> crate::UfResult<()> {
        let Some(http) = &self.http else {
            return Ok(());
        };
        for feed in self.uf.due_feeds(10).await? {
            if !self.uf.enabled(RSS_ID, Some(feed.guild_id)).await? {
                // Switched off: look again later without fetching.
                self.uf
                    .settle_feed(&feed, None, Some("the RSS module is turned off"))
                    .await?;
                continue;
            }
            r.feeds_checked += 1;
            let fetched = match Url::parse(&feed.url) {
                Ok(url) => rss::fetch_feed(http, &url).await,
                Err(_) => Err(rss::FeedError::Parse("bad address".into())),
            };
            match fetched {
                Err(e) => {
                    self.uf
                        .settle_feed(&feed, None, Some(&e.to_string()))
                        .await?;
                }
                Ok(parsed) => {
                    let fresh = self.uf.mark_seen(feed.id, &parsed.entries).await?;
                    if feed.seeded {
                        r.feed_posts += self.post_entries(&feed, &parsed.title, fresh).await;
                    }
                    let title = Some(parsed.title.as_str()).filter(|t| !t.is_empty());
                    self.uf.settle_feed(&feed, title, None).await?;
                }
            }
        }
        Ok(())
    }

    /// Post the new entries that pass the feed's filters, oldest first, at most [`MAX_POSTS_PER_RUN`].
    async fn post_entries(
        &self,
        feed: &rss::DueFeed,
        feed_title: &str,
        fresh: Vec<Entry>,
    ) -> usize {
        // Feeds list newest first: keep the newest few, then post them in time order.
        let mut picked: Vec<Entry> = fresh
            .into_iter()
            .filter(|e| rss::passes_filters(e, &feed.include_keywords, &feed.exclude_keywords))
            .take(MAX_POSTS_PER_RUN)
            .collect();
        picked.reverse();
        let source = if feed_title.is_empty() {
            feed.title.as_str()
        } else {
            feed_title
        };
        let mut posted = 0;
        for e in picked {
            let card = Card {
                title: Some(e.title.clone()).filter(|t| !t.is_empty()),
                description: Some(e.summary.clone()).filter(|t| !t.is_empty()),
                url: e.link.clone(),
                color: ORANGE,
                thumbnail: e.image.clone(),
                footer: Some(
                    [Some(source), e.author.as_deref()]
                        .into_iter()
                        .flatten()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{b7} "),
                )
                .filter(|s| !s.is_empty()),
                ..Card::default()
            };
            match self.poster.post(feed.channel_id, card).await {
                Ok(_) => posted += 1,
                Err(err) => {
                    tracing::warn!(feed = feed.id, error = %err, "feed entry could not be posted");
                    break;
                }
            }
        }
        posted
    }

    async fn library(&self, r: &mut TickReport) -> crate::UfResult<()> {
        for guild in self.uf.guilds_with(LIBRARY_FEED_ID).await? {
            match self.uf.run_library_feed(&*self.poster, guild).await {
                Ok(n) => r.library_posts += n,
                Err(e) => tracing::warn!(guild, error = %e, "library feed failed"),
            }
        }
        Ok(())
    }

    async fn birthdays(&self, r: &mut TickReport) -> crate::UfResult<()> {
        for guild in self.uf.guilds_with(BIRTHDAYS_ID).await? {
            let cfg = self.uf.settings(BIRTHDAYS_ID, Some(guild)).await?;
            let Some(channel) = cfg.id("channel") else {
                continue;
            };
            let template = cfg
                .text("message")
                .unwrap_or("Happy birthday, {user}!")
                .to_owned();
            let role = cfg.id("role");
            for c in self.uf.birthdays_today(guild).await? {
                if !self.uf.mark_announced(guild, c.user_id, role).await? {
                    continue;
                }
                let mention = format!("<@{}>", c.user_id);
                let age = c.age.map(|a| a.to_string()).unwrap_or_default();
                let text = render_template(
                    &template,
                    &[("user", &mention), ("name", &c.name), ("age", &age)],
                );
                let card = Card {
                    content: text,
                    color: PINK,
                    ping_users: vec![c.user_id],
                    ..Card::default()
                };
                match self.poster.post(channel, card).await {
                    Ok(_) => {
                        r.birthdays += 1;
                        if let Some(role) = role {
                            self.poster.add_role(guild, c.user_id, role).await;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(guild, error = %e, "birthday announcement failed");
                        self.uf.unmark_announced(guild, c.user_id).await?;
                    }
                }
            }
        }
        for (guild, user, role) in self.uf.expired_birthday_roles(50).await? {
            self.poster.remove_role(guild, user, role).await;
            self.uf.clear_birthday_role(guild, user, role).await?;
            r.roles_removed += 1;
        }
        Ok(())
    }
}

fn log(what: &str, result: crate::UfResult<()>) {
    if let Err(e) = result {
        tracing::warn!(error = %e, "{what} pass failed");
    }
}
