# feat/p28-byos-legal handoff (BYOS-LEG-001, OPS-IDX-001)

## Resume here
Slice 1 done (non-Rust OPS-IDX-001, committed). Next, in order:
1. OPS-IDX-001 Rust: `X-Robots-Tag: noindex, nofollow` in `crates/sr-api/src/security_headers.rs` (every response), `routes/activities/proxy.rs`, sr-voice HTTP answers; Rust test in sr-api that API, `/cdn`, `/developers`, health responses carry it. (Commander: first cargo build only after slice 1 commit, one cargo at a time, `-j 2`.)
2. OPS-IDX-001 docs: one sentence in docs/09-retention-and-privacy.md, one row in docs/15-threat-model.md; connect wizard guidance on bucket listing (optional).
3. BYOS-LEG-001: migration 1460 (`storage_object` item kind), sr-moderation takedown, sr-admin data_requests export/erasure of mounts, admin storage delist route, `delisted` stub in scopes, lawyer brief Q20-Q23, moderation-gate step.
Worktree has node_modules (offline install done); wasm not built.

## Done
- OPS-IDX-001 (a) `apps/web/public/robots.txt` (Allow: /$, Disallow: /).
- (b, Caddy part) `Caddyfile.prod`: `@notLanding not path /` + `header @notLanding X-Robots-Tag` at site level (covers /api, /gateway, /cdn, /healthz, /developers, SPA); `activities.snippet` header. `render-caddyfile.mjs` only rewrites the CSP line, so needs no change; `csp.test.ts` pins the lines and robots.txt.
- (c) `apps/web/src/lib/useNoindex.ts`, called from RootLayout in `router.tsx`; Vitest `useNoindex.test.tsx` (sample routes + real router).
- (e2e) `e2e/tests/noindex.spec.ts`, wired into prod-smoke.yml stack job (`entrance-smoke noindex`), YAML validated with js-yaml. Not run (needs the Caddy stack; CI only).

## Tests run
vitest useNoindex.test.tsx (11), csp.test.ts (13); eslint + tsc on apps/web clean; e2e tsc clean.

## Open
Everything under "Resume here". No owner questions yet.
