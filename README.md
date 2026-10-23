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

**Built, not yet installed for real.** The application exists: the Rust workspace under `crates/`, the web app under `apps/web`, the shipped deployment files under `deploy/`. Every component in the [inventory](docs/inventory/README.md) files 01 to 19 is ticked except three that are closed or wait on a run (`DON-PAY-009`, `XC-CI-005`, `DEV-SDK-003`). Moderation (Phase 7) is built through the dashboards, suspicion review and age tools.

**Proven.** At the phase 19 integration (2026-10-22) the merged tree passed 2,824 Rust tests, 516 Postgres-backed tests, 2,489 web unit tests and the Chromium Playwright suite on a development machine ([handoff](docs/dev/handoff/feat-p19-integration.md)). That is proof of the code, not of an installation.

**Not yet proven.** The phase exit gates in [docs/13-roadmap-and-phases.md](docs/13-roadmap-and-phases.md) are mostly unrun: the production Compose stack, `deploy/install.sh`, the systemd units and the production Caddyfile have never booted on a real host; ffmpeg has never run in this project (transcoding is tested on canned output); `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` have never run on GitHub; the full load target and the manual screen-reader pass are open. The plan that closes these is [docs/dev/roadmap-p20.md](docs/dev/roadmap-p20.md) (waves 20 to 24, decision D-352), with its items in [docs/inventory/20-launch-readiness.md](docs/inventory/20-launch-readiness.md).

**Going live.** What only the owner can do is in [docs/dev/owner-checklist.md](docs/dev/owner-checklist.md), the ordered install and launch steps are in [deploy/GO-LIVE.md](deploy/GO-LIVE.md), and the questions for a lawyer are in [docs/legal/lawyer-brief.md](docs/legal/lawyer-brief.md). Everything you need to work on the code is in [`docs/`](docs/README.md).

## Start here

- **Humans:** read [docs/00-vision-and-scope.md](docs/00-vision-and-scope.md), then [docs/02-architecture.md](docs/02-architecture.md).
- **AI coding agents:** read [AGENTS.md](AGENTS.md) first. It tells you exactly what to read for the task you were given.
- **Every decision the owner has made**, with its rationale, is in [docs/01-decisions-log.md](docs/01-decisions-log.md). When docs disagree, the decisions log wins.

## Repository layout (target)

```
apps/web/            React + TypeScript + Vite PWA (all user-facing UI)
crates/              Rust workspace (servers, workers, shared libraries, WASM modules)
packages/            Shared TypeScript packages (generated API client, UI kit, WASM bindings)
deploy/              Docker Compose, systemd units, provisioning scripts
docs/                Specifications, ADRs, component inventory
```

## License

To be decided by the owner. Until a license file is added, all rights are reserved.
