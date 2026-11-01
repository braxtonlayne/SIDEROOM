# Integration of wave 42 (ARC-ROOM-001, ARC-SBX-001): the Arcade, the hardened sandbox, two tiers

Branch `integration/phase42`, base `origin/main` 57b91ca9 plus wave 41 (`origin/integration/phase41`, PR #72, merged
twice: once as the base and again for bed2fd5d, the Library bar on phones). Then `feat/p42-arcade-sandbox` (f225e561)
and `feat/p42-arcade-room` (a8d30626), both `--no-ff`. Not pushed; no PR opened.

Handoffs: [feat-p42-arcade-room](feat-p42-arcade-room.md), [feat-p42-arcade-sandbox](feat-p42-arcade-sandbox.md).
Decisions: D-405 to D-411 (integrator defaults; the owner may change any).

## Merge conflicts and how they were resolved

- `crates/sr-activities/src/bundle.rs`: wave 41's `CAPABILITIES = [datachannel, byos_read, byos_write]` against the
  sandbox's `[datachannel, storage]`. Resolved, then made **one source of truth**: `arcade::FLAGS` (arcade-room's
  catalogue, now with `storage`) is what the validator (`accepts`), the runner (`is_active`, the flags a development
  instance gets) and `immutable_rules` read; `bundle::CAPABILITIES` is gone. This also fixed a real bug of the union:
  `is_active` checked `CAPABILITIES` first, so the reserved `byos_write` would have been handed to running instances.
- `docs/15-threat-model.md`: wave 41 and the sandbox both took T39. Wave 41 keeps T39 (shelf access); the hostile
  Arcade game is **T40**.
- `apps/web/src/lounge/Chrome.tsx`: wave 41's `RoomsNav` kept; arcade-room's hand-made links dropped.
- `crates/sr-api/tests/route_matrix.tsv`: union.
- OpenAPI, api-client and MSW handlers: regenerated after all changes (1170 operations, `mocks:check` green).

## What the integration changed

1. **URL-mapping writes from the sandbox (D-399 regression, D-408).** The opaque-origin game sends `Origin: null`,
   which the proxy refused, and `connect-src` dropped `'self'` whenever the app had URL mappings: no game could talk
   to its developer's server. Now:
   - migration **2070** `activity_proxy_tokens` (SHA-256 digest only; one per participant of a running instance);
   - `POST /activity-instances/{instance_id}/proxy-token` (membership: a current participant) mints it; the host
     mints it at the handshake and sends `proxy_token` in `ready`; `sdk.mappedFetch(path, init)` sends it as
     `X-SR-Activity-Token` (cors, no credentials);
   - the proxy accepts `Origin: null` on a mapped write only with a token valid for a running instance of **that**
     application whose member has not left (leaving or the end drops it; minting again replaces it). The CORS
     preflight is answered only for `Origin: null` under a mapping; only a checked write carries
     `Access-Control-Allow-Origin: null`. Any other origin is still 403. A request without `Origin` is unchanged;
   - reviewed URL mappings admit `'self'` in `connect-src`; mapped `GET`s answer `ACAO: *` (no credentials); error
     answers keep the strictest CSP.
   - Proofs: `sr-api` `a_game_with_a_url_mapping_posts_through_it_with_its_proxy_token` (Postgres: the game POSTs
     through `/api/scores` and reads the answer; no token, a made-up token, the token on another app and a stale
     token after leaving are 403 and never forwarded), `an_opaque_origin_without_a_proxy_token_cannot_write`,
     `the_preflight_admits_the_token_header_under_a_mapping_only`; `sr-activities` db
     `proxy_tokens_belong_to_a_participant_of_a_running_instance`; host `bridge.test.ts` and SDK `sdk.test.ts`.
   - The authorization matrix seeds a running instance that owner A takes part in, reached only by the new route
     (an `OVERLAYS` row): no `WRITE_OBSERVED` entry and no ceiling raised.
2. **RoomsNav**: the Arcade link no longer depends on `useEmulatorOn()` (D-411); `study.test.tsx` pins the three links.
3. **`trust_db.rs`**: `is_featured` is a plain `EXISTS` on `arcade_featured`; the test uses the real table.
4. **Guild cartridges on shelf objects (D-410)**: migration **2071** adds `shelf_id` + `object_hash` (FK to
   `shelf_objects`, both or neither). The lister needs an object in the catalog, not delisted, on a shelf they own, the
   server owns, or one granted to a role of the server; `source_ref` becomes `shelf:<id>/<hash>`. A delisted object
   leaves the cartridge listing. The card offers "Get <title> from the shelf", which mints the per-object URL with the
   member's own grant. Proof: `sr-activities` arcade `a_cartridge_names_a_shelf_object_and_a_delisting_hides_it`,
   Vitest `arcade.test.tsx`.
5. **Arcade cards render the sandbox's trust (D-409)** through arcade-room's `TrustProvider` seam (new `Card`
   component; `sandboxTrust` = `useBuildTrust` + `BuildTrust`, badges and Vouch for Build). **Key: `code_sha256`**
   (now on `FeaturedCard`), because the signature and the vouches are pinned to the code hash; `bundle_sha256`
   changes with art and audio. A changed code hash remounts the card's trust.
6. **Owner defaults** (already in the agents' code, recorded as D-405 to D-407): good standing = active, non-bot,
   account >= 3 days, <= 30 vouches/day; OPERATOR-FEATURED only while the current code is signed; UNVERIFIED UPDATE
   names the previous build by an 8-character hash prefix.

## Gates on the merged tree

GATES_TABLE

## Not done (with reasons)

- **`byos_read` in the sandbox** (CSP `connect-src` for presigned per-object hosts): not small. The presigned host is
  each member's own bucket endpoint, unknown when staff review a bundle, and the CSP is one per application response,
  not per member; there is also no bridge command that hands a game a per-object URL. Needs a design (for example a
  bridge `shelfObjectUrl` command plus a host-side fetch that returns bytes, so no bucket host enters the CSP). The flag
  stays "active" in the catalogue but does nothing yet.
- **A shelf picker for listing a cartridge.** The API takes `shelf_id` + `object_hash`; the web form still takes a
  shelf path (`source_ref`).
- From the agents, still open: Notify-me delivery (`ArcadeService::watchers`), a QR for profile tip links, Bitcoin
  address checksums, Tier 2 launching (wave 46).

## Owner items

See `docs/dev/owner-checklist.md`, section 4b (wave 42): D-405 to D-411, Notify delivery not built, profile BTC QR,
BTC checksum.

## Housekeeping

Scratch databases `sr_p42_integ` and `sr_p42_e2e`; `sr_p42_arcade_room` (the arcade-room agent's) still exists:
dropping it was refused by the session's permission check, so the owner drops it (`DROP DATABASE sr_p42_arcade_room;`).
