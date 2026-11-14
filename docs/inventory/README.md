# Component Inventory

This is an exhaustive, atomic list (**1,018 items** at creation, 1,442 on 2026-11-13 with the launch-readiness and feature-phase files) of everything that must be built for SIDE ROOM to function. It runs top-down through each section, and for the Lounge it walks through every Discord menu, button, and screen visible in the reference material or known Discord behavior. Agents claim items by ID.

## How to read an item

```
- [ ] **LNG-MSG-014** [FE][BE] Edit message — inline editor, Esc cancel / Enter save, "(edited)" tag → PATCH /channels/{id}/messages/{id} · dep: LNG-MSG-001
```

| Part | Meaning |
|---|---|
| `[ ]` / `[x]` | Not done / done. Tick in the PR that completes it |
| **ID** | `<AREA>-<GROUP>-<NNN>`. Stable. Never renumber. New items take the next free number |
| Layer tags | `[DB]` migration/schema · `[BE]` Rust backend · `[GW]` gateway event/op · `[FE]` React UI · `[WASM]` Rust→WASM · `[JOB]` background job · `[OPS]` deploy/infra · `[TEST]` test-only · `[DOC]` docs |
| `dep:` | Items that must exist first |
| `→` | Main API route or event involved |

An item is "done" when it meets the definition of done in [AGENTS.md](../../AGENTS.md#5-definition-of-done) and its spec's acceptance criteria.

## Files

| # | File | Area prefix | Phase |
|---|---|---|---|
| 01 | [Foundation & design system](01-foundation.md) | `FND`, `DS` | 0 |
| 02 | [Entrance](02-entrance.md) | `ENT` | 0 |
| 03 | [Lounge: shell, navigation, home, friends, DMs, profiles](03-lounge-shell.md) | `LNG-SHL/RAIL/HOME/FRD/DM/PRF` | 1 |
| 04 | [Lounge: servers, channels, threads, forums, invites](04-lounge-servers-channels.md) | `LNG-GLD/CHN/THR/FRM/INV` | 1 |
| 05 | [Lounge: messages, composer, expression, search, notifications](05-lounge-messages.md) | `LNG-MSG/CMP/EXP/SRC/NTF` | 1 |
| 06 | [Lounge: server settings](06-lounge-server-settings.md) | `LNG-SSET` | 1 |
| 07 | [Lounge: user settings](07-lounge-user-settings.md) | `LNG-USET` | 1 |
| 08 | [Lounge: voice, video, E2EE](08-lounge-voice.md) | `VOX` | 2 |
| 09 | [Built-in feature modules](09-feature-modules.md) | `FTR` | 1+ |
| 10 | [Staff Server & admin](10-admin.md) | `ADM` | 3 |
| 11 | [Donations & perks](11-donations.md) | `DON` | 3 |
| 12 | [Submissions & review](12-review.md) | `REV` | 4 |
| 13 | [Library](13-library.md) | `LIB` | 4 |
| 14 | [Cinema](14-cinema.md) | `CIN` | 5 |
| 15 | [Live TV](15-live-tv.md) | `TV` | 5 |
| 16 | [Watch parties](16-watch-parties.md) | `WP` | 5 |
| 17 | [Developer platform & extensions](17-developer-platform.md) | `DEV`, `EXT` | 6 |
| 18 | [Cross-cutting](18-cross-cutting.md) | `XC` | all |
| 19 | [Moderation & compliance](19-moderation.md) | `MOD` | 7 |
| 20 | [Launch readiness](20-launch-readiness.md) | `RDY` | waves 20 to 24 ([roadmap](../dev/roadmap-p20.md), D-352) |
| 21 | [Feature phase](21-feature-phase.md) | `BYOS`, `SEC`, `MUS`, `PRF`, `ARC`, `ACT` and others | waves 27 to 65 ([roadmap-p27](../dev/roadmap-p27.md), D-380; [roadmap-p28](../dev/roadmap-p28.md) for waves 41 to 49; the audit rows of waves 53 to 65 are `SEC-AUD-*`) |

## Counting progress

On 2026-11-13: 1,402 of 1,442 rows are ticked. Open rows: 32 in files 01 to 19 (each carries a note saying what is built and what is left), 5 in file 20 and 3 in file 21 (these wait on a run, a download or the owner).

`grep -c "\- \[x\]" docs/inventory/*.md` vs `grep -c "\- \[ \]"`. A CI job publishes a per-file progress table to the PR summary (item XC-CI-006).
