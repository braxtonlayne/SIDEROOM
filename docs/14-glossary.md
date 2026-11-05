# 14 — Glossary

| Term | Meaning |
|---|---|
| **Account** | A login. Called `user` in the API |
| **Activity (app)** | An embedded iframe app launched in a voice or text channel |
| **Activity (presence)** | "Watching X / Reading Y / Playing Z" status shown on profiles |
| **App developer** | An approved third-party developer (D-061). Not staff |
| **Arcade** | The games room (`/games`): built-in games, community Activities, Guild Cartridges and the Emulator. Its front page is the **Front Page**, where the Operator features games (ARC-ROOM-001, D-400, D-476) |
| **Bridge (Discord bridge)** | The optional mirror between mapped SIDE ROOM and Discord channels, run with the server owner's own Discord bot (BRG-COR-001, D-381) |
| **Cap** | The 20,000 counted-account ceiling |
| **Cartridge (Guild Cartridge)** | A game a server lists in the Arcade; it may name an object on a shelf (D-410) |
| **Cinema** | The video section (movies, series, anime, Live TV) |
| **Code hash (`code_sha256`)** | SHA-256 over the executable code of an Activity bundle. Operator signatures and community vouches are pinned to it (D-409) |
| **Counted account** | An account in state `active` |
| **Credit (invite credit)** | Permission to have one platform invite redeemed |
| **Edition** | One language/release variant of a reading series |
| **Egress governor** | SFU logic that lowers quality when total egress nears the budget |
| **Entrance isolation** | The rule that a visitor without a login receives only the minimal public surfaces and no app code (D-385; [dev/entrance-isolation](dev/entrance-isolation.md)) |
| **Entrance** | Landing + login + registration pages |
| **Feature module** | A unit of functionality registered in `sr-features` with an auto-rendered settings schema |
| **Guest link** | A `/play/<token>` link that lets a person without an account play one running Activity. The guest sees only that game session (D-397, wave 43) |
| **Guild** | API/internal name for a "server" (see the naming rule below) |
| **High-level staff** | Staff holding `READ_PRIVATE_CONTENT` |
| **Hold** | A moderation hold that prevents purging a message or attachment |
| **Large attachment** | A chat file > 30 MB. Expires after 7 days |
| **Library** | The reading section (manga, manhwa, comics, ebooks) |
| **Lounge** | The chat section (Discord equivalent) |
| **Mount (storage mount)** | One S3-compatible bucket (plus prefix) that a member, a server or the platform owns (BYOS-MNT-001). A **shelf** is a mount seen as a catalog of objects (wave 41, D-393) |
| **Operator** | The platform permission level (`OPERATOR`, doc 07) for infrastructure settings and Arcade featuring |
| **Owner** | The operator/host. Owner of the Staff Server, highest authority |
| **Patron** | A personal monthly donor ($5 default) |
| **People you know** | The union of members of all servers you're in |
| **Person** | A staff-side record of a human, which may span several accounts |
| **Platform invite** | A single-use code that creates one account |
| **Platform switch** | An instance-wide on/off setting under `platform.*` (for example `platform.external_metadata`, `platform.showroom`); the Study storefront surfaces are off by default (D-400) |
| **Rendition** | One HLS quality level of a video asset |
| **Scrapbook** | A server's vault of messages copied into its own bucket when they collect enough of a trigger reaction (PRF-SCR-001) |
| **Server invite** | A Discord-style invite into a guild |
| **Server Level** | A pooled-donation tier for a server (L0–L3) |
| **SFU** | Selective Forwarding Unit: the voice/video router (`sr-voice`) |
| **Showroom** | A preview mode, switched on by `platform.showroom`, that fills the rail and the Study and Arcade with sample data on a test deployment (UX-SHOW-001, D-435) |
| **Staff Server** | The special guild whose members are platform staff |
| **Status ring** | A 24-hour post pinned to a member's avatar: photo, short clip, text or a track (PRF-RNG-001) |
| **Study (The Study)** | The room (`/study`) that holds the shelves by source, with the Cinema as Media and the Library as Reading (wave 41, D-400) |
| **Submission** | Content (or an app/package) sent for staff review |
| **Virtual-linear** | Live TV technique: a live playlist stitched from existing VOD segments by wall-clock time |
| **Vouch** | A member's "I trust this person" on a profile (MOD-VCH-001), or "I played this exact build and it is fine" on an Arcade build (ARC-SBX-001). Profile vouches set the tier Citizen / Pillar / Elder and the penalty multiplier (D-427) |

## Naming rule: "guild" in code, "server" for people

Decided by the owner. A Discord-style community of channels and members is a **guild** in code, routes (`/guilds`), database tables, gateway events and Rust/TypeScript identifiers, to match Discord's API so bots port easily. In user-facing text (UI copy, messages shown to users, prose in the docs) it is a **server**, as in "Add a Server" and "Staff Server". Do not rename either side. When prose means the machine or process that runs SIDE ROOM, write "the host" or name the binary (`sr-server`) to avoid confusion.
