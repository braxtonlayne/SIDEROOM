# Wave 50 answers, builder A (fix/p50-ans-a)

Builds items 1 to 6, 8, 9 and 10 of Fable's build list (`SIDEROOM_wt/p50-fable-answers.md`, D-528 to D-538). Items 7
(section bar) and 11 (drawer theme root) belong to the other builder; item 12 (Opus for soundboard sounds, D-534) is
deferred to the next round by decision.

## Done (commits on fix/p50-ans-a, newest last)

| Item | Inventory ID | What |
|---|---|---|
| 1 | MUS-JB-003 | The listening room card's `canManage` is the channel-level answer: `loungeChannel.permissions.manageChannels ?? viewer.can('MANAGE_CHANNELS')` (`settings/server/sections/Channels.tsx`, via the Lounge data hook); test in `settings/server/__tests__/channels.test.tsx` (an overwrite that denies it shows the sentence and disables the card). |
| 2 | PRF-PRO-001 | `GuestbookNoteEvidence`: Remove asks for a reason in the `ReasonDialog` the hold release uses; after the press the button becomes "Note removed" (success) or "This note was already removed" (404). Keys in `p50-ans-a-1.ts`; `admin/__tests__/guestbookNoteEvidence.test.tsx`. |
| 3 | MUS-SKN-002 | `player/store.ts` `clear()` resets `rate`, `pitchFollowsRate`, `pan` (D-530); test in `rate-pan-sleep.test.ts`. |
| 4 | MUS-SKN-002 | Circuit and Valve read the real rate: the fader rests at its end stop, the readout shows "+25.0%" / "1.50x" (D-531); one test each in `backdrop-skins.test.tsx`. |
| 5 | PRF-UNF-001 | `PlayHere` takes a `partyTitle` (only `e.title`); `StartPartyDialog`'s `external.label` is optional and the request omits `title` without it; the description then reads "Start a party for this video. Its name is taken from the video if you leave it blank." (`p50-unfurl-1.ts`). `embedsPlay.test.tsx`; the e2e PeerTube case already creates the party without a title and asserts `Stub tone` (comment added). |
| 6 | MUS-JB-003 | The empty-list hint ends in a link-styled button "Open the music library" (`openPlayerLibrary()`); `.ss-link-inline` in `server-settings.css`; test in `lounge/rooms/__tests__/settings.test.tsx`. |
| 8 | ACT-SDK-001 | `ShelfPicker` notes "This shelf has more than 3,000 files; only the first 3,000 are listed." when the 15th page still has a `next_cursor` (count from `MAX_PAGES * PAGE_SIZE`); `shelfPicker.test.tsx`. Done in wave 50. |
| 9 | UTL-AUD-001 | `AudioTool` start handle: at the limit, dragging it forward carries the end along (stops at the file's end, where the part shrinks); test with a 4 s file and a 1 s limit. |
| 10 | PRF-PRO-001 | `sr_moderation::Moderation::remove_guestbook_note` runs the reason through `rules::reason` after the permission check and before the transaction (so before any log row); missing or blank = 400 `ReasonRequired`. Route docs/OpenAPI mark `reason` required and add the 400. `moderation/api.ts removeGuestbookNote(noteId, reason)`; `crates/sr-moderation/tests/guestbook_note.rs` (no reason: 400, no log row, note still there); e2e `moderation-gate.spec.ts` sends `?reason=` and asserts the 400 without. |

Decisions D-528 to D-538 are in `docs/01-decisions-log.md` after D-527; the five wave-50 feature handoffs have
an "Answered: D-5xx" line at the top of their owner-question sections.

## Not done

- Items 7, 11 (other builder), 12 (D-534, next round).

RESULTS_PLACEHOLDER
