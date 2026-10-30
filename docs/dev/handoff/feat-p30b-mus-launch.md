# feat/p30b-mus-launch (MUS-PLY-002)

## Done
- Rail button (music note, after Discover; `apps/web/src/lounge/GuildRail.tsx`), label and tooltip `lounge.guildRail.music`
  ("Music library", fragment `p30b-mus-launch-1.ts`), a real `<button>` inside the rail's roving nav.
  The shared icon set has no music note, so the rail draws a local inline SVG in the same stroke style
  (`packages/ui` is not ours this wave).
- `player/presence.ts` gained `openPlayerLibrary()` / `closePlayerLibrary()` and hooks (`usePlayerLibraryOpen`,
  `usePlayerLibraryNonce`). The rail imports only this tiny module, never the store (D-385; bundle `player` row green).
- `PlayerGate` mounts the host when queued OR the library was asked for (app routes only). `PlayerHost` shows the dock
  when queued or open and switches to the library skin on each open; `LibrarySkin` returns to its Library tab on each
  open. The Close button now also ends the library view. A public page or sign-out (`forgetPlayer`) withdraws the request.

## Decisions
- The button switches the member's skin to "Full Library Manager" (and that choice persists like any skin change),
  because the Library tab exists only in that skin. Clicking it again while open re-selects the Library tab.
- Closing with an empty queue hides the player entirely; removing the last queued track while the library is open keeps it open.
- Nonce effects act only while the request is open, so a stale nonce never forces the skin after a remount.

## Tests run
- vitest: `src/player` (incl. new `launch.test.tsx`), `src/lounge/__tests__/p30bMusLaunch.test.tsx`, `src/music`,
  `src/lounge`, `src/i18n`, `src/__tests__`: 874 passed; 2 files (`channelPerms`, `p24ProfileCard`) fail to load only
  because this worktree has no wasm `theme` build for them (environment, not the change).
- web `tsc --noEmit`, eslint (touched files), prettier --check: clean. `vite build` + `check-bundle-budget.mjs`: all ok
  (entrance 248.3 KB, `player` row ok). The worktree's untracked `packages/wasm/dist` was copied from other local checkouts (no cargo).

## Open
- Owner: a music-note icon in `@sideroom/ui` would replace the local SVG. No Playwright run (not allowed this wave).
