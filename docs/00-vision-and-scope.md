# 00 — Vision and Scope

## 1. One-paragraph pitch

SIDE ROOM is a self-hosted, invitation-only "private corner of the internet." It combines a Discord-equivalent chat platform (**the Lounge**), a Netflix-style video library with a nostalgic retro TV guide (**the Cinema**), and a manga/comics/ebook library and reader (**the Library**). All three sit behind one invite-gated front door (**the Entrance**). Members contribute media and reading material through a reviewed submission pipeline. Platform staff run everything from inside the product, through a special **Staff Server**. The backend is written in Rust.

## 2. Who it's for

- **Primary:** the owner's friend group and a few allied private communities. Realistic production size is under ~100 accounts.
- **Engineering ceiling:** 20,000 registered (active) accounts per installation, with roughly 2,000 concurrent users as the sizing target at that ceiling.
- **Secondary:** another operator who wants to self-host it. Documentation and deployment must be complete enough for a stranger to run it, but this is not a product goal.

## 3. Sections

| Section | Route prefix | Default visual style | Spec |
|---|---|---|---|
| Entrance (landing, login, register-with-invite) | `/`, `/login`, `/join/:code`, `/invite/:code` | Side Room (dark, cream, muted gold, serif) | [entry-landing-and-login](specs/entry-landing-and-login.md) |
| Lounge (chat, voice, video) | `/channels/...` | Discord look-alike | [lounge-chat](specs/lounge-chat.md), [lounge-voice-video](specs/lounge-voice-video.md) |
| Cinema (movies, series, anime, Live TV) | `/cinema/...` | Side Room Cinema | [cinema-media](specs/cinema-media.md), [cinema-live-tv](specs/cinema-live-tv.md), [watch-parties](specs/watch-parties.md) |
| Library (manga, manhwa, comics, ebooks) | `/library/...` | Side Room Library | [library-reader](specs/library-reader.md) |
| Staff Server + admin pages | `/channels/<staff-guild-id>/...`, `/admin/...` | Discord look-alike | [admin-staff-server](specs/admin-staff-server.md) |
| Donations | `/support`, plus server settings → Boosts | Side Room | [08-donations…](08-donations-perks-and-cost-model.md) |
| Developer portal | `/developers/...` | Discord look-alike | [11-extensibility…](11-extensibility-and-developer-platform.md) |

A global top bar (`SIDE ROOM / The lounge · The cinema · The library`) links the sections, as in the mockups. **"The board" is cut. Do not build it.**

## 4. Goals

1. **Discord parity in the Lounge.** Someone who knows Discord should feel at home immediately. Exceptions are listed in the decisions log.
2. **Free by default.** Nearly every feature is free. Only features that measurably raise hosting cost (high-bitrate/many-stream video, large uploads, media downloads, extra platform invites) sit behind donations.
3. **Extensible without bots.** If someone wants a feature, adding it as a configurable feature module should be easy. People shouldn't have to hunt for a bot.
4. **Curated, reviewed content.** Library and Cinema content, extensions, themes, and developer apps all go through staff review before going live.
5. **Accountable membership.** Platform entry requires a single-use invite. The full invite graph (who invited whom) is kept permanently.
6. **Operable by one person.** One rented server, a documented install, backups, and GPU transcoding done in batches off the box.
7. **Built right from day one.** It's sized for 20k accounts, so growth doesn't force a rewrite.

## 5. Non-goals (for now)

- Native desktop/mobile apps. These come later. Until then the product is the browser plus an installable PWA.
- Public sign-up, SEO, or growth features.
- Card or PayPal payments. Donations are **crypto only**.
- Moderation and compliance tooling beyond basic hooks. This is a later phase (see roadmap). Data structures reserve space for it now.
- Arbitrary user code or custom CSS. Customization is declarative and reviewed.
- Content above 1080p (no 4K/HDR).
- **The Board.**
- **Bounties** on requested media. These are documented as a future idea only ([future-bounties](specs/future-bounties.md)).

## 6. Delivery phases (summary)

See [13-roadmap-and-phases.md](13-roadmap-and-phases.md).

0. Foundation: repo, CI, auth, invites, Entrance pages, design system
1. Lounge text: servers, channels, messages, DMs, friends, settings
2. Lounge voice/video: SFU, voice channels, calls, streaming, E2EE
3. Staff Server, admin pages, donations and perks
4. Library (manga): submissions, review, reader
5. Cinema: ingest, transcoding, player, Live TV, watch parties
6. Developer platform: bot API, apps, extensions, themes, activities
7. Moderation and compliance
8. Native clients (out of scope for this spec set)
