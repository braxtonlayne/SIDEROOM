# feat/p34-player-skins (MUS-SKN-001): backdrop skins for the floating player

Branch `feat/p34-player-skins`, base `integration/phase34`. Owner direction: design/player-skin-references/README.md
("match the vibe, not the mock-up; only the real parts are interactable; small extras should really work") plus the
owner's later request for a fifth skin in the style of the classic stacked desktop player (reference 04).

## Done

**A new skin kind: the backdrop skin** (`apps/web/src/player/skins/backdrop/`)
- `frame.tsx` `BackdropFrame`. Each skin is painted artwork (CSS, inline SVG, `--ui-*` tokens and skin-local custom
  properties built from them with `color-mix`). Only the controls the player really has are live, laid over the art.
  Artwork goes through `<Art>`: `aria-hidden`, `pointer-events: none`, never focusable.
- Each skin is drawn for one colour scheme. The frame wears a fixed Lounge theme (Onyx for dark skins, Light for
  bright ones) with the member's appearance settings, so the mood holds whatever the member's theme is and contrast
  is checked against known tokens.
- Motion follows the music. `data-motion="on"` only while playing and reduced motion is off, and the CSS animates
  only under it. `@media (prefers-reduced-motion)` stops everything as well.
- Level-driven art (needles, LED meters, spectra, glow) reads `--bd-level`, `--bd-low`, `--bd-high` and `--bd-b0..15`.
  The frame writes these once per animation frame from the player's analyser, so nothing re-renders per frame.
- `parts.tsx` holds the extra live controls on store actions: shuffle, the EQ switch, open library, cue pads and the
  queue as buttons.

**Five skins, each a lazy chunk** (`PlayerHost` `lazy()`, with the Retro skin as the Suspense fallback)

| id | Name | Look | Real extras |
|---|---|---|---|
| `valve` | Valve & Walnut | Brass and walnut tube cabinet, glowing valve, reels, Nixie readout (refs 01, 17) | Two VU meters (low and high band) driven by the analyser, Nixie position readout, Tone switch (EQ on/off), shuffle |
| `tidepool` | Tidepool Glass | Y2K pearl chrome and aqua glass, gel wings, bubbles (refs 16, 05, 12) | Queue wing (click plays), Library button, shuffle, play orb that glows with the level, wave that follows the level |
| `circuit` | Night Circuit | Neon cyberpunk deck, hazard stripes, radar (refs 20, 03, 07) | Spectrum visualiser (24 bars from the analyser), master LED meter, 4 cue pads (jump to 0/25/50/75%), queue table |
| `study` | Pastel Study Deck | Pastel lo-fi desk, turntable, notebook (refs 19, 08) | Focus timer and notes pad through `SkinExtras` (below), rainbow level meter, queue, shuffle |
| `stack` | Steel Stack | Three docked brushed-steel windows (main, equalizer, playlist), bolt emblems, gold pinstripes (ref 04 / owner image) | See below |

Steel Stack, real parts:
- **Main window:** the LCD time is the real position, with a play-state glyph and a level meter from the analyser. The ticker reads "n. Title — Artist". Also live: volume, seek, prev, play, pause, stop, next, shuffle with LED, and repeat. Eject opens the music library. EQ and PL show and hide those windows; the setting is kept for the tab.
- **Format readout:** shows the file's container (MP3, FLAC and so on) when the library knows it. This is new plumbing: `TrackDto.container`, `LibraryTrack.container`, `PortTrack.format` and `PlayerTrack.format`. The kbps, kHz and mono/stereo cells stay dark (unlit segments, generated content), because nothing in the player knows them. They never show made-up numbers.
- **Equalizer window:** ON is the EQ switch. PRESETS lists the library's presets. Each of the 10 bands is a vertical slider that drives the player's EQ store. The curve is drawn from the gains. The preamp column shows the automatic headroom cut, which is not a control.
- **Playlist window:** this is the real queue, as a listbox. A click selects a track. A double-click or Enter plays it. Delete, Backspace or REM removes the selected track. SEL selects the playing track. ADD opens the library. The time readout shows the position against the queue's total length.
- **Painted:** AUTO (lit, because the preamp is always automatic), MISC, the bolts and the pinstripes.

**Shuffle in the player store** (new, small). `setShuffle`/`toggleShuffle` put what is still to come in a random order and keep the current track playing. Turning shuffle off restores the earlier order. `enqueue` and `removeAt` keep the saved order in step. A new queue (`play(tracks)`) starts unshuffled. Shuffle is not persisted.

**Wiring:** `PLAYER_SKINS` now has 8 entries, so the cycle button, the Settings > Appearance picker (`PlayerSkinCard`) and the persisted-skin check pick them up. The names and descriptions are in the catalog.

**i18n:** new fragment `apps/web/src/i18n/messages/en/p34-player-skins-1.ts`. The skins reuse the library's `music.lib.eq.*` keys for the EQ.

## SkinExtras: what the integrator must wire after wave 32 lands

`apps/web/src/player/skins/backdrop/extras.ts` defines the adapter (`SkinFocusTimer`, `SkinNotes`, `SkinExtras`) and `registerSkinExtras()`. Wave 32's hooks were NOT copied. Bind them once at start-up, for example in `wireMusicLibrary()` or a new `player/toolsWiring.ts` that the host imports:

```ts
import { registerSkinExtras } from './skins/backdrop/extras';
import { useFocusTimer } from '../tools/timers/useFocusTimer';
import { useNotes } from '../tools/notes/useNotes';

registerSkinExtras({
  useFocusTimer: () => {
    const f = useFocusTimer('pomodoro');
    return { clock: f.clock, running: f.running, progress: f.progress, segment: f.state.segment,
             toggle: f.toggle, reset: f.reset, skip: f.skip };
  },
  useNotes: () => {
    const { state, store } = useNotes();
    // Pick the note the skin shows (e.g. the first, or one titled for the player) and map it:
    // status: state.status === 'ready' ? 'ready' : state.status === 'loading' ? 'loading' : 'unavailable',
    // text: <that note's text>, setText: (s) => store?.<update that note>(s), persistent: true
  },
});
```

**Default until then (documented choice):**
- **Timer:** a working minimal pomodoro (25 minutes focus, 5 minutes break, Start/Pause, Take a break or Back to focus, Reset). It runs in memory for the tab and survives a skin change.
- **Notes pad:** works in memory for this tab only and says so ("Kept in this tab only, not saved."). Nothing is written to storage, because members' notes belong in wave 32's encrypted scratchpad, not in plain localStorage.

When `useNotes` returns `status: 'unavailable'`, the pad hides. Before importing `tools/*` into the player chunk, check the bundle `player` row and D-385. The player is app-only, so that is allowed, but keep it lazy.

## Not done / open

- **Ambience mixer (rain, crackle, café; ref 19):** not built. It needs sound files, which means a download plus a licence decision for the owner.
- **Lyrics panel (refs 01, 17), A-B loop, tape speed, balance, sleep timer:** not built. Each needs engine or graph work (playback rate, a stereo panner, lyrics plumbing from MUS-WASM-001). They are good candidates for a follow-up.
- **Bitrate, sample rate and channels in Steel Stack:** dark. The library API does not send them. If music-index starts storing them, add them to `TrackDto` and pass them through like `container`.
- **e2e:** `player-layout.spec.ts` now loops over all 8 skins (fit at 1280x800, 360x740 and 320x640, plus axe with contrast). It also has a new reduced-motion and hidden-art test for the backdrop skins. It was NOT run locally: it needs sr-server and 5173, which this wave reserves for the integrator. The integrator should run it.

## Tests run

- `npx vitest run src/player src/music --maxWorkers=2`: 19 files, 254 tests passed. New: `player/__tests__/backdrop-skins.test.tsx`, 52 tests. They cover renders, axe, live controls, aria-hidden art, motion and reduced motion, valve tone, tidepool queue and library, circuit cues and meters, study timer, notes and the bound extras, stack EQ, presets, windows, playlist keyboard and REM, format readout and queue time, shuffle, the analyser band reader, and the settings picker.
- `npx vitest run src/i18n src/settings/user --maxWorkers=2`: 38 files, 281 tests passed.
- `npx pnpm@10.12.1 -r --if-present typecheck`: all workspace projects passed, including e2e.
- eslint on `src/player` and the touched files: clean. `prettier --check`: clean. `node scripts/check-css-tokens.mjs`: 64 files clean.
- `node scripts/i18n-audit.mjs`: TOTAL 0. The `--ts` run has no hits in touched files.
- `node scripts/build-web.mjs`, `check-entrance-isolation.mjs` and `check-bundle-budget.mjs`: all ok. The `player` row reports "no player or music library code in the entrance". Each skin is its own chunk (`ValveSkin-*.js/css` and so on).
- Browser check on a throwaway Vite server, port 5178, which was stopped afterwards: `node e2e/tools/player-skins-preview.mjs`. All five skins fit inside the viewport with no sideways scroll at 1280x800 and 360x740, and axe (wcag2a/aa, 21a/aa, best-practice, colour contrast) reported none at either size.

Screenshots for the owner: `C:\Users\braxt\Desktop\SIDEROOM\screenshots\player-skins\` has `<skin>-1280x800-player.png`, `<skin>-1280x800-fullviewport.png`, `<skin>-360x740-player.png` and `<skin>-360x740-fullviewport.png` for valve, tidepool, circuit, study and stack. In each, a generated tone is playing with a queue of 3 tracks, at deviceScaleFactor 2.

## Decisions

- **Skin palettes:** built only from `--ui-*` tokens on a fixed Lounge theme per skin. There are no raw colours, which `check-css-tokens` covers in `player/`.
- **Original names:** no trademarks and none of the made-up brand names from the references appear in names, strings, comments or files.
- **Stack transport:** separate Play and Pause buttons, as in the reference. The other skins use the shared toggle.
- **Labels:** the visible text of every labelled button starts its accessible name (axe `label-content-name-mismatch`). For example, "EQ: equalizer window" and "Rem: remove the selected track from the queue".

## Owner questions

1. Which extras should each skin get next? Candidates: an ambience mixer (needs sound files and a licence), a lyrics panel, a sleep timer, A-B loop, tape speed.
2. Should the Pastel Study Deck notes pad keep working in tab-memory before wave 32 is bound, or hide until then?

## CHANGELOG lines

- MUS-SKN-001: five new player skins: Valve & Walnut, Tidepool Glass, Night Circuit, Pastel Study Deck and Steel Stack. Each is painted artwork with only the real controls live, and decorative motion that follows the music and stops under reduced motion.
- MUS-SKN-001: real extras in the skins: VU meters, a spectrum visualiser and LED meters from the player's analyser, cue pads, a queue wing or table, a focus timer and notes pad (bound to wave 32's tools by the integrator), and Steel Stack's equalizer and playlist windows.
- MUS-SKN-001: shuffle in the floating player (it reorders what is still to come and restores the order when turned off).
- MUS-SKN-001: the player knows a track's file format from the library (`container`) and shows it where a skin has a format readout.
