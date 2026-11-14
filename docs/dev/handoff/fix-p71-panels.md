# Wave 71 panels handoff (SEC-AUD-262..265; D-737, D-739, D-740, D-741)

Branch `fix/p71-panels`, web only. No Rust, no migration (3070-3079 unused), no `openapi.json` change.

## Done (commits)
- `07e32822` SEC-AUD-264: a relative timestamp in a one-line text is written as the short date and time (`msg/SingleLine.tsx`, the `timestamp` case only).
- `7396e7bd` SEC-AUD-262 and 263: pins drawn as messages, search lines never typed text, search on a phone (`PinsButton.tsx`, `SearchBox.tsx`, `searchContext.tsx`, new `header/PhoneSearch.tsx`, `ChannelHeader.tsx` (the phone search button only), `lounge.css`, `header/header.css`, fragment `en/p71-panels-1.ts`, `__tests__/p71Panels.test.tsx`, `docs/specs/lounge-chat.md`).
- a last commit: the pin height cap, the pin's own markdown styles, the 320px header rule, the shots spec and this handoff.
- SEC-AUD-265: **no change** (see item 4).

## Items

### 262 pins as messages, no typed text in search (D-739)
- Confirm tests (all in `p71Panels.test.tsx`, kept): the pins panel with `psst ||the ending|| <@123> **bold**` (a), a hit without `sr_snippet` (b), the lines either side of a hit (c), the parser not loaded. **Watched fail on the base**: I stashed the five source files and ran the file: 13 of 17 failed (pins: spoiler/mention/bold/code block/parser-not-loaded/blocked-with-Show; search: no-snippet, context lines, parser-not-loaded; phone: all five; timestamp: `R`). The four that passed on base are the ones that must not change (attachment and no-text fall-backs, the server snippet kept, styles other than `R`, notification text). With the change: 17 of 17.
- (d) What the server's snippet showed beside a spoiler, in the real stack: for `before the needlehaystack word comes ||a secret spoiler|| after it` the hit reads `before the needlehaystack word comes [spoiler] after it`. A mention in a snippet reads `@user` (the index's plain text, not the person's name: the line under the hit, which the web writes, says `@owner`). So no server fault: the snippet hides the spoiler. The `@user` wording is the documented index text.
- Renderer: `PinText` uses `Markdown` from `@sideroom/markdown-react` with `parseMarkdown`/`isJumbo` (the same parser, the same node components) and resolvers = `storeResolvers(guild)` plus `richCardResolvers` and the `CustomEmoji` resolver, i.e. what `MessageList` builds minus bridged-member colours and role colours. It honours the "always reveal spoilers" preference. It does not use `MessageItem` (that pulls in the whole item). Left out: embeds, stickers, reactions, edit marker, forwarded frame, components, attachments (the one-line "Attachment" fall-back stays), suspicious-link rewrite, bridged-member names and role colours on mentions.
- **A finding the plan did not have: `packages/markdown-react/src/markdown.css` is imported nowhere in the app** (`grep -rn markdown.css` finds only the package's own `exports` entry). In the dev stack a spoiler in the **message list** has `opacity: 1` and no background (measured with `getComputedStyle` on a posted `||hidden words||`), so it is not visually hidden there either. I did not touch the message list (not mine). For the pins panel I put the few rules it needs in `header/header.css` (hidden spoiler, revealed spoiler, mention chip, code, quote bar), so a pin's spoiler is hidden until clicked in all three themes (shots). **The commander should decide how the list gets the same**: the simplest fix is one `import '@sideroom/markdown-react/markdown.css'` in a lazy Lounge file (e.g. `MessageList.tsx`), then my pin rules can shrink. I did not build the production bundle, so I cannot say whether production differs from the dev stack; the sources import it nowhere.
- Long pins: the 1,500-character pin filled the whole 460px panel (shot), so a pin's body is capped at 12 lines (`max-block-size: 16.8em`) with a fade, applied only when the text really overflows (`data-capped`, measured with a `ResizeObserver`). The Jump button opens it whole. The panel still scrolls. The cap is mine, as the task allowed.
- The hidden-author branch of wave 70 is unchanged (test: "1 blocked message", then Show message draws the pin).
- Search: the fall-back at `SearchBox.tsx` and the context lines (`searchContext.tsx`) go through `messageLineText`; an attachment-only neighbour keeps its own words (a `plain` flag, so `neighborsOf`'s old test shape is unchanged).
- Docs: `docs/specs/lounge-chat.md` (one new paragraph in 6.1 after the wave 70 bullet; the wave 70 bullet itself did not say the pins panel showed typed text, so nothing was contradicted).

### 263 search on a phone (D-740)
- Confirm: the header at phone width had no control for search (the inline box is `display: none` under `[data-mobile]`; the unit test renders the header with and without `onBack`, which the shell gives only at phone width). In the stack at 375x812 there was no other way (no channel-list or menu entry; the keyboard shortcut needs a keyboard). Not run against a base stack, only read: I did not start a base stack.
- Built: `PhoneSearch` (new file, lazy with the header): an icon button after the title, named from `p71-panels-1.ts`; it opens the existing `SearchBox` with a new `sheet` prop in a sheet that is portalled into the chat pane (`.lng-main`, the positioned pane at phone width), so the tab bar stays and the swipe between panes works after it closes (asserted: swipe to the list after closing). Box focused on open, filters and chips all there, results scroll inside, choosing a hit closes the sheet and jumps (`navigate` with `replace`, see below).
- How it closes: its back-arrow button; Escape (the box and a document listener); the back gesture (one history entry is pushed with the current state while it is open; `popstate` closes it, and a close by button or Escape takes the entry off with `history.back()`; a jump replaces it). Moving to another channel closes it. Focus returns to the button. The shots spec asserts, in the real router: Escape keeps the URL, `page.goBack()` closes the sheet and stays in the channel, the close button keeps the URL, a hit changes the URL to the message.
- Touch target: the header row is crowded (ten controls at 375px), so a 44px button would push the channel title out. The button keeps the 28px footprint of its neighbours and gets a 44x44 target from a `::after` (measured: 44x44 in the browser). It reaches 4px into the gap and 4px over each neighbour. The sheet's back button, chips and paging links are 44px. **Owner question 1.**
- At 320px wide the header had no room for the channel icon plus ten buttons (the last button was cut off). Under 340px the decorative channel icon is hidden (`lounge.css`, last rule); then all ten fit and the title is gone (it is gone at 320 on any header with that many buttons; at 375 it shows "g."). Not compared with the base at 320 (no base stack); I only saw the clipped button with my change before the rule.
- A direct message header on a phone: the button sits with the call buttons, outside the "More" strip (shots `dm-*`).
- A blocked author's hit uses the same rows (wave 70's wording; in the shots one result is davey's).
- Docs: `docs/specs/lounge-chat.md` section 9. No Handbook or help page describes search (grep).

### 264 relative timestamp (D-737)
- Confirm: `singleLineText` with `<t:1700000000:R>` gave the relative phrase on the base (test "writes the relative style as the short date and time" failed on the base, passed after). `t`, `D`, `F` and the default are unchanged (test). `notificationText` follows (test), the reply line follows (same function). `notifyText.test.ts` (13) passes.
- Docs: the one-line text sentence is in the new 6.1 paragraph. No shots (one line of text).

### 265 Editorial's online colour (D-741): no change
- Where: Editorial is `loungeEditorial` in `packages/ui/src/tokens/themes.ts` (`--ui-status-online: #8c9a6a` on `--ui-bg: #23221d`); the glyph (`Avatar.module.css:43-45`) is drawn in the status colour on `--ui-bg`.
- Contrast (WCAG ratio, computed): Editorial: online 5.27, idle 7.51, do-not-disturb 3.16, offline 3.03 (on `--ui-bg`; on the member list's `--bg-base-lower` 5.70, 8.13, 3.41, 3.27). Lounge Dark (`#313338`): online 3.97, idle 6.69, dnd 3.35, offline 3.38. Lounge Light (`#ffffff`; idle is overridden to `#b07800`): online 3.18, idle 3.80, dnd 3.77, offline 3.74. All pairs are 3:1 or more, and `packages/ui/src/theme/tokens-contrast.test.ts` already asserts it for online, idle and dnd in every theme. So the rule says change nothing, and I changed nothing. Nothing is under 3:1 on `--ui-bg` (Light's online on `--bg-base-lower` is 2.87, but the status sits on `--ui-bg`).
- What makes it look faint: not luminance but chroma and size. `#8c9a6a` is a desaturated olive, close to the warm brown surface in hue and saturation, and the glyph is a 1px outline about 13px wide; Lounge's `#23a55a` is a vivid green. The close-ups (`status-*`, `glyph-closeup2-*`) show Editorial's glyph and dot readable but muted next to Light and Dark, the same as wave 70. A change would be a colour decision (a more saturated Editorial green, or a filled glyph), not a contrast fix. **Owner question 2.**
- Shots: the p69 test "the phone-width member list with the smartphone glyph, and close-ups at scale 2" ran unedited into `p71-panels` (6 shots), and my spec's Editorial test took the desktop member list with online, idle and phone-only members in all three themes (3 lists plus 9 close-ups). Compared with wave 70's by eye: no change (nothing was changed).
- `check-entrance-isolation` not run: no CSS of item 4 exists, and nothing I changed is loaded by a public page.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p71-panels`, 76 PNGs, **all 76 opened and looked at** (not contact sheets): pins before/after clicking the spoiler/scrolled, desktop and phone x 3 themes (18); search results on desktop with a spoiler beside the word and a blocked author's hit (3); phone header with the button at 375 and 320 (6); the open sheet empty (6), with results (6), with a filter chip (3), after the jump (3), swiped to the list after (1); direct-message header and sheet at 375 and 320 (12); member list desktop in the three themes (3); status close-ups (9); glyph list and close-ups (6). Specs: `e2e/tests/p71-panels-shots.spec.ts` (local only: `SR_P71_PANELS=1`, `SR_P71_PANELS_OUT`), plus p69's glyph test unedited. Chromium only (the other projects do not launch on this machine). What I saw and fixed on the way: the pin cap; the pins' hidden spoiler (the missing stylesheet); the sheet's search box overflowing at 375 in one theme before `min-inline-size: 0`; the 44px button squeezing the title; the 320px header. What I saw and left: the header title is gone at 320 and nearly gone at 375 (crowding that was there before; the title had about 40px at 375 in wave 70's shot).
Clicked in the stack by the spec: open, type, chip, Escape, back gesture, close button, choose a hit, swipe; and the pins panel's spoiler click, in three themes at both sizes.

## Tests run
- `p71Panels.test.tsx` 17/17 (13 fail on the base, above); `p70Lines` 25, `search` 9, `p70Plain` 52, `p67Previews` 23, `notifyText.test.ts` 13: all pass. Web `tsc --noEmit` clean; eslint and prettier clean on my files; `node scripts/i18n-audit.mjs --max 0`: 0; `check-doc-links` ok.
- Playwright: `p71-panels-shots.spec.ts` (2 tests) and the p69 glyph test pass in chromium. The register route rate-limited me after several runs (a restart of my server cleared it): only my local stack.
- First-paint grep: no first-paint file (`store/**`, `gateway/**`, `routes/**`, `app/main.tsx`, `router.tsx`, `pwa/**`, `i18n/**`, `VoiceBridge`) imports `ChannelHeader`, `PinsButton`, `SearchBox`, `searchContext` or `PhoneSearch` (the only hits are i18n key strings like `library.searchBox.*`). `ChannelHeader` is imported by `ChannelView`, `StageView`, `VoiceChannelView` (Lounge chunk). `PinsButton` now also imports `markdown`, `msg/CustomEmoji`, `msg/RichCards`, `shell/notifyText`: all already Lounge-chunk files. No build run (not asked: no public-page or first-paint file touched).
- Not run: bundle and entrance checks, clippy (no Rust).

## Base binaries
`C:/Users/braxt/Desktop/SIDEROOM_wt/p71-base-bin/` holds `sr-server.exe` and `sr-cli.exe` built from `0c6b26d8` ("e2e: drop an unused import", the unchanged base), with `BUILT_FROM.txt`. They were in place at about 09:41 (cold build 11m26s). I left the folder for the commander. My stack ran from a copy.

## Docs changed
`docs/specs/lounge-chat.md` (6.1 new paragraph; 9 phone search). Searched `docs/`, README and the help sources for the pins/search/timestamp facts: nothing else states them. `docs/15-threat-model.md` and the decisions log match "Pinned Messages"; I did not read or edit them (the commander's files): the threat-model rows below say what to add.

## Proposed inventory rows
- SEC-AUD-262: A pin is drawn as a message (spoiler hidden until clicked, mentions as names, no raw marks), a search line never falls back to typed text. Closed by `PinsButton.tsx`, `SearchBox.tsx`, `searchContext.tsx`. Tests: `p71Panels.test.tsx`.
- SEC-AUD-263: Search on a phone: header button and full-pane sheet. `PhoneSearch.tsx`, `ChannelHeader.tsx`.
- SEC-AUD-264: A relative timestamp in a one-line text is the short date and time. `SingleLine.tsx`.
- SEC-AUD-265: Editorial's online colour: measured, 5.27:1 on `--ui-bg`; no change; the faintness is chroma and size, an owner question.
- New row to consider (outside my files): `markdown-react`'s stylesheet is imported nowhere; a spoiler in the message list is not hidden by CSS in the dev stack.

## Proposed threat-model rows
- The pins panel was the last place where a spoiler was readable as typed text (wave 70 closed the reply line, Inbox, toast, delete confirmation and link card). Closed: it is drawn by the message renderer, with the spoiler hidden until clicked. Residual: the spoiler's text is in the DOM (aria-hidden, `opacity: 0`), as in the message list; the hiding is CSS, and the stylesheet is the finding above.
- The search fall-back and context lines can no longer show typed text.

## Changelog sentences (ops's file)
- "Search works on a phone: a search button in the channel header opens it as a sheet over the chat."
- "The pinned messages panel shows each pin as the message it is: spoilers stay hidden until clicked, mentions show names, lists and code blocks are drawn."
- "A time written as relative (`<t:...:R>`) in a one-line preview is shown as a date and time."

## Dead keys
None (the pins and search strings are unchanged; new keys only: `lounge.phoneSearch.open`, `.title`, `.close`).

## Owner questions (to the Fable planner)
1. A 44px search button does not fit the phone header without removing the channel title. I kept the 28px footprint with a 44px invisible target. Is that acceptable, or should the header lose another control (for example Help) on a phone?
2. Editorial's status colours meet 3:1 and the contrast test already enforces it, yet the glyph reads faint (desaturated olive, thin outline, 13px). Do you want a more saturated Editorial green, or a filled glyph (a change in `packages/ui`)? D-741 as written is met without a change.
3. `markdown.css` is not imported by the app (spoilers, mention chips, code and quote styles of the message list come from nowhere): do you want it imported in the Lounge chunk for the list too? (I did only the pins panel.)

## Cleanup
Dropped `sr_p71_panels_ui` (checked: none left); deleted `p71-panels-target` and `p71-panels-bin`; `p71-base-bin` left in place. My Vite (PID 34984) and sr-server were stopped by PID; nothing of mine is queued or running; the heavy lock is released. Untracked scratch not committed: `e2e/pw-p71*.log`, `e2e/test-results`, `p71-panels-build.log` in the parent folder.

## Not done
Nothing dropped. Not done by design: base-stack "before" shots (no base stack run; the before state is proved by the failing tests), the production bundle build.
