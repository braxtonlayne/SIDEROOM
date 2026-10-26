# feat/p26-lounge-followups (PAR-FU-001, PAR-FU-002)

Resume here: both items are built and ticked (U-02, U-03). The only open point is the Playwright run of
`view-as-role.spec.ts` (see "Tests run"); everything else is done.

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

## Tests run

- `cargo test -p sr-chat --lib`: 331 passed (includes `all_level_flags_a_friends_spam_and_non_friends_level_does_not`:
  both levels, Off, the other direction, strangers).
- `cargo test -p sr-db --test p24_parity -- --include-ignored --test-threads=1` on scratch DB `sr_p26_lounge_followups`:
  2 passed (new: `the_all_level_flags_a_friends_spammy_message_without_filing_a_request`).
- `cargo fmt --check` and `cargo clippy -p sr-chat -p sr-db --all-targets -D warnings`: clean.
- Web: `viewAsMode.test.tsx` (5), `p26LikelySpam.test.tsx` (3), `messages.test.tsx`, plus i18n, threads, live and
  settings/server suites (241 passed); typecheck and eslint clean; prettier applied.

## Decisions and owner questions

Open owner questions:
1. P24-PAR-1 and P24-PAR-5 in the decisions log still say the filter never reads friends and that the preview "does not
   narrow the message or member views". U-02 and U-03 (owner backlog) ask for exactly that, so I built it; the two log
   rows now read stale. Should they be amended (I did not edit the decisions log)?
2. The `LIKELY_SPAM` flag is on the message object everyone in the DM receives, so the sender's client could read it
   (the notice is never rendered for the author, but the bit is visible in the API). Conservative default kept; hiding it
   from the author would need a per-viewer message builder.
3. The flag is set at send time and stays after an edit (an edited friend message is not re-screened). Re-screening on
   edit was left out as out of scope.
4. Member-list semantics under preview: members are filtered by whether their own roles can view the open channel
   (Discord's real member-list rule), and the list is empty when the previewed roles cannot see the channel.
