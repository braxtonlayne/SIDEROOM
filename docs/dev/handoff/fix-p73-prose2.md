# Wave 73 prose2 handoff (SEC-AUD-291; D-775)

Branch `fix/p73-prose2` (base: `fix/p73-prose`, d6d75f76). Web only: one function, one new vitest file, a local shots spec. No Rust, no migration, no route, no string, no `packages/**`, no `openapi.json`.

## Done (commits)
- `36aed7eb` SEC-AUD-291: `ProfileBio` (`apps/web/src/lounge/profile/profileMarkdown.tsx`, that function only) starts the parser in an effect on mount (`loadMarkdownEngine()`, rejection caught and turned into a `failed` flag, never unhandled). Until the parser is ready and the load has not failed, the `.lng-pop-bio` element is drawn **empty** (nothing of the text: no `**`, no spoiler in clear). After a failed load it draws the typed text as before. Ready from the first render: drawn at once. Empty bio: still returns `null`. `profileSubset` and `profileBioText` untouched. No new static import (WASM stays a dynamic import inside `markdown.ts`).
- `p73Bio.test.tsx` (new, 4 tests), `e2e/tests/p73-prose2-shots.spec.ts` (local only, `SR_P73_PROSE2=1`; a later commit makes it press "Try again" when the dev proxy refuses a request).

Why an empty element and not no element: the element is empty (zero height) either way; keeping it leaves `data-profile-bio` in place for the popout/profile code and the layout of the card is the same as with `null`. The card grows by the bio's height when the parser arrives (the WASM was fetched in well under the 3 s the shots wait).

## Confirm step (watched fail on the base)
Unit: with `setMarkdownEngine(null)` and `@sideroom/wasm` mocked (`loadMarkdown` spy), `<ProfileBio text="**bold** ||secret|| <@123>" />` alone: on the base the loader was called 0 times (both the "loads once on mount" and the "failed load" tests failed with "expected spy to be called 1 times, but got 0"). Real stack: base shots/records show the typed text (`**bold**`, `||a bio spoiler||`, `<@id>`, `# a bio heading`) in the preview with `/settings/user/profile` loaded first, and no `markdown_bg.wasm` request. The fault was there.

## After
Settings loaded first: the preview is markdown (bold, italic, inline code, link, spoiler closed and covered, heading and quote flattened by `profileSubset`, mention dropped), `markdown_bg.wasm` fetched once. Typing in the About me field: the preview follows as markdown; the spoiler in the preview opens on click (asserted and shot). Entered from the Lounge: identical. Popout in the Lounge: unchanged (identical records base vs after).
- A Settings page with no bio, loaded first (`/settings/user/account`, `/settings/user/appearance`): **no WASM request** (recorded `wasm []`). `ProfileBio` mounts only on bio surfaces.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p73-prose2`. **after/: 21 PNGs, all opened**: settings-profile-direct (desktop x3 themes, phone x3), fromlounge (desktop x3, phone Lounge Light), typed (desktop x3), spoiler-open (desktop x3), popout (desktop x3), nobio (2). **base/: 18 PNGs** (my worktree before the change, `profileMarkdown.tsx` from d6d75f76 swapped in for the run, restored with a copy; not the prose builder's shots): I opened 2 of them (direct desktop Light, direct phone Editorial); the rest are the same records (`RECORDED` lines show raw text for every one) and I did not open them. Chip/code/spoiler backdrops are not transparent or black in any after shot (Settings looks like Settings; code `0.1` alpha tint, spoiler solid grey as the prose handoff recorded). The phone "from the Lounge" shot sits at the top of the page (not scrolled to the bio); the desktop one shows the same card. Note: the dev proxy refused a request now and then (`localhost` resolving to `::1`: "Couldn't load your profile", button Try again); the spec presses Try again. Unrelated to the change.

## Checks
- vitest: `p73Bio.test.tsx` 4/4; `profile.test.tsx`, `p24ProfileCard.test.tsx`, `p11ShellProfile.test.tsx`, `settings/user/__tests__/p13profile.test.tsx` all green (28 tests), no test needed a change.
- web tsc clean; eslint + prettier clean on my files; `i18n-audit --max 0` TOTAL 0 (no string added); `check-doc-links` ok.
- Web build, bundle budget, entrance isolation, staff delivery: all pass. First paint `app/app-ki0f0x-b.js`: **215,862 bytes gzip** (prose builder: 215,858; +4 = the chunk name), room 40,138 bytes.
- No cargo, no Rust, no Postgres tests.

## Docs
Searched `docs/`, README, deploy, Handbook text for a sentence saying the Lounge shell loads the parser or the Settings preview needs the Lounge: none wrong in a file nobody owns. `docs/specs/lounge-chat.md` and `docs/10-design-system-and-theming.md` state nothing about who loads the parser. For the commander, `apps/web/src/lounge/markdown.ts:5` (not mine): replace "(the Lounge shell calls it on mount)" with "(the Lounge shell, the composer and About Me call it on mount)".

## Proposed inventory row
SEC-AUD-291 (closes the open remainder of SEC-AUD-285): About Me starts the markdown parser itself (D-775): the Settings Profile preview and the profile card draw the bio as markdown when Settings is the first page loaded; nothing of the typed text (spoiler included) is drawn until the parser is ready; a failed load draws the typed text. Tests `p73Bio.test.tsx`; shots `visual-sweep-shots/p73-prose2`.

## Changelog sentence (for status)
The About Me preview in Profile settings now shows your bio as formatted text when Settings is the page you open first, instead of the raw `**` and `||` marks, and a spoiler never shows its text while the page loads.

## Not done / notes
Nothing dropped. The popout and full profile were not changed (they cost nothing new: `loadMarkdownEngine()` resolves at once when the parser is loaded). I did not touch any stash; none seen.

## Cleanup
Scratch DB `sr_p73_prose2_ui` dropped; `p73-prose2-bin` deleted; stack stopped by PID; lock released; no Rust target was created. Untracked scratch (`p73-prose2-scratch`, `e2e/test-results`) is outside commits.

## Owner questions
1. While the parser loads, the bio area is empty for a moment, then fills in (the card grows). Is that fine, or should a fixed-height placeholder hold the space? (I took the empty element: no extra markup, no flash of raw text.)
