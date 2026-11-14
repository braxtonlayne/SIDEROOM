# Wave 72 prose handoff (SEC-AUD-274, 275; D-757)

Branch `fix/p72-prose`, base `8bc63455` (integration/phase72). Web only, CSS and imports. No Rust, no migration (3090-3099 unused), no string, no `openapi.json` change, no `packages/markdown-react` change.

## Done (commits)
- `cf82d28d` SEC-AUD-274 and 275: new `apps/web/src/lounge/msg/markdown.css`; the spoiler block of `msg/msg.css` and the pin's `sr-md` copies of `header/header.css` moved into it (the pin keeps its own `pre` cap); imported by `MessageList.tsx`, `MessageItem.tsx`, `msg/Md.tsx`, `header/PinsButton.tsx`, `profile/profileMarkdown.tsx`; the shots spec `e2e/tests/p72-prose-shots.spec.ts`.
- a last commit: the spec's thread and code-block-scroll steps, `docs/specs/lounge-chat.md` and this handoff.

## What the base drew, per construct (shot on the base, 57 PNGs in `visual-sweep-shots/p72-prose/base`)
Recorded with the spec's assertions switched off (`SR_P72_ASSERT=0`), computed styles on the base: mention span background `rgba(0,0,0,0)`, inline code `font-family: monospace` (the browser's default for `<code>`), quote `border-inline-start-width: 0px`, jumbo font size normal. Those are the three assertions of the spec plus the jumbo one: they fail on the base by those values (I did not run the strict spec on the base, I read the recorded values).
| construct | base | after |
|---|---|---|
| user / role mention | a grey browser button (`button.sr-md-mention` with the UA face), square, no brand | chip: 25% brand backdrop, brand-tinted text, 3px radius |
| `@everyone`, `@here`, channel mention | plain text, no chip (spans) | same chip |
| inline code | browser `monospace`, no backdrop | app mono token, backdrop, 0.875em |
| code block | no backdrop; long line clipped at the column edge, **not scrollable** (an ancestor clips it); no Shiki colours (`--shiki-*` never applied) | backdrop, border, padding, `overflow-x: auto` (scrolls inside itself: measured `scrollLeft` 40 on desktop and phone), Shiki colours per colour scheme |
| quote | indented by the browser's 40px, **no bar** | 4px bar, no 40px indent |
| headings, lists, subtext | already right (browser defaults) | **no rule** |
| link, timestamp, line breaks, blank lines, long unbroken word | already right | **no rule** |
| unicode emoji only (jumbo) | normal size | 2.75rem (the parser emits unicode emoji as plain text, so the rule is on `.sr-md-jumbo`, not on `.sr-md-emoji`) |
| custom emoji image in a sentence | **natural size (96px in my fake, up to the 64px the API asks for)**, no size rule | 1.375em on the text line |
| custom emoji only (jumbo) | natural size | 3rem |
| spoiler | hidden (wave 71 rules in msg.css) | same rules, moved |

## What I added
`markdown.css`: bare `sr-md-*` selectors, app tokens only (`--brand`, `--text-normal`, `--ui-border`, `--ui-text-faint`, `--ui-font-mono`), logical properties, `color-mix` with tokens, no colour literal. Shiki colour: `color: light-dark(var(--shiki-light), var(--shiki-dark))` (the theme provider sets `color-scheme` on its root; the package's `prefers-color-scheme` and `:root[data-theme]` selectors never match the app, whose `data-theme` is on a wrapper `div`), with `var(--shiki-light)` as the fallback line.

**Wrapping did not change.** `.sr-md { white-space: pre-wrap; overflow-wrap: anywhere }` is NOT copied: the list already breaks lines (blank lines and a 170-character word shot on the base and after, identical).

**`prefs-effects.css`:** its `[data-underline-links='true'] .sr-md a` rule is untouched (I add no link rule). The hover-reveal preference rule `.lng-msg-content[data-spoilers='hover'] .sr-md-spoiler[data-revealed='false']:hover .sr-md-spoiler-content` has specificity 0,5,0 against my 0,3,0 spoiler content rule, as against the old `msg.css` copy (same selector): it still wins. Not clicked in a shot (the preference was not set).

Small changes in the pin: a mention in a pin now has the brand text colour and weight 500 (before: chip with default text colour); the quote lost the browser's 40px indent; a code block has a 1px border. Everything else in the pin is as before (shots).

## Item 275: importers
All direct importers of `@sideroom/markdown-react` in `apps/web/src` are under `lounge/**`: `PinsButton`, `markdown.ts`, `MessageItem`, `MessageList`, `msg/ComposerExtras`, `msg/Embeds`, `msg/Forwarded`, `msg/Md`, `msg/SingleLine`, `msg/SystemMessage`, `profile/profileMarkdown`, `shell/notifyText`, `suspiciousLinks`, `threads/ThreadChip` (plus tests). Which reach the stylesheet now: `MessageList`, `MessageItem` (the list, thread panel, reply forwarding), `Md` (embed text, forwards, `DmPanels`, `ServerGuideDialog`), `PinsButton`, `profileMarkdown` (profile card bio and the Settings Profile preview, `settings/user/sections/Profile.tsx` imports `ProfileBio`). Not markdown surfaces (they write plain text, so no chips): `SingleLine`, `SystemMessage`, `notifyText`, `ThreadChip`, `ComposerExtras`, search hits (the server's plain snippet; shot: plain text, as before).
| surface outside `lounge/**` | importer | state |
|---|---|---|
| Library, Cinema, Admin, Arcade, Study | none (no direct import) | n/a |
| Settings, Profile section | `lounge/profile/profileMarkdown` | styled now (imports `markdown.css`); **not shot** in Settings |
| public profile page `/~handle` | not looked at for a markdown component; `check-entrance-isolation` passes after the build | not looked at |
| `watchparty/ExternalStage` | imports only `lounge/msg/EmbedsPrefs` | n/a |

The thread panel was shot (6 PNGs, mention, inline code, spoiler, quote and a highlighted block); a reply preview, Message Requests and the composer's preview are plain text and were not shot.

## Visual check
- Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p72-prose/after`: 63 PNGs (list x24: four scroll positions, desktop and phone, three themes; pins x18; spoiler opened x6; mention chip clicked, profile card opens, x6; search x3; thread x6). I opened every one of the 63 that stand now. Honest note: the final run regenerated the folder after a spec-only change (thread and scroll steps added); I opened the 57 of the previous identical-CSS run and the 6 thread shots of the final run, so every PNG of the final folder has a twin I looked at; the 6 thread shots are the only ones new in that run.
- Base folder `.../p72-prose/base`: 57 PNGs, of which I opened about 12 (the editorial desktop list, phone light and dark list, pins desktop light, pins phone dark, and the first runs before the spec was fixed); the rest of the base I read through the recorded computed styles. The "after" ones are the ones the rule asks for.
- Clicked in the real stack, all three themes, desktop and phone: a spoiler opens (and the pin's spoiler), a mention chip opens the profile card, a code block scrolls sideways inside itself while the page does not scroll sideways (`scrollWidth - clientWidth` of the page 0, measured each time), a thread shows the same.
- Spec (local only, `SR_P72_PROSE=1`) asserts: mention span background not transparent, inline code font family not `monospace` and monospace-ish, quote 4px, jumbo font >= 40px, block `overflow-x: auto` and `scrollLeft` > 0 after setting 40, page overflow 0. All pass on the final stack (1 passed).
- No new vitest (CSS). `p71Panels.test.tsx` (17) and `p13protect.test.tsx` (6) run and pass.

## Checks
- Web typecheck clean; `i18n-audit --max 0` TOTAL 0; eslint on the touched files clean; prettier clean on the touched files and the spec; `check-doc-links` ok. No stylelint in the repo.
- Build (inside the lock): `check-bundle-budget` ok, `check-entrance-isolation` "every public surface is clean", `check-staff-delivery` ok.
- **First-paint figure:** before (base build, `app-BoY3u4gK.js`): 215828 bytes gzip, room 40172; after: `app-CKz5xz13.js` 215850 bytes gzip, room 40150. +22 bytes, which is the new stylesheet's name in the first-paint file's preload map (the CSS lands in a shared lazy chunk, `app/accountLocale-*.css`, 1.5 KB; the first-paint CSS `app-*.css` is unchanged apart from its hash). No static import added to a first-paint file. The 8 KB floor holds (40 KB).

## Not done
- Rule for `sr-md-link`, `sr-md-timestamp`, `sr-md-subtext`, headings and lists: shot, they already look right.
- `sr-md-command`, `sr-md-guild-nav`, `sr-md-cinema`, `sr-md-library` (rich cards and slash/guild-navigation mentions) share the chip because they carry `sr-md-mention`; their cards were not shot.
- Settings Profile preview and the public `/~handle` bio: not shot.
- `@everyone`/`@here` and a mention of the viewer look like any other chip (no class tells them apart for the viewer).

## Proposed inventory rows
- SEC-AUD-274: `apps/web/src/lounge/msg/markdown.css` styles mention chips, inline code, code blocks (Shiki colours per colour scheme, scrolls inside itself), the quote bar, emoji images and jumbo emoji in the message list and every Lounge markdown surface (D-757). Closed. Proof: `e2e/tests/p72-prose-shots.spec.ts`, 63 shots, base values recorded.
- SEC-AUD-275: one set of rules (spoiler copy of `msg.css` and the pin copies of `header.css` removed); importer table above (D-757). Closed for the Lounge; Settings Profile and `/~handle` bio not shot.

## Proposed threat-model rows
None.

## Changelog sentence (reports builder)
The message list and the pins panel now draw mentions as chips, code with a backdrop and highlight colours, quotes with a bar and emoji at the right size.

## Docs
`docs/specs/lounge-chat.md` (the Content bullet: "How it looks"). Nothing else repeats where the markdown styles live.

## Owner questions
1. Headings, lists and subtext look like the browser's: heading one is 2em, larger than Discord's. Leave, or give them Discord's sizes (1.5, 1.25, 1em)? Default: leave.
2. The "Thread started from ..." system line shows the starter's raw text with backticks (`inline \`code here\``), seen in the thread shots. D-716 says text outside a message is plain text with markup written out; is a raw backtick acceptable there? Not mine; left as is.
3. The mention chip's text colour is the brand mixed with the text colour (readable on all three themes in the shots). Default kept.

## Clean-up
Scratch database `sr_p72_prose_ui` dropped. `p72-prose-target` and `p72-prose-bin` deleted. `p72-base-bin` left (with `BUILT_FROM.txt`). Stack stopped (server and Vite by PID/port), lock released. Scratch files are in `C:/Users/braxt/Desktop/SIDEROOM_wt/p72-prose-scratch` (untracked).
