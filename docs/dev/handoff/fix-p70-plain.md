# Wave 70 plain handoff (SEC-AUD-242..245; D-722, D-723, D-728, D-731)

Branch `fix/p70-plain` (base `integration/phase70`). All four items are done. No migration, no route or response shape change: `openapi.json` and the generated client are untouched, nothing for the commander to regenerate. No new i18n key (no new string). Nothing pushed.

## Done (commits)
1. `ed64e10a` SEC-AUD-242: the preview cut never splits markup, server and client (D-728). `crates/sr-chat/src/preview.rs`, the shared case list `crates/sr-chat/src/preview_cases.json`, `lounge/msg/excerpt.ts`, `lounge/previewLive.ts` (comment only), the two route tests, `e2e/tests/p70-plain-shots.spec.ts`.
2. `6b8d598e` SEC-AUD-243 and SEC-AUD-245 together (D-722, D-731): they edit the same function, the same corpus goldens and the same web function, so one commit. `sr-markdown/src/util.rs` (`plain()`, `is_block`, `line_start`, `utc_date`), `tests.rs`, the corpus, `sr-search/src/chat.rs` (test), `lounge/msg/SingleLine.tsx`, `lounge/shell/notifyText.ts`, `lounge/__tests__/p70Plain.test.tsx`.
3. `8a2f40fa` SEC-AUD-244: a restored message counts in its thread (D-723). `lounge/threads/threadStore.ts` (`countMessages`).
4. `4c0922b5` docs: `docs/05-api-and-gateway.md` (the request preview, the thread row's `sr_last_message`, the push body), `docs/specs/lounge-chat.md` (requests, thread row and its live count, notifications, search).
5. The last commit: the shots spec fixes and this handoff.

## Not done
Nothing dropped. Two things deliberately left, both outside my files:
- `lounge/shell/DmToasts.tsx` (lines): the DM toast "sent you a message" still shows the raw text. The shots caught it: after the stranger sends `look ||the swordfish is under ...`, the toast reads `look ||the swordfish is under the third stone ...` (see the `request-live-*` shots). That is lines's item (the DM toast as typed, plan fact 6); it can use `notificationText`.
- The list work-around in `crates/sr-server/src/push_notify.rs::plain_text` stays (it now adds a space the folding removes); the commander deletes it later, as the plan says.

## What the parser does with an unclosed spoiler (how serious item 1 was)
**It is a real leak.** `sr_markdown::parse` of the raw 140-character cut of `a ||` + 200 letters + `||` has no `Spoiler` node: the unclosed `||` is plain text and `to_plain_text` contains the hidden letters. Confirmed on the base two ways: the Rust test `a_cut_never_leaves_a_spoiler_open` (it asserts the parse has no spoiler and the plain text has the letters, then that the excerpt has none: it failed at the excerpt), and both route tests: the thread row's `sr_last_message.content` and the request's `sr_request_preview.content` came back as `look ||hiddenhidden...` with the hidden words in the clear, from the real server code. On the web, base `excerpt` gave the same text and `singleLineText(parseMarkdown(...))` showed the letters (vitest "confirm (b)" fails on base). A mention cut in the middle (`<@12345`) was shown as text (confirm (c) fails on base too).

## The cut rule in two sentences, and its cost
`excerpt(content, 140)` folds white space and, when the text is over 140 characters, takes the cut as before (with the combining-mark rule), then keeps it only if `to_plain_text(parse(cut))` (trimmed at the end) is a prefix of `to_plain_text(parse(whole folded text))`; otherwise the cut moves back to just before the last markup opener in the cut (`||`, `<`, backtick, `[`, `*`, `_`, `~`), then to the previous white space, alternately, at most 16 times, and a cut that reaches 0 gives `None` (an empty `content`). The web twin does the same with `parseMarkdown` and `singleLineText` (spoiler word `[spoiler]`, default names) and returns `null` when a cut is needed and the parser is not loaded; a text of 140 characters or fewer is never parsed on either side.
- Worst case measured (`worst_case_time_is_small`, a debug build): 4,400 characters of nested markup (`**a _b ~c ||d `e` [f](http://x) <@1>|| ~_** ` repeated 100 times): 880 microseconds for the whole `excerpt` call (one full parse and up to 17 short ones).
- The notification (`notifyText.ts`) cuts a text that is already plain (it ran `singleLineText` first), which has no markup left to split, so it uses the new `excerptPlain` (the old rule, no parser): what it shows did not change. Every `previewLive.ts` caller does `excerpt(...) ?? ''`: a `null` gives an empty `content`, and the row shows the attachment count or nothing.

## Block pairs: before and after (`plain_text_keeps_blocks_apart`, Rust)
"Before" is the output of the failing run on the base. Rows marked WRONG were wrong before and are fixed.

| pair | input | before | after |
|---|---|---|---|
| text then list | `two\n- a` | `twoa\n` WRONG | `two\na\n` |
| text then quote | `two\n> q` | `twoq\n` WRONG | `two\nq\n` |
| text then heading | `two\n# h` | `twoh\n` WRONG | `two\nh\n` |
| text then subtext | `two\n-# s` | `twos\n` WRONG | `two\ns\n` |
| a list inside a quote | `> two\n> - a` | `twoa\n\n` WRONG | `two\na\n\n` |
| text then code block | `two`, a fenced block `x` | `two\nx` (the parser keeps the line break) | same |
| code block then text | a fenced block `x`, then `two` | `x\ntwo` | same |
| quote then text, heading then text, list then text, two lists, a list alone, plain lines | | right | same |

A block's own text is unchanged; there is no double `\n` (a `LineBreak` right after a block gets no second one).

## Every corpus value that changed (60; `plain` only, `ast` and `jumbo` untouched)
Regenerated with `SR_MD_BLESS=1 cargo test -p sr-markdown --test corpus`, then each read by eye. All are "a separator appeared" or "a timestamp became a date". Format: file, case, input, before -> after.

```
code-blocks.json | block-005 | "a ```x``` b" | "a x b" -> "a \nx\n b"
code-blocks.json | block-034 | "before ```\ncode\n``` after" | "before code after" -> "before \ncode\n after"
code-blocks.json | block-035 | "```\ncode\n``` trailing" | "code trailing" -> "code\n trailing"
code-blocks.json | block-038 | "- ```\n  code\n  ```" | "-   code\n  " -> "- \n  code\n  "
code-blocks.json | block-039 | "# ```x```" | "# x" -> "# \nx"
code-blocks.json | block-040 | "**```x```**" | "**x**" -> "**\nx\n**"
code-blocks.json | block-041 | "||```x```||" | "||x||" -> "||\nx\n||"
code-blocks.json | block-042 | "```\nx\n``` ```\ny\n```" | "x y" -> "x\n \ny"
headings.json | head-014 | "a\n# b" | "ab\n" -> "a\nb\n"
headings.json | head-033 | "a\n-# s" | "as\n" -> "a\ns\n"
inline-code.json | icode-035 | "````x````" | "`x`" -> "`x\n`"
jumbo.json | jumbo-071 | "😄 <t:1>" | "😄 1" -> "😄 1970-01-01 00:00 UTC"
lists.json | list-005 | "- a\n  - b" | "ab\n" -> "a\nb\n"
lists.json | list-006 | "- a\n  - b\n    - c" | "abc\n" -> "a\nb\nc\n"
lists.json | list-007 | "- a\n    - b" | "ab\n" -> "a\nb\n"
lists.json | list-008 | "- a\n- b\n  - c\n- d" | "a\nbc\nd\n" -> "a\nb\nc\nd\n"
lists.json | list-016 | "1. a\n   - b" | "ab\n" -> "a\nb\n"
lists.json | list-017 | "- a\n  1. b" | "ab\n" -> "a\nb\n"
lists.json | list-018 | "1. a\n   1. b" | "ab\n" -> "a\nb\n"
lists.json | list-035 | "x\n- a" | "xa\n" -> "x\na\n"
lists.json | list-036 | "x\n1. a" | "xa\n" -> "x\na\n"
lists.json | list-037 | "- a\nx\n- b" | "a\nxb\n" -> "a\nx\nb\n"
lists.json | list-047 | "- ```x```" | "- x" -> "- \nx"
lists.json | list-048 | "1. ```x```" | "1. x" -> "1. \nx"
mixed.json | mixed-002 | "hey <@1>, check <#2> and <@&3> at <t:1700000000:R> :tada:" | "hey @user, check #channel and @role at 1700000000 :tada:" -> "hey @user, check #channel and @role at 2023-11-14 22:13 UTC :tada:"
mixed.json | mixed-003 | "# Title\nSome **text**\n> quote\n- item 1\n- item 2\n```js\ncode\n```" | "Title\nSome textquote\nitem 1\nitem 2\ncode" -> "Title\nSome text\nquote\nitem 1\nitem 2\ncode"
mixed.json | mixed-004 | "> quote **b**\n> more\nnormal\n>>> multi\nline" | "quote b\nmore\nnormalmulti\nline\n" -> "quote b\nmore\nnormal\nmulti\nline\n"
mixed.json | mixed-005 | "- a\n  - b\n    - c\n- d\n\n1. x\n2. y" | "abc\nd\nx\ny\n" -> "a\nb\nc\nd\nx\ny\n"
mixed.json | mixed-025 | "a```b```c```d```e" | "abcde" -> "a\nb\nc\nd\ne"
mixed.json | mixed-026 | "```a``` ```b```" | "a b" -> "a\n \nb"
mixed.json | mixed-030 | "1. **a**\n   - b\n   - c\n2. d" | "ab\nc\nd\n" -> "a\nb\nc\nd\n"
mixed.json | mixed-088 | "a\n- b\nc" | "ab\nc" -> "a\nb\nc"
mixed.json | mixed-089 | "a\n1. b\nc" | "ab\nc" -> "a\nb\nc"
mixed.json | mixed-090 | "a\n> b\nc" | "ab\nc" -> "a\nb\nc"
mixed.json | mixed-091 | "a\n# b\nc" | "ab\nc" -> "a\nb\nc"
mixed.json | mixed-092 | "a\n-# b\nc" | "ab\nc" -> "a\nb\nc"
quotes.json | quote-012 | "> a\n>\n> b" | "a\n>b\n" -> "a\n>\nb\n"
quotes.json | quote-019 | "x\n>>> y\nz" | "xy\nz\n" -> "x\ny\nz\n"
quotes.json | quote-021 | "a\n> b\nc" | "ab\nc" -> "a\nb\nc"
quotes.json | quote-040 | ">> a\n> b" | ">> ab\n" -> ">> a\nb\n"
timestamps.json | ts-001 | "<t:1700000000:t>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-002 | "<t:1700000000:T>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-003 | "<t:1700000000:d>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-004 | "<t:1700000000:D>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-005 | "<t:1700000000:f>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-006 | "<t:1700000000:F>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-007 | "<t:1700000000:R>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-008 | "<t:1700000000>" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-009 | "<t:0>" | "0" -> "1970-01-01 00:00 UTC"
timestamps.json | ts-010 | "<t:0:R>" | "0" -> "1970-01-01 00:00 UTC"
timestamps.json | ts-013 | "<t:-0>" | "0" -> "1970-01-01 00:00 UTC"
timestamps.json | ts-032 | "<t:1><t:2>" | "12" -> "1970-01-01 00:00 UTC1970-01-01 00:00 UTC"
timestamps.json | ts-033 | "at <t:1700000000:t> sharp" | "at 1700000000 sharp" -> "at 2023-11-14 22:13 UTC sharp"
timestamps.json | ts-034 | "**<t:1700000000:R>**" | "1700000000" -> "2023-11-14 22:13 UTC"
timestamps.json | ts-037 | "> <t:1:R>" | "1\n" -> "1970-01-01 00:00 UTC\n"
timestamps.json | ts-038 | "- <t:1:R>" | "1\n" -> "1970-01-01 00:00 UTC\n"
timestamps.json | ts-039 | "# <t:1:R>" | "1\n" -> "1970-01-01 00:00 UTC\n"
timestamps.json | ts-040 | "[<t:1:R>](https://e.com)" | "1" -> "1970-01-01 00:00 UTC"
timestamps.json | ts-042 | "<t:1:R>\n<t:2:R>" | "1\n2" -> "1970-01-01 00:00 UTC\n1970-01-01 00:00 UTC"
timestamps.json | ts-043 | "<t:00001>" | "1" -> "1970-01-01 00:00 UTC"
```

Things to know about those values: (a) the code-block rows (`block-005`, `block-034`, `mixed-025`, ...) are an inline triple-backtick block in the middle of a line: it is a `CodeBlock` node, so by the rule it stands on its own line (`a \nx\n b`; the spaces next to the new lines are harmless because every consumer folds white space). (b) `ts-032` (`<t:1><t:2>`) now reads as two dates glued together, as two adjacent mentions are: inline nodes get no separator, by design. (c) No changed value is anything else.

## Who calls the wasm's `to_plain_text`
Nobody in `apps/web/src` or `packages/*/src`: only the wasm package itself exports it (`packages/wasm/src/index.js`, `index.d.ts`, from `crates/sr-markdown-wasm`). The web builds its one-line text from the AST (`singleLineText`), not from the wasm string. **For the commander:** `packages/wasm/test/corpus.test.ts` compares the wasm's `plain` with the corpus goldens, so it fails until the wasm is rebuilt at integration (the parser is unchanged, only `plain`). The web vitest files I ran use `testFallbackEngine`, or the old `dist` wasm for parsing only, which does not matter for them.

## The restore path and what a resync does (item 3)
- Server: `crates/sr-chat/src/memory/mod.rs:437-453` (`apply_restore`: `t.message_count += 1` at line 451 for the restored message's channel when it is a thread); Postgres twin `crates/sr-db/src/chat_repo.rs:1305-1325` (`UPDATE thread_metadata SET message_count = message_count + 1` at 1319, only when the row came back, `rows_affected() > 0`). The event: `crates/sr-events/src/event.rs:241-249` (`MessageRestored`), mapped to `MESSAGE_CREATE` with `restored: true` in `crates/sr-gateway/src/hub.rs:353-357`, in the thread's own channel. So the premise holds: a restore adds one to the stored count and the client gets the flag.
- Web: `threadStore.countMessages` now counts a `restored: true` create whatever its id (below `COUNT_CAP`), and moves `lastMessageId` only when the id is newer. An ordinary `MESSAGE_CREATE` replay (id not newer) still does not count.
- Resync: `GUILD_CREATE` and `THREAD_LIST_SYNC` go through `upsert` and `toThread`, which takes `message_count` from the wire (`w.message_count ?? prev`), so a resync reloads the counts of the threads it names. A replayed restored event that double counted would be healed by the next resync or load. I did not build a seen-ids set (it is not three lines and the heal exists). Owner question 2.

## Search index
`sr-search/src/chat.rs:94` is the only Rust caller. New test `a_list_is_indexed_apart_from_the_text_before_it` (`two\n- a` indexes `two` and `a`). Messages indexed before the change keep the old text (`twoa`) until `sr-cli reindex`: a sentence for ops below. A timestamp is now indexed as its UTC date.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p70-plain`: **24 PNGs, all opened and looked at**: `thread-loaded-*`, `thread-live-*`, `request-loaded-*`, `request-live-*`, each at desktop 1280x800 and phone 375x812 on Editorial, Lounge Light and Lounge Dark. Spec `e2e/tests/p70-plain-shots.spec.ts` (local only, `SR_P70_PLAIN=1`; real stack, two accounts; account B's latest message in a thread and in a request is `look ||` plus a sentence with "swordfish" four times plus `||`). What I saw: every thread chip reads `look…` (loaded from the server and arriving live; the spec asserts the chip text has no "swordfish", says "2 messages" and "just now"), every request row reads `look…` after "Show message" (a new account is filed as likely spam: that step is the existing rule and the spec clicks it), on the three themes and at phone width, nothing overflows. The spec took four runs because of my own selectors (the spam "Show message" step and its accessible name, and old requests from the failed runs in the same DB); the last run passed.
- Seen in `request-live-*` and not mine: the DM toast shows the raw text (see "Not done").
- I did not run the page sweep: nothing I changed is a page, only the text of two rows (the thread chip and the request row), which the spec shows on the pages they live on (the channel and Message Requests).

## Tests run, and which I watched fail without the change
Base runs were done with the old `util.rs` and `preview.rs` swapped in (and the web source files stashed for the web), the new tests kept. Each failed on the base for the reason named, and passes now.
- `sr-chat` `safe_cut_tests`: `a_cut_never_leaves_a_spoiler_open` (confirm a: letters leaked, no spoiler node), `a_cut_never_leaves_half_a_mention` (confirm c), `shared_cut_cases`, `cut_text_reads_as_the_start_of_the_whole` failed on base; `worst_case_time_is_small` is a timing print.
- `sr-api` `a_long_spoiler_never_reaches_the_thread_row` and `a_long_spoiler_never_reaches_the_request_preview`: failed on base with the hidden words in `content`. The thread one is in `routes/messages/tests.rs`, beside the existing `sr_last_message` tests and `seed_thread` (`routes/threads/tests.rs` has no thread-row test and no seeding for one). That file is mentions's: I added one test in the middle of the file, not at the end, so a merge should not collide.
- `sr-markdown`: `plain_text_keeps_blocks_apart`, `plain_text_blocks_with_a_resolver_across_the_edge`, `plain_text_timestamp_is_a_utc_date` failed on base; the whole `sr-markdown` suite (the corpus included) passes now.
- `sr-search`: `a_list_is_indexed_apart_from_the_text_before_it` failed on base.
- `sr-server`: `cargo test -p sr-server --lib -- push_notify line_breaks_and_lists_are_one_line` is green (not my crate; it stays green).
- Web `p70Plain.test.tsx` (52 tests; the shared cases run against the stand-in parser and against the real wasm parser from `packages/wasm/dist`): 30 failed on base (the cut cases, confirm (b) and (c), the blocks, the timestamp, "counts an old restored message"), 52 pass now. `p67Previews`, `p68Previews`, `p69Threads`, `p69Notify` and `notifyText.test.ts` are green, one by one, unchanged.
- `cargo clippy -j 4 -p sr-markdown -p sr-chat -p sr-api -p sr-search --all-targets -- -D warnings` clean. `cargo fmt` run on those crates (committed formatted). Web `tsc --noEmit`, eslint and prettier on my files, `i18n-audit --max 0` (TOTAL 0), the e2e package's `tsc` and `check-doc-links`: clean.
- Not run (left to CI): whole-crate suites, the route matrix, the bundle and entrance checks (I touched no first-paint file; see below).

## First paint
`grep -rn "SingleLine\|msg/excerpt\|previewLive\|threadStore\|notifyText" apps/web/src`: every importer is under `apps/web/src/lounge/` (forum, threads, shell, msg, `useShellBoot`, tests). The Lounge route is lazy (`routes/lounge.tsx`: `lazyNamed(() => import('./LoungeRoot'))`), and none of `main.tsx`, `router.tsx`, `store/**`, `gateway/**`, `pwa/**`, `i18n/**` imports them. `excerpt.ts` now imports `../markdown` and `./SingleLine`, both already Lounge code. No static import was added near first paint.

## Docs updated
`docs/05-api-and-gateway.md` (message request preview: the cut never splits markup, the text may be absent; the thread row's `sr_last_message.content`; the push body: blocks apart, timestamps as UTC dates) and `docs/specs/lounge-chat.md` (requests, the thread row with the restored count, notifications, the search paragraph with the indexed text and the reindex). Nothing outside my files is made wrong that I found.

## Proposed inventory rows
- SEC-AUD-242 (fixed, D-728): "A thread row's `sr_last_message` and a Message Request's `sr_request_preview` cut the raw message at 140 characters before it was parsed, so a spoiler cut in half (the parser reads an unclosed `||` as text) showed the hidden words, and a mention cut showed `<@1234`. `excerpt` (server) and `excerpt.ts` (web) keep a cut only when its plain text is the start of the whole's plain text, else move it back, else give no text. Tests: `preview.rs` `safe_cut_tests` and `preview_cases.json` (shared with `p70Plain.test.tsx`), the two route tests."
- SEC-AUD-243 (fixed, D-722): "`to_plain_text_with` and `singleLineText` put no separator between text and a following list, quote, heading or subtext (`two\n- a` was `twoa`, also in the search index). Blocks now start a line. Messages indexed before keep the old text until `sr-cli reindex`."
- SEC-AUD-244 (fixed, D-723): "A message a moderator restores in a thread did not count in the open app's thread count (the replay guard skipped its old id). It counts now, below the cap."
- SEC-AUD-245 (fixed, D-731): "A timestamp was written as its number in plain text (push, search index) and dropped from the web's one-line text. The server writes `YYYY-MM-DD HH:MM UTC`; the web writes the message renderer's own format."
- It closes the rows the plan names for these four items (previews that show what a message hides; the plain-text gaps; the restored count).

## Proposed threat-model rows
- The preview cut as a place a spoiler could leak: asset = text a sender hid behind `||`; threat = any preview that cuts a message before parsing it (thread row, request row, a future list) turns a cut spoiler into plain text; control = the cut is checked against the whole message's plain text (server `preview.rs`, web twin), with a shared case list and a corpus property test; residual = a new preview built on a raw slice instead of `excerpt`.
- The search index text: the index stores `to_plain_text` (spoilers as `[spoiler]`, unchanged); the change only adds separators and dates, so no new content enters the index; residual = old entries keep the old text until a reindex.

## Changelog and upgrade sentences (ops's files)
- `CHANGELOG.md` (members): "A thread's row and a Message Request show a cut message without ever showing what a spoiler hides; a list after a line of text reads as separate words; a timestamp in a notification or a search hit reads as a date; a message a moderator restores counts in its thread at once."
- `deploy/UPGRADE.md` (operators): "Optional: run `sr-cli reindex` once after upgrading so messages indexed earlier get the corrected search text (a list after a line of text was indexed as one word, a timestamp as its number); search works without it."

## Owner questions (to the Fable agent)
1. A cut that would split markup gives no text when nothing is left (a message that is one long spoiler): the row shows the attachment count or nothing. The Discord-like default was taken; should such a row say "spoiler" instead? (Needs a string.)
2. A replayed restore event would double count until the next resync or load; I built no seen-ids set. Fine?
3. The web's one-line text writes a timestamp in the viewer's zone and the node's own style (a relative `:R` one reads relative to the time the row was built, not live). OK, or always the date?
4. An inline triple-backtick block in the middle of a line now gets its own line in plain text (`a \nx\n b`). Fine?

## Cleanup done
Scratch databases `sr_p70_plain` (never created: my route tests are in memory) and `sr_p70_plain_ui` (dropped, checked), my sr-server and Vite (stopped by port, checked), the cargo target `p70-plain-target` and my binaries folder `p70-plain-bin` deleted, my server and Vite logs and `e2e/test-results` removed. The heavy lock is released.
