//! Per-block and per-audience profile privacy (PRF-PRO-001, D-382). Pure policy: no database, no I/O, so the
//! property test can walk every tier against every viewer rank.
//!
//! The tiers are ordered `public` (0) < `members` (1) < `mutual_servers` (2) < `friends` (3) < `only_me` (4).
//! A viewer's rank is the highest tier they qualify for: anonymous and crawlers 0, a signed-in account of this
//! instance 1, one that shares a server with the owner 2, a mutual friend 3, the owner 4. A block renders when
//! the viewer's rank is at least the block's tier and at least the page's tier; a block can be stricter than
//! the page, never looser (the clamp). A missing row is the `private` preset: every tier `only_me`.
//!
//! The policy only ever narrows what the document says: [`PrivacyPolicy::visible`] is the one question every
//! caller asks, and the service builds a page from the blocks it admits, never by removing fields from a page
//! it already built.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::profile_doc::BlockKind;

/// One audience level. The derived order is the rank order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[schema(as = ProfileTier)]
pub enum Tier {
    /// Anyone on the web (only effective when the instance allows public profiles).
    Public,
    /// Any signed-in account of this instance.
    Members,
    /// Accounts that share a server with the owner.
    MutualServers,
    /// Mutual friends.
    Friends,
    /// Only the owner.
    OnlyMe,
}

/// The rank of a viewer is the highest [`Tier`] they qualify for.
pub type Rank = Tier;

impl Tier {
    pub const ALL: [Tier; 5] = [
        Tier::Public,
        Tier::Members,
        Tier::MutualServers,
        Tier::Friends,
        Tier::OnlyMe,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Members => "members",
            Self::MutualServers => "mutual_servers",
            Self::Friends => "friends",
            Self::OnlyMe => "only_me",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "public" => Self::Public,
            "members" => Self::Members,
            "mutual_servers" => Self::MutualServers,
            "friends" => Self::Friends,
            "only_me" => Self::OnlyMe,
            _ => return None,
        })
    }
}

/// The named bundles of the settings panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ProfilePreset)]
pub enum Preset {
    Public,
    Members,
    Friends,
    Private,
    /// Any explicit write.
    Custom,
}

impl Preset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Members => "members",
            Self::Friends => "friends",
            Self::Private => "private",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "public" => Self::Public,
            "members" => Self::Members,
            "friends" => Self::Friends,
            "private" => Self::Private,
            "custom" => Self::Custom,
            _ => return None,
        })
    }
}

/// The audience of the page and of each kind of block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfilePrivacyFields)]
pub struct PrivacyPolicy {
    pub page_tier: Tier,
    pub links_tier: Tier,
    pub top8_tier: Tier,
    pub now_playing_tier: Tier,
    pub bio_tier: Tier,
    pub guestbook_tier: Tier,
    pub media_tier: Tier,
    pub ring_tier: Tier,
    /// Anonymous visitors may leave a (held) guestbook post when the page and the guestbook are public.
    pub guestbook_accepts_public: bool,
}

impl Default for PrivacyPolicy {
    /// The `private` preset, which is also what a missing row means.
    fn default() -> Self {
        Self::all(Tier::OnlyMe, Tier::OnlyMe)
    }
}

impl PrivacyPolicy {
    fn all(tier: Tier, ring: Tier) -> Self {
        Self {
            page_tier: tier,
            links_tier: tier,
            top8_tier: tier,
            now_playing_tier: tier,
            bio_tier: tier,
            guestbook_tier: tier,
            media_tier: tier,
            ring_tier: ring,
            guestbook_accepts_public: false,
        }
    }

    /// The fields a named preset fills in. `Custom` has no fields of its own and gives the private ones.
    pub fn from_preset(p: Preset) -> Self {
        match p {
            Preset::Public => Self {
                page_tier: Tier::Public,
                links_tier: Tier::Public,
                bio_tier: Tier::Public,
                media_tier: Tier::Public,
                top8_tier: Tier::Members,
                now_playing_tier: Tier::Members,
                guestbook_tier: Tier::Members,
                ring_tier: Tier::Friends,
                guestbook_accepts_public: false,
            },
            Preset::Members => Self::all(Tier::Members, Tier::Friends),
            Preset::Friends => Self::all(Tier::Friends, Tier::Friends),
            Preset::Private | Preset::Custom => Self::default(),
        }
    }

    /// Every block at least as strict as the page: a block can be stricter than the page, never looser.
    #[must_use]
    pub fn clamped(mut self) -> Self {
        let page = self.page_tier;
        for t in [
            &mut self.links_tier,
            &mut self.top8_tier,
            &mut self.now_playing_tier,
            &mut self.bio_tier,
            &mut self.guestbook_tier,
            &mut self.media_tier,
            &mut self.ring_tier,
        ] {
            *t = (*t).max(page);
        }
        // Anonymous posts need a public page and a public guestbook; anything else cannot honour the flag.
        if self.page_tier != Tier::Public || self.guestbook_tier != Tier::Public {
            self.guestbook_accepts_public = false;
        }
        self
    }

    /// What is in force on an instance. When the operator's switch (`platform.public_profiles`) is off, the
    /// `public` tier is stored but never effective: it is raised to `members`, so no anonymous rank qualifies.
    #[must_use]
    pub fn effective(self, public_possible: bool) -> Self {
        let mut p = self.clamped();
        if !public_possible {
            for t in [
                &mut p.page_tier,
                &mut p.links_tier,
                &mut p.top8_tier,
                &mut p.now_playing_tier,
                &mut p.bio_tier,
                &mut p.guestbook_tier,
                &mut p.media_tier,
                &mut p.ring_tier,
            ] {
                *t = (*t).max(Tier::Members);
            }
            p.guestbook_accepts_public = false;
        }
        p
    }

    /// The tier that governs one kind of block, before the page tier is applied.
    pub fn block_tier(&self, kind: BlockKind) -> Tier {
        match kind {
            BlockKind::Header | BlockKind::Custom | BlockKind::Favorites | BlockKind::Song => {
                self.page_tier
            }
            BlockKind::About => self.bio_tier,
            BlockKind::TopFriends | BlockKind::FeaturedGuilds => self.top8_tier,
            BlockKind::NowWatching | BlockKind::NowPlaying => self.now_playing_tier,
            BlockKind::Gallery => self.media_tier,
            BlockKind::Guestbook => self.guestbook_tier,
            BlockKind::Linktree => self.links_tier,
        }
    }

    /// The whole page is visible to this rank.
    pub fn page_visible(&self, rank: Rank) -> bool {
        rank >= self.page_tier
    }

    /// A block of this kind is visible to this rank: at least the block's tier and at least the page's.
    pub fn visible(&self, kind: BlockKind, rank: Rank) -> bool {
        rank >= self.block_tier(kind).max(self.page_tier)
    }

    /// The guestbook may be signed by a signed-in viewer of this rank (the owner always may).
    pub fn may_sign(&self, rank: Rank) -> bool {
        rank == Tier::OnlyMe || (rank >= Tier::Members && self.visible(BlockKind::Guestbook, rank))
    }
}

/// Where a viewer stands with the owner of a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Either side blocked the other: the page is 404 and nothing "only me" is hidden from them because
    /// nothing is shown at all.
    Blocked,
    Viewer(Rank),
}

/// The facts a rank is made of.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    pub signed_in: bool,
    pub is_owner: bool,
    pub blocked: bool,
    pub shares_server: bool,
    pub friends: bool,
}

/// The rank of a viewer from the facts. Blocking wins over everything, the owner included in the sense that the
/// owner is never blocked from themselves.
pub fn standing(f: Facts) -> Standing {
    if f.is_owner {
        return Standing::Viewer(Tier::OnlyMe);
    }
    if f.blocked {
        return Standing::Blocked;
    }
    Standing::Viewer(if !f.signed_in {
        Tier::Public
    } else if f.friends {
        Tier::Friends
    } else if f.shares_server {
        Tier::MutualServers
    } else {
        Tier::Members
    })
}

/// The kinds of block the document model has, for walks over "every block".
pub const ALL_KINDS: [BlockKind; 12] = [
    BlockKind::Header,
    BlockKind::About,
    BlockKind::TopFriends,
    BlockKind::NowWatching,
    BlockKind::Favorites,
    BlockKind::Gallery,
    BlockKind::Guestbook,
    BlockKind::Song,
    BlockKind::Custom,
    BlockKind::Linktree,
    BlockKind::FeaturedGuilds,
    BlockKind::NowPlaying,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with(page: Tier, block: Tier) -> PrivacyPolicy {
        PrivacyPolicy {
            page_tier: page,
            links_tier: block,
            top8_tier: block,
            now_playing_tier: block,
            bio_tier: block,
            guestbook_tier: block,
            media_tier: block,
            ring_tier: block,
            guestbook_accepts_public: true,
        }
    }

    #[test]
    fn the_missing_row_is_private() {
        let p = PrivacyPolicy::default();
        for kind in ALL_KINDS {
            for rank in Tier::ALL {
                assert_eq!(
                    p.visible(kind, rank),
                    rank == Tier::OnlyMe,
                    "{kind:?} {rank:?}"
                );
            }
        }
    }

    #[test]
    fn every_tier_pair_and_rank_follows_the_table() {
        for page in Tier::ALL {
            for block in Tier::ALL {
                let p = policy_with(page, block).clamped();
                for kind in ALL_KINDS {
                    for rank in Tier::ALL {
                        let own = p.block_tier(kind);
                        let want = rank >= own && rank >= page;
                        assert_eq!(
                            p.visible(kind, rank),
                            want,
                            "{kind:?} p={page:?} b={block:?} r={rank:?}"
                        );
                        // Never looser than the page, whatever the block says.
                        if rank < page {
                            assert!(!p.visible(kind, rank));
                        }
                    }
                }
                // The clamp: no block tier is below the page tier.
                for t in [
                    p.links_tier,
                    p.top8_tier,
                    p.bio_tier,
                    p.guestbook_tier,
                    p.media_tier,
                ] {
                    assert!(t >= page);
                }
            }
        }
    }

    #[test]
    fn without_the_operator_switch_nobody_anonymous_qualifies() {
        for page in Tier::ALL {
            for block in Tier::ALL {
                let p = policy_with(page, block).effective(false);
                for kind in ALL_KINDS {
                    assert!(!p.visible(kind, Tier::Public), "{kind:?}");
                }
                assert!(!p.page_visible(Tier::Public));
                assert!(!p.guestbook_accepts_public);
            }
        }
    }

    #[test]
    fn presets_are_the_table() {
        let public = PrivacyPolicy::from_preset(Preset::Public).effective(true);
        assert!(public.visible(BlockKind::Linktree, Tier::Public));
        assert!(public.visible(BlockKind::About, Tier::Public));
        assert!(public.visible(BlockKind::Gallery, Tier::Public));
        assert!(!public.visible(BlockKind::TopFriends, Tier::Public));
        assert!(public.visible(BlockKind::TopFriends, Tier::Members));
        assert!(!public.visible(BlockKind::Guestbook, Tier::Public));
        assert_eq!(public.ring_tier, Tier::Friends);
        let members = PrivacyPolicy::from_preset(Preset::Members);
        assert!(!members.page_visible(Tier::Public));
        assert!(members.page_visible(Tier::Members));
        let friends = PrivacyPolicy::from_preset(Preset::Friends);
        assert!(!friends.page_visible(Tier::MutualServers));
        assert!(friends.page_visible(Tier::Friends));
        let private = PrivacyPolicy::from_preset(Preset::Private);
        assert!(!private.page_visible(Tier::Friends));
        assert!(private.page_visible(Tier::OnlyMe));
    }

    #[test]
    fn a_looser_block_on_a_stricter_page_is_clamped() {
        let mut p = PrivacyPolicy::from_preset(Preset::Friends);
        p.links_tier = Tier::Public;
        let p = p.clamped();
        assert_eq!(p.links_tier, Tier::Friends);
        assert!(!p.visible(BlockKind::Linktree, Tier::Members));
    }

    #[test]
    fn standing_ranks() {
        let f = Facts {
            signed_in: true,
            ..Facts::default()
        };
        assert_eq!(standing(Facts::default()), Standing::Viewer(Tier::Public));
        assert_eq!(standing(f), Standing::Viewer(Tier::Members));
        assert_eq!(
            standing(Facts {
                shares_server: true,
                ..f
            }),
            Standing::Viewer(Tier::MutualServers)
        );
        assert_eq!(
            standing(Facts {
                shares_server: true,
                friends: true,
                ..f
            }),
            Standing::Viewer(Tier::Friends)
        );
        assert_eq!(
            standing(Facts {
                blocked: true,
                friends: true,
                ..f
            }),
            Standing::Blocked
        );
        assert_eq!(
            standing(Facts {
                is_owner: true,
                ..f
            }),
            Standing::Viewer(Tier::OnlyMe)
        );
    }

    #[test]
    fn signing_needs_a_signed_in_viewer_who_can_see_the_book() {
        let mut p = PrivacyPolicy::from_preset(Preset::Public);
        p.guestbook_tier = Tier::Members;
        let p = p.clamped();
        assert!(!p.may_sign(Tier::Public));
        assert!(p.may_sign(Tier::Members));
        assert!(p.may_sign(Tier::OnlyMe));
        let mut f = PrivacyPolicy::from_preset(Preset::Public);
        f.guestbook_tier = Tier::Friends;
        let f = f.clamped();
        assert!(!f.may_sign(Tier::MutualServers));
        assert!(f.may_sign(Tier::Friends));
    }
}
