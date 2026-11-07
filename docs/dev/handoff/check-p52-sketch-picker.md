# Check: the shelf picker search inside Sketch & Slander (wave 51 follow-up)

Branch `check/p52-sketch-picker` (from main 2b727dbc). No product code changed; no defect found.

## What was run
- Built `sr-server` and `sr-cli` in a private target dir (inside the heavy lock), ran them with `SR_VOICE__ENABLED=true`,
  `SR_ACTIVITIES__ENABLED=true`, memory storage; Vite with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`. DB `sr_p52_sketch`.
- A temporary spec (a copy of `e2e/tests/sketch-gate.spec.ts`, not committed) with `SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1
  SR_E2E_VOICE=1`: member and guest phone in a call, Sketch started, "Choose a file..." opens the app's shelf picker.
  The shelf routes are played by Playwright routes (as in the gate spec); the `objects` route filters on `?q=` like the
  server. Queries seen: `""`, `"picked"`, `"zzzz"`.
- Theme is set with `PATCH /users/@me/settings` (no reload: a reload would drop the voice call and the Activity). It applies
  live. Themes: `lounge-dark`, `lounge-light`, `lounge-editorial`. Each shot desktop 1280x1000 and phone 375x812.
- Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p52-sketch` (`<theme>-{desktop,phone}-{1-picker,2-search,3-nomatch}.png`,
  plus panel/share/guest shots). Looked at: dark desktop+phone all three stages, light desktop search and phone no-match,
  editorial desktop no-match and phone search.

## Verdicts
- Search box themed in all three themes (field, label, focus ring follow the theme): OK.
- Results filter ("picked" leaves picked.json; the "2 files are hidden" note shows only unfiltered): OK.
- Empty result sentence ("No files here that this Activity can use.") shows: OK. Wording is the generic empty sentence, not a
  "nothing matches your search" one (owner question below).
- Phone: dialog fits, nothing overflows, no horizontal scroll, long title wraps: OK.
- Picker sits above the Activity panel and the call dock (dimmed backdrop over the frame): OK.
- Flake seen twice and unrelated: Chromium voice "Voice Connected" timeout / ECONNREFUSED to `::1` at startup; a re-run passed.

## Owner questions
- When a search matches nothing, should the empty sentence say so ("No files match your search.") instead of the generic one?
- Keep a permanent shot of this? Not added: the stack needs guest relay + voice, and `sketch-gate.spec.ts` already runs it in CI.

Stack stopped by PID, DB dropped, lock released. No fix commits.
