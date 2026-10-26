# feat/p26-lounge-followups (PAR-FU-001, PAR-FU-002)

Resume here: both items are built, tested and ticked (U-02, U-03). Nothing is left open; the sections below are the
record and the owner questions.

## Done

- **PAR-FU-001 (U-02), View as Role filters the member list and the thread lists.** Client-side only, through the same
  `sr-permissions` WASM (`viewAs.ts` `canViewAs`). The preview session (`viewAsMode.tsx`) gains `canSeeChannel`
  (built in `ViewAsRole.tsx` from the engine, the roles and the channel overwrites) and two hooks:
  `useViewAsMembers` (member list: nobody when the previewed roles cannot see the open channel, otherwise only the
  members who can view it with their own roles; the owner always stays) and `useViewAsThreadParent` (threads of channels
  the previewed roles cannot read are dropped from the Threads popover and the nested thread list). Applied in
  `MemberList.tsx`, `ThreadsButton.tsx`, `NestedThreads.tsx`. No server state.
- **PAR-FU-002 (U-03), the DM filter level "All" screens friends.** `SocialService::friend_dm_spam_flag` (called before the
  message is stored through the new `MessageHooks::extra_flags`, default none) returns true when a recipient's filter is
  `All`, the author is a mutual friend and the content trips `dm_spam_reason`. The message then carries the new stored,
  server-set message flag `LIKELY_SPAM = 1 << 29` (not in the client-settable flag set). A friend's conversation is never
  moved to Message Requests; the client (`liveMap.ts` -> `Message.likelySpam` -> `LikelySpamGate` in
  `lounge/msg/LikelySpamNotice.tsx`, used by `MessageItem.tsx`) shows it behind a notice with Show / Hide message. No
  migration (the flags column already exists).

## Tests run (second round: P26-LNG-1)

sr-chat lib 332 passed; sr-api `social::` 54 passed; sr-gateway `likely_spam` passed; sr-db `p24_parity` 3 passed (scratch DB,
dropped); web `p26LikelySpam` 4 passed.

## Tests run (first round)

- `cargo test -p sr-chat --lib`: 331 passed (includes `all_level_flags_a_friends_spam_and_non_friends_level_does_not`:
  both levels, Off, the other direction, strangers).
- `cargo test -p sr-db --test p24_parity -- --include-ignored --test-threads=1` on scratch DB `sr_p26_lounge_followups`:
  2 passed (new: `the_all_level_flags_a_friends_spammy_message_without_filing_a_request`).
- `cargo fmt --check` and `cargo clippy -p sr-chat -p sr-db --all-targets -D warnings`: clean.
- Playwright (chromium, own ports 8082/5175, scratch DB, stopped by PID afterwards): `view-as-role.spec.ts` 2 passed,
  including the new member-list test.
- Web: `viewAsMode.test.tsx` (5), `p26LikelySpam.test.tsx` (3), `messages.test.tsx`, plus i18n, threads, live and
  settings/server suites (241 passed); typecheck and eslint clean; prettier applied.

## Decisions

Decided: P26-LNG-1 (docs/01-decisions-log.md; Fable, owner-delegated). Built from it:

- The author never receives `LIKELY_SPAM`: `message_flags::for_viewer` in `render_many` and the author's REST response in
  `render_for_create` (the broadcast payload keeps it), and a mask in `Hub::deliver_filtered` for the author's sessions.
  Tests: `the_author_never_receives_the_likely_spam_flag` in sr-api social tests and in sr-gateway tests.
- A content edit re-screens (`ChatService::edit_message`, flags-only edits keep the bit). Tests:
  `editing_a_message_re_screens_its_server_flags` (sr-chat), a Postgres edit test in `p24_parity.rs`, and the web case
  "an update without the bit un-flags the message".
- The decisions log rows P24-PAR-1 and P24-PAR-5 carry the amendment; member-list semantics confirmed as built.
