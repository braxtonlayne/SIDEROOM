# Handoff: feat/p32-utl-search (UTL-SRC-001)

## Done
- Day-one checkpoint (commit 4527fc8b): `crates/sr-tools-wasm` (cdylib + rlib, wasm-bindgen; deps strsim 0.11.1,
  unicode-normalization 0.1, serde_json, all already in the lock), `lib.rs` with one `pub mod` per module
  (calc, dice, diff, gif, hash, palette, percent, search, units), empty stub files for the other agents' modules,
  and the `loadTools` entry in `packages/wasm/src/index.{js,d.ts}`.
- Three-stage search in `crates/sr-tools-wasm/src/search/` (`mod.rs`, `norm.rs`, `bm25.rs`, `tests.rs`):
  1. prefix over names and aliases (exact, name prefix, prefix of a later word: "converter" finds "Unit Converter");
  2. fuzzy with strsim Damerau-Levenshtein and Jaro-Winkler (edit allowance by length 0/1/2, also against each word
     and the same-length head of the name), with "Did you mean" when stage 1 found nothing and the best fuzzy score is >= 0.85;
  3. BM25 (k1 1.2, b 0.75) over the per-tool task phrases, with a 20-line English suffix stripper, stopwords dropped,
     diacritics folded through NFD.
  Channels and people come from the client cache (`setEntities`, names and aliases only, stages 1 and 2). A leading
  `>`, `#` or `@` limits to tools, channels or people. Query cut at 200 chars, limit capped at 100.
- Corpus `apps/web/src/tools/corpus.json`: 31 tools, 374 phrases (10 to 30 each), embedded with `include_str!`.
- `scripts/check-tools-corpus.mjs` (+ `scripts/check-tools-corpus.test.mjs`, picked up by `node --test "scripts/*.test.mjs"`):
  duplicate ids, names/aliases shadowing, empty tools or phrases, duplicate phrases within and across tools, 10..30 count.
- Facade: `loadTools()` returns `{ raw, search }`; `search.create()` gives `{ setEntities(list), search(q, limit), free() }`
  (typed in index.d.ts). Other agents add their key to the object `loadTools` returns.

## Tests run
- `cargo test -j 2 -p sr-tools-wasm`: 15 pass (every tool reachable by three of its phrases in the top 3, by its name
  with one typo (swap, delete, substitute, insert) in the top 3, by a 4-letter prefix in the top 5, and by full name at rank 0;
  3000-round hostile fuzz plus every prefix of every phrase, no panic).
- `cargo clippy -D warnings --all-targets` and `cargo fmt --check` on sr-tools-wasm: clean.
- `node scripts/check-tools-corpus.mjs` clean; `node --test scripts/check-tools-corpus.test.mjs` 5 pass.
- `wasm-pack build --release --no-opt` of the crate: 352,849 bytes raw, 118,820 bytes brotli (limit 300 KB; wasm-opt would shrink it).
- `packages/wasm` vitest `test/tools-search.test.ts` (3 tests, against the built wasm) pass; prettier clean.

## Still open
- `wasm-pack test` (browser/node wasm runner) was not run; the same assertions run natively with `cargo test`. The wasm
  facade test above covers the JS bridge.
- The FE half (drawer input calling `loadTools`, rendering "Did you mean", feeding the channel/people cache) belongs to
  utl-shell (`tools/drawer`); this item only provides the engine and facade.
- No CI step runs `check-tools-corpus.mjs` by itself (the `.test.mjs` runs in repo-checks via the scripts glob and checks
  the real corpus). The integrator may add `node scripts/check-tools-corpus.mjs` to repo-checks.yml.
- Tool ids in the corpus (calculator, programmer, units, currency, percent, timezone, json, regex, base64, url, jwt,
  md-table, counter, hash, diff, dice, coin, wheel, loot, notes, pomodoro, stopwatch, countdown, image-studio, redact,
  palette, gif, audio-trimmer, tier-list, poll, invite) must match `tools/registry.ts` ids (utl-shell); rename in the corpus
  if the registry differs. Names are English match text; the UI label comes from t().

## Decisions
- Search results are ordered by stage, then score, then shorter label. Stage 3 never repeats a stage 1 or 2 hit.
- Entities are not in BM25 (as specified).

## Owner questions
- None.

## CHANGELOG lines (Phase 32)
- Utility drawer search: `sr-tools-wasm` finds tools, channels and people by prefix, by typo ("Did you mean") and by task phrase, in the browser with no model (UTL-SRC-001).
