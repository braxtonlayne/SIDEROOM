# Handoff: feat/p15-jassub (K-13, CIN-PLY-004)

## Result: STOPPED, nothing shipped. Owner must re-confirm.

The owner approved JASSUB on the stated basis "nothing GPL in the shipped form". Adding `jassub@2.5.16` (newest version at least two weeks old; 2.5.17 is 13 days old) showed otherwise:

- `jassub` declares `LGPL-2.1-or-later AND (FTL OR GPL-2.0-or-later) AND MIT AND ... ISC AND NTP AND Zlib AND BSL-1.0`.
- Its hard dependency `rvfc-polyfill@1.0.8` (imported unconditionally by `dist/jassub.js`) is **GPL-3.0** (package.json and LICENSE both; `pnpm licenses list --prod` reports it). It would be bundled into the JASSUB chunk, which makes the shipped web app a GPL-3.0 combination question, not an LGPL one.
- Other new transitive deps: `abslink` (Apache-2.0), `lfa-ponyfill`, `throughput` (not flagged by `pnpm licenses`).

Because the approval premise is false, I did not commit the dependency, the loader or the notices. The docs only carry a note: `docs/open-questions.md` item 11 and `P15-RST-2` in `docs/open-question-answers.md` say "approved, blocked on the GPL-3.0 dependency". No decisions-log entry, doc 03 row or inventory tick was made (no D-258 allocated).

## Ready-to-apply work
A complete, tested draft is saved outside the repo: `C:\Users\braxt\AppData\Local\Temp\claude\C--Users-braxt-Desktop-SIDEROOM\58ac6655-62c7-4643-8650-a14f872feb96\scratchpad\k13-jassub-wip.patch` (package.json + lockfile, `cinema/player/jassubLoader.ts` with a stubbable lazy loader using `jassub/dist/worker/worker.js?worker&url` and the two `.wasm?url` files, `installJassub()` called from `WatchRoute.tsx`, `ass.ts` comment, `__tests__/jassubLoader.test.ts` with the library stubbed). Not yet run through the gates (typecheck, lint, vitest, build, budget): I stopped before them.

## Options for the owner
1. Approve GPL-3.0 `rvfc-polyfill` (not recommended: GPL-3.0 on a browser bundle conflicts with the LGPL-only reasoning and likely the project licence).
2. Approve JASSUB with a Vite alias replacing `rvfc-polyfill` by an empty module (jassub itself stays unmodified; only old browsers without `requestVideoFrameCallback`, Firefox before 132 and Safari before 15.4, lose ASS rendering and fall back to WebVTT). Needs the owner's explicit yes.
3. Close CIN-PLY-004 as WebVTT-only.

## Other notes
- Fonts: the track object exposes only `vtt_url`/`ass_url`; there is no fonts URL, so the draft uses `queryFonts: 'local'` plus JASSUB's bundled default font; K-13's `availableFonts` needs a server field.
- `web.yml` licence allow-list step (K-13) must also name `abslink`-style Apache-2.0 and exclude `rvfc-polyfill`; allow-list check not yet written.
- Tests run: none beyond dependency/licence inspection. No cargo, no Playwright.
