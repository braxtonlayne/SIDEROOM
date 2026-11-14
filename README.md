# SIDE ROOM

> *The internet, with the door closed.*

SIDE ROOM is a private, invitation-only platform for a group of friends and a few related communities. One installation provides:

| Section | Product name | What it is |
|---|---|---|
| **The Lounge** | Chat | A Discord-equivalent: servers, channels, DMs, voice, video, screen sharing, bots |
| **The Cinema** | Media | A Netflix-style library of movies, shows, and anime, plus a retro "Live TV" guide and watch parties |
| **The Library** | Manga | A manga/manhwa/comics/ebook library and reader |
| **The Entrance** | Landing + login | The invite-only front door |
| **Staff Server** | Admin | A special server whose members are platform staff. All administration happens there |

**Design capacity:** up to 20,000 registered accounts per installation. Expected production use is under ~100 users. The code is still built so it doesn't need rewriting if usage grows.

**Language policy:** The backend is Rust. The web frontend is TypeScript/React, with Rust compiled to WebAssembly wherever that has a concrete benefit. See [docs/03-tech-stack-and-rust-policy.md](docs/03-tech-stack-and-rust-policy.md).

## Status

**Built, not yet installed for real.** The application exists: the Rust workspace under `crates/`, the web app under `apps/web`, the shipped deployment files under `deploy/`. Every component in the [inventory](docs/inventory/README.md) files 01 to 19 is ticked except 32 rows: 30 whose built and left note records a gap found when the checklist was checked against the code on 2026-11-13 (a missing control, link or option, for example the Live TV shortcut, the account switcher, the theme validator's restricted CSS subset), and 2 that are closed or wait on a run (`DON-PAY-009`, `XC-CI-005`); files 20 (launch readiness) and 21 (the feature phase and the audit and hardening waves, waves 27 to 65) are ticked where their proof ran, and what is open there waits on a run, a later wave or the owner. Moderation (Phase 7) is built through the dashboards, suspicion review and age tools. Beyond the four sections above the app has The Arcade (`/games`, an editorial games front page for the built-in Activities, Sketch & Slander among them) and The Study (`/study`, the shelves), and the signed-out surfaces carry none of the app's code (D-385); the Admin app's code is sent only to staff (D-651 to D-653). Waves 53 to 65 were audit and hardening waves (sign-in and step-up rules, blocks, storage response types, takedowns, resumable large Activity packages, the beta bug reporter, gateway resync): [CHANGELOG.md](CHANGELOG.md) lists what changed for members and operators.

**Proven.** At the phase 19 integration (2026-10-22) the merged tree passed 2,824 Rust tests, 516 Postgres-backed tests, 2,489 web unit tests and the Chromium Playwright suite on a development machine ([handoff](docs/dev/handoff/feat-p19-integration.md)). At the phase 46 integration (2026-11-02) the web suite was 446 files and 4,062 tests, with the Rust, authorization-matrix and IMMUTABLE RULE gates run as listed in [its handoff](docs/dev/handoff/integration-phase46.md). That is proof of the code, not of an installation.

**Not yet proven.** The phase exit gates in [docs/13-roadmap-and-phases.md](docs/13-roadmap-and-phases.md) say, row by row, which have a recorded CI run and which do not. Several now do: the production Compose stack boots, bootstraps and signs in through Caddy in `prod-smoke.yml`; real ffmpeg transcoding, the HLS ladder and Live TV airing run in `media.yml`; `e2e.yml`, `storage.yml` (live S3 against Garage) and `openapi.yml` have run on GitHub; the load target passed on CI (2,000 sessions, 200 msg/s). Still open: the stack, `deploy/install.sh`, the systemd units and the production Caddyfile have never run on a real host; no run of `release.yml` (it runs on a `v*` tag; its pull-request twin is `release-dry-run.yml`) is recorded; a GPU transcode worker and Safari playback have not been run; the ten-member, two-hour voice session, two weeks of real chat, a signet payment, a real Discord server and the manual screen-reader pass are the owner's. The plan that closes these is [docs/dev/roadmap-p20.md](docs/dev/roadmap-p20.md) (waves 20 to 24, decision D-352), with its items in [docs/inventory/20-launch-readiness.md](docs/inventory/20-launch-readiness.md).

**Going live.** What only the owner can do is in [docs/dev/owner-checklist.md](docs/dev/owner-checklist.md), the ordered install and launch steps are in [deploy/GO-LIVE.md](deploy/GO-LIVE.md), and the questions for a lawyer are in [docs/legal/lawyer-brief.md](docs/legal/lawyer-brief.md). Everything you need to work on the code is in [`docs/`](docs/README.md).

**Running it.** The operator handbook, the one page to start from once the instance is up, is [deploy/OPERATOR.md](deploy/OPERATOR.md).

## Start here

- **Humans:** read [docs/00-vision-and-scope.md](docs/00-vision-and-scope.md), then [docs/02-architecture.md](docs/02-architecture.md).
- **AI coding agents:** read [AGENTS.md](AGENTS.md) first. It tells you exactly what to read for the task you were given.
- **Contributing** (branches, commits, checks): [CONTRIBUTING.md](CONTRIBUTING.md).
- **Every decision the owner has made**, with its rationale, is in [docs/01-decisions-log.md](docs/01-decisions-log.md). When docs disagree, the decisions log wins.

## Repository layout

```
apps/web/            React + TypeScript + Vite PWA (all user-facing UI; the public entrance pages are
                     separate small builds, see docs/dev/entrance-isolation.md)
crates/              Rust workspace (servers, workers, shared libraries, WASM modules)
packages/            Shared TypeScript packages (generated API client, UI kit, WASM bindings, bot and
                     Embedded App SDKs, Markdown renderer, profile blocks, starter packs and the
                     built-in Activities under starter-packs/activities)
e2e/                 Playwright suites, the page sweep, the static-page and 3D starter suites and the
                     SDK compatibility run
scripts/             Repo checks and generators (bundle budget, doc links, ops files, entrance isolation,
                     staff delivery, secret scan, licences, i18n audit)
deploy/              Docker Compose, Dockerfiles, systemd units, monitoring (alerts, Grafana),
                     install, backup and restore scripts, and the operator documents (OPERATOR.md,
                     GO-LIVE.md, RUNBOOK.md and others)
docs/                Specifications, decisions log, threat model, ADRs, component inventory, owner
                     checklist and developer notes
```

## License

**Unpublished; not proprietary; no licence chosen yet** (D-309, D-353). The owner's recorded intent is that the code is not meant to be proprietary, only not yet published, and that GPL dependencies are acceptable. No licence file exists, so until one is added the code is not offered under any licence to anyone outside the project. The Rust workspace and the npm packages carry no licence expression and are marked unpublishable (`publish = false`, `"private": true`); the earlier `LicenseRef-Proprietary` label is withdrawn.

Third-party licences and their texts are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Two of them are copyleft and their obligations attach when the **web bundle is distributed**, which includes serving the web app to people outside the operator: `rvfc-polyfill` (GPL-3.0, bundled in the JASSUB chunk) and JASSUB (LGPL-2.1-or-later, shipped as its own unminified chunk, worker and `.wasm` so it can be replaced). An operator who opens an instance to others should offer the corresponding source of the web bundle alongside the notices (D-309, D-314). Not legal advice.
