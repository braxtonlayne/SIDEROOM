//! `sr-cli seed --scale` (RDY-PERF-001): a dataset the size of the sizing claim, loaded with `COPY`.
//!
//! The full profile is 20,000 accounts, 500 guilds with a skewed membership, 5 million messages with
//! attachments and reactions, 2,000 library chapters and 300 cinema titles. Everything is derived from the
//! seed name and the counts (a splitmix64 stream, no extra crate), so a run is reproducible; the whole load
//! is ONE transaction, so a failed run leaves nothing behind and a second run with the same name finds its
//! marker account and stops (idempotent per seed name).
//!
//! Membership skew: guild of rank `r` (1 is the largest) has `max(3, 0.6 * accounts / r)` members, so the
//! first guild holds 60 % of the accounts and the 500th a couple of dozen. Messages follow the same skew
//! (a message lands in a guild with a probability proportional to its size, and inside it favours the first
//! channels). About 6 % of messages carry an attachment row and 10 % carry one to three reactions.
//!
//! The rows are plain `COPY` text, built by the functions below. They touch only tables the product reads;
//! nothing here talks to object storage (attachment and library rows point at keys that do not exist, which is
//! fine for database load and `EXPLAIN` work, and wrong for opening a page in the reader).

use std::fmt::Write as _;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use sqlx::{PgConnection, PgPool};

/// Counts of one seed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaleProfile {
    pub accounts: u32,
    pub guilds: u32,
    pub messages: u64,
    /// Library chapters in total (spread over series of ten chapters each).
    pub chapters: u32,
    /// Cinema titles.
    pub titles: u32,
}

impl ScaleProfile {
    /// The sizing claim of the roadmap (RDY-PERF-001).
    pub const FULL: Self = Self {
        accounts: 20_000,
        guilds: 500,
        messages: 5_000_000,
        chapters: 2_000,
        titles: 300,
    };
}

/// What a run did (all zero with `skipped` when the seed name was already loaded).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScaleReport {
    pub skipped: bool,
    pub accounts: u64,
    pub guilds: u64,
    pub channels: u64,
    pub memberships: u64,
    pub messages: u64,
    pub attachments: u64,
    pub reactions: u64,
    pub series: u64,
    pub chapters: u64,
    pub titles: u64,
    /// `(phase, seconds)` in load order.
    pub phases: Vec<(&'static str, f64)>,
}

/// Text channels per guild (plus one category).
pub const TEXT_CHANNELS: u32 = 5;
/// Messages carry an attachment with this chance (percent) ...
const ATTACHMENT_PCT: u64 = 6;
/// ... and one to three reactions with this chance (percent).
const REACTION_PCT: u64 = 10;
/// The window the messages are spread over, ending now.
const WINDOW_MS: i64 = 30 * 24 * 3600 * 1000;
const EMOJI: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "🔥"];
const GENRES: [&str; 6] = ["action", "drama", "comedy", "fantasy", "mystery", "romance"];
const SERIES_KINDS: [&str; 4] = ["manga", "comic", "webtoon", "manhwa"];
const TITLE_KINDS: [&str; 3] = ["movie", "series", "anime"];
const WORDS: [&str; 16] = [
    "morning", "anyone", "playing", "tonight", "released", "chapter", "stream", "watching",
    "agree", "thanks", "lol", "brb", "nice", "episode", "voice", "later",
];
/// Id ranges per kind, in milliseconds after the id base, so kinds never overlap.
const KIND_STRIDE_MS: i64 = 1 << 28;

/// A reproducible 64-bit stream (splitmix64).
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a over the seed name.
fn name_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A seed name is lower-case letters and digits, 1 to 16 long (it is part of every username).
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 16
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        bail!("the seed name must be 1 to 16 lower-case letters or digits");
    }
    Ok(())
}

/// Members of the guild of rank `rank` (1 based), at least 3 (or all accounts when fewer).
pub fn guild_size(accounts: u32, rank: u32) -> u32 {
    let skewed = (f64::from(accounts) * 0.6 / f64::from(rank)).round() as u32;
    skewed.max(3).min(accounts)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The accounts of one guild: `start + j * stride (mod accounts)` with the stride coprime to the account
/// count, so the first `size` values never repeat.
#[derive(Debug, Clone, Copy)]
struct Members {
    start: u64,
    stride: u64,
    size: u64,
    accounts: u64,
}

impl Members {
    fn new(accounts: u32, rank: u32) -> Self {
        let a = u64::from(accounts);
        let mut stride = u64::from(rank) * 2 + 1;
        while gcd(stride % a.max(1), a.max(1)) != 1 && a > 1 {
            stride += 2;
        }
        Self {
            start: (u64::from(rank) * 7919) % a.max(1),
            stride,
            size: u64::from(guild_size(accounts, rank)),
            accounts: a,
        }
    }

    /// Account index (0 based) of member `j`.
    fn account(&self, j: u64) -> u64 {
        (self.start + j * self.stride) % self.accounts
    }
}

/// Plan of one text channel in the message distribution.
struct ChannelSlot {
    guild: usize,
    /// Channel id.
    id: i64,
}

struct Plan {
    tag: u64,
    seed: u64,
    profile: ScaleProfile,
    id_base_ms: i64,
    now_ms: i64,
    members: Vec<Members>,
    slots: Vec<ChannelSlot>,
    /// Cumulative weights over `slots`.
    cumulative: Vec<u64>,
}

/// Where message `m` goes and who wrote it.
struct MessagePlan {
    id: i64,
    ts_ms: i64,
    slot: usize,
    author_j: u64,
    rnd: u64,
}

impl Plan {
    fn new(name: &str, profile: ScaleProfile, now_ms: i64) -> Self {
        let h = name_hash(name);
        let mut p = Self {
            tag: h & 0x3FF,
            seed: h,
            profile,
            id_base_ms: now_ms - 120 * 24 * 3600 * 1000,
            now_ms,
            members: Vec::new(),
            slots: Vec::new(),
            cumulative: Vec::new(),
        };
        p.members = (1..=profile.guilds)
            .map(|r| Members::new(profile.accounts, r))
            .collect();
        let mut total = 0u64;
        for (g, m) in p.members.iter().enumerate() {
            for c in 0..TEXT_CHANNELS {
                // The channel number inside the guild is the third kind of id slot of that guild.
                let id = p.sid_idx(
                    3,
                    g as u64 * u64::from(TEXT_CHANNELS + 1) + u64::from(c) + 1,
                );
                p.slots.push(ChannelSlot { guild: g, id });
                total += m.size * (60 / u64::from(c + 1));
                p.cumulative.push(total);
            }
        }
        p
    }

    /// Snowflake-shaped id of the `idx`-th row of `kind` (0 account, 1 guild, 2 reserved, 3 channel, 4 series,
    /// 5 edition, 6 chapter, 7 title). The seed name's hash sits in the worker and process bits, so two seed
    /// names do not collide.
    fn sid_idx(&self, kind: i64, idx: u64) -> i64 {
        let ts =
            (self.id_base_ms - SIDEROOM_EPOCH_MS) + kind * KIND_STRIDE_MS + (idx / 4096) as i64;
        ((ts as u64) << 22 | self.tag << 12 | (idx % 4096)) as i64
    }

    fn account_id(&self, i: u64) -> i64 {
        self.sid_idx(0, i)
    }

    fn guild_id(&self, g: u64) -> i64 {
        self.sid_idx(1, g)
    }

    /// Message `m` (deterministic).
    fn message(&self, m: u64) -> MessagePlan {
        let spacing = (WINDOW_MS / self.profile.messages.max(1) as i64).max(1);
        let ts_ms = self.now_ms - WINDOW_MS + m as i64 * spacing;
        let id =
            ((((ts_ms - SIDEROOM_EPOCH_MS) as u64) << 22) | self.tag << 12 | (m & 0xFFF)) as i64;
        let rnd = mix(self.seed ^ m.wrapping_mul(0x2545_F491_4F6C_DD1D));
        let total = *self.cumulative.last().unwrap_or(&1);
        let pick = rnd % total.max(1);
        let slot = self.cumulative.partition_point(|&c| c <= pick);
        let slot = slot.min(self.slots.len().saturating_sub(1));
        let size = self.members[self.slots[slot].guild].size.max(1);
        MessagePlan {
            id,
            ts_ms,
            slot,
            author_j: mix(rnd) % size,
            rnd,
        }
    }
}

/// First millisecond of the SIDE ROOM epoch (2026-01-01 UTC), as in `sr_core`.
const SIDEROOM_EPOCH_MS: i64 = sr_core::snowflake::SIDEROOM_EPOCH_MS;

/// `YYYY-MM-DD HH:MM:SS.mmm+00` for Unix milliseconds (civil-from-days, proleptic Gregorian).
pub fn ts(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let milli = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{milli:03}+00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn username(name: &str, i: u64) -> String {
    format!("{name}_{i:05}")
}

/// Stream `n` generated rows into one `COPY ... FROM STDIN`; `row` appends the text of row `i` including its
/// newline. Sends 1 MiB at a time.
async fn copy(
    conn: &mut PgConnection,
    statement: &str,
    n: u64,
    mut row: impl FnMut(u64, &mut String),
) -> Result<u64> {
    let mut c = conn
        .copy_in_raw(statement)
        .await
        .with_context(|| format!("starting {statement}"))?;
    let mut buf = String::with_capacity(1 << 20);
    for i in 0..n {
        row(i, &mut buf);
        if buf.len() >= 1 << 20 {
            c.send(buf.as_bytes()).await.context("sending COPY data")?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        c.send(buf.as_bytes()).await.context("sending COPY data")?;
    }
    c.finish().await.context("finishing COPY")
}

fn phase(label: &'static str, t0: Instant, report: &mut ScaleReport) {
    report.phases.push((label, t0.elapsed().as_secs_f64()));
}

/// Load the dataset. `password_hash` is stored for every account; `everyone_permissions` is the bit set of the
/// `@everyone` role. Returns early with `skipped` when this seed name was loaded before.
pub async fn seed_scale(
    pool: &PgPool,
    name: &str,
    profile: ScaleProfile,
    password_hash: &str,
    everyone_permissions: i64,
    now_ms: i64,
) -> Result<ScaleReport> {
    validate_name(name)?;
    if profile.accounts < 2 || profile.guilds < 1 || profile.messages < 1 {
        bail!("a scale seed needs at least 2 accounts, 1 guild and 1 message");
    }
    if profile.accounts > 1_000_000 || profile.guilds > 100_000 || profile.messages > 100_000_000 {
        bail!("that profile is far beyond the sizing claim; lower it");
    }
    let marker = username(name, 0);
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE username = $1)")
            .bind(&marker)
            .fetch_one(pool)
            .await
            .context("looking for an earlier run")?;
    if exists {
        return Ok(ScaleReport {
            skipped: true,
            ..Default::default()
        });
    }
    let plan = Plan::new(name, profile, now_ms);
    let mut report = ScaleReport::default();
    let mut tx = pool
        .begin()
        .await
        .context("starting the seed transaction")?;
    sqlx::query("SET LOCAL synchronous_commit = off")
        .execute(&mut *tx)
        .await?;
    // The message partitions this window needs (idempotent; a month that already exists is skipped).
    for t in [now_ms - WINDOW_MS, now_ms] {
        sqlx::query("SELECT sr_ensure_message_partition((to_timestamp($1::bigint / 1000.0) AT TIME ZONE 'UTC')::date)")
            .bind(t)
            .execute(&mut *tx)
            .await
            .context("ensuring message partitions")?;
    }

    let t0 = Instant::now();
    let hash = password_hash.replace('\\', "\\\\");
    let created = ts(plan.id_base_ms);
    copy(
        &mut tx,
        "COPY persons (id) FROM STDIN",
        u64::from(profile.accounts),
        |i, b| {
            let _ = writeln!(b, "{}", plan.account_id(i));
        },
    )
    .await?;
    report.accounts = copy(
        &mut tx,
        "COPY accounts (id, person_id, username, display_name, email, email_verified_at, password_hash, \
         created_at) FROM STDIN",
        u64::from(profile.accounts),
        |i, b| {
            let id = plan.account_id(i);
            let u = username(name, i);
            let _ = writeln!(b, "{id}\t{id}\t{u}\tScale {i}\t{u}@scale.example.test\t{created}\t{hash}\t{created}");
        },
    )
    .await?;
    phase("accounts", t0, &mut report);

    let t0 = Instant::now();
    report.guilds = copy(
        &mut tx,
        "COPY guilds (id, name, owner_id, created_at) FROM STDIN",
        u64::from(profile.guilds),
        |g, b| {
            let owner = plan.account_id(plan.members[g as usize].account(0));
            let _ = writeln!(
                b,
                "{}\tScale {name} {}\t{owner}\t{created}",
                plan.guild_id(g),
                g + 1
            );
        },
    )
    .await?;
    copy(
        &mut tx,
        "COPY roles (id, guild_id, name, position, permissions) FROM STDIN",
        u64::from(profile.guilds),
        |g, b| {
            let id = plan.guild_id(g);
            let _ = writeln!(b, "{id}\t{id}\t@everyone\t0\t{everyone_permissions}");
        },
    )
    .await?;
    // Channels: one category and TEXT_CHANNELS text channels per guild. The last message of each text channel
    // is known from a pre-pass over the generator, so the rows are written once.
    let mut last = vec![0i64; plan.slots.len()];
    for m in 0..profile.messages {
        let mp = plan.message(m);
        last[mp.slot] = mp.id;
    }
    let per_guild = u64::from(TEXT_CHANNELS + 1);
    report.channels = copy(
        &mut tx,
        "COPY channels (id, guild_id, type, name, position, parent_id, last_message_id, created_at) FROM STDIN",
        u64::from(profile.guilds) * per_guild,
        |k, b| {
            let (g, c) = (k / per_guild, k % per_guild);
            let guild = plan.guild_id(g);
            let cat = plan.sid_idx(3, g * per_guild);
            if c == 0 {
                let _ = writeln!(b, "{cat}\t{guild}\t4\tText Channels\t0\t\\N\t\\N\t{created}");
            } else {
                let slot = (g * u64::from(TEXT_CHANNELS) + c - 1) as usize;
                let lm = last[slot];
                let lm = if lm == 0 { "\\N".to_owned() } else { lm.to_string() };
                let _ = writeln!(
                    b,
                    "{}\t{guild}\t0\tchannel-{c}\t{}\t{cat}\t{lm}\t{created}",
                    plan.slots[slot].id,
                    c - 1
                );
            }
        },
    )
    .await?;
    // Memberships: guild by guild.
    let total_members: u64 = plan.members.iter().map(|m| m.size).sum();
    let mut offsets = Vec::with_capacity(plan.members.len() + 1);
    let mut acc = 0u64;
    for m in &plan.members {
        offsets.push(acc);
        acc += m.size;
    }
    report.memberships = copy(
        &mut tx,
        "COPY guild_members (guild_id, user_id, joined_at) FROM STDIN",
        total_members,
        |k, b| {
            let g = offsets.partition_point(|&o| o <= k) - 1;
            let j = k - offsets[g];
            let m = plan.members[g];
            let joined = ts(plan.id_base_ms + 1000 * (j as i64 % 86_400));
            let _ = writeln!(
                b,
                "{}\t{}\t{joined}",
                plan.guild_id(g as u64),
                plan.account_id(m.account(j))
            );
        },
    )
    .await?;
    phase("guilds, channels, members", t0, &mut report);

    // Messages, then the rows that hang on them.
    let t0 = Instant::now();
    report.messages = copy(
        &mut tx,
        "COPY messages (id, channel_id, guild_id, author_id, content, created_at) FROM STDIN",
        profile.messages,
        |m, b| {
            let mp = plan.message(m);
            let slot = &plan.slots[mp.slot];
            let author = plan.account_id(plan.members[slot.guild].account(mp.author_j));
            let w = |n: u64| WORDS[(mix(mp.rnd ^ n) % WORDS.len() as u64) as usize];
            let _ = writeln!(
                b,
                "{}\t{}\t{}\t{author}\t{} {} {} {} #{m}\t{}",
                mp.id,
                slot.id,
                plan.guild_id(slot.guild as u64),
                w(1),
                w(2),
                w(3),
                w(4),
                ts(mp.ts_ms)
            );
        },
    )
    .await?;
    phase("messages", t0, &mut report);

    let t0 = Instant::now();
    let with_attachment: Vec<u64> = (0..profile.messages)
        .filter(|&m| mix(plan.seed ^ m ^ 0xA77A) % 100 < ATTACHMENT_PCT)
        .collect();
    report.attachments = copy(
        &mut tx,
        "COPY attachments (id, message_id, uploader_id, filename, content_type, size_bytes, width, height, \
         storage_key, state, channel_id, created_at) FROM STDIN",
        with_attachment.len() as u64,
        |k, b| {
            let mp = plan.message(with_attachment[k as usize]);
            let slot = &plan.slots[mp.slot];
            let author = plan.account_id(plan.members[slot.guild].account(mp.author_j));
            let size = 20_000 + mp.rnd % 4_000_000;
            let _ = writeln!(
                b,
                "{id}\t{id}\t{author}\tscale-{id}.png\timage/png\t{size}\t800\t600\tscale/{name}/{id}.png\tready\t{ch}\t{t}",
                id = mp.id,
                ch = slot.id,
                t = ts(mp.ts_ms)
            );
        },
    )
    .await?;
    let with_reaction: Vec<u64> = (0..profile.messages)
        .filter(|&m| mix(plan.seed ^ m ^ 0x4EAC) % 100 < REACTION_PCT)
        .collect();
    // One to three reactions per chosen message, each a different emoji from a different member.
    let mut reaction_rows = 0u64;
    let mut starts = Vec::with_capacity(with_reaction.len());
    for &m in &with_reaction {
        starts.push(reaction_rows);
        reaction_rows += 1 + mix(plan.seed ^ m ^ 0x77) % 3;
    }
    report.reactions = copy(
        &mut tx,
        "COPY reactions (message_id, emoji_key, user_id, created_at) FROM STDIN",
        reaction_rows,
        |k, b| {
            let idx = starts.partition_point(|&s| s <= k) - 1;
            let n = k - starts[idx];
            let mp = plan.message(with_reaction[idx]);
            let slot = &plan.slots[mp.slot];
            let mem = plan.members[slot.guild];
            let user = plan.account_id(mem.account(mix(mp.rnd ^ (n + 1)) % mem.size.max(1)));
            let emoji = EMOJI[((mp.rnd >> 7) as usize + n as usize) % EMOJI.len()];
            let _ = writeln!(b, "{}\t{emoji}\t{user}\t{}", mp.id, ts(mp.ts_ms + 60_000));
        },
    )
    .await?;
    phase("attachments, reactions", t0, &mut report);

    // Library: series of ten chapters, one default edition each.
    let t0 = Instant::now();
    let series = profile.chapters.div_ceil(10);
    let author = plan.account_id(0);
    report.series = copy(
        &mut tx,
        "COPY reading_series (id, kind, title, status, year, synopsis, genres, published, published_at, \
         chapter_count, last_chapter_at, created_by, created_at) FROM STDIN",
        u64::from(series),
        |s, b| {
            let in_series = (profile.chapters as u64 - s * 10).min(10);
            let when = ts(now_ms - 86_400_000 * (s as i64 % 90));
            let _ = writeln!(
                b,
                "{}\t{}\tScale Series {name} {s}\tongoing\t{}\tGenerated for load tests.\t{{{}}}\ttrue\t{when}\t{in_series}\t{when}\t{author}\t{created}",
                plan.sid_idx(4, s),
                SERIES_KINDS[s as usize % SERIES_KINDS.len()],
                2000 + s % 25,
                GENRES[s as usize % GENRES.len()],
            );
        },
    )
    .await?;
    copy(
        &mut tx,
        "COPY reading_editions (id, series_id, language, label, is_default, created_at) FROM STDIN",
        u64::from(series),
        |s, b| {
            let _ = writeln!(
                b,
                "{}\t{}\ten\tEnglish\ttrue\t{created}",
                plan.sid_idx(5, s),
                plan.sid_idx(4, s)
            );
        },
    )
    .await?;
    report.chapters = copy(
        &mut tx,
        "COPY reading_chapters (id, series_id, edition_id, number, title, page_count, format, published, \
         published_at, created_by, created_at) FROM STDIN",
        u64::from(profile.chapters),
        |c, b| {
            let (s, n) = (c / 10, c % 10 + 1);
            let when = ts(now_ms - 3_600_000 * (c as i64 % 2000));
            let _ = writeln!(
                b,
                "{}\t{}\t{}\t{n}\tChapter {n}\t20\timages\ttrue\t{when}\t{author}\t{created}",
                plan.sid_idx(6, c),
                plan.sid_idx(4, s),
                plan.sid_idx(5, s),
            );
        },
    )
    .await?;
    // Cinema titles (no media: the catalogue only).
    report.titles = copy(
        &mut tx,
        "COPY video_titles (id, kind, title, sort_title, year, runtime_min, content_rating, synopsis, genres, \
         published, published_at, created_by, created_at) FROM STDIN",
        u64::from(profile.titles),
        |t, b| {
            let when = ts(now_ms - 86_400_000 * (t as i64 % 365));
            let title = format!("Scale Title {name} {t:04}");
            let _ = writeln!(
                b,
                "{}\t{}\t{title}\t{}\t{}\t{}\tPG-13\tGenerated for load tests.\t{{{}}}\ttrue\t{when}\t{author}\t{created}",
                plan.sid_idx(7, t),
                TITLE_KINDS[t as usize % TITLE_KINDS.len()],
                title.to_lowercase(),
                1980 + t % 45,
                80 + t % 90,
                GENRES[t as usize % GENRES.len()],
            );
        },
    )
    .await?;
    phase("library, cinema", t0, &mut report);

    tx.commit().await.context("committing the seed")?;
    // Fresh statistics so the planner (and EXPLAIN) see the real sizes.
    let t0 = Instant::now();
    for table in [
        "accounts",
        "guilds",
        "guild_members",
        "channels",
        "messages",
        "attachments",
        "reactions",
        "reading_series",
        "reading_chapters",
        "video_titles",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("ANALYZE {table}")))
            .execute(pool)
            .await
            .with_context(|| format!("analysing {table}"))?;
    }
    phase("analyze", t0, &mut report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_civil_utc() {
        assert_eq!(ts(0), "1970-01-01 00:00:00.000+00");
        assert_eq!(ts(SIDEROOM_EPOCH_MS), "2026-01-01 00:00:00.000+00");
        assert_eq!(
            ts(SIDEROOM_EPOCH_MS + 86_400_000 * 59 + 1234),
            "2026-03-01 00:00:01.234+00"
        );
        // 2028 is a leap year.
        assert_eq!(
            ts(SIDEROOM_EPOCH_MS + 86_400_000 * (730 + 59)),
            "2028-02-29 00:00:00.000+00"
        );
    }

    #[test]
    fn names_are_checked() {
        assert!(validate_name("scale1").is_ok());
        for bad in ["", "Scale", "a_b", "x".repeat(17).as_str(), "a b"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn membership_is_skewed_and_never_repeats() {
        assert_eq!(guild_size(20_000, 1), 12_000);
        assert_eq!(guild_size(20_000, 500), 24);
        assert_eq!(guild_size(20_000, 5_000), 3);
        for (accounts, rank) in [
            (20_000u32, 1u32),
            (20_000, 2),
            (20_000, 77),
            (40, 1),
            (40, 5),
            (7, 3),
        ] {
            let m = Members::new(accounts, rank);
            let mut seen = std::collections::HashSet::new();
            for j in 0..m.size {
                assert!(m.account(j) < u64::from(accounts));
                assert!(
                    seen.insert(m.account(j)),
                    "repeat in guild {rank} of {accounts}"
                );
            }
        }
    }

    #[test]
    fn the_plan_is_reproducible_and_ids_do_not_collide() {
        let profile = ScaleProfile {
            accounts: 40,
            guilds: 5,
            messages: 500,
            chapters: 20,
            titles: 5,
        };
        let now = SIDEROOM_EPOCH_MS + 200 * 86_400_000;
        let a = Plan::new("t1", profile, now);
        let b = Plan::new("t1", profile, now);
        let c = Plan::new("t2", profile, now);
        let mut ids = std::collections::HashSet::new();
        for m in 0..profile.messages {
            let (x, y) = (a.message(m), b.message(m));
            assert_eq!((x.id, x.slot, x.author_j), (y.id, y.slot, y.author_j));
            assert!(ids.insert(x.id), "message id repeats");
            assert_ne!(x.id, c.message(m).id, "two seed names share a message id");
        }
        let mut other = std::collections::HashSet::new();
        for i in 0..40 {
            assert!(other.insert(a.account_id(i)));
        }
        for g in 0..5 {
            assert!(other.insert(a.guild_id(g)));
        }
        for s in &a.slots {
            assert!(other.insert(s.id));
        }
    }

    #[test]
    fn messages_favour_the_big_guilds() {
        let profile = ScaleProfile {
            accounts: 2_000,
            guilds: 20,
            messages: 20_000,
            chapters: 10,
            titles: 1,
        };
        let p = Plan::new("skew", profile, SIDEROOM_EPOCH_MS + 90 * 86_400_000);
        let mut per_guild = vec![0u32; 20];
        for m in 0..profile.messages {
            per_guild[p.slots[p.message(m).slot].guild] += 1;
        }
        assert!(per_guild[0] > per_guild[9] * 3, "{per_guild:?}");
        assert!(per_guild.iter().all(|&n| n > 0), "{per_guild:?}");
    }
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
    }

    async fn count(pool: &PgPool, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// RDY-PERF-001: a tiny scale loads through COPY with every foreign key satisfied, keeps the skew and the
    /// message/author/guild relations, and a second run with the same name changes nothing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_tiny_scale_seed_loads_once_and_keeps_its_shape() {
        let db = sr_db::testing::TestDb::new().await.unwrap();
        let pool = db.pool().clone();
        let profile = ScaleProfile {
            accounts: 40,
            guilds: 5,
            messages: 300,
            chapters: 25,
            titles: 7,
        };
        let perms = i64::try_from(sr_chat::perms::default_everyone_permissions().bits()).unwrap();
        let r = seed_scale(&pool, "tiny", profile, "hash", perms, now_ms())
            .await
            .unwrap();
        assert!(!r.skipped);
        assert_eq!((r.accounts, r.guilds, r.messages), (40, 5, 300));
        assert_eq!(r.channels, 5 * 6);
        assert_eq!(r.memberships, 24 + 12 + 8 + 6 + 5);
        assert_eq!((r.series, r.chapters, r.titles), (3, 25, 7));
        assert!(r.attachments > 0 && r.reactions > 0, "{r:?}");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM accounts WHERE split_part(username, '_', 1) = 'tiny'"
            )
            .await,
            40
        );
        assert_eq!(count(&pool, "SELECT count(*) FROM messages").await, 300);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM attachments").await,
            r.attachments as i64
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM reactions").await,
            r.reactions as i64
        );
        // Membership skew: the first guild is the biggest.
        let sizes: Vec<i64> = sqlx::query_scalar(
            "SELECT count(*) FROM guild_members GROUP BY guild_id ORDER BY count(*) DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(sizes, [24, 12, 8, 6, 5]);
        // Every author is a member of the guild the message was posted in.
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM messages m WHERE NOT EXISTS (SELECT 1 FROM guild_members gm \
                 WHERE gm.guild_id = m.guild_id AND gm.user_id = m.author_id)"
            )
            .await,
            0
        );
        // Each text channel's last message is the newest one in it.
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM channels c WHERE c.type = 0 AND c.last_message_id IS DISTINCT FROM \
                 (SELECT max(id) FROM messages m WHERE m.channel_id = c.id)"
            )
            .await,
            0
        );
        // The marker stops a second run; another name loads beside it.
        let again = seed_scale(&pool, "tiny", profile, "hash", perms, now_ms())
            .await
            .unwrap();
        assert!(again.skipped);
        assert_eq!(count(&pool, "SELECT count(*) FROM messages").await, 300);
        let other = seed_scale(&pool, "tiny2", profile, "hash", perms, now_ms())
            .await
            .unwrap();
        assert!(!other.skipped);
        assert_eq!(count(&pool, "SELECT count(*) FROM messages").await, 600);
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM accounts WHERE split_part(username, '_', 1) LIKE 'tiny%'"
            )
            .await,
            80
        );
        db.cleanup().await.unwrap();
    }
}
