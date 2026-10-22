# Handoff: feat/p18-mod-restore (MOD-CASE-009 remainder, N-21)

Base `feat/p18-mod-cases`. Migration range used: **1130** only (`1130_attachment_removal.sql`).

## Done

- **Restore a deleted message from a case.**
  - `sr-chat`: `message_rules::validate_restore` / `RestorePlan`, `ChatService::restore_message`, `MessageRepository::apply_restore` (Postgres, both memory repos). The acting person needs `MANAGE_MESSAGES` in the channel (D-213 stays: it is the person's own server bit, re-checked by the service). The message must still exist and not be due for purge (D-121: a purged message is gone; the purge job deletes the stored files first, so a restore racing it would leave a message without attachments). A hold stays on the restored message (D-122); `deleted_at` and `purge_after` are cleared; a thread counts it again. A `staff_removed` message is restored only by a caller with a platform permission (`ChatCaseActions` passes `!by.permissions.is_empty()`); a server moderator is refused.
  - Gateway: new `DomainEvent::MessageRestored`, dispatched by the hub as `MESSAGE_CREATE` with the full object (clients dropped the message on `MESSAGE_DELETE`, and the web reducer ignores a `MESSAGE_UPDATE` for a message it does not hold). Separate from `MessageSent` on purpose: pushes, sticky/auto-thread/quote modules and bots that react to new messages do not fire; search re-indexes it (`search_jobs::jobs_for_event`).
  - Audit: guild audit action **214** (`ModerationMessageRestored`), reason `case:<id>`, target = author, a `message_id` change. Web text for 214 and 215 in `settings/server/auditText.ts`.
- **Remove one attachment from a case.**
  - `PerformRequest.attachment_id` (required for the action; `ActionBody.attachment_id` on `POST /moderation/cases/{id}/actions`), `CaseActions::remove_attachment(by, case, channel, message, attachment)`. New `ActionError::Conflict(String)` (a 409 with the text) for "not deleted", "about to be purged", "attachment not on the message".
  - `MessageStore::detach_attachment` / `forget_removed_attachment`. The attachment leaves the message at once (`message_id` cleared). Not held: object deleted from storage (a missing one counts as gone), then the row, then `MESSAGE_UPDATE` if the message is visible. Held (its own hold, or the message's, which it joins so releasing the hold frees it): object and row are kept for the evidence (D-122), marked by new columns `removed_at`, `removed_by`, `removed_case_id`, `removed_from_message_id`. After release, `messages.purge_deleted` also runs `purge_removed_attachments` (object, then row). `MANAGE_MESSAGES` in the channel is required (`ChatService::authorize_moderation`).
  - Audit: action **215** (`ModerationAttachmentRemoved`), reason `case:<id>`, changes name the message, the attachment and `held`.
- **Timeout carries `case:<id>`.** `GuildBackend::edit_member_with_reason` (default method = plain edit; `ChatBackend` writes the reason on the `MemberUpdate` row). `ChatCaseActions::timeout_member` uses it. The Postgres test asserts the reason.
- **Web**: the "Take action" dialog offers "Restore the reported message" and "Remove one attachment from the reported message" (the attachment is picked from the evidence snapshot's target message; the option appears only when the snapshot shows one). No 409 for them any more. Strings in `i18n/messages/en/p18-mod-restore-1.ts`.
- Docs: spec section 4.4 and section 15 (restore, removal, timeout reference), inventory `MOD-CASE-009` note, N-21 note in `answers-followups-p16.md`. `MOD-CASE-009` was already ticked; it is now complete.

## Tests

- `sr-server` (Postgres): `a_deleted_message_is_restored_from_a_case` (not-deleted is 409, hold stays, event, audit reason, no-permission refused, due-for-purge refused, `staff_removed` platform rule), `one_attachment_is_removed_from_a_case` (id required, foreign attachment 409, object and row deleted, `MESSAGE_UPDATE` without the attachment, held keeps object and row, released then swept), and the existing timeout test now checks `case:<id>`.
- `sr-moderation` (Postgres): recording fake extended (restore, remove with id, Conflict mapping, six performed actions on the timeline).
- `sr-api` route tests: `message_actions_from_a_case_reach_the_services` (message kinds over HTTP; missing attachment 400, bad id 404, outsider 404) next to the user-kind test.
- Web: `admin/__tests__/moderation.test.tsx` (options listed, restore and remove send the right body).
- Gates: see the final report of the agent for the exact commands and results.

## Still open / notes

- A restored message that mentions a member raises their mention badge again (clients treat `MESSAGE_CREATE` as new). Cosmetic; a flag on the payload could suppress it. *Answered 2026-10-22 (delegated): D-338, `restored: true` on the dispatched payload; follow-up N-41. Resolved at integration (`feat-p18-integration.md`).*
- The case page offers removal only for attachments the evidence snapshot shows (taken when the report was filed). A staff-opened message case has no snapshot and no channel id, so it offers neither (it never could act on the message, same as before).
- Attachments deleted with their channel (FK cascade) never lose their stored object; that is the existing behaviour for all attachments, not new.
- No browser run (brief forbids servers and Playwright).

## Owner questions

Answered 2026-10-22 (delegated): D-337 and D-338 in `docs/01-decisions-log.md`, P18-RST-1 and P18-RST-2 in `docs/open-question-answers.md`, follow-ups N-40 and N-41 in `docs/dev/answers-followups-p16.md`.

1. Should a server moderator be able to restore a message that a platform member removed (`staff_removed`)? Default taken: no (platform staff only). Nothing in the code sets `staff_removed` yet, so this is a guard for the takedown work. *Answer: no, the default stands (D-337); the takedown register's uphold (`feat/p18-mod-tkd`) now sets `staff_removed`, and a message a takedown still holds is not restorable from a case at all, only through the register (N-40).*
