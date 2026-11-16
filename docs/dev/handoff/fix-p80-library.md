# Wave 80 handoff: library (SEC-AUD-317, 318; D-819, D-820, D-829) - PARTIAL (checkpoint after the build)

T = 07:30 (2026-11-16): no new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30.

## Item 1, SEC-AUD-317: done
- Confirmed: `git log 8bc63455..HEAD -- crates` lists 8 commits (SEC-AUD-289/290, 292, 309 and wave 72's).
- Build `cargo build -j 4 -p sr-server -p sr-cli --offline`, dev profile, own target: start 07:28, end 07:37, 9m14s, base `c8982549`, no warnings, no download. Binaries and `BUILT_FROM.txt` (written last) are in `C:/Users/braxt/Desktop/SIDEROOM_wt/p80-base-bin/`. `p80-library-target` deleted. Lock released 07:37.
- Not yet done: the stack's start log line (version) and `/readyz` from the copy: that is in the shots round.

## Item 2, SEC-AUD-318: CSS and spec committed (`fb51c377`); shots round NOT run
- `library.css`, reader's-bars dark block: added `--ui-danger: #b4503f; --ui-danger-ink: #fff6f2; --ui-danger-text: #d97a68; --ui-success: #8c9a6a; --ui-warning: #c2b08f; --ui-info: #9aa8a0;` and, from the comparison with the `sideroom` set, `--ui-focus: #d9c9a4` (the kit's Button, Controls and Input read it). `--ui-bg`, `--ui-bg-overlay`, `--ui-backdrop`, `--ui-status-*` are not read by kit parts on the bars. check-css-tokens clean, prettier clean.
- Spec `e2e/tests/p80-library-shots.spec.ts` (local: `SR_P80_LIBRARY=1`, `SR_P80_BASE=1` for the base case; typechecks). Finding while writing it: `storage.endpoint = memory:` has no working presigned PUT, so a real upload through the Library's upload route cannot work on the UI stack (it needs the live Garage stack). The spec therefore makes real series and published chapters through the Admin API (18+ series with an images chapter; a book series with a pdf and an epub chapter), real comments and reviews from a second account, and answers the pdf/epub chapters' manifest `source_url` and the page images with `page.route` from files the spec writes itself (a 2-page PDF string, an EPUB as a stored zip). Run the server with `SR_ADMIN_REQUIRE_REAUTH=0`. Unverified until run: that the Admin routes accept `source_key` chapters and publish them.
- Run it once held back: `cp library.css` to scratch, `git show c8982549:apps/web/src/library/library.css > library.css`, run with `SR_P80_BASE=1` (probe not asserted), restore.

## Not done
The shots round (base probe, 28 shots, click-through, contrast and overflow figures), the handoff tail (figures per page, inventory rows, changelog sentence, D-820 doc sentence), cleanup of `sr_p80_library_ui` (never created) and `p80-library-bin` (never created).

## Proposed inventory rows
- SEC-AUD-317: "The UI test binaries are rebuilt from the base (D-819): `p80-base-bin`, built from `c8982549` in 9m14s." Closed.
- SEC-AUD-318: open until the shots round runs.
