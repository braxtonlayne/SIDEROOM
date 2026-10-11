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

📐 **Specification phase.** No application code exists yet. Everything you need to start building is in [`docs/`](docs/README.md).

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
