//! Profile pages through the privacy filter (PRF-PRO-001, D-382): the page a viewer is shown is built from the
//! blocks their rank admits (`profile_privacy`), the guestbook with its held public posts, and the member's
//! own policy row. Nothing here builds a full page and then removes fields; hidden blocks are dropped from
//! the document first and only the data of the blocks that remain is loaded.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use utoipa::ToSchema;

use super::{
    FeaturedGuild, GUESTBOOK_PER_MINUTE, GuestbookEntry, MAX_GUESTBOOK_ENTRIES, PAGE,
    PackageService, PageUser, ProfilePage, USER_COLS, ms, user_of,
};
use crate::error::{PackageError, PackageResult};
use crate::profile_doc::{self, BlockKind, ProfileDoc};
use crate::profile_privacy::{Facts, Preset, PrivacyPolicy, Rank, Standing, Tier, standing};

/// Anonymous guestbook posts one address may leave per hour (`abuse.public_guestbook_per_hour`, 5).
pub const PUBLIC_GUESTBOOK_PER_HOUR: i64 = 5;
/// Held posts one page may have waiting at once.
pub const MAX_PENDING_PER_PAGE: i64 = 50;

/// `GET /users/@me/profile-privacy`: what is stored, what is in force on this instance, and whether `public`
/// can be in force at all.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = ProfilePrivacyView)]
pub struct PrivacyView {
    pub preset: Preset,
    /// The member's choices after the clamp (no block looser than the page).
    pub stored: PrivacyPolicy,
    /// What is in force now: `public` raised to `members` while the operator's switch is off.
    pub effective: PrivacyPolicy,
    /// The operator allows public profile pages (`platform.public_profiles`).
    pub public_possible: bool,
}

/// `PUT /users/@me/profile-privacy`: a preset name, or explicit fields (stored as `custom`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfilePrivacyUpdate)]
pub struct PrivacyUpdate {
    pub preset: Option<Preset>,
    pub page_tier: Option<Tier>,
    pub links_tier: Option<Tier>,
    pub top8_tier: Option<Tier>,
    pub now_playing_tier: Option<Tier>,
    pub bio_tier: Option<Tier>,
    pub guestbook_tier: Option<Tier>,
    pub media_tier: Option<Tier>,
    pub ring_tier: Option<Tier>,
    pub guestbook_accepts_public: Option<bool>,
}

/// `GET /public/profiles/{handle}`: the page at rank 0, with the approved guestbook when the guestbook is public.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = PublicProfile)]
pub struct PublicProfile {
    pub page: ProfilePage,
    /// Present only when the guestbook block is visible to everyone.
    pub guestbook: Option<Vec<GuestbookEntry>>,
    /// Anonymous visitors may leave a held post.
    pub accepts_guestbook: bool,
}

/// A viewer for the "view as" strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ProfileViewAs)]
pub enum ViewAs {
    Public,
    Member,
    SharesServer,
    Friend,
}

impl ViewAs {
    pub fn rank(self) -> Rank {
        match self {
            Self::Public => Tier::Public,
            Self::Member => Tier::Members,
            Self::SharesServer => Tier::MutualServers,
            Self::Friend => Tier::Friends,
        }
    }
}

fn policy_of(r: &sqlx::postgres::PgRow) -> Result<(Preset, PrivacyPolicy), sqlx::Error> {
    let tier = |c: &str| -> Result<Tier, sqlx::Error> {
        let s: String = r.try_get(c)?;
        Ok(Tier::parse(&s).unwrap_or(Tier::OnlyMe))
    };
    let preset: String = r.try_get("preset")?;
    Ok((
        Preset::parse(&preset).unwrap_or(Preset::Private),
        PrivacyPolicy {
            page_tier: tier("page_tier")?,
            links_tier: tier("links_tier")?,
            top8_tier: tier("top8_tier")?,
            now_playing_tier: tier("now_playing_tier")?,
            bio_tier: tier("bio_tier")?,
            guestbook_tier: tier("guestbook_tier")?,
            media_tier: tier("media_tier")?,
            ring_tier: tier("ring_tier")?,
            guestbook_accepts_public: r.try_get("guestbook_accepts_public")?,
        },
    ))
}

/// Plain text only: no link of any shape (a public post is held, but a link still never reaches a page).
pub fn has_link(s: &str) -> bool {
    let lower = s.to_lowercase();
    if lower.contains("://") || lower.contains("www.") || lower.contains("mailto:") {
        return true;
    }
    lower.split_whitespace().any(|tok| {
        let tok = tok.split(['/', '?', '#']).next().unwrap_or(tok);
        let tok = tok.trim_matches(|c: char| !c.is_alphanumeric());
        match tok.rsplit_once('.') {
            Some((head, tld)) => {
                !head.is_empty()
                    && tld.len() >= 2
                    && tld.chars().all(|c| c.is_ascii_alphabetic())
                    && head.chars().last().is_some_and(char::is_alphanumeric)
            }
            None => false,
        }
    })
}

/// The salted hash a held post keeps of its sender's address, for the hourly limit only.
pub fn address_hash(ip: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"sr-public-guestbook-v1:");
    h.update(ip.as_bytes());
    data_encoding::HEXLOWER.encode(&h.finalize())
}

impl PackageService {
    // ------------------------------------------------------------ the policy row

    /// The stored policy and preset (the private preset when the member never chose).
    pub async fn privacy_row(&self, user: i64) -> PackageResult<(Preset, PrivacyPolicy)> {
        let row = sqlx::query("SELECT * FROM profile_privacy WHERE user_id = $1")
            .bind(user)
            .fetch_optional(self.pool())
            .await?;
        Ok(match row {
            Some(r) => policy_of(&r)?,
            None => (Preset::Private, PrivacyPolicy::default()),
        })
    }

    pub async fn privacy(&self, user: i64, public_possible: bool) -> PackageResult<PrivacyView> {
        let (preset, policy) = self.privacy_row(user).await?;
        Ok(PrivacyView {
            preset,
            effective: policy.clone().effective(public_possible),
            stored: policy.clamped(),
            public_possible,
        })
    }

    /// Save my policy: a preset fills every field; explicit fields start from what is stored, are clamped (no
    /// block looser than the page) and are stored as `custom`.
    pub async fn set_privacy(
        &self,
        user: i64,
        update: PrivacyUpdate,
        public_possible: bool,
    ) -> PackageResult<PrivacyView> {
        let (preset, policy) = match update.preset {
            Some(p) if p != Preset::Custom => (p, PrivacyPolicy::from_preset(p)),
            _ => {
                let (_, mut cur) = self.privacy_row(user).await?;
                macro_rules! set {
                    ($f:ident) => {
                        if let Some(v) = update.$f {
                            cur.$f = v;
                        }
                    };
                }
                set!(page_tier);
                set!(links_tier);
                set!(top8_tier);
                set!(now_playing_tier);
                set!(bio_tier);
                set!(guestbook_tier);
                set!(media_tier);
                set!(ring_tier);
                set!(guestbook_accepts_public);
                (Preset::Custom, cur)
            }
        };
        let p = policy.clamped();
        sqlx::query(
            "INSERT INTO profile_privacy (user_id, preset, page_tier, links_tier, top8_tier, now_playing_tier, \
             bio_tier, guestbook_tier, media_tier, ring_tier, guestbook_accepts_public, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now()) \
             ON CONFLICT (user_id) DO UPDATE SET preset = EXCLUDED.preset, page_tier = EXCLUDED.page_tier, \
             links_tier = EXCLUDED.links_tier, top8_tier = EXCLUDED.top8_tier, \
             now_playing_tier = EXCLUDED.now_playing_tier, bio_tier = EXCLUDED.bio_tier, \
             guestbook_tier = EXCLUDED.guestbook_tier, media_tier = EXCLUDED.media_tier, \
             ring_tier = EXCLUDED.ring_tier, guestbook_accepts_public = EXCLUDED.guestbook_accepts_public, \
             updated_at = now()",
        )
        .bind(user)
        .bind(preset.as_str())
        .bind(p.page_tier.as_str())
        .bind(p.links_tier.as_str())
        .bind(p.top8_tier.as_str())
        .bind(p.now_playing_tier.as_str())
        .bind(p.bio_tier.as_str())
        .bind(p.guestbook_tier.as_str())
        .bind(p.media_tier.as_str())
        .bind(p.ring_tier.as_str())
        .bind(p.guestbook_accepts_public)
        .execute(self.pool())
        .await?;
        self.privacy(user, public_possible).await
    }

    // ------------------------------------------------------------ ranks

    /// Where `viewer` stands with `owner`: blocked, or a rank.
    pub async fn standing_of(&self, viewer: Option<i64>, owner: i64) -> PackageResult<Standing> {
        let Some(v) = viewer else {
            return Ok(standing(Facts::default()));
        };
        if v == owner {
            return Ok(standing(Facts {
                signed_in: true,
                is_owner: true,
                ..Facts::default()
            }));
        }
        let row = sqlx::query(
            "SELECT \
               EXISTS(SELECT 1 FROM relationships WHERE type = 2 AND \
                 ((user_id = $1 AND target_id = $2) OR (user_id = $2 AND target_id = $1))) AS blocked, \
               EXISTS(SELECT 1 FROM relationships WHERE user_id = $1 AND target_id = $2 AND type = 1) \
                 AND EXISTS(SELECT 1 FROM relationships WHERE user_id = $2 AND target_id = $1 AND type = 1) AS friends, \
               EXISTS(SELECT 1 FROM guild_members x JOIN guild_members y ON x.guild_id = y.guild_id \
                 WHERE x.user_id = $1 AND y.user_id = $2) AS shares",
        )
        .bind(v)
        .bind(owner)
        .fetch_one(self.pool())
        .await?;
        Ok(standing(Facts {
            signed_in: true,
            is_owner: false,
            blocked: row.try_get("blocked")?,
            friends: row.try_get("friends")?,
            shares_server: row.try_get("shares")?,
        }))
    }

    // ------------------------------------------------------------ pages

    async fn featured_for(
        &self,
        viewer: Option<i64>,
        owner: i64,
        doc: &ProfileDoc,
    ) -> PackageResult<Vec<FeaturedGuild>> {
        let named = profile_doc::featured_guild_ids(doc);
        if named.is_empty() {
            return Ok(Vec::new());
        }
        // The owner must still be in the server; the viewer must be in it too, or it must be in the directory.
        let rows = sqlx::query(
            "SELECT g.id, g.name, g.icon_key FROM guilds g \
             JOIN guild_members om ON om.guild_id = g.id AND om.user_id = $2 \
             WHERE g.id = ANY($1) AND ( \
               EXISTS(SELECT 1 FROM guild_directory d WHERE d.guild_id = g.id AND d.status = 'approved') \
               OR ($3::bigint IS NOT NULL AND EXISTS(SELECT 1 FROM guild_members vm \
                    WHERE vm.guild_id = g.id AND vm.user_id = $3)))",
        )
        .bind(&named)
        .bind(owner)
        .bind(viewer)
        .fetch_all(self.pool())
        .await?;
        let mut out: Vec<FeaturedGuild> = rows
            .iter()
            .map(|r| {
                Ok(FeaturedGuild {
                    id: r.try_get::<i64, _>("id")?.to_string(),
                    name: r.try_get("name")?,
                    icon: r.try_get("icon_key")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?;
        // The order the member chose.
        out.sort_by_key(|g| {
            named
                .iter()
                .position(|n| n.to_string() == g.id)
                .unwrap_or(usize::MAX)
        });
        Ok(out)
    }

    /// The page for a rank, built from the blocks the policy admits.
    #[allow(clippy::too_many_arguments)]
    async fn build_page(
        &self,
        viewer: Option<i64>,
        user: PageUser,
        raw: &Value,
        rank: Rank,
        policy: &PrivacyPolicy,
        is_owner: bool,
    ) -> PackageResult<ProfilePage> {
        let owner: i64 = user.id.parse().unwrap_or(0);
        let mut doc = profile_doc::parse_doc(raw, false).unwrap_or_default();
        doc.blocks.retain(|b| policy.visible(b.kind, rank));
        let mut friends = Vec::new();
        if doc.blocks.iter().any(|b| b.kind == BlockKind::TopFriends) {
            let named = profile_doc::top_friend_ids(&doc);
            let still = self.friends_among(owner, &named).await?;
            for id in named.iter().filter(|i| still.contains(i)) {
                if let Some((u, _)) = self.user_by("a.id = $1", Some(*id), None).await? {
                    friends.push(u);
                }
            }
        }
        let featured_guilds = if doc
            .blocks
            .iter()
            .any(|b| b.kind == BlockKind::FeaturedGuilds)
        {
            self.featured_for(viewer, owner, &doc).await?
        } else {
            Vec::new()
        };
        let guestbook_shown = doc.blocks.iter().any(|b| b.kind == BlockKind::Guestbook);
        let guestbook_count = if is_owner && guestbook_shown {
            Some(
                sqlx::query_scalar(
                    "SELECT count(*) FROM profile_guestbook WHERE profile_id = $1 AND status = 'approved'",
                )
                .bind(owner)
                .fetch_one(self.pool())
                .await?,
            )
        } else {
            None
        };
        let can_sign = viewer.is_some() && guestbook_shown && policy.may_sign(rank);
        let media = self.media_named_by(owner, &doc).await?;
        Ok(ProfilePage {
            media,
            guestbook_count,
            guestbook_limit: MAX_GUESTBOOK_ENTRIES,
            customised: !doc.blocks.is_empty(),
            user,
            doc,
            friends,
            featured_guilds,
            is_owner,
            can_sign,
        })
    }

    /// The profile page of a person, by username or id, as `viewer` sees it. `NotFound` when there is no such
    /// active account, either side blocked the other, or the viewer's rank is below the page's tier.
    pub async fn page(
        &self,
        viewer: i64,
        username: Option<&str>,
        id: Option<i64>,
        public_possible: bool,
    ) -> PackageResult<ProfilePage> {
        let found = match (username, id) {
            (Some(n), _) => {
                self.user_by("a.username = $1::citext", None, Some(n))
                    .await?
            }
            (_, Some(i)) => self.user_by("a.id = $1", Some(i), None).await?,
            _ => None,
        };
        let (user, raw) = found.ok_or(PackageError::NotFound("profile"))?;
        let owner: i64 = user.id.parse().unwrap_or(0);
        let Standing::Viewer(rank) = self.standing_of(Some(viewer), owner).await? else {
            return Err(PackageError::NotFound("profile"));
        };
        let (_, stored) = self.privacy_row(owner).await?;
        let policy = stored.effective(public_possible);
        if !policy.page_visible(rank) {
            return Err(PackageError::NotFound("profile"));
        }
        self.build_page(Some(viewer), user, &raw, rank, &policy, viewer == owner)
            .await
    }

    /// The owner's own page as a viewer of the given kind would see it (the "view as" strip). Goes through the
    /// same filter as a real visit.
    pub async fn preview_as(
        &self,
        owner: i64,
        as_: ViewAs,
        public_possible: bool,
    ) -> PackageResult<Option<ProfilePage>> {
        let Some((user, raw)) = self.user_by("a.id = $1", Some(owner), None).await? else {
            return Err(PackageError::NotFound("profile"));
        };
        let (_, stored) = self.privacy_row(owner).await?;
        let policy = stored.effective(public_possible);
        let rank = as_.rank();
        if !policy.page_visible(rank) {
            return Ok(None);
        }
        // A stranger preview carries no viewer: no server membership is assumed.
        let viewer = (rank > Tier::Public).then_some(0_i64);
        self.build_page(viewer, user, &raw, rank, &policy, false)
            .await
            .map(Some)
    }

    /// The public page of a handle: rank 0, only while the operator allows public pages and the member's page is
    /// public. `NotFound` otherwise, whatever the reason (no oracle).
    pub async fn public_page(
        &self,
        handle: &str,
        public_possible: bool,
    ) -> PackageResult<PublicProfile> {
        if !public_possible {
            return Err(PackageError::NotFound("profile"));
        }
        let handle = handle.trim_start_matches('~');
        let (user, raw) = self
            .user_by("a.username = $1::citext", None, Some(handle))
            .await?
            .ok_or(PackageError::NotFound("profile"))?;
        let owner: i64 = user.id.parse().unwrap_or(0);
        let (_, stored) = self.privacy_row(owner).await?;
        let policy = stored.effective(true);
        if !policy.page_visible(Tier::Public) {
            return Err(PackageError::NotFound("profile"));
        }
        let page = self
            .build_page(None, user, &raw, Tier::Public, &policy, false)
            .await?;
        let book_visible = page
            .doc
            .blocks
            .iter()
            .any(|b| b.kind == BlockKind::Guestbook);
        let guestbook = if book_visible {
            Some(self.guestbook_rows(None, owner, None, false).await?)
        } else {
            None
        };
        Ok(PublicProfile {
            accepts_guestbook: book_visible && policy.guestbook_accepts_public,
            guestbook,
            page,
        })
    }

    // ------------------------------------------------------------ guestbook

    async fn guestbook_rows(
        &self,
        viewer: Option<i64>,
        profile: i64,
        before: Option<i64>,
        owner_view: bool,
    ) -> PackageResult<Vec<GuestbookEntry>> {
        let sql = format!(
            "SELECT g.id AS gid, g.body, g.status, g.author_id, {} AS created_ms, {USER_COLS} \
             FROM profile_guestbook g LEFT JOIN accounts a ON a.id = g.author_id \
             WHERE g.profile_id = $1 AND ($2::bigint IS NULL OR g.id < $2) \
               AND (g.status = 'approved' OR $3) \
             ORDER BY g.id DESC LIMIT {PAGE}",
            ms("g.created_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(profile)
            .bind(before)
            .bind(owner_view)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                let author_id: Option<i64> = r.try_get("author_id")?;
                let status: String = r.try_get("status")?;
                Ok(GuestbookEntry {
                    id: r.try_get::<i64, _>("gid")?.to_string(),
                    author: match author_id {
                        Some(_) => Some(user_of(r, "u_")?),
                        None => None,
                    },
                    pending: status == "pending",
                    body: r.try_get("body")?,
                    created_at_ms: r.try_get("created_ms")?,
                    can_remove: viewer == Some(profile)
                        || (author_id.is_some() && author_id == viewer),
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// The guestbook of a profile, newest first, as the viewer may see it: not at all (404) below the guestbook
    /// tier, never to a blocked party, held posts only to the owner.
    pub async fn guestbook(
        &self,
        viewer: i64,
        profile: i64,
        before: Option<i64>,
        public_possible: bool,
    ) -> PackageResult<Vec<GuestbookEntry>> {
        let Standing::Viewer(rank) = self.standing_of(Some(viewer), profile).await? else {
            return Err(PackageError::NotFound("profile"));
        };
        let (_, stored) = self.privacy_row(profile).await?;
        let policy = stored.effective(public_possible);
        if !policy.visible(BlockKind::Guestbook, rank) {
            return Err(PackageError::NotFound("profile"));
        }
        self.guestbook_rows(Some(viewer), profile, before, viewer == profile)
            .await
    }

    /// Sign a guestbook: the owner, or a signed-in viewer whose rank the guestbook tier admits; only while the
    /// owner's page has a Guestbook block, never when either side blocked the other.
    pub async fn sign(
        &self,
        author: i64,
        profile: i64,
        body: &str,
        public_possible: bool,
    ) -> PackageResult<GuestbookEntry> {
        let body = body.trim();
        let n = body.chars().count();
        if n == 0 || n > 500 || body.chars().any(|c| c.is_control() && c != '\n') {
            return Err(PackageError::Invalid(
                "body",
                "Write 1 to 500 characters.".to_owned(),
            ));
        }
        let page = self
            .page(author, None, Some(profile), public_possible)
            .await?;
        if !page.can_sign {
            return Err(PackageError::Forbidden);
        }
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_guestbook WHERE author_id = $1 AND created_at > now() - interval '1 minute'",
        )
        .bind(author)
        .fetch_one(self.pool())
        .await?;
        if recent >= GUESTBOOK_PER_MINUTE {
            return Err(PackageError::Conflict(
                "You are signing guestbooks too fast. Try again in a minute.".to_owned(),
            ));
        }
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_guestbook WHERE profile_id = $1 AND status = 'approved'",
        )
        .bind(profile)
        .fetch_one(self.pool())
        .await?;
        if total >= MAX_GUESTBOOK_ENTRIES {
            return Err(PackageError::Conflict("This guestbook is full".to_owned()));
        }
        let id = (self.inner.ids)();
        sqlx::query("INSERT INTO profile_guestbook (id, profile_id, author_id, body) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(profile)
            .bind(author)
            .bind(body)
            .execute(self.pool())
            .await?;
        let mut list = self
            .guestbook_rows(Some(author), profile, Some(id + 1), author == profile)
            .await?;
        list.retain(|e| e.id == id.to_string());
        list.pop().ok_or(PackageError::NotFound("entry"))
    }

    /// An anonymous visitor leaves a held post: only when the operator allows public pages, the page is public,
    /// the guestbook tier is `public` and the member opted in. Plain text, 500 characters, no link; at most
    /// [`PUBLIC_GUESTBOOK_PER_HOUR`] per address per hour. The post is `pending` until the owner approves it.
    /// `Err(Conflict)` is the rate limit.
    pub async fn sign_public(
        &self,
        handle: &str,
        body: &str,
        ip: &str,
        public_possible: bool,
    ) -> PackageResult<()> {
        if !public_possible {
            return Err(PackageError::NotFound("profile"));
        }
        let handle = handle.trim_start_matches('~');
        let (user, raw) = self
            .user_by("a.username = $1::citext", None, Some(handle))
            .await?
            .ok_or(PackageError::NotFound("profile"))?;
        let owner: i64 = user.id.parse().unwrap_or(0);
        let (_, stored) = self.privacy_row(owner).await?;
        let policy = stored.effective(true);
        if !policy.page_visible(Tier::Public)
            || !policy.visible(BlockKind::Guestbook, Tier::Public)
            || !policy.guestbook_accepts_public
        {
            return Err(PackageError::NotFound("profile"));
        }
        let doc = profile_doc::parse_doc(&raw, false).unwrap_or_default();
        if !doc.blocks.iter().any(|b| b.kind == BlockKind::Guestbook) {
            return Err(PackageError::NotFound("profile"));
        }
        let body = body.trim();
        let n = body.chars().count();
        if n == 0 || n > 500 || body.chars().any(|c| c.is_control() && c != '\n') {
            return Err(PackageError::Invalid(
                "body",
                "Write 1 to 500 characters.".to_owned(),
            ));
        }
        if has_link(body) {
            return Err(PackageError::Invalid(
                "body",
                "Public guestbook posts are plain text without links.".to_owned(),
            ));
        }
        let hash = address_hash(ip);
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_guestbook WHERE ip_hash = $1 AND created_at > now() - interval '1 hour'",
        )
        .bind(&hash)
        .fetch_one(self.pool())
        .await?;
        if recent >= PUBLIC_GUESTBOOK_PER_HOUR {
            return Err(PackageError::Conflict(
                "Too many posts from this address. Try again later.".to_owned(),
            ));
        }
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_guestbook WHERE profile_id = $1 AND status = 'pending'",
        )
        .bind(owner)
        .fetch_one(self.pool())
        .await?;
        if pending >= MAX_PENDING_PER_PAGE {
            return Err(PackageError::Conflict(
                "This guestbook is not taking new posts right now.".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO profile_guestbook (id, profile_id, author_id, body, status, anonymous, ip_hash) \
             VALUES ($1, $2, NULL, $3, 'pending', true, $4)",
        )
        .bind((self.inner.ids)())
        .bind(owner)
        .bind(body)
        .bind(&hash)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The owner approves a held post: it joins the visible entries and its address hash is dropped.
    pub async fn approve_entry(&self, owner: i64, entry: i64) -> PackageResult<()> {
        let n = sqlx::query(
            "UPDATE profile_guestbook SET status = 'approved', ip_hash = NULL \
             WHERE id = $1 AND profile_id = $2 AND status = 'pending'",
        )
        .bind(entry)
        .bind(owner)
        .execute(self.pool())
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PackageError::NotFound("entry"));
        }
        Ok(())
    }

    /// The owner removes any entry; an author removes their own.
    pub async fn remove_entry(&self, actor: i64, profile: i64, entry: i64) -> PackageResult<()> {
        let n = sqlx::query(
            "DELETE FROM profile_guestbook WHERE id = $1 AND profile_id = $2 AND (author_id = $3 OR profile_id = $3)",
        )
        .bind(entry)
        .bind(profile)
        .bind(actor)
        .execute(self.pool())
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PackageError::NotFound("entry"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_found_in_every_shape_and_plain_text_passes() {
        for s in [
            "see https://example.com",
            "go to www.example.com",
            "example.com is cool",
            "visit my.site.org/x",
            "mailto:a@b.co",
            "hello.world",
        ] {
            assert!(has_link(s), "{s}");
        }
        for s in [
            "Great page! Love the colours.",
            "see you at 5 p.m. tonight",
            "e.g. this works",
            "Hi.",
            "wow...",
        ] {
            assert!(!has_link(s), "{s}");
        }
    }

    #[test]
    fn an_address_hash_is_stable_and_hides_the_address() {
        let h = address_hash("203.0.113.9");
        assert_eq!(h, address_hash("203.0.113.9"));
        assert_ne!(h, address_hash("203.0.113.10"));
        assert!(!h.contains("203"));
        assert_eq!(h.len(), 64);
    }
}
