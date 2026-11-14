# Wave 69 notify handoff (SEC-AUD-227..231; D-711, D-712, D-716, D-717)

Branch `fix/p69-notify`. All five items are done. No migration, no route or response shape change (`openapi.json` and the generated client are untouched; nothing for the commander to regenerate).

## Done (commits, in order)
1. SEC-AUD-231 one presence announcement at connect (`sr-gateway`).
2. SEC-AUD-227 notifications are plain text: push (`sr-server`) and desktop (web).
3. SEC-AUD-228 nothing from a blocked or ignored account.
4. SEC-AUD-229 the reaction notification says which message.
5. SEC-AUD-230 the smartphone glyph.
6. Docs, e2e/shots spec, this handoff.

## Not done
Nothing dropped. No fragment `p69-notify-1.ts` was needed: no new string (the spoiler word is the existing `lounge.preview.spoiler`; the empty body needs no text).

## What each body is, for `psst ||the ending|| <@owner>`
- Push before: `psst ||the ending|| <@123456789012345678>`. After: `psst [spoiler] @Maya` (display name, else username).
- Desktop before: `msg.content.slice(0, 140)` (same raw text). After, recorded on the real stack by the e2e: `{"title":"bob_… mentioned you","tag":"ch-…","body":"psst [spoiler] @owner"}`.
- Reaction notification, recorded: `{"title":"bob_… reacted 👍 to your message","tag":"react-<id>","body":"my older message that gets a reaction"}` (the Owner had never opened the channel).

## Item 1: plain text
- Server (`push_notify.rs`): `payload` is unchanged and pure; it now receives plain text made by `plain_text(content, resolver)` (parse, `to_plain_text_with`, white space folded to one space). `load_names(src: &dyn NameSource, guild, content)` reads names: one query per kind, only for kinds present, at most 20 ids per kind, DM = users only, a failed query leaves that kind unresolved (default `@user`/`@role`/`#channel`), push still sent. Names are read once per message in `deliver`. `PushNotifier` implements `NameSource` (`social.summaries` for users, `list_roles` and `list_guild_channels` filtered to the ids, so one query each). Added path dependency `sr-markdown` to `crates/sr-server/Cargo.toml` (`Cargo.lock` gains one line; a workspace edge).
- Channel names: sent only to a recipient who passes `access(..).require_view()` for that channel (per recipient, only when the text has a channel mention).
- Finding: `sr_markdown::to_plain_text_with` writes no separator between a paragraph and a list that follows it (`two\n- a` gives `twoa`), on the server and in the web (same parser). Not changed (sr-markdown is not mine); both of my functions put a space before each top-level list. Commander: consider fixing it in `sr-markdown` (and `singleLineText` for the thread/request rows, which have the same gap for a list after text).
- Other pushes in the file: the chapter push (`Chapter N: title`, series title) and, in `events_jobs.rs`, the event start push (title = event name, body = "{server} · starting now") carry no user-written markdown; not changed. `jukebox_jobs.rs` builds no push.
- Web: new `lounge/shell/notifyText.ts` (`notificationText`, `storeResolvers`, `reactionBody`); engine not loaded gives `''`. Resolvers are made from `useStore.getState()` (the MessageList builder is a hook), roles and channels only of the message's server.
- Cost of a name lookup: none without mentions; with mentions at most 1 user query, 1 role-list query, 1 channel-list query per message (not per recipient).
- Also edited outside the file list (required by the new `NotifyContext.hidden` field): `lounge/shell/DmToasts.tsx` (passes `hiddenAuthorsNow()`, so a blocked DM sender also raises no toast) and `lounge/__tests__/p18restored.test.tsx` (context literal).

## Item 2: blocked and ignored
- Confirmed first: `desktopDecision` returned a kind for a blocked author (the new vitest cases fail on the base: 8 failures in `desktopNotify.test.ts` and `p69Notify.test.tsx`, run with the old `desktopNotify.ts`).
- `NotifyContext.hidden`; `ReactionContext.blocked` removed; sets from `hiddenAuthorsNow()` (a non-hook twin in `hiddenAuthors.ts`, the hook behaves as before). Sound follows the decision.
- A blocked account's mention still counts in the channel's unread and mention badges (the server's counts are untouched) and the message list still shows "blocked message". Owner question below.

## Item 3: the reaction's text
- Reads through `apiFetch` `GET /channels/{c}/messages/{m}` (the helper `SystemMessage` and `MessageLinkCard` use; no new wrapper), `retries: 0`, raced against 2 s, module cache of 20 (oldest out), shared in-flight read, error remembered as empty, nothing written to the store. A late answer is cached for the next reaction.
- Encrypted conversations: none found. Chat messages are plain on the server; E2EE here is voice media and notes, so the route's `content` is always readable. The empty body happens for: a read error, a late read, an empty message, the parser not loaded (not cached in that case).
- Request counts are tested: loaded (0), unloaded (1), two in flight (1), third later (0), 404 (1 ever), 21st evicts the first, and 0 requests for own reaction, switch off, notifications off or permission denied, ignored reactor.

## Item 4: smartphone
- `packages/ui/src/icons/paths.ts`: `smartphone` (rounded upright rectangle 12 x 20 on the 24 grid, a 4-unit home line near the bottom), 84 bytes. `phoneStatus.tsx` uses it; `phone` stays for calls and the incoming-call button. `ICON_NAMES`-driven icon test and story pick it up automatically.
- Shots (`visual-sweep-shots/p69-notify`): member list, popout, friends, DM list, DM header, own avatar, glyph close-up: desktop and phone, Editorial, Lounge Light, Lounge Dark. Looked at: popout and friends show a clear upright phone with a home line; at 12 px (member list) it reads as a small phone, not a battery. Not done: close-ups at device scale 2 (the close-ups are at scale 1); the phone-width member list shot does not show the member pane (my swipe helper, copied from wave 68, does not reach it), the popout, friends, DM and self shots at phone width are fine.
- First paint (`app-*.js`, gzip): wave 68 215,491 bytes; now 215,594 (+103), room 40,406. Bundle, entrance isolation and staff-delivery checks pass after `pnpm --filter @sideroom/web build`.

## Item 5: one announcement
- Confirmed on the base: `a_phone_connect_is_announced_once` and `an_identify_with_a_presence_is_announced_once` fail (two updates: `web` then `mobile` / `dnd`).
- `create_session` now takes `platform` and the IDENTIFY `presence` and records them (new `record_session_presence`) before the session is inserted, then announces once: `refresh_presence(user, force)` with `force` only when IDENTIFY carried a presence (as op 3 forces). `conn.rs` no longer calls `set_session_platform` (removed, unused) nor announces again. `resync.rs` builds sessions itself and is untouched. RESUME untouched.
- A second session that changes nothing others see announces nothing (the non-forced path compares against the last announcement; test `a_second_session_that_changes_nothing_announces_nothing` passes, also on the base). Note: an IDENTIFY that carries a presence still forces an announcement, as before.
- READY carries only the session list (`session_id`, `status`, `active`), no `client_status`, so nothing to fix there. `#[allow(clippy::too_many_arguments)]` on `create_session` (8 parameters).

## Tests
- Rust (inside the lock): `cargo test -p sr-server --lib -- push_notify` 12 pass (`a_spoiler_is_not_in_the_push`, `a_mention_is_a_name_in_the_push`, `line_breaks_and_lists_are_one_line`, `a_message_without_mentions_costs_no_name_query`, `at_most_twenty_ids_of_a_kind_are_named`, `a_failed_name_lookup_still_pushes`, `a_message_that_is_only_markup_falls_back_to_the_attachment_text`, existing length/attachment cases). `cargo test -p sr-gateway --lib` the five presence tests pass. `cargo clippy -p sr-gateway -p sr-server --all-targets -- -D warnings` clean. `rustfmt` run on my four Rust files.
- Watched fail without the change: the gateway confirm tests (base `hub.rs`/`conn.rs`: two of the three new tests failed with the `web` then `mobile` pair, quoted above); the web `desktopNotify.test.ts` hidden-author cases and `p69Notify.test.tsx` (13 failures with the old `desktopNotify.ts`). The Rust push tests were not run on a base: `plain_text`/`load_names` do not exist there, and the old `payload` put `content` into `body` verbatim by construction (the first run of `line_breaks_and_lists_are_one_line` failed and found the list gap above).
- Vitest, single files: `notifyText.test.ts` (13), `p69Notify.test.tsx` (8), `desktopNotify.test.ts` (13), `p68ReactionNotify.test.tsx` (3, now injects the test markdown engine), `p68PhoneGlyph.test.tsx` (1, now asserts `smartphone`). `tsc --noEmit` clean, eslint and prettier clean on my files. The `packages/ui` icon test was not run separately (it iterates `ICON_NAMES`; the build passed).
- e2e (local only, `SR_P69_NOTIFY=1`, `e2e/tests/p69-notify-shots.spec.ts`): 2 passed on the real stack: the glyph test, and a second account sending `psst ||the ending|| <@owner>` (bodies recorded above), a reaction on an unloaded older message, then a block: no third notification.

## Docs updated
`docs/05-api-and-gateway.md` (Web Push body paragraph after the keywords; presence at IDENTIFY and the glyph), `docs/specs/lounge-chat.md` section 10 (plain text, blocked/ignored, reaction text, smartphone), `docs/04-data-model.md` line 148 (what ignoring hides). Links and prettier checked. The "two updates in a row" sentence was only in wave 68's handoff, not in a doc.

## Changelog sentences (for ops)
- A notification no longer shows a spoiler's text and names people instead of showing ids, in the app and in the push to a closed app.
- No notification or sound in the open app from someone you blocked or ignore.
- A reaction notification shows the message that was reacted to.
- Someone on their phone shows a smartphone instead of a handset.
- Others see one presence update when you connect, not two.

## Dead keys
None.

## Proposed inventory rows (after SEC-AUD-224; the commander numbers them 227..231)
- SEC-AUD-227: push and desktop notifications show a message as plain text (spoiler hidden, mentions named); closes the spoiler leak on lock screens. Tests: push_notify tests, notifyText/p69Notify vitest, e2e body.
- SEC-AUD-228: the open app raises no notification or sound for a blocked or ignored account's messages or reactions (D-717).
- SEC-AUD-229: the reaction notification shows the message text, read once and cached (D-712).
- SEC-AUD-230: `smartphone` icon for the phone-only glyph (D-711).
- SEC-AUD-231: one `PRESENCE_UPDATE` at connect.

## Proposed threat-model rows
- The push body was a place a spoiler leaked (lock screen); now plain text, `[spoiler]` stands in. Residual: a spoiler containing a mention still triggers the name lookup (the name is not sent).
- Single-message read for the reaction text: it is the viewer's own message in a channel they could read; `GET /channels/{c}/messages/{m}` checks channel access, so a 403/404 gives an empty body.
- Names in a push: user names are those of users mentioned in the message itself; role names of the same server; channel names only to a recipient who can view that channel.

## Owner questions (to the Fable agent)
1. Should a blocked account's mention still count as a mention badge (it does now; the server decides)? Discord also still shows a mention badge for a blocked user's mention in a shared server but sends no notification; the default kept here follows that.
2. Should `sr-markdown`'s plain-text function put a separator before a list that follows text (a small fix in the shared crate)?

## Clean-up
Dropped `sr_p69_notify_ui` (and `sr_p69_notify`, never created); deleted `p69-notify-target` and `p69-notify-bin`; the stack (sr-server 8472, Vite 5472) is stopped; the heavy lock is released.
