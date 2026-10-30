# Handoff: feat/p30-player-shell (wave 30, MUS-PLY-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p30-player-shell`, branch `feat/p30-player-shell`, base `integration/phase30`.
No migration, no Rust, no new dependency, nothing pushed, no PR. The Vite dev server on 5173 was started for the
local Playwright run and stopped again.

## Resume here

MUS-PLY-001 is complete except the items under "Still open". Everything is committed. If you continue: run
`e2e/tests/player.spec.ts` once with the full stack (the Cinema to Settings case needs a signed-in Owner), and the
Firefox and WebKit projects (neither could run here).

## Done

- `apps/web/src/player/` (new):
  - `store.ts`: the zustand `player` store (queue, index, status, position, duration, volume, muted, skin, dock,
    repeat, EQ gains, crossfade seconds, library slot). Skin, dock, volume, mute, repeat, EQ and crossfade persist in
    `localStorage` (`sr.player.v1`, validated by `sanitizePersisted`); the queue never does (presigned URLs expire).
  - Exported actions for music-lib and later waves (import from `player/store`, never touch the store directly):
    `play(track | tracks | index | nothing)`, `pause`, `toggle`, `enqueue(tracks, {next})`, `removeAt`, `clear`,
    `next`, `prev`, `seek`, `setVolume`, `toggleMute`, `setSkin`, `setDock`, `setRepeat`/`cycleRepeat`, `setEq`,
    `setEqEnabled`, `setCrossfade`, and `registerLibraryPanel(Component)` which returns the remover.
    `PlayerTrack` (in `types.ts`) has `id, title, artist?, album?, src, artworkUrl?, durationSec?` and an optional
    `resolveSrc(track)` so a long queue can re-presign right before a track starts.
  - `engine.ts` + `graph.ts`: one `<audio>` element, `MediaElementSource` to ten EQ biquads (low shelf, eight
    peaking, high shelf) to a crossfade gain to a master gain to an analyser to the destination. Falls back to the
    bare element when Web Audio is missing. `mediaSession.ts`: metadata with artwork, play/pause/prev/next/seekto,
    position state.
  - `PlayerHost.tsx` (the host, lazy), `PlayerGate.tsx` (mounts it the first time something is queued, then keeps it
    for the session), three skins in `skins/`: `RetroSkin` ("Retro MP3 Hardware", default: LCD, ticker, VU bar,
    hardware keys), `PillSkin` ("Compact Pill / Dock"), `LibrarySkin` ("Full Library Manager": transport, tabs
    Library and Queue; the Library tab renders the component music-lib registers, a placeholder until then).
    `controls.tsx` holds the shared controls; `player.css` uses `var(--ui-*)` and logical properties only (the player
    directory is now in `scripts/check-css-tokens.mjs` TARGETS and passes). The ticker stops under reduced motion
    (Settings > Accessibility or the OS) and while not playing; the VU bar is also still under reduced motion.
  - `PlayerSkinCard.tsx`: the skin and dock picker in Settings > Appearance (card `us-player`, also in the settings
    search index).
- `router.tsx`: `<PlayerGate />` in the root layout above every route, so it survives Lounge, Cinema, Library,
  Settings and Admin. The host is its own chunk.
- `lounge/shortcuts.ts`: group "Player", ids `playerToggle` (Alt+Shift+P), `playerPrev` (Alt+Shift+Left),
  `playerNext` (Alt+Shift+Right), `playerMute` (Alt+Shift+M); rebindable in Keybinds like the others, shown in the
  shortcut sheet. They are handled by the host on every route (`LoungeShell` skips them: one-line change).
- PWA update risk (RDY-PWA-001): `pwa/state.ts` gained `setUpdateGuard`, `flushDeferredUpdate`,
  `isUpdateDeferred` and the `updateDeferred` field. While a track plays, `applyPendingUpdate` does not reload; it
  records the request, and the update toast and the Lounge notice say "The update will install when this track
  ends." The engine reloads at the next track boundary (it parks the queue in `sessionStorage` and does not start
  the next track under the reload), or when the member pauses or playback stops. After the reload the queue comes
  back paused (`restoreResume`).
- i18n fragment `apps/web/src/i18n/messages/en/p30-player-shell-1.ts`.
- Dev/test handles (dev server and Vitest only, absent from production builds): `window.__srPlayerActions` (store
  actions) and `window.__srPlayer` (actions plus the audio element).

## Tests run

- Vitest (`--maxWorkers=2`): `src/player` 75 tests (store 23, engine 18 incl. the update deferral, skins incl.
  axe on all three skins, host, shortcuts); `src/pwa` (new deferral cases); `src/i18n`, `src/router.test.tsx`;
  `src/settings/user` (233); `lounge` logic and home flow tests. All green.
- Playwright, Chromium, local, against the Vite dev server with no API (a scratch config without the global setup,
  not committed): `playback survives navigation between public routes` (same page load, `audio.currentTime`
  advances, controls and keys work), `each skin works and passes axe` (real axe with contrast, serious and
  critical fail), `reduced motion stops the ticker`, `the ticker scrolls while playing` - 4 of 4 passed.
- `tsc --noEmit` (web) clean, eslint on the touched dirs clean, prettier clean, `check-css-tokens` clean (46 files)
  and its node test, `vite build` + `check-bundle-budget.mjs` all ok (the build needed `packages/wasm/dist`, copied
  from the `p29-integ` worktree, ignored by git).

## Not run

- `a track started in Cinema keeps playing in Settings` (the acceptance case, in `player.spec.ts`): it needs a
  signed-in Owner and therefore the API and Postgres; `%TEMP%\sr_pg_pw.txt` is missing. It compiles, and its
  playback half is proven by the public-route case. CI runs it. It also drives the skin picker in Settings.
- Firefox (would not launch here: `spawn UNKNOWN`) and WebKit (not installed).

## Still open / decisions

- The PWA e2e spec (`pwa-update.spec.ts`) has no new browser case: it serves production builds, which do not carry
  the dev-only `__srPlayer` handle. The deferral is covered by Vitest (`engine.test.ts`, `pwa.test.tsx`).
- Crossfade here is a fade-out before the boundary and a fade-in on the next track through the one gain node (a
  dip, not an overlap). True overlap and gapless need the second prefetch `<audio>` of MUS-LIB-001 (music-lib); the
  graph and `crossfadeSec` are the hooks.
- No EQ, playlist, search or favorites UI: those are music-lib's, mounted through `registerLibraryPanel`.
- The dock is one of four corners (a button in every skin and the Settings picker), not free dragging.
- Skin/dock/volume are per browser (`localStorage`), not in the synced account prefs; syncing them would mean
  touching `prefs.ts`/`prefsSync.ts`, which this wave does not own.
- A cross-origin presigned URL is requested with `crossOrigin = 'anonymous'` (Web Audio outputs silence for a
  cross-origin file loaded without CORS), so the bucket must send CORS headers for `GET` with `Range` from the app
  origin, or the track fails to load. Check this when music-lib wires real presigned URLs.

## Owner questions

None blocking. For music-lib: confirm the bucket CORS rule allows `GET` with `Range` from the app origin.
