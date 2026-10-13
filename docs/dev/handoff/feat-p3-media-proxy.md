# Handoff: feat/p3-media-proxy (F-08, F-09, F-20, parts of F-34)

Branch `feat/p3-media-proxy`, based on `integration/phase2`. Not pushed.

## Done

**F-08 bucket CORS (D-175)**
- `crates/sr-storage/src/s3admin.rs`: `S3Admin::put_bucket_cors(origins)` and `get_bucket_cors()`. `object_store` only presigns object requests, so the SigV4 header signer is hand written over `hmac` and `sha2` (checked against the AWS documentation example). The HTTP call uses `reqwest` with rustls (already in the tree through `sr-unfurl`; no new crate in `Cargo.lock` beyond the dependency edge). The document is one rule: `PUT, GET, HEAD`, `Content-Type`, expose `ETag`. Wildcard and non-origin values are refused.
- `sr-cli storage ensure-cors [--origin <url>]...` (defaults to `server.public_url`), idempotent, runs before the database connection so it needs no Postgres.
- `deploy/compose/garage/bootstrap.sh` calls it when an `sr-cli` binary is found; `deploy/INSTALL.md` step 4 documents it.
- Tests: signer vector, timestamps, origin rules, XML, an in-process fake S3 endpoint (`tests/s3_cors_mock.rs`), and an ignored live test `ensure_cors_is_applied_and_idempotent` in `tests/s3_live.rs` (runs in `storage.yml`; not run locally, there is no Garage here).

**F-09 image proxy (D-176, LNG-MSG-014)**
- `sr-storage::cdn::CdnSigner::external_path / verify_external`: `/cdn/external/{hmac}/{url_b64}`, HMAC key derived from `server.secret_key` ("sideroom/external-image/v1"), no expiry.
- `sr-unfurl`: `SafeFetcher::fetch_image(url, max_bytes)` reads a whole image (10 MiB for the proxy), same per-hop validation; `HopRequest.full_image_cap`; transport enforces the cap while streaming; `production_fetcher`. New `Policy.allow_hosts` (config `[unfurl] allow_hosts`), deny list unchanged.
- `sr-api` `routes/cdn/external.rs`: the handler (HMAC, cache hit, negative cache, per-user/IP/global limits, concurrency 6, fetch, sniff, `img::normalize`, store, strict headers, ETag/304), `gc_expired`, and `proxy_embed_images`.
- `proxy_url` is added at render time (REST, gateway payloads, forward snapshots) for `image`, `thumbnail`, and `author/footer.proxy_icon_url`; client-sent `proxy_*` values are removed. Nothing is stored, so no migration and key rotation just works.
- `sr-server`: `ExternalImageDeps` extension, `external_images.gc` hourly singleton job (id 141, `external_jobs.rs`).
- Rate buckets added to `RateLimitConfig` defaults: per address 300/10 s, fetch 30/min per user (or address), global 240/min.
- Tests: private/loopback/metadata/decimal/hex/octal/short IPv4, IPv6 mapped/NAT64/6to4/ULA/link-local, DNS rebinding across hops (stateful resolver), redirects into private space (IP, decimal, mapped, name, protocol-relative, scheme), size cap declared and streamed, 10 s timeout (paused clock), real-socket transport tests, route tests (headers, metadata stripped from a PNG with a tEXt chunk, 2048 px bound, only real images, expiry, GC, negative cache, rate limits, 304, signature failures, the normal CDN route unaffected).

**F-20 web**
- `apps/web/src/lounge/msg/Embeds.tsx`: text always renders; third-party image URLs are never put in an `<img>` until the reader clicks the per-embed "Load image" control (then with `referrerPolicy="no-referrer"`); a `proxy_url` loads directly only if it is a same-origin `/cdn/external/` path (anything else is ignored). `RawEmbed` gained `proxy_url` / `proxy_icon_url`. 9 new tests in `lounge/__tests__/embeds.test.tsx`.

**Docs**
- doc 02 §4.5 (CORS) and new §4.6 (third-party images); doc 05 §3 (`proxy_url` semantics) and §5.2 (the proxy and its status table); inventory LNG-MSG-014 ticked; follow-ups F-08, F-09, F-20 ticked with notes, F-34 annotated (partly done).
- `packages/api-client/openapi.json` regenerated: no change (the CDN routes and embed values are not in the OpenAPI document), so the client was not regenerated.

## Gates
`cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (968 passed, 0 failed, 89 ignored: Postgres and live S3), `cargo deny check` all green. Web: `tsc --noEmit`, `eslint .`, `vitest run` (434 passed), `prettier --check` green. New tests: about 42 Rust, 9 web.

## Not done / differences
- Output is JPEG (JPEG sources), animated GIF (GIF) or PNG, capped at 2048 px. The F-09 text asks for WebP/JPEG, but the pure-Rust `image` encoder has no lossy WebP; adding `webp` (F-04's libwebp binding) later would allow it. `fast_image_resize` was not needed.
- Operator allow/deny lists exist as config (`[unfurl] allow_hosts`, `deny_hosts`) only; the Admin → Features → Embeds page is not built.
- `video` URLs get no `proxy_url` (images only); the existing click-to-load iframe gate is unchanged.
- "Load image" for an embed without a proxy copy still ends in an `<img>` pointing at the other site after the click (the only way to show a cross-origin image without CORS); it is click-gated and referrer-free.
- The "Load remote images automatically" setting does not exist in the client, so nothing was hidden.
- Garage was not available: the SigV4 call is verified against the AWS vector and a fake endpoint, and the live test is written but unrun. Garage ignoring the checksum headers is expected; AWS-style stores need one of them, which is why SHA-256 is sent.
- F-34: the SPOILER_ rule and READY `recipients` parts are left for the branches that own D-177 and D-181.
- Job id 141 is reserved for `external_images.gc`; check against other branches for collisions (ids in use: 1 to 4, 110, 120, 130).

## Owner decisions
1. Should unauthenticated visitors be able to use the proxy with a valid link (today: yes, like `/cdn/*` signed links, limited per address), or require a session? Requiring one would stop link holders outside the app but costs a session lookup per cached image.
2. Is 30 per minute per user and 240 per minute globally for cache misses the right default for a 20k-account ceiling?
3. Accept PNG/JPEG/GIF output now, or take on libwebp (F-04) first to emit lossy WebP?
