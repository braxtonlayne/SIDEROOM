# Wave 73 prose handoff (SEC-AUD-283..285; D-763, D-764, D-766)

Branch `fix/p73-prose`, base `1a0ae040` (integration/phase73). Web only: one stylesheet, one function, one new vitest file, a local shots spec, two doc sentences. No Rust, no migration, no route, no string (no new fragment), no `openapi.json` change, no `packages/**` change.

## Done (commits)
- `fad2146c` SEC-AUD-284: the "Thread started from" line is D-729's plain line (`SystemMessage.tsx`, `ThreadStarterPreview` only) and `apps/web/src/lounge/__tests__/p73Prose.test.tsx` (5 tests).
- `d5a8f9c0` SEC-AUD-283 and 285: `markdown.css` (headings, subtext, hovered spoiler), `e2e/tests/p73-prose-shots.spec.ts`, `docs/specs/lounge-chat.md` (the Content bullet and the thread paragraph).
- a small prettier-only commit on `SystemMessage.tsx`, and this handoff.

## Item 1: SEC-AUD-284, the thread starter line (D-764)
- **Confirm (watched fail on the base):** the vitest was written first and run against the unchanged `SystemMessage.tsx`: 3 of 5 failed. Today's line was `see ||the ending|| and \`code\` <@123>` as typed; a blocked author's text was shown; before the parser was ready the typed text was shown. The other two (plain sentence unchanged, attachment-only still says "Attachment") passed on the base on purpose: they pin behaviour that must not change.
- **What the fetch returns:** `GET /channels/{id}/messages/{id}` already returns `author {id, username, global_name}` and `content`; I only widened the local response type to read `author.id` (route and shape unchanged). The state now holds `{author, authorId, text}`.
- **What is passed to the plain-line function:** `<MessageLine content={original.text} authorId={original.authorId} guildId={guildId} />`, with `guildId` from the store (`channels[channelId].guild_id`) so role and channel names resolve in the right server. `MessageLine` is the component the reply line uses; it applies the block/ignore check, `storeResolvers`, the spoiler word and the 140 cut. `lineText.tsx` and `notifyText.ts` are untouched. `.slice(0, 200)` is gone. Empty text keeps `lounge.systemMessage.attachment` (decided on the raw text, so a not-yet-ready parser shows an empty quote, never "Attachment"). `threadGone`, `threadStarterFailed`, the name, the `<q>` and "Jump to original" are as before.
- **Result in the real stack** (all 6 thread shots): `"see [spoiler] and code @owner end"`; the assertion in the shots spec checks no `the ending`, no backtick, no `<@`.
- Tests (all pass): spoiler/backtick/mention; plain sentence; attachment; blocked author shows "1 blocked message" and nothing of the text; parser not ready gives an empty `<q>`.

## Item 2: SEC-AUD-283, headings (D-763)
- **Confirm (base, recorded with `SR_P73_ASSERT=0`, computed styles on a real stack, text 16px in the list, 14px in the pin):** h1 32px, h2 24px, h3 18.72px (2em / 1.5em / 1.17em), weight 700, line-height 44px, margin-block 21.44px above and below (0.67em); in the pin 28 / 21 / 16.38px. The strict spec was not run on the base; the recorded values are the base evidence.
- **After:** h1 24px, h2 20px, h3 16px (1.5em / 1.25em / 1em, as ratios of the text size, list, thread and pin: pin 21 / 17.5 / 14px), weight 700, line-height 1.375, margin-block 16px above and 8px below, 0 above for `.sr-md > .sr-md-heading:first-child`. The shots spec asserts the cap of every pin body (`clientHeight <= max-height`): holds (4 pins, three themes, both sizes). A 150-character heading with a 140-character unbroken word wraps in the list and in the pin: page `scrollWidth - clientWidth` = 0 on desktop and phone in all three themes.
- **Subtext:** base drew `<small>` inline, so a line before it ran on ("a sentence betweena subtext line, small", seen in the base shot) and it was 13.33px in the text colour. Now `display: block`, 0.75em (12px), `--text-muted`. This is a real fault found by looking, not in the plan; one rule.
- **Lists:** looked at next to Discord's (markers inside the column, indent, ordered list numbered): fine, no rule. (A `1.` line directly under a `-` list joins that list: parser behaviour, not drawn wrong.)
- **Known limit (owner question 1):** `:first-child` only sees elements. The renderer emits typed text as bare text nodes, so a heading that follows some typed text in the same message (`before\n# Heading`) also matches and sits close under that text (shots `list-mixed-*`). A heading that opens the message (the common case) is right. Fix would be wrapping the text in `packages/markdown-react`, not mine.

## Item 3: SEC-AUD-285, the surfaces not looked at (D-766)
| surface | state |
|---|---|
| `sr-md-command` (`</ping:ID>`) | shot, fine: brand chip, three themes, phone |
| `sr-md-guild-nav` (`<ID:customize>`, `:browse`, `:guide`, `:linked-roles`; the id is numeric first, `<id:customize>` is not parsed and prints as typed) | shot, fine: four chips |
| `sr-md-cinema`, `sr-md-library` | shot with the two lookups faked by `page.route` (no title or series exists on a local stack): real cards (`msg.css`, not `markdown.css`) look right in all themes, desktop and phone; a missing id draws the `Cinema title` / `Library series` chip, fine. Card rules are `lng-rich*` in `msg.css`, outside this task |
| Settings Profile bio preview | shot **entered from the Lounge** (parser loaded): chip/code/spoiler backdrops resolve in Settings, none transparent or black, three themes, desktop and phone; the bio flattens mentions, quote and heading by design (`profileSubset`). **Reported, not fixed (not mine):** loaded directly (`/settings/user/profile` as the first page) the preview and the card show the bio as typed (`**bold**`, `\|\|spoiler\|\|`, `<@id>`), because nothing in Settings starts the parser (`apps/web/src/lounge/profile/profileMarkdown.tsx:72` `ProfileBio` only calls `useMarkdownReady`, which waits; the engine is loaded by the Lounge shell). Shots `settings-profile-direct-*`. Fix: `ProfileBio` should call `loadMarkdownEngine()`, in a file outside my list |
| Profile card (popout) bio | shot, fine, three themes, desktop and phone |
| public `/~handle` | read `apps/web/src/public/profile/PublicProfile.tsx`: draws no bio and no markdown (grep for bio/markdown/about: nothing). Nothing shot, nothing styled, no import |
| hover-reveal spoiler preference | set `spoilers: 'mine'`, hover a closed spoiler in the owner's own message: text shows (opacity 1) but the cover stayed under it: text contrast **2.18 / 2.19 / 1.95** (Editorial / Light / Dark), the base value (measured as the same cover with the same text). **Fixed** in `markdown.css`: a hovered closed spoiler in a `data-spoilers='hover'` message takes the revealed backdrop: **7.79 / 8.19 / 6.13**. Click-to-reveal after hover still works (`data-revealed` true). The rule names the Lounge's own `data-spoilers` attribute, which is set only by `MessageItem` |
| `@everyone` / `@here` / viewer mention | chips alike, as decided. The viewer's row IS highlighted: `data-mention`, a 10% tinted background and a 2px inset bar in the accent (recorded per theme) |

## Visual check
- Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p73-prose/after`: **84 PNGs** (list headings x6, mixed x6, long heading x6, rich x6, cards x6, viewer mention x6, profile card x6, pins x6, pins scrolled x6, thread starter x6, thread headings x6, Settings profile x6 + direct x3, spoiler hover before/open/click x9), three themes, desktop 1280x800 and phone 375x812. `base` has 20 PNGs (Editorial only; the run stopped at the thread step on a spec error: a hidden first match). I opened every one of the 84 after-shots. One honest note: the last full run only added the nine spoiler shots (hover rule) and the earlier full run produced the other 75; the 75 were opened from the run before the spoiler rule, and CSS in between touched only a hovered spoiler. The 9 spoiler PNGs now in the folder are from the final run and I opened them. Of the base shots I opened 2 (`list-mixed-desktop`, `list-longheading-phone`); the others were read through the recorded computed styles.
- Clicked in the real stack: the thread line (three themes, both sizes), a mention chip opens the profile card, hover and click on a spoiler, the pins panel and its scroll, Settings entered from the Lounge.
- Spec `e2e/tests/p73-prose-shots.spec.ts` (local only, `SR_P73_PROSE=1`): 1 test, passed on the final stack. `SR_P73_ASSERT=0` records without asserting (base).

## Checks
- Vitest: `p73Prose.test.tsx` 5 pass (3 failed on the base), `p70Plain.test.tsx` 64, `p71Panels.test.tsx` 17, `p54SilentFailures.test.tsx` 13 (it renders `SystemMessage`): all pass. Web typecheck clean; eslint clean on the two files; prettier clean on all four; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok.
- Web build inside the lock: `check-bundle-budget` ok (31 ok lines), `check-entrance-isolation` "every public surface is clean", `check-staff-delivery` ok.
- **First-paint figure:** `app/app-DDihmDQE.js` 215,858 bytes gzip, room 40,142 bytes. Wave 72's was 215,870 (status handoff) and 215,850 (wave 72 prose): within a name's few bytes. I did not build the base separately. No static import was added (`SystemMessage.tsx` imports `./lineText`, already imported by `MessageItem`, in the Lounge chunk).

## Not done
- A rule for the text-before-heading case (needs the renderer, see Item 2).
- The Settings direct-load bio (reported above).

## Proposed inventory rows
- SEC-AUD-283: headings in a message are 1.5em / 1.25em / 1em, bold, 16px above and 8px below, none above a first heading, in every Lounge markdown surface; subtext is small, muted and on its own line (D-763). Closed. Proof: `p73-prose-shots.spec.ts`, base values recorded (32/24/18.72px), 84 shots. Open follow-up: a heading after typed text is close under it (renderer wraps no text).
- SEC-AUD-284: the "Thread started from" line shows the starter as D-729's plain line; a spoiler, code and mention are no longer printed as typed, a blocked or ignored author's text is not shown, and before the parser is ready the quote is empty (D-764). Closed. Proof: `p73Prose.test.tsx`.
- SEC-AUD-285: surfaces not looked at: rich-card mentions, Settings bio preview, public `/~handle` (draws no bio), hover-reveal spoiler (contrast 1.95-2.19 raised to 6.13-8.19), viewer mention row (highlighted). Closed in the Lounge. Open: Settings profile preview shows the bio as typed when the page is the first one loaded (`profileMarkdown.tsx:72`).

## Proposed threat-model row (T79 family)
The "Thread started from" system line drew the starter's text as typed: a spoiler in clear and a blocked or ignored author's text. Now D-729's plain line (spoiler word, block/ignore wording, empty before the parser is ready). Test: `p73Prose.test.tsx`.

## Changelog sentences (for status)
- The "Thread started from" line now shows a plain one-line version of the original message: a spoiler stays hidden, code and mentions are written as names, and a blocked or ignored author's text is not shown.
- Headings in messages are now sized as Discord's (smaller than before), and subtext sits on its own line in a muted colour.
- A spoiler that the "On hover for mine" preference shows is now readable (its text no longer sits on the dark cover).

## Owner questions
1. A heading after typed text in the same message (`intro\n# Title`) sits close under the text, because CSS cannot see the renderer's bare text nodes. Accept, or have the renderer (packages/markdown-react) wrap text? Default: accept.
2. The Settings Profile preview and profile card show the bio as typed when Settings is opened as the first page (parser not loaded). Make `ProfileBio` start the parser? Default: yes, a one-line change outside my files.
3. The thread row's latest-message line in the channel list prints `Heading one ## Heading two ###` for a message of headings (`lounge/threads/ThreadChip.tsx` or the server's preview); seen in the shots, not mine, not investigated. Look at it next wave?

## Clean-up
Stack stopped by PID (sr-server 28516, Vite node 12024), lock released. Scratch database `sr_p73_prose_ui` dropped, `p73-prose-bin` deleted (no cargo target was built). Scratch files are in `C:/Users/braxt/Desktop/SIDEROOM_wt/p73-prose-scratch` (untracked). No stash touched.
