# Handoff: feat/p12-web-followups

Web and docs items from `docs/dev/answers-followups-p12.md`. No Rust touched.

## Done

| Item | What | Where |
|---|---|---|
| G-18 | Share Target files: manifest is a POST multipart target with `files`; `sw.js` classifies `POST /share` as `share`, stores files in IndexedDB (`sr-share`) and redirects 303 to `/share?files=1`; `SharePage` shows the file count and queues them for the chosen channel; the channel's `Composer` takes them on mount | `public/manifest.webmanifest`, `public/sw.js`, `pwa/shareFiles.ts`, `lounge/queuedFiles.ts`, `pwa/SharePage.tsx`, `lounge/Composer.tsx` |
| G-26 | Library Merge shows only with `PUBLISH_READING` and `DELETE_CONTENT` | `admin/library/LibraryCatalogSection.tsx` |
| G-30 | Follow button on announcement channels (channel picker, `POST /channels/{id}/followers`); "Updates" badge for `public_updates_channel_id` (read from `GET /guilds/{id}/sr/community`) | `lounge/header/FollowButton.tsx`, `useUpdatesChannel.ts`, `ChannelView.tsx` |
| G-31 | `/channels/:guild/:thread` resolves a thread (store or `GET /channels/{id}`) to its parent with the thread in full view; "Open full view" navigates there, "Back to side pane" and Close return | `lounge/threads/useThreadRoute.ts`, `LoungeShell.tsx`, `ThreadPane.tsx` |
| G-35 | Community wizard: step 1 has the two safety switches, Next disabled until both are on, Finish no longer sets them. A switch already on is locked; one turned on here can be turned back off | `settings/server/sections/CommunityWizard.tsx` |
| G-39 | Decline calls `POST /channels/{id}/call/stop-ringing`; overlay already followed CALL_UPDATE/DELETE; local fallback timer is 45 s | `lounge/voice/IncomingCall.tsx`, `voice/calls.ts` |
| G-49 | One update notice: the PwaHost toast yields while the Lounge notices bar is mounted | `pwa/state.ts`, `PwaHost.tsx`, `lounge/shell/Banners.tsx` |
| G-56 | Every Copy ID entry (server, channel, category, message, user in members, DMs, profile, voice menus) only with Developer Mode; Copy Link stays | `useDeveloperMode` in `settings/user/prefs.ts` and the menu hooks; Developer mode description text updated |
| G-17 (web part) | Language picker calls `chooseLanguage`: switches UI, mirrors into `prefs.locale` (settings blob) and sends `PATCH /users/@me {locale}`; boot (`useShellBoot`) adopts `me.locale` on a device with no saved language | `i18n/accountLocale.ts` |
| G-59 (partial) | See open | `library/pdf/` |
| Docs | G-06, G-15, G-24, G-32, G-46, G-62, G-63 done (ADR-0006 addendum and doc 03 rows, watch-party drift band, ADM-GRAPH-003 re-scope, voice spec and LNG-SSET-026, doc 05 activity type 6, REV-CORE-001, moderation spec folded plus doc 07 bits 22 and 23 and the Moderator / Senior Staff values 4201473 / 14401535) | `docs/` |

Also updated inventory notes: XC-PWA-006, LNG-USET-056, LNG-THR-005, LNG-CHN-021 (ticked), LIB-ING-004 (re-scoped and ticked), LIB-RDR-011 and VOX-UI-010 (notes only, still open).

## Still open and why

- **G-02**: left to p12-devplat-be as assigned.
- **G-59**: `pdfjs-dist` is not in `apps/web/package.json` or the lockfile and I did not download it. The integration loads it through `import.meta.glob('/node_modules/pdfjs-dist/build/pdf.min.mjs')`, which matches nothing now (the build is clean) and starts working as soon as the package is added, with no code change. Until then the PDF reader and `renderPdfCover` give a typed `PdfNotInstalledError` and the reader shows "PDF reading is not installed". Not done: submitting the rendered cover (there is no cover endpoint for member submissions; `renderPdfCover(file)` returns the JPEG blob and is ready), image mode, reading progress for PDFs. Tests use `__setPdfjsForTests`.
- LIB-RDR-011 and VOX-UI-010 stay unticked for the reasons above (and no outgoing ringing state).

## Backend dependencies

- G-17: `PATCH /users/@me` must accept `locale` (BCP 47 tags with catalogues). Today the field is ignored; the blob mirror still carries the choice across devices. Boot adoption only fires on a device with no saved language (so a registration default cannot override a German browser that already chose).
- G-30: none. The Updates badge needs `public_updates_channel_id` from `GET /guilds/{id}/sr/community` (any member). It would be cheaper if the guild object carried it.
- G-18: a POST to `/share` only works while the service worker is active; the server has no `/share` POST route (a fresh visit before the worker installs gets the shell or a 405 from the proxy). Fine for installed PWAs.
- G-59: cover submission endpoint (see above).

## Tests run

- Web: `tsc --noEmit`, `eslint .` clean; `prettier --check apps docs` clean; full `vitest run --maxWorkers=2`: 145 files, all pass (channelPerms needs the wasm `dist`, copied from the main checkout for the run; not committed); `vite build` plus `scripts/check-bundle-budget.mjs`: all budgets ok.
- New tests: `pwa/pwa.test.tsx`, `pwa/pwaFeatures.test.tsx` (G-18, G-49), `admin/__tests__/p10.test.tsx` (G-26), `lounge/__tests__/p12FollowUpdates.test.tsx` (G-30), `p12ThreadRoute.test.tsx` (G-31), `settings/server/__tests__/content.test.tsx` (G-35), `lounge/voice/__tests__/incomingCall.test.tsx` (G-39), `p11ShellRail` / `shellFlows` / `callUi` / `p11ShellProfile` (G-56), `i18n/accountLocale.test.ts` (G-17), `library/__tests__/pdf.test.tsx` (G-59).

## Decisions

- G-35: the safety switches write the guild immediately when toggled (an explicit admin action), so Finish never changes verification level or content filter.
- G-56: the earlier tests that expected Copy ID without Developer Mode were changed to turn it on.
- G-17: the pseudo-locales never go to the account; the "Language & region" formatting select is untouched.
- New catalog fragment `i18n/messages/en/p12-web-followups-1.ts`; I also edited the Developer mode description in `usrset-5.ts` and the wizard strings in `p10-sset-4.ts` (removed `passed` and `willFix`).

## Open owner questions

- G-59: approving `pdfjs-dist` as an npm dependency (a download), and whether the worker should run as a real worker (`worker-src 'self' blob:` already allows it).
