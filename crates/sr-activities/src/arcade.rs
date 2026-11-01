//! The Arcade (ARC-ROOM-001, wave 42): what a game manifest declares and what the Arcade surface lists.
//!
//! * **Tiers.** A game manifest (`sideroom.json`) declares exactly one of two tiers, [`Tier::Original`]
//!   (`TIER_1_ORIGINAL`: all code, art, audio and IP are the developer's own) or [`Tier::Reimplementation`]
//!   (`TIER_2_REIMPLEMENTATION`: clean-room engine code with no bundled proprietary assets, desktop only). The
//!   validator refuses any other value: there is no tier for fan IP, and games that use someone else's names,
//!   characters or assets are not accepted on any surface, unlisted cartridges included.
//! * **Capability flags** (roadmap rule 5, D-399). The general capabilities a game may ask for are manifest flags
//!   ([`FLAGS`]). A flag is `active` (the platform enforces it today) or `reserved` (the validator accepts the
//!   name so a manifest written today keeps working, and the platform never hands it to an instance until it
//!   ships; see [`is_active`]).
//! * **Tip links.** `tip_links` in the manifest, validated by [`sr_core::tips`].
//! * **The surface.** [`ArcadeService`]: the Operator-curated Featured list, the Coming Soon showcase with a
//!   per-member "Notify me" toggle, and Guild Cartridges (unlisted games and mods a server lists from its shelf).
//!
//! The HTTP layer is `sr_api::routes::arcade`; the Operator's curation is audited there.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_core::tips::{TipError, TipKind, TipLink, validate_tips};
use utoipa::ToSchema;

use crate::bundle::{self, BundleError, MANIFEST, MAX_MANIFEST_BYTES};
use crate::error::{ActivityError, ActivityResult};
use crate::service::IdSource;

/// `TIER_1_ORIGINAL`.
pub const TIER_1: &str = "TIER_1_ORIGINAL";
/// `TIER_2_REIMPLEMENTATION`.
pub const TIER_2: &str = "TIER_2_REIMPLEMENTATION";

/// Most games on the Featured front page.
pub const MAX_FEATURED: i64 = 24;
/// Most Coming Soon entries.
pub const MAX_COMING_SOON: i64 = 40;
/// Most cartridges one server lists.
pub const MAX_CARTRIDGES: i64 = 50;

/// The two content tiers of a game manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeTier)]
pub enum Tier {
    #[serde(rename = "TIER_1_ORIGINAL")]
    Original,
    #[serde(rename = "TIER_2_REIMPLEMENTATION")]
    Reimplementation,
}

/// Where a tier may be played or shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeEligibility)]
pub struct Eligibility {
    /// Plays in the browser, in a call.
    pub browser_play: bool,
    /// May be offered on a shareable `/play/<token>` guest link (wave 43).
    pub guest_links: bool,
    /// May be featured on the Front Page.
    pub featured: bool,
    /// Runs in the desktop app only.
    pub desktop_only: bool,
    /// Needs the local base-game hint of the desktop launcher (46-B).
    pub needs_base_game_hint: bool,
}

impl Tier {
    pub const ALL: [Tier; 2] = [Self::Original, Self::Reimplementation];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => TIER_1,
            Self::Reimplementation => TIER_2,
        }
    }

    /// Exactly the two declared values; anything else (a third tier, another case) is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Eligibility per tier (roadmap 42-A): Tier 1 plays in the browser, takes guest links and may be featured;
    /// Tier 2 is desktop-only, needs the base-game hint and may be featured.
    pub fn eligibility(self) -> Eligibility {
        match self {
            Self::Original => Eligibility {
                browser_play: true,
                guest_links: true,
                featured: true,
                desktop_only: false,
                needs_base_game_hint: false,
            },
            Self::Reimplementation => Eligibility {
                browser_play: false,
                guest_links: false,
                featured: true,
                desktop_only: true,
                needs_base_game_hint: true,
            },
        }
    }
}

/// Whether a flag is enforced today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ArcadeFlagStatus)]
pub enum FlagStatus {
    Active,
    Reserved,
}

/// One manifest capability flag.
#[derive(Debug, Clone, Copy)]
pub struct Flag {
    pub name: &'static str,
    pub status: FlagStatus,
    pub summary: &'static str,
}

/// The capability flags a manifest may declare (roadmap rule 5, D-399). Active: `datachannel` (the SFU data
/// channel, ACT-CH-001) and `byos_read` (scoped read of a shelf, STU-ZK-001 on the wave-41 branch; this wave only
/// names it). Reserved: the rest; the validator accepts them and nothing hands them to an instance yet.
pub const FLAGS: &[Flag] = &[
    Flag {
        name: "datachannel",
        status: FlagStatus::Active,
        summary: "Shared game state and inputs through the call's encrypted data channel.",
    },
    Flag {
        name: "byos_read",
        status: FlagStatus::Active,
        summary: "Read files from a shelf the member granted, through short-lived links.",
    },
    Flag {
        name: "byos_write",
        status: FlagStatus::Reserved,
        summary: "Save to the developer's own bucket.",
    },
    Flag {
        name: "guest_play",
        status: FlagStatus::Reserved,
        summary: "Playable by guests on a /play link (TIER_1_ORIGINAL only).",
    },
    Flag {
        name: "lobby_channel",
        status: FlagStatus::Reserved,
        summary: "A lobby room for the game session, with its own keys.",
    },
    Flag {
        name: "ingame_voice",
        status: FlagStatus::Reserved,
        summary: "Voice between the players of one game session.",
    },
    Flag {
        name: "external_launch",
        status: FlagStatus::Reserved,
        summary: "Launch a local engine from the desktop app (TIER_2_REIMPLEMENTATION only).",
    },
    Flag {
        name: "asset_cache",
        status: FlagStatus::Reserved,
        summary: "Keep downloaded game assets between sessions.",
    },
];

/// The flag called `name`.
pub fn flag(name: &str) -> Option<&'static Flag> {
    FLAGS.iter().find(|f| f.name == name)
}

/// Whether the manifest validator accepts `name`: the capabilities the bundle module knows plus every flag above.
pub fn accepts(name: &str) -> bool {
    bundle::CAPABILITIES.contains(&name) || flag(name).is_some()
}

/// Whether the platform enforces `name` today. A reserved flag is never handed to a running instance.
pub fn is_active(name: &str) -> bool {
    bundle::CAPABILITIES.contains(&name)
        || flag(name).is_some_and(|f| f.status == FlagStatus::Active)
}

/// The rules that tie flags to tiers. `Err` is the text for the developer.
pub fn check_flags(tier: Option<Tier>, capabilities: &[String]) -> Result<(), String> {
    let has = |n: &str| capabilities.iter().any(|c| c == n);
    if has("guest_play") && tier != Some(Tier::Original) {
        return Err(format!(
            "declares guest_play, which needs \"tier\": \"{TIER_1}\" (guest links are for original games only)"
        ));
    }
    if has("external_launch") && tier != Some(Tier::Reimplementation) {
        return Err(format!(
            "declares external_launch, which needs \"tier\": \"{TIER_2}\" (it launches a local engine)"
        ));
    }
    Ok(())
}

/// What a `sideroom.json` declares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    /// Sorted, no duplicates.
    pub capabilities: Vec<String>,
    /// `None` for a manifest written before tiers; such a game is not listed on the Arcade.
    pub tier: Option<Tier>,
    pub tip_links: Vec<TipLink>,
}

fn problem(m: impl Into<String>) -> BundleError {
    BundleError::at(MANIFEST, m)
}

/// Parse and validate a `sideroom.json`. Refused: a manifest over [`MAX_MANIFEST_BYTES`], one that is not a JSON
/// object, a capability the platform does not know, a `tier` that is not one of the two values, a tip link the
/// allow-list does not admit and a flag the tier does not permit.
pub fn parse_manifest(data: &[u8]) -> Result<Manifest, BundleError> {
    if data.len() > MAX_MANIFEST_BYTES {
        return Err(problem("is larger than 8 KiB"));
    }
    let v: serde_json::Value =
        serde_json::from_slice(data).map_err(|_| problem("is not valid JSON"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| problem("must be a JSON object"))?;
    let mut capabilities: Vec<String> = Vec::new();
    if let Some(c) = obj.get("capabilities") {
        let list = c
            .as_array()
            .ok_or_else(|| problem("has capabilities that are not a list"))?;
        for item in list {
            let name = item
                .as_str()
                .ok_or_else(|| problem("has a capability that is not text"))?;
            if !accepts(name) {
                return Err(problem(format!(
                    "declares the unknown capability \"{}\"",
                    name.chars().take(40).collect::<String>()
                )));
            }
            if !capabilities.iter().any(|x| x == name) {
                capabilities.push(name.to_owned());
            }
        }
    }
    capabilities.sort();
    let tier = match obj.get("tier") {
        None => None,
        Some(serde_json::Value::String(s)) => Some(Tier::parse(s).ok_or_else(|| {
            problem(format!(
                "declares the unknown tier \"{}\" (use {TIER_1} or {TIER_2})",
                s.chars().take(40).collect::<String>()
            ))
        })?),
        Some(_) => return Err(problem("has a tier that is not text")),
    };
    let tip_links: Vec<TipLink> = match obj.get("tip_links") {
        None => Vec::new(),
        Some(t) => serde_json::from_value(t.clone()).map_err(|_| {
            problem("has tip_links that are not a list of {\"kind\", \"url\"} (kinds: btc, lightning, kofi, patreon)")
        })?,
    };
    validate_tips(&tip_links).map_err(|e| problem(format!("has tip_links that {}", e.text())))?;
    check_flags(tier, &capabilities).map_err(problem)?;
    Ok(Manifest {
        capabilities,
        tier,
        tip_links,
    })
}

// ---------------------------------------------------------------------------------------------------- wire types

/// One tip link as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeTip)]
pub struct TipView {
    /// `btc`, `lightning`, `kofi` or `patreon`.
    pub kind: String,
    /// A `bitcoin:` or `lightning:` URI (drawn as a QR code) or an https link on an allow-listed host.
    pub url: String,
}

impl From<&TipLink> for TipView {
    fn from(l: &TipLink) -> Self {
        Self {
            kind: l.kind.as_str().to_owned(),
            url: l.url.clone(),
        }
    }
}

/// A capability flag in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeFlag)]
pub struct FlagView {
    pub name: String,
    pub status: FlagStatus,
    pub summary: String,
}

/// The flag catalogue.
pub fn flag_views() -> Vec<FlagView> {
    FLAGS
        .iter()
        .map(|f| FlagView {
            name: f.name.to_owned(),
            status: f.status,
            summary: f.summary.to_owned(),
        })
        .collect()
}

/// A game on the Front Page. The Operator chose it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeFeaturedCard)]
pub struct FeaturedCard {
    pub application_id: String,
    pub name: String,
    pub description: String,
    /// The Operator's one-line note for the card.
    pub blurb: Option<String>,
    pub tier: Tier,
    pub eligibility: Eligibility,
    /// What the reviewed build declares.
    pub capabilities: Vec<String>,
    pub tip_links: Vec<TipView>,
    /// SHA-256 of the reviewed bundle; the key the trust badges (code signing, vouches) attach to.
    pub bundle_sha256: String,
    pub position: i32,
    pub featured_at_ms: i64,
}

/// A game being built, in the Coming Soon showcase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeComingSoonCard)]
pub struct ComingSoonCard {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub tier: Tier,
    /// The project's repository (https).
    pub repo_url: String,
    /// Whether the caller asked to be told when it is out.
    pub notify: bool,
    pub created_at_ms: i64,
}

/// The Front Page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ArcadeFront {
    pub featured: Vec<FeaturedCard>,
    pub coming_soon: Vec<ComingSoonCard>,
    pub flags: Vec<FlagView>,
}

/// A released Activity the Operator could feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeCandidate)]
pub struct CandidateView {
    pub application_id: String,
    pub name: String,
    pub tier: Tier,
    pub bundle_sha256: String,
    pub featured: bool,
}

/// What the Operator sets when featuring.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, ToSchema)]
#[schema(as = ArcadeFeatureInput)]
pub struct FeatureInput {
    /// Lower first. Default 0.
    pub position: Option<i32>,
    /// One line of at most 120 characters.
    pub blurb: Option<String>,
}

/// A Coming Soon entry to add.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = ArcadeComingSoonInput)]
pub struct ComingSoonInput {
    pub title: String,
    pub summary: String,
    pub tier: Tier,
    pub repo_url: String,
}

/// A cartridge a server lists from its shelf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = ArcadeCartridge)]
pub struct CartridgeView {
    pub id: String,
    pub guild_id: String,
    pub title: String,
    pub description: String,
    /// `game` or `mod`.
    pub kind: String,
    pub tier: Tier,
    pub eligibility: Eligibility,
    /// Where on the server's shelf the cartridge lives (an opaque locator the shelf code resolves).
    pub source_ref: String,
    pub capabilities: Vec<String>,
    pub tip_links: Vec<TipView>,
    pub added_by: String,
    pub added_at_ms: i64,
}

/// A cartridge to list.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = ArcadeCartridgeInput)]
pub struct CartridgeInput {
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// `game` or `mod`.
    pub kind: String,
    pub tier: Tier,
    pub source_ref: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tip_links: Vec<TipInput>,
}

/// A tip link in a request.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = ArcadeTipInput)]
pub struct TipInput {
    pub kind: String,
    pub url: String,
}

// ---------------------------------------------------------------------------------------------------- input rules

fn invalid(field: &'static str, m: impl Into<String>) -> ActivityError {
    ActivityError::Invalid(field, m.into())
}

fn text(field: &'static str, v: &str, min: usize, max: usize) -> ActivityResult<String> {
    let t = v.trim();
    let n = t.chars().count();
    if n < min || n > max || t.chars().any(char::is_control) {
        return Err(invalid(
            field,
            format!("must be {min} to {max} characters of plain text."),
        ));
    }
    Ok(t.to_owned())
}

/// An https repository link: no credentials, no port, no fragment, at most 200 characters.
pub fn repo_url(v: &str) -> ActivityResult<String> {
    let bad = || {
        invalid(
            "repo_url",
            "must be an https link to the project's repository.",
        )
    };
    let t = v.trim();
    if t.len() > 200 {
        return Err(bad());
    }
    let u = url::Url::parse(t).map_err(|_| bad())?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || u.fragment().is_some()
    {
        return Err(bad());
    }
    Ok(u.to_string())
}

/// A shelf locator: letters, digits and `. _ - / :` only, no `..` segment.
fn source_ref(v: &str) -> ActivityResult<String> {
    let t = v.trim();
    let ok = !t.is_empty()
        && t.len() <= 300
        && !t.split('/').any(|s| s == ".." || s == ".")
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':'));
    if ok {
        Ok(t.to_owned())
    } else {
        Err(invalid(
            "source_ref",
            "must be a shelf path of letters, digits and . _ - / : (no .. segments).",
        ))
    }
}

fn tips_of(links: &[TipInput]) -> ActivityResult<Vec<TipLink>> {
    let mut out = Vec::with_capacity(links.len());
    for l in links {
        let kind = TipKind::parse(&l.kind)
            .ok_or_else(|| invalid("tip_links", TipError::UnknownKind.text()))?;
        out.push(TipLink {
            kind,
            url: l.url.clone(),
        });
    }
    validate_tips(&out)
        .map_err(|e| invalid("tip_links", format!("The tip links {}.", e.text())))?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------- service

/// The Arcade backend. Cheap to clone.
#[derive(Clone)]
pub struct ArcadeService {
    pool: PgPool,
    ids: IdSource,
}

fn ms(col: &str) -> String {
    format!("(EXTRACT(EPOCH FROM {col}) * 1000)::bigint")
}

fn tier_of(r: &PgRow, col: &str) -> Result<Tier, sqlx::Error> {
    let s: String = r.try_get(col)?;
    Tier::parse(&s).ok_or_else(|| sqlx::Error::Decode(format!("unknown tier {s}").into()))
}

pub(crate) fn tips_from_json(v: serde_json::Value) -> Vec<TipView> {
    // The column only ever holds what `validate_tips` accepted; a row that no longer parses shows no tips.
    serde_json::from_value::<Vec<TipLink>>(v)
        .map(|l| l.iter().map(TipView::from).collect())
        .unwrap_or_default()
}

/// Released, or in review with an approved release: the same rule as the Activity shelf.
const LIVE_APP: &str = "(a.release_state::text = 'released' OR (a.release_state::text = 'in_review' AND \
     EXISTS (SELECT 1 FROM application_versions v WHERE v.application_id = a.id AND v.state = 'approved')))";

impl ArcadeService {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }

    /// Shared handle for code that wants `Arc<ArcadeService>`.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    // ---------------------------------------------------------------- front page

    /// The Front Page as `user` sees it: the Operator's Featured list and the Coming Soon showcase.
    pub async fn front(&self, user: i64) -> ActivityResult<ArcadeFront> {
        Ok(ArcadeFront {
            featured: self.featured().await?,
            coming_soon: self.coming_soon(user).await?,
            flags: flag_views(),
        })
    }

    /// Featured games in the Operator's order. A game drops off by itself when its application is suspended
    /// or withdrawn, or when the reviewed build no longer declares a tier.
    pub async fn featured(&self) -> ActivityResult<Vec<FeaturedCard>> {
        let sql = format!(
            "SELECT f.application_id, f.position, f.blurb, {} AS featured_ms, a.name, a.description, \
                    b.sha256, b.tier, b.capabilities, b.tip_links \
             FROM arcade_featured f \
             JOIN applications a ON a.id = f.application_id \
             JOIN application_activities c ON c.application_id = a.id \
             JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id \
             WHERE {LIVE_APP} AND b.tier IS NOT NULL \
             ORDER BY f.position, f.featured_at, f.application_id LIMIT {MAX_FEATURED}",
            ms("f.featured_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                let tier = tier_of(r, "tier")?;
                Ok(FeaturedCard {
                    application_id: r.try_get::<i64, _>("application_id")?.to_string(),
                    name: r.try_get("name")?,
                    description: r.try_get("description")?,
                    blurb: r.try_get("blurb")?,
                    tier,
                    eligibility: tier.eligibility(),
                    capabilities: r.try_get("capabilities")?,
                    tip_links: tips_from_json(r.try_get("tip_links")?),
                    bundle_sha256: r.try_get("sha256")?,
                    position: r.try_get("position")?,
                    featured_at_ms: r.try_get("featured_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Released Activities with a declared tier, for the Operator to pick from.
    pub async fn candidates(&self) -> ActivityResult<Vec<CandidateView>> {
        let sql = format!(
            "SELECT a.id, a.name, b.sha256, b.tier, \
                    EXISTS (SELECT 1 FROM arcade_featured f WHERE f.application_id = a.id) AS featured \
             FROM applications a \
             JOIN application_activities c ON c.application_id = a.id \
             JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id \
             WHERE {LIVE_APP} AND b.tier IS NOT NULL \
             ORDER BY lower(a.name), a.id LIMIT 200"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                Ok(CandidateView {
                    application_id: r.try_get::<i64, _>("id")?.to_string(),
                    name: r.try_get("name")?,
                    tier: tier_of(r, "tier")?,
                    bundle_sha256: r.try_get("sha256")?,
                    featured: r.try_get("featured")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Feature `app` (or change its place and blurb). The application must be live with a reviewed build that
    /// declares a tier, and the tier must be eligible for the Front Page.
    pub async fn feature(
        &self,
        operator: i64,
        app: i64,
        input: FeatureInput,
    ) -> ActivityResult<FeaturedCard> {
        let blurb = match input.blurb.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(b) => Some(text("blurb", b, 1, 120)?),
        };
        let position = input.position.unwrap_or(0).clamp(0, 10_000);
        let sql = format!(
            "SELECT b.tier FROM applications a \
             JOIN application_activities c ON c.application_id = a.id \
             JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id \
             WHERE a.id = $1 AND {LIVE_APP}"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Err(ActivityError::NotFound("activity"));
        };
        let tier: Option<String> = row.try_get("tier")?;
        let Some(tier) = tier.as_deref().and_then(Tier::parse) else {
            return Err(invalid(
                "application_id",
                format!(
                    "The reviewed build declares no tier. Only {TIER_1} and {TIER_2} games are listed."
                ),
            ));
        };
        if !tier.eligibility().featured {
            return Err(invalid("application_id", "This tier cannot be featured."));
        }
        let mut tx = self.pool.begin().await?;
        // One lock so two Operators cannot both pass the count check.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('arcade_featured'))")
            .execute(&mut *tx)
            .await?;
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM arcade_featured WHERE application_id = $1)",
        )
        .bind(app)
        .fetch_one(&mut *tx)
        .await?;
        if !already {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM arcade_featured")
                .fetch_one(&mut *tx)
                .await?;
            if n >= MAX_FEATURED {
                return Err(ActivityError::Conflict(format!(
                    "The Front Page lists at most {MAX_FEATURED} games."
                )));
            }
        }
        sqlx::query(
            "INSERT INTO arcade_featured (application_id, position, blurb, featured_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (application_id) DO UPDATE SET position = $2, blurb = $3",
        )
        .bind(app)
        .bind(position)
        .bind(&blurb)
        .bind(operator)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.featured()
            .await?
            .into_iter()
            .find(|c| c.application_id == app.to_string())
            .ok_or(ActivityError::NotFound("activity"))
    }

    /// Take `app` off the Front Page. `false` when it was not there.
    pub async fn unfeature(&self, app: i64) -> ActivityResult<bool> {
        let r = sqlx::query("DELETE FROM arcade_featured WHERE application_id = $1")
            .bind(app)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    // ---------------------------------------------------------------- coming soon

    /// The showcase, newest last-added first inside the Operator's order.
    pub async fn coming_soon(&self, user: i64) -> ActivityResult<Vec<ComingSoonCard>> {
        let sql = format!(
            "SELECT s.id, s.title, s.summary, s.tier, s.repo_url, {} AS created_ms, \
                    EXISTS (SELECT 1 FROM arcade_coming_soon_notify n \
                            WHERE n.coming_soon_id = s.id AND n.user_id = $1) AS notify \
             FROM arcade_coming_soon s ORDER BY s.position, s.created_at DESC, s.id LIMIT {MAX_COMING_SOON}",
            ms("s.created_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                Ok(ComingSoonCard {
                    id: r.try_get::<i64, _>("id")?.to_string(),
                    title: r.try_get("title")?,
                    summary: r.try_get("summary")?,
                    tier: tier_of(r, "tier")?,
                    repo_url: r.try_get("repo_url")?,
                    notify: r.try_get("notify")?,
                    created_at_ms: r.try_get("created_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Add a Coming Soon entry.
    pub async fn add_coming_soon(
        &self,
        operator: i64,
        input: ComingSoonInput,
    ) -> ActivityResult<ComingSoonCard> {
        let title = text("title", &input.title, 2, 60)?;
        let summary = text("summary", &input.summary, 1, 240)?;
        let repo = repo_url(&input.repo_url)?;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM arcade_coming_soon")
            .fetch_one(&self.pool)
            .await?;
        if n >= MAX_COMING_SOON {
            return Err(ActivityError::Conflict(format!(
                "The showcase lists at most {MAX_COMING_SOON} games."
            )));
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO arcade_coming_soon (id, title, summary, tier, repo_url, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(&title)
        .bind(&summary)
        .bind(input.tier.as_str())
        .bind(&repo)
        .bind(operator)
        .execute(&self.pool)
        .await?;
        self.coming_soon(operator)
            .await?
            .into_iter()
            .find(|c| c.id == id.to_string())
            .ok_or(ActivityError::NotFound("entry"))
    }

    /// Remove a Coming Soon entry (and everyone's "Notify me" on it). `false` when unknown.
    pub async fn remove_coming_soon(&self, id: i64) -> ActivityResult<bool> {
        let r = sqlx::query("DELETE FROM arcade_coming_soon WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Turn "Notify me" on or off for `user`. Idempotent. `NotFound` for an unknown entry.
    pub async fn set_notify(&self, user: i64, id: i64, on: bool) -> ActivityResult<()> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM arcade_coming_soon WHERE id = $1)")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        if !exists {
            return Err(ActivityError::NotFound("entry"));
        }
        if on {
            sqlx::query(
                "INSERT INTO arcade_coming_soon_notify (coming_soon_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(user)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM arcade_coming_soon_notify WHERE coming_soon_id = $1 AND user_id = $2",
            )
            .bind(id)
            .bind(user)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// The members who asked to be told when entry `id` ships (for the release notice).
    pub async fn watchers(&self, id: i64) -> ActivityResult<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM arcade_coming_soon_notify WHERE coming_soon_id = $1 ORDER BY user_id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?)
    }

    // ---------------------------------------------------------------- guild cartridges

    fn cartridge_of(r: &PgRow) -> Result<CartridgeView, sqlx::Error> {
        let tier = tier_of(r, "tier")?;
        Ok(CartridgeView {
            id: r.try_get::<i64, _>("id")?.to_string(),
            guild_id: r.try_get::<i64, _>("guild_id")?.to_string(),
            title: r.try_get("title")?,
            description: r.try_get("description")?,
            kind: r.try_get("kind")?,
            tier,
            eligibility: tier.eligibility(),
            source_ref: r.try_get("source_ref")?,
            capabilities: r.try_get("capabilities")?,
            tip_links: tips_from_json(r.try_get("tip_links")?),
            added_by: r.try_get::<i64, _>("added_by")?.to_string(),
            added_at_ms: r.try_get("added_ms")?,
        })
    }

    fn cartridge_columns() -> String {
        format!(
            "id, guild_id, title, description, kind, tier, source_ref, capabilities, tip_links, added_by, {} AS added_ms",
            ms("added_at")
        )
    }

    /// The cartridges a server lists, newest first. Never part of the Front Page: they are unlisted.
    pub async fn cartridges(&self, guild: i64) -> ActivityResult<Vec<CartridgeView>> {
        let sql = format!(
            "SELECT {} FROM arcade_guild_cartridges WHERE guild_id = $1 \
             ORDER BY added_at DESC, id DESC LIMIT {MAX_CARTRIDGES}",
            Self::cartridge_columns()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(Self::cartridge_of)
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// List a cartridge on a server. The same manifest rules as a bundle: a known tier, known flags that the
    /// tier permits, tip links on the allow-list.
    pub async fn add_cartridge(
        &self,
        guild: i64,
        user: i64,
        input: CartridgeInput,
    ) -> ActivityResult<CartridgeView> {
        let title = text("title", &input.title, 2, 60)?;
        let description = if input.description.trim().is_empty() {
            String::new()
        } else {
            text("description", &input.description, 1, 240)?
        };
        if !matches!(input.kind.as_str(), "game" | "mod") {
            return Err(invalid("kind", "must be game or mod."));
        }
        let source = source_ref(&input.source_ref)?;
        let mut caps: Vec<String> = Vec::new();
        for c in &input.capabilities {
            if !accepts(c) {
                return Err(invalid(
                    "capabilities",
                    format!(
                        "declares the unknown capability \"{}\".",
                        c.chars().take(40).collect::<String>()
                    ),
                ));
            }
            if !caps.contains(c) {
                caps.push(c.clone());
            }
        }
        caps.sort();
        check_flags(Some(input.tier), &caps).map_err(|m| invalid("capabilities", m))?;
        let tips = tips_of(&input.tip_links)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('arcade_cartridges')::bigint # $1)")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM arcade_guild_cartridges WHERE guild_id = $1")
                .bind(guild)
                .fetch_one(&mut *tx)
                .await?;
        if n >= MAX_CARTRIDGES {
            return Err(ActivityError::Conflict(format!(
                "A server lists at most {MAX_CARTRIDGES} cartridges."
            )));
        }
        let id = (self.ids)();
        let tips_json =
            serde_json::to_value(&tips).map_err(|e| ActivityError::Storage(e.to_string()))?;
        sqlx::query(
            "INSERT INTO arcade_guild_cartridges \
             (id, guild_id, title, description, kind, tier, source_ref, capabilities, tip_links, added_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(id)
        .bind(guild)
        .bind(&title)
        .bind(&description)
        .bind(&input.kind)
        .bind(input.tier.as_str())
        .bind(&source)
        .bind(&caps)
        .bind(tips_json)
        .bind(user)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let sql = format!(
            "SELECT {} FROM arcade_guild_cartridges WHERE id = $1",
            Self::cartridge_columns()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(Self::cartridge_of(&row)?)
    }

    /// Unlist a cartridge. `false` when it is not this server's.
    pub async fn remove_cartridge(&self, guild: i64, id: i64) -> ActivityResult<bool> {
        let r = sqlx::query("DELETE FROM arcade_guild_cartridges WHERE id = $1 AND guild_id = $2")
            .bind(id)
            .bind(guild)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Whether the reviewed build of `app` is a desktop-only (Tier 2) game: the browser refuses to launch it.
    pub async fn is_desktop_only(&self, app: i64) -> ActivityResult<bool> {
        desktop_only(&self.pool, app).await
    }
}

/// See [`ArcadeService::is_desktop_only`].
pub(crate) async fn desktop_only(pool: &PgPool, app: i64) -> ActivityResult<bool> {
    {
        let tier: Option<Option<String>> = sqlx::query_scalar(
            "SELECT b.tier FROM application_activities c \
             JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id \
             WHERE c.application_id = $1",
        )
        .bind(app)
        .fetch_optional(pool)
        .await?;
        Ok(tier
            .flatten()
            .as_deref()
            .and_then(Tier::parse)
            .is_some_and(|t| t.eligibility().desktop_only))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Manifest, BundleError> {
        parse_manifest(json.as_bytes())
    }

    #[test]
    fn exactly_two_tiers_parse() {
        assert_eq!(Tier::parse("TIER_1_ORIGINAL"), Some(Tier::Original));
        assert_eq!(
            Tier::parse("TIER_2_REIMPLEMENTATION"),
            Some(Tier::Reimplementation)
        );
        for other in [
            "TIER_3_FAN_IP",
            "TIER_3",
            "tier_1_original",
            "TIER_1_ORIGINAL ",
            "",
            "FAN_IP",
        ] {
            assert_eq!(Tier::parse(other), None, "{other:?}");
        }
        assert_eq!(Tier::ALL.len(), 2);
    }

    #[test]
    fn eligibility_follows_the_tier() {
        let t1 = Tier::Original.eligibility();
        assert!(t1.browser_play && t1.guest_links && t1.featured);
        assert!(!t1.desktop_only && !t1.needs_base_game_hint);
        let t2 = Tier::Reimplementation.eligibility();
        assert!(!t2.browser_play && !t2.guest_links);
        assert!(t2.featured && t2.desktop_only && t2.needs_base_game_hint);
    }

    #[test]
    fn a_manifest_declares_a_tier() {
        let m = parse(r#"{"tier":"TIER_1_ORIGINAL","capabilities":["datachannel"]}"#).unwrap();
        assert_eq!(m.tier, Some(Tier::Original));
        assert_eq!(m.capabilities, vec!["datachannel"]);
        // No tier: still a valid manifest (older bundles), but not one the Arcade lists.
        assert_eq!(parse("{}").unwrap().tier, None);
    }

    #[test]
    fn the_validator_rejects_a_third_tier() {
        for body in [
            r#"{"tier":"TIER_3_FAN_IP"}"#,
            r#"{"tier":"TIER_3"}"#,
            r#"{"tier":"original"}"#,
            r#"{"tier":""}"#,
            r#"{"tier":1}"#,
            r#"{"tier":["TIER_1_ORIGINAL"]}"#,
            r#"{"tier":null}"#,
        ] {
            let e = parse(body).unwrap_err();
            assert_eq!(e.entry.as_deref(), Some("sideroom.json"), "{body}");
        }
        let e = parse(r#"{"tier":"TIER_3_FAN_IP"}"#).unwrap_err();
        assert!(e.message.contains("TIER_1_ORIGINAL"), "{}", e.message);
    }

    #[test]
    fn general_flags_are_accepted_and_reserved_ones_are_inert() {
        let m = parse(
            r#"{"tier":"TIER_1_ORIGINAL","capabilities":["asset_cache","ingame_voice","lobby_channel","guest_play","byos_read","byos_write","datachannel"]}"#,
        )
        .unwrap();
        assert_eq!(m.capabilities.len(), 7);
        assert!(is_active("datachannel") && is_active("byos_read"));
        for n in [
            "guest_play",
            "lobby_channel",
            "ingame_voice",
            "byos_write",
            "external_launch",
            "asset_cache",
        ] {
            assert!(accepts(n) && !is_active(n), "{n}");
        }
        assert!(!accepts("root") && !is_active("root"));
        assert!(parse(r#"{"capabilities":["root"]}"#).is_err());
    }

    #[test]
    fn flags_are_tied_to_tiers() {
        assert!(parse(r#"{"capabilities":["guest_play"]}"#).is_err());
        assert!(
            parse(r#"{"tier":"TIER_2_REIMPLEMENTATION","capabilities":["guest_play"]}"#).is_err()
        );
        assert!(parse(r#"{"tier":"TIER_1_ORIGINAL","capabilities":["guest_play"]}"#).is_ok());
        assert!(parse(r#"{"tier":"TIER_1_ORIGINAL","capabilities":["external_launch"]}"#).is_err());
        assert!(
            parse(r#"{"tier":"TIER_2_REIMPLEMENTATION","capabilities":["external_launch"]}"#)
                .is_ok()
        );
    }

    #[test]
    fn manifest_tip_links_use_the_allow_list() {
        let ok = parse(
            r#"{"tier":"TIER_1_ORIGINAL","tip_links":[{"kind":"kofi","url":"https://ko-fi.com/dev"},{"kind":"btc","url":"bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"}]}"#,
        )
        .unwrap();
        assert_eq!(ok.tip_links.len(), 2);
        for bad in [
            r#"{"tip_links":[{"kind":"kofi","url":"https://evil.example/dev"}]}"#,
            r#"{"tip_links":[{"kind":"paypal","url":"https://paypal.me/dev"}]}"#,
            r#"{"tip_links":[{"kind":"kofi","url":"https://ko-fi.com/a"},{"kind":"kofi","url":"https://ko-fi.com/b"}]}"#,
            r#"{"tip_links":"https://ko-fi.com/dev"}"#,
            r#"{"tip_links":[{"kind":"kofi","url":"https://ko-fi.com/a","extra":1}]}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn repo_links_are_plain_https() {
        assert!(repo_url("https://github.com/openmw/openmw").is_ok());
        for bad in [
            "http://github.com/x/y",
            "https://user:pw@github.com/x/y",
            "https://github.com:8443/x/y",
            "javascript:alert(1)",
            "https://github.com/x/y#frag",
            "github.com/x/y",
        ] {
            assert!(repo_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn shelf_locators_cannot_climb() {
        assert!(source_ref("shelf-1/games/pong.zip").is_ok());
        for bad in ["", "../x", "a/../b", "a b", "a\\b", "a/./b"] {
            assert!(source_ref(bad).is_err(), "{bad}");
        }
    }
}
