# 14 — Glossary

| Term | Meaning |
|---|---|
| **Account** | A login. Called `user` in the API |
| **Activity (presence)** | "Watching X / Reading Y / Playing Z" status shown on profiles |
| **Activity (app)** | An embedded iframe app launched in a voice or text channel |
| **App developer** | An approved third-party developer (D-061). Not staff |
| **Cap** | The 20,000 counted-account ceiling |
| **Cinema** | The video section (movies, series, anime, Live TV) |
| **Counted account** | An account in state `active` |
| **Credit (invite credit)** | Permission to have one platform invite redeemed |
| **Edition** | One language/release variant of a reading series |
| **Egress governor** | SFU logic that lowers quality when total egress nears the budget |
| **Entrance** | Landing + login + registration pages |
| **Feature module** | A unit of functionality registered in `sr-features` with an auto-rendered settings schema |
| **Guild** | API/internal name for a "server" (see the naming rule below) |
| **High-level staff** | Staff holding `READ_PRIVATE_CONTENT` |
| **Hold** | A moderation hold that prevents purging a message or attachment |
| **Large attachment** | A chat file > 30 MB. Expires after 7 days |
| **Library** | The reading section (manga, manhwa, comics, ebooks) |
| **Lounge** | The chat section (Discord equivalent) |
| **Owner** | The operator/host. Owner of the Staff Server, highest authority |
| **Patron** | A personal monthly donor ($5 default) |
| **People you know** | The union of members of all servers you're in |
| **Person** | A staff-side record of a human, which may span several accounts |
| **Platform invite** | A single-use code that creates one account |
| **Rendition** | One HLS quality level of a video asset |
| **Server invite** | A Discord-style invite into a guild |
| **Server Level** | A pooled-donation tier for a server (L0–L3) |
| **SFU** | Selective Forwarding Unit: the voice/video router (`sr-voice`) |
| **Staff Server** | The special guild whose members are platform staff |
| **Submission** | Content (or an app/package) sent for staff review |
| **Virtual-linear** | Live TV technique: a live playlist stitched from existing VOD segments by wall-clock time |

## Naming rule: "guild" in code, "server" for people

Decided by the owner. A Discord-style community of channels and members is a **guild** in code, routes (`/guilds`), database tables, gateway events and Rust/TypeScript identifiers, to match Discord's API so bots port easily. In user-facing text (UI copy, messages shown to users, prose in the docs) it is a **server**, as in "Add a Server" and "Staff Server". Do not rename either side. When prose means the machine or process that runs SIDE ROOM, write "the host" or name the binary (`sr-server`) to avoid confusion.
