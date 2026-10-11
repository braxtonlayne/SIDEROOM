# SIDE ROOM documentation

## Foundation
| # | Doc | What it covers |
|---|---|---|
| 00 | [Vision and scope](00-vision-and-scope.md) | What, who, goals, non-goals |
| 01 | [Decisions log](01-decisions-log.md) | **Authoritative** owner decisions (D-xxx) |
| 02 | [Architecture](02-architecture.md) | Processes, crates, request paths, events, jobs |
| 03 | [Tech stack & Rust policy](03-tech-stack-and-rust-policy.md) | Libraries, allowed non-Rust components |
| 04 | [Data model](04-data-model.md) | Logical schema |
| 05 | [API & gateway](05-api-and-gateway.md) | Discord-compatible REST + WebSocket, SIDE ROOM extensions |
| 06 | [Identity, accounts, invites](06-identity-accounts-invites.md) | Registration, login, platform invites, credits, invite graph |
| 07 | [Permissions & staff](07-permissions-and-staff.md) | Guild bits, platform bits, Staff Server |
| 08 | [Donations, perks, cost model](08-donations-perks-and-cost-model.md) | Patron, Server Levels, BTC, pricing math |
| 09 | [Retention & privacy](09-retention-and-privacy.md) | Purges, holds, E2EE scope, activity privacy |
| 10 | [Design system & theming](10-design-system-and-theming.md) | Tokens, themes, MySpace-mode profiles |
| 11 | [Extensibility & developer platform](11-extensibility-and-developer-platform.md) | Feature modules, apps, activities, packages |
| 12 | [Deployment & operations](12-deployment-and-operations.md) | Install, backups, transcode workers, monitoring |
| 13 | [Roadmap & phases](13-roadmap-and-phases.md) | Phases and exit gates |
| 14 | [Glossary](14-glossary.md) | Terms |

## Section specs
| Spec | Phase |
|---|---|
| [Entrance: landing & login](specs/entry-landing-and-login.md) | 0 |
| [Lounge: chat](specs/lounge-chat.md) | 1 |
| [Lounge: voice & video](specs/lounge-voice-video.md) | 2 |
| [Admin & Staff Server](specs/admin-staff-server.md) | 3 |
| [Submissions & review pipeline](specs/submissions-and-review.md) | 4 (shared) |
| [Ingest naming conventions](specs/ingest-naming-conventions.md) | 4–5 |
| [Library: reader](specs/library-reader.md) | 4 |
| [Cinema: media](specs/cinema-media.md) | 5 |
| [Cinema: Live TV](specs/cinema-live-tv.md) | 5 |
| [Watch parties](specs/watch-parties.md) | 5 |
| [Future: bounties](specs/future-bounties.md) | idea only |

## ADRs
[docs/adr/](adr/), the architecture decision records.

## Component inventory
[docs/inventory/](inventory/README.md), the exhaustive atomic build list (delivered in a follow-up PR).

## Mockups
[docs/assets/mockups/](assets/mockups/README.md)
