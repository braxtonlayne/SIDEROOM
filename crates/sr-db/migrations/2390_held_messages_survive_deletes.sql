-- 2390: a message or attachment under a moderation hold survives a channel or DM delete (wave 53, SEC-AUD-006;
-- D-122, D-212, D-213).
--
-- `messages.channel_id` cascades from `channels` (0012) and `attachments.message_id` from `messages` (0013), so a
-- channel delete took held evidence with it. The delete paths now check first and answer with a clear refusal
-- (a server channel) or keep the channel and only drop its recipients (a group DM whose last member leaves);
-- this trigger is the backstop, the same one migration 1120 put on the held library and cinema rows: any
-- delete that reaches a held row, directly or by cascade, fails with `restrict_violation` (SQLSTATE 23001) and
-- rolls the whole statement back. A hold is released by clearing `hold_id` (sr-moderation `release_hold_in`),
-- after which the row deletes as before.
CREATE TRIGGER messages_held_delete BEFORE DELETE ON messages
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();
CREATE TRIGGER attachments_held_delete BEFORE DELETE ON attachments
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();
