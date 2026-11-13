//! OPS-FLG-001: the switches of the modules waves 31 to 37 shipped without one.
//!
//! Every module of waves 27 to 39 has its switches in the one registry, so Admin > Features and Guild Settings >
//! Modules render them with no screen of their own (doc 11 section 1). The ids the roadmap names and the ids
//! that shipped are the same, except for one: the roadmap's `lounge.status_rings` is `lounge.rings` (registered
//! by wave 34, `sr-userfeat`); a shipped id is never renamed. The table is in
//! `docs/dev/handoff/feat-p39-ops-modules.md`.
//!
//! * `lounge.jukebox` (platform and server): the voice jukebox (queue, vote-skip, DJ, synced playback).
//! * `lounge.listening_rooms` (platform and server): the 24/7 listening room of a voice channel. Dies with the
//!   jukebox it plays through.
//! * `platform.tools`: the utility drawer; one sub-flag `platform.tools.<tool>` per tool ([`TOOL_FLAGS`]), each
//!   dying with the parent. A tool that is only client side is hidden by the web; one with a server part
//!   (currency rates, notes, loot tables, tier lists and polls, cards, the voice timer) also has its routes
//!   refused.
//! * `platform.activities_datachannel`: the SFU data channel of Activities. Off, sr-voice ignores the channel
//!   a member opens and relays nothing, and an activity instance is created without the `datachannel` capability.
//! * `platform.emulator`: the emulator (`/games`): the ROM list route and the core files.
//! * `platform.watch_together_embeds`: parties on a provider embed or a direct file link.
//!
//! `platform.game_servers` (the "Coming soon" page's switch) is registered by GSV-PAGE-001, not here.
//!
//! No switch here reaches end-to-end encryption, authorization or the audit and access logs (the IMMUTABLE RULE):
//! a kill switch only refuses a feature's routes, and only a refusal is ever added.

use crate::feature::{Feature, FeatureError, FeatureMeta, FeatureRegistry, Scope};
use crate::schema::SettingsSchema;

/// The voice jukebox.
pub const JUKEBOX_ID: &str = "lounge.jukebox";
/// The 24/7 listening room.
pub const LISTENING_ROOMS_ID: &str = "lounge.listening_rooms";
/// The utility drawer.
pub const TOOLS_ID: &str = "platform.tools";
/// The activity data channel of the SFU.
pub const ACTIVITIES_DATACHANNEL_ID: &str = "platform.activities_datachannel";
/// The emulator.
pub const EMULATOR_ID: &str = "platform.emulator";
/// Watch Together on a provider embed or a direct file.
pub const WATCH_TOGETHER_EMBEDS_ID: &str = "platform.watch_together_embeds";

/// STU-NAV-001 (wave 41, The Study): the storefront and inducement surfaces. Each is OFF until the operator
/// switches it on at platform scope; the routes and engines stay (hide, never delete), only the surface is hidden
/// and, for external metadata, the outbound lookup is refused.
pub const EXTERNAL_METADATA_ID: &str = "platform.external_metadata";
/// The Movies, Series and Anime storefront categories.
pub const STOREFRONT_CATEGORIES_ID: &str = "platform.storefront_categories";
/// The Global Trending rails and rankings.
pub const GLOBAL_TRENDING_ID: &str = "platform.global_trending";
/// The global Requests board.
pub const REQUESTS_BOARD_ID: &str = "platform.requests_board";
/// The staff upload and ingest review queues.
pub const INGEST_QUEUES_ID: &str = "platform.ingest_queues";

/// The five switches of the Study reframe, in the order the registry lists them.
pub const STUDY_SWITCH_IDS: [&str; 5] = [
    EXTERNAL_METADATA_ID,
    STOREFRONT_CATEGORIES_ID,
    GLOBAL_TRENDING_ID,
    REQUESTS_BOARD_ID,
    INGEST_QUEUES_ID,
];

/// UX-SHOW-001 (wave 45-B, D-435): the Showroom, a populated preview for beta and test deployments. OFF by default.
/// On, signed-in members see a "Preview populated state" toggle that overlays made-up sample data on the web client
/// (servers rail, The Study, The Arcade) with no request to the server. It reaches nothing else.
pub const SHOWROOM_ID: &str = "platform.showroom";

/// SEC-AUD-166 (wave 64, D-647): the beta bug reporter. OFF by default. On, signed-in members can send staff a bug
/// report (a description, an allow-listed debug bundle and, if they choose, a picture they checked). It gates only
/// `POST /bug-reports`: staff can read and close reports after it is turned off. It reaches nothing the IMMUTABLE RULE
/// protects.
pub const BETA_REPORTS_ID: &str = "platform.beta_reports";

const PLATFORM_GUILD: &[Scope] = &[Scope::Platform, Scope::Guild];
const PLATFORM_ONLY: &[Scope] = &[Scope::Platform];

/// One switchable thing: the common shape of the modules of this file.
struct Switch {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    section: &'static str,
    phase: u8,
    scopes: &'static [Scope],
    gated_by: Option<&'static str>,
    /// Whether the switch is on until the operator says otherwise.
    default_on: bool,
}

impl Switch {
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: self.name.into(),
            description: self.description.into(),
            section: self.section.into(),
            phase: self.phase,
            default_enabled: self.default_on,
        }
    }
}

macro_rules! switch_feature {
    ($ty:ident, $sw:expr) => {
        #[derive(Default)]
        pub struct $ty;
        impl Feature for $ty {
            fn id(&self) -> &'static str {
                $sw.id
            }
            fn meta(&self) -> FeatureMeta {
                $sw.meta()
            }
            fn client_visible(&self) -> bool {
                true
            }
            fn gated_by(&self) -> Option<&'static str> {
                $sw.gated_by
            }
            fn settings_schema(&self) -> SettingsSchema {
                SettingsSchema::default()
            }
            fn scopes(&self) -> &[Scope] {
                $sw.scopes
            }
        }
    };
}

const JUKEBOX: Switch = Switch {
    id: JUKEBOX_ID,
    name: "Voice jukebox",
    description: "A shared queue in voice channels, with vote-skip, a DJ and synced playback from members' own storage. Off, the jukebox routes are refused and the strip disappears from voice channels.",
    section: "Music",
    phase: 31,
    scopes: PLATFORM_GUILD,
    gated_by: None,
    default_on: true,
};
const LISTENING_ROOMS: Switch = Switch {
    id: LISTENING_ROOMS_ID,
    name: "Listening rooms",
    description: "A voice channel that plays a playlist around the clock for whoever is in it. Off, the room routes are refused and no room advances. It also stops while the jukebox is off.",
    section: "Music",
    phase: 31,
    scopes: PLATFORM_GUILD,
    gated_by: Some(JUKEBOX_ID),
    default_on: true,
};
const TOOLS: Switch = Switch {
    id: TOOLS_ID,
    name: "Utility tools",
    description: "The utility drawer (Ctrl or Cmd+K, then >). Off, the drawer lists no tool and every tool route that talks to the server is refused. Each tool also has a switch of its own.",
    section: "Tools",
    phase: 32,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: true,
};
const DATACHANNEL: Switch = Switch {
    id: ACTIVITIES_DATACHANNEL_ID,
    name: "Activity data channel",
    description: "Lets activities in voice channels share state and input through the call's data channel, sealed with the call's keys. Off, new data channels are refused by the voice server and activities start without the capability.",
    section: "Activities",
    phase: 36,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: true,
};
const EMULATOR: Switch = Switch {
    id: EMULATOR_ID,
    name: "Emulator",
    description: "Plays the ROMs members keep in their own storage in the browser, alone or together. Off, the games page is hidden and the ROM list and the emulator cores are refused.",
    section: "Games",
    phase: 37,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: true,
};
const EMBEDS: Switch = Switch {
    id: WATCH_TOGETHER_EMBEDS_ID,
    name: "Watch Together embeds",
    description: "Lets a Watch Together party play a link to a provider's privacy-enhanced embed or to a media file. Off, a party can only play the Cinema catalog and Live TV, and a link is refused.",
    section: "Cinema",
    phase: 34,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: true,
};

const EXTERNAL_METADATA: Switch = Switch {
    id: EXTERNAL_METADATA_ID,
    name: "External metadata lookup",
    description: "Fetches titles, posters and details from TMDB, AniList, MangaDex, MangaUpdates and ComicVine. Off by default: nothing on the server calls an external metadata host until an operator switches this on.",
    section: "Study",
    phase: 41,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const STOREFRONT_CATEGORIES: Switch = Switch {
    id: STOREFRONT_CATEGORIES_ID,
    name: "Storefront categories",
    description: "The Movies, Series and Anime browse pages of the Cinema. Off by default: the Study shows shelves and sources instead of a catalog storefront. The routes stay.",
    section: "Study",
    phase: 41,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const GLOBAL_TRENDING: Switch = Switch {
    id: GLOBAL_TRENDING_ID,
    name: "Global trending",
    description: "The trending rails and rankings of the Cinema and the Library. Off by default; the routes stay.",
    section: "Study",
    phase: 41,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const REQUESTS_BOARD: Switch = Switch {
    id: REQUESTS_BOARD_ID,
    name: "Requests board",
    description: "The global board where members ask for titles. Off by default; the routes stay.",
    section: "Study",
    phase: 41,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const INGEST_QUEUES: Switch = Switch {
    id: INGEST_QUEUES_ID,
    name: "Upload and ingest review queues",
    description: "The staff queues that review uploads and ingested files, and the member upload and submission pages that feed them. Off by default; the routes and the engine stay.",
    section: "Study",
    phase: 41,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const SHOWROOM: Switch = Switch {
    id: SHOWROOM_ID,
    name: "Showroom (populated preview)",
    description: "Shows signed-in members a Preview populated state toggle that fills the web client with made-up sample servers, shelves and games, with a banner on every screen. The sample data lives only in the browser: nothing is read or written on the server. For beta and test deployments; off by default.",
    section: "Study",
    phase: 45,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};
const BETA_REPORTS: Switch = Switch {
    id: BETA_REPORTS_ID,
    name: "Beta bug reporter",
    description: "Lets signed-in members send staff a bug report with a description, technical details of their app and, if they choose, a screenshot they have checked. Staff can read the reports in Admin. For a small beta (under 100 members); turn it off when the beta ends. Off by default.",
    section: "Study",
    phase: 64,
    scopes: PLATFORM_ONLY,
    gated_by: None,
    default_on: false,
};

switch_feature!(ExternalMetadata, EXTERNAL_METADATA);
switch_feature!(StorefrontCategories, STOREFRONT_CATEGORIES);
switch_feature!(GlobalTrending, GLOBAL_TRENDING);
switch_feature!(RequestsBoard, REQUESTS_BOARD);
switch_feature!(IngestQueues, INGEST_QUEUES);
switch_feature!(Showroom, SHOWROOM);
switch_feature!(BetaReports, BETA_REPORTS);
switch_feature!(Jukebox, JUKEBOX);
switch_feature!(ListeningRooms, LISTENING_ROOMS);
switch_feature!(Tools, TOOLS);
switch_feature!(ActivitiesDatachannel, DATACHANNEL);
switch_feature!(Emulator, EMULATOR);
switch_feature!(WatchTogetherEmbeds, EMBEDS);

/// One tool of the utility drawer: its sub-flag id, the tool id of the web registry
/// (`apps/web/src/tools/registry.ts`, hyphens written as underscores here) and its name.
/// The drawer's own `guide` has no switch: it keeps the drawer explainable. A Vitest keeps this list equal to the
/// web registry.
pub struct ToolFlag {
    pub id: &'static str,
    pub tool: &'static str,
    pub name: &'static str,
}

macro_rules! tool_flags {
    ($(($tool:literal, $name:literal)),+ $(,)?) => {
        &[$(ToolFlag {
            id: concat!("platform.tools.", $tool),
            tool: $tool,
            name: $name,
        }),+]
    };
}

/// Every tool with a sub-flag, in registry order. The list between the markers is read by a Vitest.
// tool-flags:begin
pub const TOOL_FLAGS: &[ToolFlag] = tool_flags![
    ("json", "JSON tool"),
    ("regex", "Regex tester"),
    ("base64", "Base64"),
    ("url", "URL encoder"),
    ("jwt", "JWT decoder"),
    ("md_table", "Markdown table"),
    ("counter", "Text counter"),
    ("hash", "Hashes"),
    ("diff", "Text diff"),
    ("dice", "Dice"),
    ("coin", "Coin flip"),
    ("wheel", "Decision wheel"),
    ("loot", "Loot tables"),
    ("notes", "Scratch notes"),
    ("pomodoro", "Pomodoro timer"),
    ("stopwatch", "Stopwatch"),
    ("countdown", "Countdown"),
    ("image_studio", "Image studio"),
    ("redact", "Redact"),
    ("palette", "Palette"),
    ("gif", "GIF tool"),
    ("tierlist", "Tier list"),
    ("poll", "Poll builder"),
    ("calculator", "Calculator"),
    ("programmer", "Programmer calculator"),
    ("units", "Unit converter"),
    ("currency", "Currency converter"),
    ("percent", "Percentages"),
    ("timezone", "Time zones"),
    ("invite", "Invite cards"),
    ("audio", "Audio tool"),
];
// tool-flags:end

/// The sub-flag id of a tool of the web registry (`image-studio` is `platform.tools.image_studio`), or `None` for the
/// guide and for a tool that does not exist.
pub fn tool_flag_id(tool: &str) -> Option<&'static str> {
    let key = tool.replace('-', "_");
    TOOL_FLAGS.iter().find(|t| t.tool == key).map(|t| t.id)
}

/// One sub-flag: platform scope only, on by default, under `platform.tools`.
pub struct ToolSwitch(pub &'static ToolFlag);

impl Feature for ToolSwitch {
    fn id(&self) -> &'static str {
        self.0.id
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: format!("Tool: {}", self.0.name),
            description: format!(
                "The {} tool of the utility drawer. Off, it disappears from the drawer and its server routes, if it has any, are refused. It is also off while the utility tools are off.",
                self.0.name
            ),
            section: "Tools".into(),
            phase: 32,
            default_enabled: true,
        }
    }
    fn client_visible(&self) -> bool {
        true
    }
    fn gated_by(&self) -> Option<&'static str> {
        Some(TOOLS_ID)
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::default()
    }
    fn scopes(&self) -> &[Scope] {
        PLATFORM_ONLY
    }
}

/// Register the switches of this file.
pub fn register_all(registry: &mut FeatureRegistry) -> Result<(), FeatureError> {
    crate::register_features!(
        registry,
        Jukebox,
        ListeningRooms,
        Tools,
        ActivitiesDatachannel,
        Emulator,
        WatchTogetherEmbeds,
        ExternalMetadata,
        StorefrontCategories,
        GlobalTrending,
        RequestsBoard,
        IngestQueues,
        Showroom,
        BetaReports,
    )?;
    for flag in TOOL_FLAGS {
        registry.register(ToolSwitch(flag))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{FeatureService, InMemorySettingsStore, ScopeRef};

    fn service() -> FeatureService {
        let mut reg = FeatureRegistry::new();
        register_all(&mut reg).unwrap();
        FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None)
    }

    #[tokio::test]
    async fn a_dead_parent_kills_every_tool_and_the_rooms() {
        let svc = service();
        let tool = tool_flag_id("md-table").unwrap();
        assert!(svc.is_enabled(tool, &ScopeRef::Platform).await.unwrap());
        svc.set_enabled(TOOLS_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!svc.is_enabled(tool, &ScopeRef::Platform).await.unwrap());
        assert!(
            svc.is_enabled(JUKEBOX_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
        svc.set_enabled(JUKEBOX_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(
            !svc.is_enabled(LISTENING_ROOMS_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
    }

    #[test]
    fn tool_ids_are_unique_and_the_guide_has_none() {
        let mut ids: Vec<_> = TOOL_FLAGS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TOOL_FLAGS.len());
        assert_eq!(tool_flag_id("guide"), None);
        assert_eq!(
            tool_flag_id("image-studio"),
            Some("platform.tools.image_studio")
        );
    }
}
