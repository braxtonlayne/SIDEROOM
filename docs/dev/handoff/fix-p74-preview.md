# Wave 74 handoff: preview (SEC-AUD-292; D-782, D-783)

## Done
- Commits on `fix/p74-preview` (all start with `SEC-AUD-292`; see `git log`).
  1. The server: `crates/sr-chat/src/preview.rs` gets two private functions, `strip_marker` (one line) and `without_markers` (the whole text, line by line, outside code fences), called by `excerpt` before `fold`. Everything after is untouched (the fold, the cut, D-728's check, D-735's closers).
  2. The client's twin, `apps/web/src/lounge/msg/excerpt.ts` (the one function that builds a live row's line, see below), got the same rule (`stripMarker`, `withoutMarkers`) with `apps/web/src/lounge/__tests__/p74Preview.test.ts` beside it; the docs, `CHANGELOG.md` and `deploy/UPGRADE.md`.
- The shots spec `e2e/tests/p74-preview-shots.spec.ts` (`SR_P74_PREVIEW=1`).

## The confirm test, and that I watched it fail
`heading_and_subtext_markers_are_dropped_at_line_starts` (and `a_long_heading_is_cut_as_before`, `a_fenced_block_keeps_its_lines`'s unclosed case) were written first with the wanted result and run on the unchanged function in the first build: `# One ## Two ### Three` came out where `One Two Three` was wanted, so the fault is as the plan says. Then the function was added and the same tests ran green (21 of 21 in `preview`). The client's vitest was run only after its change (not watched failing on the base: it is a copy of the server's cases).

## How "heading" is decided
A copy of the parser's condition, not its function: `sr_markdown::block::parse_blocks` ("Headings and subtext", `crates/sr-markdown/src/block.rs` lines 212-237) is private to the block pass and works on a whole region (it also does quotes, lists and fences), so it cannot be asked about one line. Copied: 1 to 3 `#` at the very start of the line, then a space, and what follows is not empty after `trim()`; or the line starts with `-# ` and what follows is not empty after `trim()`. No leading spaces are allowed (` # x` stays), `####` stays, `#` alone and `# ` alone stay, `-#x` stays. The text kept is the trimmed rest.
Two additions of mine, both to keep the preview consistent with the parser:
- Fences: a line holding three backticks, up to the next such line, is left alone; an unclosed one is no fence (the parser needs a closing fence too), so the lines after it are read as lines. (The parser opens a fence mid-line; the same here, since the test is "contains".)
- A heading whose content holds three backticks is left as typed: stripping `# ` would put a fence opener at the start of the line and the parser would read the rest as code (this is what the existing property test `cut_text_reads_as_the_start_of_the_whole` found on its corpus input `# ```x```` ).

## One existing test was changed (please look)
`safe_cut_tests::cut_text_reads_as_the_start_of_the_whole` builds its "whole" by folding the raw text and parsing it. With markers dropped, the excerpt is of the text without markers, so its corpus input `# a\n# b` (repeated) no longer matched an oracle that still contained the `#`s. I changed one line of the test's oracle: it folds `without_markers(&text)` instead of `text`. The property is unchanged (the cut reads as the start of what the excerpt is of). All other tests are unchanged.

## D-783: what a later line that starts with a quote or list marker prints today (unchanged, recorded as tests)
`later_quote_and_list_markers_print_as_typed_today` (server) and the same in `p74Preview.test.ts` (client):
- `a\n> quoted` prints `a > quoted`
- `a\n>>> quoted` prints `a >>> quoted`
- `a\n- item` prints `a - item`
- `a\n1. item` prints `a 1. item`
So the marker prints as typed in all four (the fold joins before the client's parser sees a line start, as it did for headings).

## The client
Yes, there is a twin: `apps/web/src/lounge/msg/excerpt.ts` (`excerpt`), used by `lounge/previewLive.ts` for a row that arrives live (a new message in a thread, the sender's next message for a Message Requests row, an edit). It folded first, so a live row printed `##` too. It now has the same rule. `excerptPlain` (notification text, already plain) is not changed.

## Not done / limits
- Known edge, left: after a marker is dropped, what follows is read by the client's one-line parser as before, so `# - x` leaves `- x` (printed as typed, like D-783) and `# # x` leaves `# x` as the client's parser reads it. Not decided; rare.
- Shots: see "Shots" below (24, all opened).

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p74-preview` (local only, `SR_P74_PREVIEW=1`, `e2e/tests/p74-preview-shots.spec.ts`, chromium, the stack run from my own binaries, one test, passed). **24 PNGs, all 24 opened and looked at by me**: `thread-loaded-`, `thread-live-`, `request-loaded-` and `request-live-` x `desktop`/`phone` x `lounge-editorial`/`lounge-light`/`lounge-dark`. Phone is a 375x812 context with a mobile user agent and touch. They show: a thread whose latest message is `# Alpha heading / ## Beta heading / ### Gamma heading` reads `Alpha heading Beta heading Gamma heading`; one whose latest message is `intro words / # Title words / -# small print words` reads `intro words Title words small print words`; the live row (a new thread message while the channel is open, the sender's next DM) reads the same and no `#` (the live one goes through the client's twin; the toast "sent you a message" is clean too). The spec asserts the row text holds no `#`. The Message Requests page on a phone and the thread chip on a phone wrap normally; no other fault seen in the 24 (the "NEW" badges and the extra servers in the rail are leftovers of my earlier aborted runs in the same scratch DB). **No base shot**: there is no base server binary, the unit test is the base evidence.
Two aborted runs happened first (the spec ran in firefox and webkit, which are not installed here: `--project=chromium` fixed it); they made no shots.

## Tests run
- `cargo test -j 4 -p sr-chat --lib preview`: 21 passed (6 new, 15 existing; the existing `cut_text_reads_as_the_start_of_the_whole` with its oracle changed by one line, above). First run on the base function: the new tests failed as expected (see "confirm"), nothing else ran against the base.
- `cargo clippy -j 4 -p sr-chat --all-targets -- -D warnings`: clean. `cargo fmt -p sr-chat --check`: clean.
- `cargo build -j 4 -p sr-server -p sr-cli`: built (10.5 minutes, cold, own target).
- web: `vitest run src/lounge/__tests__/p74Preview.test.ts` 4 passed; web `tsc --noEmit` clean; eslint and prettier --check clean on both files. `node scripts/check-doc-links.mjs`: ok. No i18n string added. I did not run `p67Previews`, `p68Previews`, `p70Plain`, `p71Signals` (CI does).
- No route or response shape changed; nothing regenerated.

## Docs updated
`docs/specs/lounge-chat.md` (the Message Requests row sentence, one clause on the markers and D-783), `docs/05-api-and-gateway.md` (the `sr_request_preview.content` sentence and the `sr_last_message` sentence), `CHANGELOG.md` (Phase 74: Members, Library from uploads' handoff, Operators), `deploy/UPGRADE.md` (release notes: wave 74, no migration, nothing to do). Read at 16:24: uploads' and dots' handoffs both existed; I used uploads' changelog sentence; dots has none for members. If either handoff changes later, the commander adjusts the Library line.

## Proposed inventory row text (close SEC-AUD-292)
SEC-AUD-292 closed in wave 74 (D-782, D-783): `sr_chat::preview::excerpt` drops a heading marker (`# `, `## `, `### `) or subtext marker (`-# `) at the start of every line outside a code fence before the lines are joined, by the parser's own condition; the client's twin `lounge/msg/excerpt.ts` does the same for live rows. `C#`, `issue #3`, `####` and fenced text stay. Quote and list markers on later lines print as typed (recorded in tests, D-783).

## Threat-model note
None: a look fault, not a leak. The cut check (D-728) and closers (D-735) run unchanged on the text without markers.

## Changelog sentence
In CHANGELOG.md: "A thread's last message and a Message Requests line no longer show `#`, `##`, `###` or `-#` in front of the words..."

## Cleanup
Stack stopped (sr-server and Vite by their process ids/command lines, mine only), lock released (it was held by dots from 15:23 to about 16:00, so my first test run waited about 35 minutes), `sr_p74_preview_ui` dropped, `p74-preview-target` and `p74-preview-bin` deleted, no stash touched. Nothing queued or running.

## Owner questions
1. D-783: later-line `> `, `>>> `, `- `, `1. ` markers print as typed (`a > quoted`, `a - item`). Should the preview drop them too, as it now does `#`? Default: no (a `-` or `>` in the middle of a sentence after a line break is common; a bare quote marker reads as a quote).
2. After a marker is dropped the rest is read by the client as before: `# - x` leaves `- x`. Strip repeatedly (`# # x`)? Default: no, rare.
3. A heading that holds a fence opener (``# ```x``` ``) keeps its `# ` (the parser would read the rest as code). Fine? Default: yes.
