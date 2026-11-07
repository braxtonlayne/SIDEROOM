-- 2451 (SEC-AUD-049, D-572): a deleted Cinema or Library comment that has replies becomes a tombstone.
--
-- D-324 makes a member's comments their own record, so deleting one's comment must not erase the replies other
-- members wrote under it (until now `parent_id ... ON DELETE CASCADE` did exactly that). A comment with replies
-- is now kept as a tombstone: body cleared (''), author cleared (NULL), likes dropped, `deleted_at` set; the
-- readers show it as "Comment deleted" with its replies. A comment with no replies is hard-deleted as before, and a
-- tombstone goes when its last reply does. The reply cascade is dropped (a plain foreign key): no path may delete
-- a comment that still has replies, except a statement that deletes the replies with it (a title, episode or
-- series going away).

-- Cinema.
ALTER TABLE cinema_comments ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE cinema_comments ALTER COLUMN user_id DROP NOT NULL;
ALTER TABLE cinema_comments DROP CONSTRAINT cinema_comments_body_check;
ALTER TABLE cinema_comments ADD CONSTRAINT cinema_comments_body_check CHECK (
    CASE WHEN deleted_at IS NULL THEN user_id IS NOT NULL AND char_length(body) BETWEEN 1 AND 2000
         ELSE user_id IS NULL AND body = '' END);
ALTER TABLE cinema_comments DROP CONSTRAINT cinema_comments_parent_id_fkey;
ALTER TABLE cinema_comments ADD CONSTRAINT cinema_comments_parent_id_fkey
    FOREIGN KEY (parent_id) REFERENCES cinema_comments (id);

-- Library.
ALTER TABLE reading_comments ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE reading_comments ALTER COLUMN user_id DROP NOT NULL;
ALTER TABLE reading_comments DROP CONSTRAINT reading_comments_body_check;
ALTER TABLE reading_comments ADD CONSTRAINT reading_comments_body_check CHECK (
    CASE WHEN deleted_at IS NULL THEN user_id IS NOT NULL AND char_length(body) BETWEEN 1 AND 2000
         ELSE user_id IS NULL AND body = '' END);
ALTER TABLE reading_comments DROP CONSTRAINT reading_comments_parent_id_fkey;
ALTER TABLE reading_comments ADD CONSTRAINT reading_comments_parent_id_fkey
    FOREIGN KEY (parent_id) REFERENCES reading_comments (id);
