# fix/p53-limits (wave 53: bot intents, rate-limit identity, SFU caps)

## Done
- **SEC-AUD-004** (privileged intents during review): `Application` gains `was_released` (an earlier version was approved). The
  Postgres repo computes it in `app_cols` as `EXISTS (approved version)`, the same test the Activities lookup already uses for
  "released before" (`sr-activities/src/service.rs` `lookup`); the memory store sets it on approval. `allowed_intents()` returns
  `asked & intents_approved` for `InReview` when `was_released`, and `flags()` reports the full (not "limited") bits then.
  A development-only app in its first review keeps today's behaviour (the developer's set, test servers only). No migration needed.
  The gateway (`sr-gateway/src/conn.rs:369`) is unchanged: it enforces whatever `gateway_bot().allowed_intents` says, so the
  fix is covered by the service tests rather than a gateway test (the gateway test fixtures inject the allowed set directly).
- **SEC-AUD-007** (rate-limit identity from unverified credentials): the limiter keeps a bounded set of credential keys that
  recently authenticated (`proven`, 10 min TTL, 100k cap, only verified credentials enter it). `rate_limit` puts a
  `CredentialProof` in the request's extensions; `Principal::resolve` marks it on a successful `authenticate` (module_gate already
  resolves the principal on every API route, so this adds no DB work). A request whose credential is not proven is ALSO counted
  against the caller's address (global + route bucket; for webhook execute only the address's global share, the token bucket is
  unchanged) before its own bucket. Once proven, a credential keeps its own bucket without the address charge.
  Also the api-core MED item (real): `major_resource` now canonicalises the id (percent-decoded, numeric ids printed as `u64`), so
  `42`, `0042`, `+42` and `%34%32` share one bucket; webhook token resources are canonicalised the same way.
- **SEC-AUD-009** (SFU per-member maps): `declared` is capped at `MAX_DECLARED` (32); at the cap, inactive declarations of mids
  that never became tracks are dropped first, otherwise the publish is refused (WS closed as a bad video message, as for other
  bad video messages). `hints` is capped at `max_room_size * 4` (four hint sources per member); at the cap, hints about users
  not in the member's room are dropped first, then the message is refused.
  ICE-TCP accept loop: at most `MAX_TCP_CONNECTIONS` (4096) open at once (a semaphore; past it the new connection is closed at
  once), and an accept error backs off 50 ms instead of spinning.
  SetFlags-before-join (apps-voice MED, small, fixed): pending flags now keep the join token's `tier` and `video_full`
  (`with_pending` in `server.rs`).

Commits: `6355b4ba` (SEC-AUD-004), `b947059b` (SEC-AUD-007 + api-core per-spelling bucket; doc 05 "Abuse limits" preamble
says how unverified credentials are counted), `b329d938` (SEC-AUD-009 + ICE-TCP cap + SetFlags-before-join).

## Not done
- No gateway intents test added: `sr-gateway` only enforces the `allowed_privileged_intents` it is handed (its fixtures inject
  that number), so a gateway test could not fail without this fix. The service tests check `gateway_bot().allowed_intents`, which
  is exactly what `sr-server/src/gateway_pg.rs:104` feeds the gateway.
- I did not re-run each new test against the unfixed code (the heavy lock was busy for most of the session). By construction:
  the intents asserts get `MESSAGE_CONTENT|GUILD_PRESENCES` (memory) / `|GUILD_MEMBERS` (Postgres) without the fix; the rotating-token
  test gets 401 instead of 429 (each fake token had its own bucket); the SFU map tests see the maps grow to 500 / 2000 entries;
  the ICE-TCP test's third connection stays open; `with_pending` did not exist (the join replaced the flags wholesale).
- No visual check: no UI change. No migration, route, OpenAPI or threat-model change (the limiter is not a new surface).

## Tests run
`cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-apps -p sr-db -p sr-api -p sr-voice --all-targets -- -D warnings` and
`-p sr-api -p sr-server --all-targets` (clean); `cargo test -p sr-apps` (67), `-p sr-api --no-fail-fast` (822 lib + all integration
binaries, incl. `api.rs::rotating_made_up_credentials_are_limited_by_address`, `abuse_limits.rs`), `-p sr-voice` (lib 54 + every
integration binary, incl. e2e/video/music), `-p sr-db --test apps_repo -- --ignored` against scratch DB `sr_p53_limits` (19).
`node scripts/check-doc-links.mjs` ok.

## Owner questions
1. (gap-apps LOW, not changed) A released application's name, description and icon change live without review
   (`review_reasons` compares only scopes, redirect URIs and intents). Should a name (or icon) change of a released app need
   review, like Discord's verified-app name lock? Default kept: no review.
2. SEC-AUD-007 trade-off: a brand-new credential's first request (and every request of a credential the extractor never verifies,
   such as OAuth client Basic auth on `/oauth2/token` or a guest token) is also charged to the address. Behind one shared address
   (NAT), many fresh sign-ins at once share that address's allowance for their first request. Default: accept this.
3. SFU caps: 32 declarations per member and 4096 ICE-TCP connections per node are fixed constants, not settings. Fine as defaults?
