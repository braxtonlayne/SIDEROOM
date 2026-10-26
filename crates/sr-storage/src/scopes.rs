//! Who may read a storage mount (BYOS-SCP-001).
//!
//! [`resolve`] is the one function that decides every read of a mount: every listing and presign route
//! calls it, and it fails closed. A non-member, a non-friend, a friend without the key's kind share, a role
//! without the grant, a delisted key, a disabled mount, a mount whose owner is not active: all [`Refused`].
//!
//! The decision needs facts from the database (friendship, share rows, guild standing, grants). They come
//! through [`ScopeFacts`], so the logic here is database free: `sr_db::scopes_repo::PgScopeFacts` is the
//! production implementation and [`MemoryScopeFacts`] backs the unit tests. Staff are admitted by the caller
//! saying so in [`Viewer::staff`] (the route proves `OPERATOR` or a moderation case); a staff read of a
//! mount that is not theirs comes back as [`Scope::Staff`] and the route must log it (`storage.read`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::mounts::{Mount, MountOwner, MountStatus};

/// A kind of shelf. Friend shares and guild view grants are per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Films and shows.
    Cinema,
    /// Comics, books, documents.
    Library,
    /// Audio.
    Music,
}

impl Kind {
    /// The wire / column name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cinema => "cinema",
            Self::Library => "library",
            Self::Music => "music",
        }
    }
    /// Parse the wire name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cinema" => Some(Self::Cinema),
            "library" => Some(Self::Library),
            "music" => Some(Self::Music),
            _ => None,
        }
    }
    /// The kind a key belongs to, from its file extension. `None` is a companion file (cover, poster,
    /// subtitle, sidecar, folder marker): it is readable with any share of the mount, because a shelf
    /// without its covers and subtitles is useless.
    pub fn of_key(key: &str) -> Option<Self> {
        let name = key.rsplit('/').next().unwrap_or(key);
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "mkv" | "mp4" | "m4v" | "webm" | "avi" | "mov" | "ts" | "m2ts" | "mpg" | "mpeg"
            | "ogv" | "m3u8" => Some(Self::Cinema),
            "cbz" | "cbr" | "cb7" | "epub" | "pdf" | "mobi" | "azw3" | "fb2" | "djvu" => {
                Some(Self::Library)
            }
            "mp3" | "flac" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wav" | "wma" | "aiff"
            | "alac" | "mka" => Some(Self::Music),
            _ => None,
        }
    }
}

/// The five storage grants a guild role can carry (`guild_storage_grants.grant_name`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grant {
    /// `VIEW_GUILD_CINEMA`.
    ViewCinema,
    /// `VIEW_GUILD_LIBRARY`.
    ViewLibrary,
    /// `VIEW_GUILD_MUSIC`.
    ViewMusic,
    /// `UPLOAD_TO_GUILD_STORAGE`.
    Upload,
    /// `MANAGE_GUILD_STORAGE`.
    Manage,
}

impl Grant {
    /// Every grant, in the order the role editor shows them.
    pub const ALL: [Grant; 5] = [
        Self::ViewCinema,
        Self::ViewLibrary,
        Self::ViewMusic,
        Self::Upload,
        Self::Manage,
    ];
    /// The column / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ViewCinema => "view_cinema",
            Self::ViewLibrary => "view_library",
            Self::ViewMusic => "view_music",
            Self::Upload => "upload",
            Self::Manage => "manage",
        }
    }
    /// Parse the column / wire name.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == s)
    }
    /// The view grant of a kind.
    pub fn view_of(kind: Kind) -> Self {
        match kind {
            Kind::Cinema => Self::ViewCinema,
            Kind::Library => Self::ViewLibrary,
            Kind::Music => Self::ViewMusic,
        }
    }
}

/// A set of [`Grant`]s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantSet(BTreeSet<Grant>);

impl GrantSet {
    /// Every grant (owner and administrator).
    pub fn all() -> Self {
        Self(Grant::ALL.into_iter().collect())
    }
    /// Build from grants.
    pub fn of(grants: impl IntoIterator<Item = Grant>) -> Self {
        Self(grants.into_iter().collect())
    }
    /// Add one.
    pub fn insert(&mut self, g: Grant) {
        self.0.insert(g);
    }
    /// Membership.
    pub fn has(&self, g: Grant) -> bool {
        self.0.contains(&g)
    }
    /// Whether any of the three view grants, or `manage`, is held.
    pub fn can_see_any(&self) -> bool {
        self.has(Grant::Manage)
            || self.has(Grant::ViewCinema)
            || self.has(Grant::ViewLibrary)
            || self.has(Grant::ViewMusic)
    }
    /// Whether this set lets the holder read a key of `kind` (`None`: a companion file). `manage` reads
    /// everything: whoever administers a shelf sees it.
    pub fn can_view(&self, kind: Option<Kind>) -> bool {
        match kind {
            Some(k) => self.has(Grant::Manage) || self.has(Grant::view_of(k)),
            None => self.can_see_any(),
        }
    }
    /// The grants as wire names, in the role editor's order.
    pub fn names(&self) -> Vec<&'static str> {
        Grant::ALL
            .into_iter()
            .filter(|g| self.has(*g))
            .map(Grant::as_str)
            .collect()
    }
}

/// One Friend Shelf row: which kinds the owner shares with one friend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Share {
    /// Cinema shared.
    pub cinema: bool,
    /// Library shared.
    pub library: bool,
    /// Music shared.
    pub music: bool,
}

impl Share {
    /// Whether any kind is shared.
    pub fn any(self) -> bool {
        self.cinema || self.library || self.music
    }
    /// Whether a key of `kind` may be read (`None`: a companion file, any share admits it).
    pub fn admits(self, kind: Option<Kind>) -> bool {
        match kind {
            Some(Kind::Cinema) => self.cinema,
            Some(Kind::Library) => self.library,
            Some(Kind::Music) => self.music,
            None => self.any(),
        }
    }
}

/// What the viewer is to a guild. `None` from [`ScopeFacts::guild_standing`] means not a member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildStanding {
    /// The viewer owns the guild.
    pub is_owner: bool,
    /// The viewer's roles include `ADMINISTRATOR`.
    pub administrator: bool,
    /// The viewer's roles include `MANAGE_GUILD`.
    pub manage_guild: bool,
    /// The viewer's role ids (without the implicit everyone role: the table keys on real role rows; the
    /// everyone role id equals the guild id and the implementation must include it).
    pub role_ids: Vec<i64>,
}

/// The grants a member holds in a guild: owner and administrator hold all, `MANAGE_GUILD` implies `manage`,
/// everyone else holds the union of their roles' rows.
pub fn grants_for(standing: &GuildStanding, role_grants: &GrantSet) -> GrantSet {
    if standing.is_owner || standing.administrator {
        return GrantSet::all();
    }
    let mut set = role_grants.clone();
    if standing.manage_guild {
        set.insert(Grant::Manage);
    }
    set
}

/// The viewer asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    /// Account id.
    pub id: i64,
    /// The route has proven staff standing for this read (`OPERATOR`, or a moderation case that touches the
    /// key). Never set from a client claim.
    pub staff: bool,
}

/// How the viewer reaches the mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The viewer's own mount.
    Own,
    /// A friend's mount shared with the viewer.
    Friend {
        /// The friend who owns the mount.
        owner: i64,
    },
    /// A guild shelf the viewer reaches through a role.
    Guild(i64),
    /// Staff reading a mount that is not theirs. The route must write `staff_access_log`.
    Staff,
}

/// Why a read is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// No such mount (or deleted).
    MountMissing,
    /// The mount is switched off.
    Disabled,
    /// The mount's owner is suspended, banned, deactivated or deleted.
    OwnerInactive,
    /// The mount belongs to someone the viewer is not friends with.
    NotFriend,
    /// A friend, but the owner shares nothing, or not this key's kind.
    NoShare,
    /// Not a member of the guild that owns or shelves the mount.
    NotMember,
    /// A member whose roles lack the grant.
    NoGrant,
    /// The key was delisted (takedown).
    Delisted,
}

/// How a refusal is shown to the caller. The wire convention: a mount the viewer cannot see at all is 404
/// (no hint that it exists); a guild member who lacks only a grant gets 403.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disclosure {
    /// Answer 404.
    NotFound,
    /// Answer 403.
    Forbidden,
}

impl Refused {
    /// The status class for this refusal.
    pub fn disclosure(self) -> Disclosure {
        match self {
            Self::NoGrant => Disclosure::Forbidden,
            _ => Disclosure::NotFound,
        }
    }
}

/// The facts store failed. Callers fail closed: answer 503/500, never admit.
#[derive(Debug, thiserror::Error)]
#[error("scope facts unavailable: {0}")]
pub struct FactsError(pub String);

/// The outcome of [`resolve`]: a scope, a refusal, or a store failure.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// Refused.
    #[error("refused: {0:?}")]
    Refused(Refused),
    /// The store failed; nothing is admitted.
    #[error(transparent)]
    Facts(#[from] FactsError),
}

impl From<Refused> for ResolveError {
    fn from(r: Refused) -> Self {
        Self::Refused(r)
    }
}

/// The database facts the decision reads.
#[async_trait]
pub trait ScopeFacts: Send + Sync {
    /// Whether the mount's owner may still serve it: a user whose account is `active`, a guild that exists
    /// and whose owner account is active, the platform always.
    async fn owner_active(&self, owner: MountOwner) -> Result<bool, FactsError>;
    /// Whether `a` and `b` are mutual friends (both `relationships` rows are type 1).
    async fn are_friends(&self, a: i64, b: i64) -> Result<bool, FactsError>;
    /// The share the mount's owner gave `friend`.
    async fn share(&self, mount_id: i64, friend: i64) -> Result<Option<Share>, FactsError>;
    /// The viewer's standing in a guild; `None` when not a member.
    async fn guild_standing(
        &self,
        guild_id: i64,
        user_id: i64,
    ) -> Result<Option<GuildStanding>, FactsError>;
    /// The union of the grant rows of the given roles in the guild.
    async fn role_grants(&self, guild_id: i64, role_ids: &[i64]) -> Result<GrantSet, FactsError>;
    /// Guilds a non-guild mount is attached to as a Guild Shelf (the instance mount, BYOS-INS-001).
    async fn attached_guilds(&self, mount_id: i64) -> Result<Vec<i64>, FactsError>;
    /// Which of `keys` are delisted on this mount.
    async fn delisted(&self, mount_id: i64, keys: &[String]) -> Result<Vec<String>, FactsError>;
}

/// The viewer's reach into one mount, before any key is looked at. Everything that needs the database is
/// done once here; [`Access::permits`] then answers per key without a round trip, which is what a listing
/// page of 1000 objects needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    /// How the viewer reaches the mount.
    pub scope: Scope,
    /// For a friend: what is shared. For a guild: the grants, mapped onto a share (`manage` shares all).
    pub kinds: Share,
    /// Whether every key is readable regardless of kind (own, staff).
    pub unrestricted: bool,
    /// The mount the access is for.
    pub mount_id: i64,
}

impl Access {
    /// Whether a key of this key's kind may be read, ignoring delisting.
    pub fn permits(&self, key: &str) -> Result<(), Refused> {
        if self.unrestricted || self.kinds.admits(Kind::of_key(key)) {
            return Ok(());
        }
        Err(match self.scope {
            Scope::Guild(_) => Refused::NoGrant,
            _ => Refused::NoShare,
        })
    }
}

/// Work out how `viewer` reaches `mount` (steps 1 to 3 of [`resolve`]). Fails closed.
pub async fn access(
    facts: &dyn ScopeFacts,
    viewer: Viewer,
    mount: &Mount,
) -> Result<Access, ResolveError> {
    if mount.status == MountStatus::Disabled {
        return Err(Refused::Disabled.into());
    }
    if !facts.owner_active(mount.owner).await? {
        return Err(Refused::OwnerInactive.into());
    }
    let mk = |scope, kinds, unrestricted| Access {
        scope,
        kinds,
        unrestricted,
        mount_id: mount.id,
    };
    let all = Share {
        cinema: true,
        library: true,
        music: true,
    };
    // The owner reads their own mount.
    if mount.owner == MountOwner::User(viewer.id) {
        return Ok(mk(Scope::Own, all, true));
    }
    // A guild shelf: the owning guild, or a guild the mount is attached to.
    let mut guilds: Vec<i64> = Vec::new();
    match mount.owner {
        MountOwner::Guild(g) => guilds.push(g),
        MountOwner::Platform => guilds.extend(facts.attached_guilds(mount.id).await?),
        MountOwner::User(_) => {}
    }
    let mut best: Option<Access> = None;
    let mut saw_member = false;
    for g in guilds {
        let Some(standing) = facts.guild_standing(g, viewer.id).await? else {
            continue;
        };
        saw_member = true;
        let role_grants = facts.role_grants(g, &standing.role_ids).await?;
        let grants = grants_for(&standing, &role_grants);
        let kinds = if grants.has(Grant::Manage) {
            all
        } else {
            Share {
                cinema: grants.has(Grant::ViewCinema),
                library: grants.has(Grant::ViewLibrary),
                music: grants.has(Grant::ViewMusic),
            }
        };
        if kinds.any() && best.as_ref().is_none_or(|b| !b.kinds.any()) {
            best = Some(mk(Scope::Guild(g), kinds, false));
        }
    }
    if let Some(a) = best {
        return Ok(a);
    }
    // A friend's mount.
    let friend_refusal = match mount.owner {
        MountOwner::User(owner) => {
            if facts.are_friends(owner, viewer.id).await? {
                match facts.share(mount.id, viewer.id).await? {
                    Some(s) if s.any() => {
                        return Ok(mk(Scope::Friend { owner }, s, false));
                    }
                    _ => Some(Refused::NoShare),
                }
            } else {
                Some(Refused::NotFriend)
            }
        }
        _ => None,
    };
    // Staff reach any mount that is not theirs; the route logs it.
    if viewer.staff {
        return Ok(mk(Scope::Staff, all, true));
    }
    Err(friend_refusal
        .unwrap_or(if saw_member {
            Refused::NoGrant
        } else {
            Refused::NotMember
        })
        .into())
}

/// The single decision function: may `viewer` read `key` of `mount`? `key` is mount-relative and must
/// already have passed the key check; an empty key is the mount root (a listing of the top).
pub async fn resolve(
    facts: &dyn ScopeFacts,
    viewer: Viewer,
    mount: &Mount,
    key: &str,
) -> Result<Scope, ResolveError> {
    let acc = access(facts, viewer, mount).await?;
    // The root and folder prefixes carry no kind: reaching the mount is enough. Objects need their kind.
    if !key.is_empty() && !key.ends_with('/') {
        acc.permits(key)?;
        if !facts
            .delisted(mount.id, std::slice::from_ref(&key.to_owned()))
            .await?
            .is_empty()
        {
            return Err(Refused::Delisted.into());
        }
    }
    Ok(acc.scope)
}

/// Whether `viewer` may upload to `mount` (wave 28's write routes call this): the owner of a user mount; on
/// a guild shelf the owner, administrator or a role with `upload` or `manage`. Fail closed.
pub async fn may_upload(
    facts: &dyn ScopeFacts,
    viewer: Viewer,
    mount: &Mount,
) -> Result<bool, FactsError> {
    guild_right(facts, viewer, mount, |g| {
        g.has(Grant::Upload) || g.has(Grant::Manage)
    })
    .await
}

/// Whether `viewer` may manage `mount` (settings, shares of a guild shelf, takedown): the owner of a user
/// mount; on a guild shelf the owner, administrator, `MANAGE_GUILD` or a role with `manage`.
pub async fn may_manage(
    facts: &dyn ScopeFacts,
    viewer: Viewer,
    mount: &Mount,
) -> Result<bool, FactsError> {
    guild_right(facts, viewer, mount, |g| g.has(Grant::Manage)).await
}

async fn guild_right(
    facts: &dyn ScopeFacts,
    viewer: Viewer,
    mount: &Mount,
    ok: impl Fn(&GrantSet) -> bool,
) -> Result<bool, FactsError> {
    match mount.owner {
        MountOwner::User(u) => Ok(u == viewer.id),
        MountOwner::Platform => Ok(false),
        MountOwner::Guild(g) => {
            let Some(standing) = facts.guild_standing(g, viewer.id).await? else {
                return Ok(false);
            };
            let role_grants = facts.role_grants(g, &standing.role_ids).await?;
            Ok(ok(&grants_for(&standing, &role_grants)))
        }
    }
}

/// The grants `user` holds in `guild` (for the role editor's readout and `GET` of the member's rights).
pub async fn guild_grants(
    facts: &dyn ScopeFacts,
    guild_id: i64,
    user_id: i64,
) -> Result<GrantSet, FactsError> {
    let Some(standing) = facts.guild_standing(guild_id, user_id).await? else {
        return Ok(GrantSet::default());
    };
    let role_grants = facts.role_grants(guild_id, &standing.role_ids).await?;
    Ok(grants_for(&standing, &role_grants))
}

/// In-memory [`ScopeFacts`] for tests and for the other agents to build against.
#[derive(Debug, Default)]
pub struct MemoryScopeFacts {
    inner: Mutex<MemoryFacts>,
}

#[derive(Debug, Default)]
struct MemoryFacts {
    inactive: BTreeSet<MountOwner>,
    friends: BTreeSet<(i64, i64)>,
    shares: BTreeMap<(i64, i64), Share>,
    standing: BTreeMap<(i64, i64), GuildStanding>,
    role_grants: BTreeMap<(i64, i64), GrantSet>,
    attached: BTreeMap<i64, Vec<i64>>,
    delisted: BTreeSet<(i64, String)>,
    fail: bool,
}

impl MemoryScopeFacts {
    /// An empty store: nobody is anybody's friend or member.
    pub fn new() -> Self {
        Self::default()
    }
    fn with<R>(&self, f: impl FnOnce(&mut MemoryFacts) -> R) -> R {
        let mut g = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }
    /// Mark an owner suspended.
    pub fn suspend(&self, owner: MountOwner) {
        self.with(|m| m.inactive.insert(owner));
    }
    /// Make two accounts mutual friends.
    pub fn befriend(&self, a: i64, b: i64) {
        self.with(|m| {
            m.friends.insert((a.min(b), a.max(b)));
        });
    }
    /// End a friendship and delete the shares between them on the given mount, as the hook does.
    pub fn unfriend(&self, a: i64, b: i64) {
        self.with(|m| {
            m.friends.remove(&(a.min(b), a.max(b)));
            m.shares.retain(|(_, f), _| *f != a && *f != b);
        });
    }
    /// Set a share.
    pub fn share_with(&self, mount_id: i64, friend: i64, s: Share) {
        self.with(|m| m.shares.insert((mount_id, friend), s));
    }
    /// Add a guild member with roles.
    pub fn member(&self, guild: i64, user: i64, st: GuildStanding) {
        self.with(|m| m.standing.insert((guild, user), st));
    }
    /// Give a role grants.
    pub fn grant(&self, guild: i64, role: i64, grants: GrantSet) {
        self.with(|m| m.role_grants.insert((guild, role), grants));
    }
    /// Attach a non-guild mount to a guild.
    pub fn attach(&self, mount_id: i64, guild: i64) {
        self.with(|m| m.attached.entry(mount_id).or_default().push(guild));
    }
    /// Delist a key.
    pub fn delist(&self, mount_id: i64, key: &str) {
        self.with(|m| m.delisted.insert((mount_id, key.to_owned())));
    }
    /// Make every call fail, to prove the decision fails closed.
    pub fn break_store(&self) {
        self.with(|m| m.fail = true);
    }
    fn check(&self) -> Result<(), FactsError> {
        if self.with(|m| m.fail) {
            Err(FactsError("store down".into()))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl ScopeFacts for MemoryScopeFacts {
    async fn owner_active(&self, owner: MountOwner) -> Result<bool, FactsError> {
        self.check()?;
        Ok(!self.with(|m| m.inactive.contains(&owner)))
    }
    async fn are_friends(&self, a: i64, b: i64) -> Result<bool, FactsError> {
        self.check()?;
        Ok(self.with(|m| m.friends.contains(&(a.min(b), a.max(b)))))
    }
    async fn share(&self, mount_id: i64, friend: i64) -> Result<Option<Share>, FactsError> {
        self.check()?;
        Ok(self.with(|m| m.shares.get(&(mount_id, friend)).copied()))
    }
    async fn guild_standing(
        &self,
        guild_id: i64,
        user_id: i64,
    ) -> Result<Option<GuildStanding>, FactsError> {
        self.check()?;
        Ok(self.with(|m| m.standing.get(&(guild_id, user_id)).cloned()))
    }
    async fn role_grants(&self, guild_id: i64, role_ids: &[i64]) -> Result<GrantSet, FactsError> {
        self.check()?;
        Ok(self.with(|m| {
            let mut out = GrantSet::default();
            for r in role_ids {
                if let Some(g) = m.role_grants.get(&(guild_id, *r)) {
                    for x in Grant::ALL {
                        if g.has(x) {
                            out.insert(x);
                        }
                    }
                }
            }
            out
        }))
    }
    async fn attached_guilds(&self, mount_id: i64) -> Result<Vec<i64>, FactsError> {
        self.check()?;
        Ok(self.with(|m| m.attached.get(&mount_id).cloned().unwrap_or_default()))
    }
    async fn delisted(&self, mount_id: i64, keys: &[String]) -> Result<Vec<String>, FactsError> {
        self.check()?;
        Ok(self.with(|m| {
            keys.iter()
                .filter(|k| m.delisted.contains(&(mount_id, (*k).clone())))
                .cloned()
                .collect()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mounts::{MountOwner, MountStatus, Provider, SealedSecret};

    const OWNER: i64 = 10;
    const FRIEND: i64 = 11;
    const STRANGER: i64 = 12;
    const GUILD: i64 = 500;
    const MOUNT: i64 = 900;
    const ROLE_MEMBER: i64 = 71;

    fn mount(owner: MountOwner) -> Mount {
        Mount {
            id: MOUNT,
            owner,
            label: "m".into(),
            provider: Provider::Garage,
            endpoint: "http://localhost:3900".into(),
            region: "garage".into(),
            bucket: "b".into(),
            prefix: String::new(),
            path_style: true,
            access_key_id: "AK".into(),
            secret_key_enc: SealedSecret::from_bytes(vec![1]),
            key_hint: "AK".into(),
            status: MountStatus::Active,
            cors_ok_at: Some(1),
            last_test_at: None,
            last_test: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn viewer(id: i64) -> Viewer {
        Viewer { id, staff: false }
    }

    fn standing(roles: &[i64]) -> GuildStanding {
        GuildStanding {
            role_ids: roles.to_vec(),
            ..GuildStanding::default()
        }
    }

    async fn go(f: &MemoryScopeFacts, v: Viewer, m: &Mount, key: &str) -> Result<Scope, Refused> {
        match resolve(f, v, m, key).await {
            Ok(s) => Ok(s),
            Err(ResolveError::Refused(r)) => Err(r),
            Err(ResolveError::Facts(e)) => panic!("facts failed: {e}"),
        }
    }

    #[tokio::test]
    async fn the_owner_reads_everything_of_their_own_mount() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::User(OWNER));
        assert_eq!(
            go(&f, viewer(OWNER), &m, "a/film.mkv").await,
            Ok(Scope::Own)
        );
        assert_eq!(go(&f, viewer(OWNER), &m, "").await, Ok(Scope::Own));
    }

    #[tokio::test]
    async fn a_stranger_is_refused_as_a_non_friend() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::User(OWNER));
        let r = go(&f, viewer(STRANGER), &m, "a.mp3").await;
        assert_eq!(r, Err(Refused::NotFriend));
        assert_eq!(r.unwrap_err().disclosure(), Disclosure::NotFound);
    }

    #[tokio::test]
    async fn a_friend_without_any_share_is_refused() {
        let f = MemoryScopeFacts::new();
        f.befriend(OWNER, FRIEND);
        let m = mount(MountOwner::User(OWNER));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::NoShare)
        );
        // A share row with every kind off is no share.
        f.share_with(MOUNT, FRIEND, Share::default());
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::NoShare)
        );
    }

    #[tokio::test]
    async fn a_share_without_friendship_is_refused() {
        // A stale share row must not outlive the friendship.
        let f = MemoryScopeFacts::new();
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                music: true,
                ..Share::default()
            },
        );
        let m = mount(MountOwner::User(OWNER));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::NotFriend)
        );
    }

    #[tokio::test]
    async fn a_friend_reads_only_the_kinds_shared() {
        let f = MemoryScopeFacts::new();
        f.befriend(OWNER, FRIEND);
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                music: true,
                ..Share::default()
            },
        );
        let m = mount(MountOwner::User(OWNER));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "x/song.FLAC").await,
            Ok(Scope::Friend { owner: OWNER })
        );
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "x/film.mkv").await,
            Err(Refused::NoShare)
        );
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "x/book.epub").await,
            Err(Refused::NoShare)
        );
        // A companion file (cover art) follows any share.
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "x/cover.jpg").await,
            Ok(Scope::Friend { owner: OWNER })
        );
    }

    #[tokio::test]
    async fn unfriending_revokes() {
        let f = MemoryScopeFacts::new();
        f.befriend(OWNER, FRIEND);
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                cinema: true,
                ..Share::default()
            },
        );
        let m = mount(MountOwner::User(OWNER));
        assert!(go(&f, viewer(FRIEND), &m, "f.mp4").await.is_ok());
        f.unfriend(OWNER, FRIEND);
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "f.mp4").await,
            Err(Refused::NotFriend)
        );
    }

    #[tokio::test]
    async fn a_non_member_of_the_guild_is_refused() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::Guild(GUILD));
        let r = go(&f, viewer(STRANGER), &m, "f.mp4").await;
        assert_eq!(r, Err(Refused::NotMember));
        assert_eq!(r.unwrap_err().disclosure(), Disclosure::NotFound);
    }

    #[tokio::test]
    async fn a_member_without_the_grant_gets_403() {
        let f = MemoryScopeFacts::new();
        f.member(GUILD, FRIEND, standing(&[ROLE_MEMBER]));
        f.grant(GUILD, ROLE_MEMBER, GrantSet::of([Grant::ViewMusic]));
        let m = mount(MountOwner::Guild(GUILD));
        let r = go(&f, viewer(FRIEND), &m, "f.mp4").await;
        assert_eq!(r, Err(Refused::NoGrant));
        assert_eq!(r.unwrap_err().disclosure(), Disclosure::Forbidden);
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "s.mp3").await,
            Ok(Scope::Guild(GUILD))
        );
    }

    #[tokio::test]
    async fn a_member_with_no_grant_at_all_gets_403() {
        let f = MemoryScopeFacts::new();
        f.member(GUILD, FRIEND, standing(&[]));
        let m = mount(MountOwner::Guild(GUILD));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "s.mp3").await,
            Err(Refused::NoGrant)
        );
    }

    #[tokio::test]
    async fn each_view_grant_opens_only_its_kind() {
        let f = MemoryScopeFacts::new();
        f.member(GUILD, FRIEND, standing(&[ROLE_MEMBER]));
        let m = mount(MountOwner::Guild(GUILD));
        for (g, key, other) in [
            (Grant::ViewCinema, "a.mkv", "a.cbz"),
            (Grant::ViewLibrary, "a.cbz", "a.mp3"),
            (Grant::ViewMusic, "a.mp3", "a.mkv"),
        ] {
            f.grant(GUILD, ROLE_MEMBER, GrantSet::of([g]));
            assert_eq!(
                go(&f, viewer(FRIEND), &m, key).await,
                Ok(Scope::Guild(GUILD)),
                "{g:?}"
            );
            assert_eq!(
                go(&f, viewer(FRIEND), &m, other).await,
                Err(Refused::NoGrant),
                "{g:?}"
            );
        }
    }

    #[tokio::test]
    async fn upload_alone_reads_nothing() {
        let f = MemoryScopeFacts::new();
        f.member(GUILD, FRIEND, standing(&[ROLE_MEMBER]));
        f.grant(GUILD, ROLE_MEMBER, GrantSet::of([Grant::Upload]));
        let m = mount(MountOwner::Guild(GUILD));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::NoGrant)
        );
        assert_eq!(go(&f, viewer(FRIEND), &m, "").await, Err(Refused::NoGrant));
    }

    #[tokio::test]
    async fn owner_and_administrator_bypass_and_manage_guild_implies_manage() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::Guild(GUILD));
        f.member(
            GUILD,
            1,
            GuildStanding {
                is_owner: true,
                ..GuildStanding::default()
            },
        );
        f.member(
            GUILD,
            2,
            GuildStanding {
                administrator: true,
                ..GuildStanding::default()
            },
        );
        f.member(
            GUILD,
            3,
            GuildStanding {
                manage_guild: true,
                ..GuildStanding::default()
            },
        );
        for u in [1, 2, 3] {
            assert_eq!(
                go(&f, viewer(u), &m, "a.mkv").await,
                Ok(Scope::Guild(GUILD)),
                "user {u}"
            );
            assert!(may_manage(&f, viewer(u), &m).await.unwrap(), "user {u}");
        }
        // Owner and administrator also upload; MANAGE_GUILD implies manage only.
        assert!(may_upload(&f, viewer(1), &m).await.unwrap());
        assert!(may_upload(&f, viewer(2), &m).await.unwrap());
        assert!(
            guild_grants(&f, GUILD, 3)
                .await
                .unwrap()
                .names()
                .contains(&"manage")
        );
        assert!(!guild_grants(&f, GUILD, 3).await.unwrap().has(Grant::Upload));
    }

    #[tokio::test]
    async fn upload_and_manage_helpers_follow_the_grants() {
        let f = MemoryScopeFacts::new();
        let gm = mount(MountOwner::Guild(GUILD));
        f.member(GUILD, FRIEND, standing(&[ROLE_MEMBER]));
        assert!(!may_upload(&f, viewer(FRIEND), &gm).await.unwrap());
        f.grant(GUILD, ROLE_MEMBER, GrantSet::of([Grant::Upload]));
        assert!(may_upload(&f, viewer(FRIEND), &gm).await.unwrap());
        assert!(!may_manage(&f, viewer(FRIEND), &gm).await.unwrap());
        f.grant(GUILD, ROLE_MEMBER, GrantSet::of([Grant::Manage]));
        assert!(may_manage(&f, viewer(FRIEND), &gm).await.unwrap());
        assert!(!may_upload(&f, viewer(STRANGER), &gm).await.unwrap());
        // A user mount belongs to its owner alone; a friend with a share cannot upload or manage.
        let um = mount(MountOwner::User(OWNER));
        assert!(may_upload(&f, viewer(OWNER), &um).await.unwrap());
        assert!(may_manage(&f, viewer(OWNER), &um).await.unwrap());
        f.befriend(OWNER, FRIEND);
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                music: true,
                ..Share::default()
            },
        );
        assert!(!may_upload(&f, viewer(FRIEND), &um).await.unwrap());
        assert!(!may_manage(&f, viewer(FRIEND), &um).await.unwrap());
        // The instance mount is nobody's to upload to through this helper.
        let pm = mount(MountOwner::Platform);
        assert!(!may_manage(&f, viewer(OWNER), &pm).await.unwrap());
    }

    #[tokio::test]
    async fn a_delisted_key_is_refused_even_for_the_owner() {
        let f = MemoryScopeFacts::new();
        f.delist(MOUNT, "bad.mp4");
        let m = mount(MountOwner::User(OWNER));
        let r = go(&f, viewer(OWNER), &m, "bad.mp4").await;
        assert_eq!(r, Err(Refused::Delisted));
        assert_eq!(r.unwrap_err().disclosure(), Disclosure::NotFound);
        assert!(go(&f, viewer(OWNER), &m, "good.mp4").await.is_ok());
        // Staff do not read around a takedown either.
        let s = Viewer {
            id: 99,
            staff: true,
        };
        assert_eq!(go(&f, s, &m, "bad.mp4").await, Err(Refused::Delisted));
    }

    #[tokio::test]
    async fn a_disabled_mount_is_refused_for_everyone() {
        let f = MemoryScopeFacts::new();
        let mut m = mount(MountOwner::User(OWNER));
        m.status = MountStatus::Disabled;
        assert_eq!(
            go(&f, viewer(OWNER), &m, "a.mp3").await,
            Err(Refused::Disabled)
        );
        let s = Viewer {
            id: 99,
            staff: true,
        };
        assert_eq!(go(&f, s, &m, "a.mp3").await, Err(Refused::Disabled));
    }

    #[tokio::test]
    async fn a_failing_mount_is_still_readable() {
        // `failing` is a health flag for the owner's screen, not a refusal: the listing is how they diagnose.
        let f = MemoryScopeFacts::new();
        let mut m = mount(MountOwner::User(OWNER));
        m.status = MountStatus::Failing;
        assert_eq!(go(&f, viewer(OWNER), &m, "a.mp3").await, Ok(Scope::Own));
    }

    #[tokio::test]
    async fn a_suspended_owner_closes_the_mount_for_everyone() {
        let f = MemoryScopeFacts::new();
        f.befriend(OWNER, FRIEND);
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                music: true,
                ..Share::default()
            },
        );
        let m = mount(MountOwner::User(OWNER));
        assert!(go(&f, viewer(FRIEND), &m, "a.mp3").await.is_ok());
        f.suspend(MountOwner::User(OWNER));
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::OwnerInactive)
        );
        assert_eq!(
            go(&f, viewer(OWNER), &m, "a.mp3").await,
            Err(Refused::OwnerInactive)
        );
        let s = Viewer {
            id: 99,
            staff: true,
        };
        assert_eq!(go(&f, s, &m, "a.mp3").await, Err(Refused::OwnerInactive));
        // A guild shelf whose guild is closed is refused the same way.
        let gm = mount(MountOwner::Guild(GUILD));
        f.suspend(MountOwner::Guild(GUILD));
        f.member(GUILD, FRIEND, standing(&[]));
        assert_eq!(
            go(&f, viewer(FRIEND), &gm, "a.mp3").await,
            Err(Refused::OwnerInactive)
        );
    }

    #[tokio::test]
    async fn staff_reach_other_mounts_as_staff_and_non_staff_do_not() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::User(OWNER));
        let s = Viewer {
            id: 99,
            staff: true,
        };
        assert_eq!(go(&f, s, &m, "a.mp3").await, Ok(Scope::Staff));
        let gm = mount(MountOwner::Guild(GUILD));
        assert_eq!(go(&f, s, &gm, "a.mp3").await, Ok(Scope::Staff));
        // Staff reading their own mount is an ordinary own read, not logged.
        let own = mount(MountOwner::User(99));
        assert_eq!(go(&f, s, &own, "a.mp3").await, Ok(Scope::Own));
        // Without the staff flag the same account is a stranger.
        assert_eq!(
            go(&f, viewer(99), &m, "a.mp3").await,
            Err(Refused::NotFriend)
        );
    }

    #[tokio::test]
    async fn the_instance_mount_is_reached_through_an_attached_guild_only() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::Platform);
        f.member(GUILD, FRIEND, standing(&[ROLE_MEMBER]));
        f.grant(GUILD, ROLE_MEMBER, GrantSet::of([Grant::ViewCinema]));
        // Not attached: refused.
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mkv").await,
            Err(Refused::NotMember)
        );
        f.attach(MOUNT, GUILD);
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mkv").await,
            Ok(Scope::Guild(GUILD))
        );
        assert_eq!(
            go(&f, viewer(FRIEND), &m, "a.mp3").await,
            Err(Refused::NoGrant)
        );
        assert_eq!(
            go(&f, viewer(STRANGER), &m, "a.mkv").await,
            Err(Refused::NotMember)
        );
    }

    #[tokio::test]
    async fn the_decision_fails_closed_when_the_store_fails() {
        let f = MemoryScopeFacts::new();
        let m = mount(MountOwner::User(OWNER));
        f.break_store();
        assert!(matches!(
            resolve(&f, viewer(OWNER), &m, "a.mp3").await,
            Err(ResolveError::Facts(_))
        ));
        assert!(
            may_upload(&f, viewer(OWNER), &mount(MountOwner::Guild(GUILD)))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn folders_and_the_root_need_only_reach() {
        let f = MemoryScopeFacts::new();
        f.befriend(OWNER, FRIEND);
        f.share_with(
            MOUNT,
            FRIEND,
            Share {
                music: true,
                ..Share::default()
            },
        );
        let m = mount(MountOwner::User(OWNER));
        assert!(go(&f, viewer(FRIEND), &m, "").await.is_ok());
        assert!(go(&f, viewer(FRIEND), &m, "albums/").await.is_ok());
        // ... but a stranger cannot even list the root.
        assert_eq!(
            go(&f, viewer(STRANGER), &m, "").await,
            Err(Refused::NotFriend)
        );
    }

    #[test]
    fn kinds_come_from_the_extension() {
        assert_eq!(Kind::of_key("a/b/Movie.MKV"), Some(Kind::Cinema));
        assert_eq!(Kind::of_key("x.cbz"), Some(Kind::Library));
        assert_eq!(Kind::of_key("x.opus"), Some(Kind::Music));
        assert_eq!(Kind::of_key("cover.jpg"), None);
        assert_eq!(Kind::of_key("noext"), None);
        assert_eq!(Kind::of_key("dir.mkv/readme"), None);
        assert_eq!(Kind::parse("music"), Some(Kind::Music));
        assert_eq!(Kind::parse("games"), None);
        assert_eq!(Grant::parse("manage"), Some(Grant::Manage));
        assert_eq!(Grant::parse("MANAGE"), None);
    }
}
