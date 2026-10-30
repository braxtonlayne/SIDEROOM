# Handoff: feat/p32-utl-shell (wave 32, UTL-SHL-001)

Branch `feat/p32-utl-shell`, base `integration/phase32`. Web only (no Rust, no migration). Item: **UTL-SHL-001, the
drawer, pop-outs and pins.**

## Done

- **Tool registry** `apps/web/src/tools/registry.ts` (published first, in its own commit): `ToolEntry`
  (`id`, `nameKey`, `descriptionKey`, `icon`, `group`, `aliases`, `size`, `floatable`, lazy `load`), `ToolProps`
  (`host: 'drawer' | 'gadget' | 'rail'`, `onClose`), `TOOLS` (append your entry at the end), `registryProblems`,
  `getTool`. The only shipped tool is the **Drawer guide** (`tools/drawer/GuideTool.tsx`), so the drawer is never empty
  and e2e has something to open. How to add a tool is written at the top of the file; the registry test checks ids and
  that every `tools.<id>.name` / `.description` key exists in the English catalog.
- **Drawer** (`tools/drawer/Drawer.tsx`): Mod+K opens it on **Go to**, which is the former quick switcher, same
  ranking, prefixes (`@ # ! *`), recents and navigation (`lounge/QuickSwitcher.tsx` now exports `useGotoResults` and
  `GotoList`; the modal and its input moved into the drawer). A leading `>` (or the **Tools** tab) lists tools; Enter
  opens a tool inside the drawer, Shift+Enter pops it out; the tool view has **Pop out** and **Pin to the rail**.
  Tabs use `useRovingTabindex`; the dialog is the shared `Modal` (focus trap, Escape, focus return). Tool filter is
  `tools/drawer/toolSearch.ts` (prefix, alias, word, one-letter typo): a pure stand-in that UTL-SRC-001's wasm search
  replaces behind the single `searchTools` function.
- **Gadgets** (`tools/gadgets/*`): floating non-modal windows, draggable (pointer and arrow keys), resizable (corner grip
  and Shift+arrows), raised on press, max **6** (`MAX_GADGETS`; the seventh pop-out is refused with a note), one per
  tool, clamped to the viewport (also on window resize). Positions are measured from the inline-start / block-start
  corner so they mirror in RTL. Pure model in `layout.ts`; zustand store in `store.ts`.
- **Pin rail** (`tools/rail/PinRail.tsx`): slim vertical toolbar at the inline-end edge (roving tabindex), a button opens
  the tool in a docked panel with pop out / unpin / close. Up to 12 pins. Renders nothing while nothing is pinned.
- **Persistence per account in the synced settings** (`tools/gadgets/sync.ts`): the layout is the `tool_layout` object in
  the `user_settings` blob (same blob and PATCH as the preferences; the server keeps unknown keys, no migration), with a
  per-account localStorage copy for first paint. Account copy wins; a device-only layout is pushed once when the
  account has none; writes debounced 400 ms; offline keeps the local copy.
- **Survives navigation like the player**: `tools/drawer/DrawerGate.tsx` (lazy chunk, rendered by `router.tsx`
  `RootLayout` on non-public paths only, same pattern as `PlayerGate`) owns Mod+K and mounts `DrawerHost` the first time
  the drawer is asked for, or when a layout is remembered (localStorage hint or the settings blob). The host chunk holds
  the drawer, the gadget layer, the rail, every tool and `tools.css`. A move onto a public page closes the drawer.
  **Escape closes the drawer only** (gadgets are outside the overlay stack).
- **D-385**: nothing under `tools/` is imported by a public surface; the gate imports only the path rule, the shortcut
  table, prefs, the store and the open flag, and is itself a lazy chunk. The rail music button from wave 30b in
  `GuildRail.tsx` is untouched.
- **Shortcuts**: `lounge/shortcuts.ts` gains `DRAWER_SHORTCUTS` / `isDrawerShortcut`; the id stays `quickSwitcher` so a
  member's saved rebind keeps working. Its description is now "Open the drawer (go to a channel or person, or find a
  tool)" (`tools.shortcuts.drawer`; the old `lounge.shortcuts.quickSwitcher` key was removed). The Lounge shell ignores
  the key (like the player keys), so Mod+K now works on every signed-in route, not only the Lounge.
- i18n fragment `apps/web/src/i18n/messages/en/p32-utl-shell-1.ts` (keys `tools.*`). CSS `tools/tools.css`: tokens and
  logical properties only; reduced motion respected (transitions only under `no-preference`).

## Files outside the Split row that I had to touch (all small)

- `lounge/LoungeShell.tsx`: dropped the quick-switcher state and render, ignores the drawer key, the Home sidebar's "Find
  or start a conversation" button calls `openDrawer('goto')`.
- `i18n/messages/en/p9-1.ts`: removed the one replaced key.
- Tests that named the old text or the old modal: `lounge/__tests__/shell.test.tsx` (Ctrl+K case and the shortcut sheet
  text), `settings/user/__tests__/prefsSections.test.tsx` (the keybind row is now "Open the drawer ...").

## Tests run (and results)

- `tsc --noEmit` (apps/web): clean.
- vitest `--maxWorkers=2`: `src/tools` (layout 14, registry/search 9, sync 6, drawer integration 7 including axe on the
  open drawer, a gadget and the rail), `lounge/__tests__/logic.test.ts` (22, includes the new Mod+K precedence cases),
  `lounge/__tests__/shell.test.tsx` (24), `settings/user/__tests__/prefsSections.test.tsx`, `src/i18n` (catalog, pseudo
  locale, first paint): green, all green apart from 3 unrelated 5 s timeouts in `prefsSections.test.tsx` "Appearance extras" under machine load (they fail the same way without the drawer; not touched by this row).
- Playwright `e2e/tests/drawer.spec.ts`, **Chromium only**, 4 cases pass (open with Mod+K, pop out, navigate in app,
  Escape keeps the gadget, keyboard move survives a reload, pin and unpin on the rail, axe on dialog / gadgets / rail, a
  360 px phone viewport with reduced motion). Run with a throwaway Vite dev server on **5177** and a tiny stub answering
  `GET /api/v1/users/@me` on **8084** (the dev gate in `vite/publicDev.ts` asks `SR_DEV_API`), with a temporary
  config outside the repo's committed ones that skipped `global-setup`; all removed afterwards. 8080/5173 were never
  touched. Firefox and WebKit are left for CI (the spec uses no engine-specific API).

## Still open / limits

- **Go to outside the Lounge**: the list is built from the store the Lounge fills (servers, channels, DMs, people). On a
  non-Lounge section after a fresh reload (Settings, Cinema) the store may be empty until the Lounge has been visited,
  so Go to is short there. Gadgets and tools are unaffected. Fixing it means loading guilds/channels outside the Lounge
  (store boot), which is not in this row. In mock mode (`VITE_SR_MOCK=1`) the drawer likewise sees no mock data (the mock
  provider wraps only the Lounge); production is unaffected.
- **Rail placement** overlays the inline-end edge at mid-height; with the Lounge member list open it sits over the list's
  scrollbar. A layout slot inside the Lounge shell would be cleaner but is the Lounge shell's, not this row's.
- The search is the small TS stand-in until UTL-SRC-001 lands (then `toolSearch.ts` calls the wasm engine); stage 3 phrase
  search and "Did you mean" arrive with it.
- The drawer has no separate `>`-less shortcut to open straight on Tools (Mod+Shift+K is the Firefox console); the
  beta may ask for one (a one-line addition in `shortcuts.ts`).

## Decisions

- One gadget per tool (opening an open tool raises it). Max 6 gadgets, 12 pins.
- Layout lives in the settings blob key `tool_layout`, not in `UserPrefs` (keeps `prefs.ts` out of this row; no sanitiser
  clash). Entries for unknown tool ids are kept in storage and ignored on screen, so a build without a tool does not erase
  the layout.
- The shortcut id `quickSwitcher` was kept for saved rebinds.

## For the other tool agents

Append one object to `TOOLS` in `apps/web/src/tools/registry.ts`, own a default-exported component taking `ToolProps`,
own your `tools.<id>.*` catalog keys in your own fragment. The same component renders in the drawer, a gadget and the
rail panel (`host` tells which); keep state in a store if it must survive switching host.

## CHANGELOG lines (for the integrator)

- The Ctrl/Cmd+K quick switcher is now the utility drawer: its first tab, Go to, works as before; type `>` for tools. Tools
  can be popped out into floating windows (up to six, draggable and resizable, remembered per account) or pinned to a side
  rail, and they stay while you move around the app.

## Owner questions

1. Should the drawer's key stay Mod+K only, or also get a second binding that opens on Tools (roadmap risk note)?
2. Go to outside the Lounge: OK to leave short until the store boots earlier, or should the store boot (guilds, channels,
   DMs) move into the signed-in root? That touches the gateway connection, which is outside this wave.

## Final gate run

`tsc --noEmit`, eslint (tools, QuickSwitcher, shortcuts, LoungeShell, router), prettier --check, `check-css-tokens` (50 files),
`build-web` (public and app), `check-bundle-budget`, `check-entrance-isolation`: all pass. Drawer vitest 7/7 (needs the long
timeouts set in the file when the machine is loaded).
