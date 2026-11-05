# Follow-up wave 4 decisions (Fable, owner-delegated, 2026-11-05), for the follow-up integrator

Scope: the four owner questions in `docs/dev/handoff/fu-r2.md` (the defaults the r2 builder chose, merged to `main` by PR #86, `d7cf7380`). Rules applied: `docs/01-decisions-log.md` wins where it already decides (FU-1 to FU-24 included; FU-22 fixes the house-notes subset as "the `EntranceMarkdown` subset"); Discord parity by default where the log is silent; D-385 (the house-notes page stays script-free; everything below is static markup from `sr-server`); no new dependency (every change below is `std`/browser built-ins). Code read at `main` (`d7cf7380`), nothing edited.

Key findings that change the picture:
- Three renderers now draw operator copy and they do not agree with each other. `crates/sr-server/src/house_notes_md.rs` (served `/house-notes`) and its mirror `apps/web/vite/houseNotesMd.ts` (dev server) agree byte for byte through the shared fixtures. The in-app `EntranceMarkdown` (`apps/web/src/routes/entrance/entranceCopy.tsx`) draws the two sibling keys and, through `apps/web/src/admin/sections/Entrance.tsx`, the **preview staff see while editing `house-notes`**. It differs from the served page in four places: paragraph lines join with a space (served: `<br>`), heading ids are Unicode and carry no `op-` prefix and no dedupe (served: ASCII, `op-`, `-2`), `rel` is `noopener noreferrer nofollow` (served: no `nofollow`), and nothing else. So a staff member who types "Ünï" as a heading sees an anchor in the preview and gets `id="op-n"` on the page; one who presses Enter inside a paragraph sees one line in the preview and two on the page.
- The Lounge's Discord-parity renderer (`crates/sr-markdown/src/block.rs`) knows headings `#`..`###` only (four `#` is text), `-#` subtext, `> ` quotes line by line (each line keeps its break), and nested lists to depth 8 by indent. Discord's own rule for newlines is that every newline is a line break. Discord has no image syntax and masks only `http(s)` links.
- Rust `std` has Unicode case mapping and `char::is_alphanumeric`; no crate is needed to make ids Unicode-aware. `safe_href` already accepts Unicode letters in a `#anchor` (`c.is_alphanumeric()`), so today a same-page link `[up](#op-ünï)` is allowed while no heading can produce that id.
- The page carries `<meta name="robots" content="noindex">` (`scripts/render-legal.mjs`), so `nofollow` is about consistency with the sibling keys, not about ranking.
- `GET /admin/arcade/candidates` (`crates/sr-activities/src/arcade.rs`, `candidates`) lists every live application with a reviewed bundle that declares a tier, featured or not, ordered by name, at most 200; the release route accepts any `application_id` from that same list (`crates/sr-api/src/routes/arcade.rs`). The Coming Soon card has its own `tier`, which the route does not compare with the candidate's.

Format per question: id, the decision, why, and for a change what the implementing agent does.

## 1. Heading levels and ids (fu-r2 question 1)

### H3: `#`..`####` give h2, h3, h4, h4; five or more is text
**Keep.** FU-22 fixes the subset as `EntranceMarkdown`'s, and that renderer accepts `#{1,4}` and clamps to `h4` (`Math.min(4, Math.max(2, len))`); the preview staff see does the same, so the served page must too. Discord stops at three marks, but the log already decided this subset over Discord's, and a fourth mark becoming visible `#### ` text on the page while the preview shows a heading would be the worse outcome. The page owns its one `h1`, so `#` starting at `h2` stays (the built-in sections are `h2` too, so an operator `#` heading ranks with "The house rules", which is right: the operator's notes are a section of the page, not a second page).

### H4: heading ids keep Unicode letters and digits
**Change: the slug keeps every Unicode letter and digit, lower-cased, and turns runs of everything else into one hyphen; the `op-` prefix, trimming and `-2`/`-3` dedupe stay (S, Rust and the TS mirror, no dependency).** A heading written in any language but English gets no anchor today, or a nonsense one (`## Ünï` gives `id="op-n"`, the `n` being the one ASCII letter), while the in-app renderer's `defaultHeadingId` keeps `\p{L}\p{N}`, so the preview promises an anchor the page does not have. HTML ids may hold any character but whitespace, `safe_href` already lets a `#` link carry Unicode, and the hint "Headings get anchors such as #op-privacy" says nothing about ASCII. Rust `char::is_alphanumeric()` and `char::to_lowercase()` are `std`; JS has `/[\p{L}\p{N}]/u` and `toLowerCase()`. Case mapping is done **per character** in both (Rust `c.to_lowercase()` on each `char`, JS `ch.toLowerCase()` on each code point from `Array.from`) so neither applies a context rule (Greek final sigma) the other does not; the fixture proves the common case. A heading with no letter or digit at all (`# ---`) still gets no id.

Implementation:
- `crates/sr-server/src/house_notes_md.rs` `slug`: `if c.is_alphanumeric() { .. out.extend(c.to_lowercase()) } else { dash = true }`; doc comment becomes "Unicode letters and digits lower-cased per character, runs of other characters become one dash, trimmed."
- `apps/web/vite/houseNotesMd.ts` `slug`: `if (/[\p{L}\p{N}]/u.test(ch)) { .. out += ch.toLowerCase(); }` (iterating `for (const ch of text)` already yields code points).
- Fixtures `crates/sr-server/tests/fixtures/house-notes/headings.{md,html}`: `## Ünï` now expects `<h3 id="op-ünï">Ünï</h3>`; add `# ---` expecting `<h2>---</h2>` (no id) and `# Café 2 go` expecting `<h2 id="op-café-2-go">Café 2 go</h2>`. The Rust unit test `heading_ids_are_prefixed_and_deduped` keeps its `#### Ünï` line and asserts `id="op-ünï"`.
- `crates/sr-server/tests/web_gate.rs` `the_house_notes_copy_is_rendered_as_restricted_markdown`: one more heading in the copy, `### Über uns`, asserting `id="op-über-uns"` in the body.

## 2. Line breaks, blockquotes, nested lists (fu-r2 question 2)

### H5: a newline inside a paragraph is `<br>`
**Keep, and make the blockquote follow the same rule (see H6) and the editor preview show it (see H7).** fu3 H1 fixed "a single newline inside a paragraph is a `<br>` as today", Discord treats every newline as a line break, and the page is a legal-style text where staff write addresses, opening hours and short rule lines one per line on purpose. The in-app `EntranceMarkdown` joins such lines with a space because its other sources (`CHANGELOG.md` for What's new, `deploy/RUNBOOK.md`) are hard-wrapped markdown files; that is right for them and not for a textarea.

### H6: blockquote lines join with `<br>`, not a space
**Change: consecutive `>` lines join with `<br>` (S, Rust and the TS mirror).** One rule for the page: inside a block a newline is a line break, a blank line ends the block. Today a quote is the one block where the author's Enter is thrown away, which contradicts H5 next to it and Discord's `> ` behaviour (each quoted line keeps its break). Quotes are short in this copy (an address, a motto), where the line shape is the point.

Implementation:
- `crates/sr-server/src/house_notes_md.rs` `flush_quote`: render each line through `inline` and join with `<br>` (same shape as `flush_para`): `let lines: Vec<String> = self.quote.drain(..).map(|l| inline(&l)).collect(); ... format!("<blockquote><p>{}</p></blockquote>\n", lines.join("<br>"))`.
- `apps/web/vite/houseNotesMd.ts` `flushQuote`: `quote.map(inline).join('<br>')`.
- Fixture `basic.html`: the quote line becomes `<blockquote><p>Quiet hours run from 22:00 to 08:00.<br>Staff are asleep.</p></blockquote>`.

### H8: lists stay one level; an indented `- x` is a flat item
**Keep.** FU-22's subset is `EntranceMarkdown`'s, which is flat, and the preview is flat; a served page that nests while the preview does not would be a new mismatch of the kind H7 removes. Nothing is lost but the indent: the item's text is kept, in order. Discord nests, and if staff ask for it the change is to all three renderers at once (the Rust renderer, the TS mirror and `EntranceMarkdown`, with `sr-markdown`'s `build_list` as the model: indent deeper than the item above opens a child list); it is listed below as not scheduled, not owner-only.

### H7: the Admin preview of `house-notes` draws what the page will serve
**Change: `EntranceMarkdown` gains a `hardBreaks` prop; the `house-notes` block in Admin > Entrance passes `hardBreaks` and an `op-` heading-id function (S, web only, no dependency).** The preview is the one place staff learn what their markdown does, and today it is `EntranceMarkdown` with its defaults, which differ from the served page in exactly the places H4 to H6 touch (spaces for newlines, un-prefixed Unicode ids with no dedupe). The `headingId` prop already exists (the runbook uses it); hard breaks are a one-flag change. The sibling keys keep the defaults (their pages are drawn by `EntranceMarkdown` itself, so preview and page agree already). `EntranceMarkdown` is in the first-paint core of the public build; a boolean prop and one `<br />` branch are a few bytes, and `check-bundle-budget` stays in the gate.

Implementation:
- `apps/web/src/routes/entrance/entranceCopy.tsx`: `EntranceMarkdown({ source, headingId = defaultHeadingId, hardBreaks = false })`. In `flushPara` and `flushQuote`, when `hardBreaks`, map each line through `inline` and interleave `<br key=.. />` elements instead of `inline(lines.join(' '))`. Nothing else changes.
- `apps/web/src/admin/sections/Entrance.tsx` `CopyBlock`: for `block.key === 'house-notes'`, render `<EntranceMarkdown source={text} hardBreaks headingId={houseNotesId} />` where `houseNotesId` is built per render: `const seen = new Set<string>(); (t) => { const s = slug(t); if (!s) return ''; let id = 'op-' + s; for (let n = 2; seen.has(id); n++) id = 'op-' + s + '-' + n; seen.add(id); return id; }` with `slug` the same Unicode rule as H4 (one small exported helper in `entranceCopy.tsx`, `houseNotesHeadingId`, so the rule lives beside `defaultHeadingId`; keep it a plain function, no new module).
- Vitest `apps/web/src/routes/entrance/entranceCopy.test.tsx`: with `hardBreaks`, `"one\ntwo"` renders one `<p>` holding a `<br>`; without, it renders `one two`. Admin test (`apps/web/src/admin/__tests__/p10.test.tsx` or the Entrance section test that exists): the house-notes preview of `# Privacy\n# Privacy` shows ids `op-privacy` and `op-privacy-2`.
- `apps/web/src/i18n/messages/en/p13-web-ui2-2.ts` `admin.entrance.bytes` stays; `admin.entrance.houseNotesHint` gains one sentence: "A line break inside a paragraph stays a line break." (`i18n-audit` stays at 0.)

## 3. Images, `javascript:` links, `rel` (fu-r2 question 3)

### H9: `![alt](url)` stays literal text
**Keep.** FU-22 says images stay escaped text, and fu3 H1 said the page loads no image (D-385 spirit: the public page fetches nothing but its stylesheet and fonts). Showing the mark as typed, rather than the alt text alone, tells the staff member who wrote it that images are not a thing here; the preview does the same (`EntranceMarkdown` has no image rule either).

### H10: `[x](javascript:...)` shows the link text only
**Keep.** It is exactly what `EntranceMarkdown` does (`return link[1]`), so the preview and the page agree; the reader sees the words and no href, and nothing unsafe reaches the markup. Showing the whole literal instead would print `javascript:alert(1)` on a public page for no gain. The same rule covers `data:`, `//host` and any other refused scheme.

### H11: links carry `rel="noopener noreferrer nofollow"`
**Change: add `nofollow`, matching the in-app renderer (S, Rust and the TS mirror).** The same staff copy, drawn by the sibling renderer on the sibling pages, carries `nofollow`; one key out of three without it is the kind of unchosen inconsistency FU-22 removed. The page is `noindex`, so a crawler is unlikely to score the links either way, but an outbound link in staff-typed copy is user-supplied content from the platform's point of view, and `nofollow` is the standard mark for it. No `target` (fu3 H1 stands: the page is one document, a new tab is the reader's choice).

Implementation:
- `crates/sr-server/src/house_notes_md.rs` `construct` (`'['` arm): `rel=\"noopener noreferrer nofollow\"`; the unit test `only_safe_links_become_anchors` asserts the new value.
- `apps/web/vite/houseNotesMd.ts` `construct`: same string.
- Fixture `links.html`: the four anchors gain ` nofollow`.
- `crates/sr-server/tests/web_gate.rs` `the_house_notes_copy_is_rendered_as_restricted_markdown`: the asserted anchor string gains ` nofollow`.

## 4. The "Released as" picker (fu-r2 question 4)

### R3: the picker lists every candidate, featured or not
**Keep.** FU-20 says `application_id` only names the Activity the members will find and never features it; the candidate list is exactly "released Activities with a declared tier", which is the set a member can reach at `/games` whether or not the Operator has put one on the Front Page. Filtering to featured ones would hide the right answer whenever the Operator releases first and features after (the hint tells them to feature first, but the picker should not punish the other order); filtering by the card's tier would hide it whenever the tier changed between the promise and the release. Ordering by name and the 200 cap are the list's own and fine for an Arcade. No code.

## (a) Implementation list, by user impact

| # | Item | Size | Rust | Migration |
|---|---|---|---|---|
| 1 | H7 the Admin preview of `house-notes` draws what the page serves (`hardBreaks`, `op-` Unicode ids with dedupe) | S | no (web) | no |
| 2 | H4 heading ids keep Unicode letters and digits (`std` case mapping per character; TS mirror; fixtures) | S | yes + TS mirror | no |
| 3 | H6 blockquote lines join with `<br>` like paragraph lines (fixture) | S | yes + TS mirror | no |
| 4 | H11 links gain `nofollow` (fixtures, web_gate assertion) | S | yes + TS mirror | no |

Items 2 to 4 are one small change set to `house_notes_md.rs`, `houseNotesMd.ts` and the three fixtures; land them in one commit so the fixture test (`fixtures_match_the_expected_html` and the Vitest "agrees with sr-server on the shared fixture") is green at every step. Item 1 is web only and can ride the same PR. Everything else keeps its default and needs no code. Steps for the integrator (no decision): re-shoot `pub-house-notes` desktop and phone with a copy that has a non-ASCII heading, a two-line quote and a link (the dev mirror is enough for the look; the served path is proven by `web_gate`), and one shot of the Admin > Entrance preview beside it for the same copy; `check-entrance-isolation`, `check-bundle-budget`, `i18n-audit` (0) and `mocks:check` (nothing in OpenAPI changes) in the gate as usual.

## (b) Owner-only

- Nothing new. Unchanged from `fu3-fable-decisions.md` section (b): `sha2` in `sr-core` or `[workspace.dependencies]`, a markdown crate (`pulldown-cmark`, `comrak`) if the in-house renderer is ever unwanted, and the inherited `fu2`/`fu` lists (analogue arena3d movement, binary roster v2, the pinned `pcsx_rearmed` HLE start, `sr_p42_arcade_room`, the `lofty` bump, KTX2/Basis, a self-built Jolt, hls.js, `platform.public_profiles`, the ambience mixer, the node agent notice and the CurseForge EULA, sections 4a and 5 of the owner checklist).

Not owner-only, not scheduled: nested lists in operator copy (H8), to be added to the Rust renderer, the TS mirror and `EntranceMarkdown` together, modelled on `sr-markdown`'s `build_list`, only if staff ask; a server-side Ogg remux of WebM voice messages (FU-24) stays as before.

## (c) Decisions-log rows

Append under `## Follow-up (2026-11-02, owner-delegated)` after FU-24, same 3-column header.

| ID | Decision | Status |
|---|---|---|
| FU-25 | **House-notes headings: `#` to `####` render `h2`, `h3`, `h4`, `h4` (the page owns its `h1`; a fourth mark clamps, five or more is text), as `EntranceMarkdown` does; heading ids are `op-` plus the heading's Unicode letters and digits lower-cased per character with runs of other characters as one hyphen, deduped `-2`, `-3`, so a heading in any language gets an anchor; a heading with no letter or digit gets none.** The ASCII-only slug gave `## Ünï` the id `op-n` while the editor preview promised a real anchor. (fu-r2 question 1) | Adopted (owner-delegated, Fable) |
| FU-26 | **In house-notes copy a newline inside a paragraph or a blockquote is a `<br>` and a blank line ends the block (Discord: every newline is a line break); lists stay one level, an indented item is a flat item; the Admin > Entrance preview of `house-notes` renders with hard breaks and the same `op-` ids, through `EntranceMarkdown`'s `hardBreaks` and `headingId` props, so staff see what the page will serve. The sibling keys and the runbook/What's new keep the soft-wrap default.** (fu-r2 question 2) | Adopted (owner-delegated, Fable) |
| FU-27 | **In house-notes copy an image mark stays the literal text as typed, a link with a refused scheme (`javascript:`, `data:`, `//host`, anything but `http(s)://`, `/path`, `#anchor`) shows its link text only, and every rendered link carries `rel="noopener noreferrer nofollow"` and no `target`, the same `rel` the in-app renderer gives the sibling keys.** (fu-r2 question 3) | Adopted (owner-delegated, Fable) |
| FU-28 | **The "Released as" picker on a Coming Soon entry lists every candidate the Admin page fetches (released Activities with a declared tier), featured or not, in name order, with "No Activity (keep the card's title)" first; it is not filtered by the card's tier or by featuring.** FU-20: `application_id` names the game, it never features it, and an unfeatured Activity is still in the Arcade. (fu-r2 question 4) | Adopted (owner-delegated, Fable) |
