-- PRF-PRO-001 (D-382): public guestbook posts. A visitor's post has no author account (`anonymous`) and waits
-- as `pending` until the page's owner approves it; only the owner ever sees a pending post. `ip_hash` is a
-- salted hash kept for the per-address hourly limit and removed when the post is approved.
ALTER TABLE profile_guestbook ALTER COLUMN author_id DROP NOT NULL;
ALTER TABLE profile_guestbook
    ADD COLUMN status    TEXT NOT NULL DEFAULT 'approved' CHECK (status IN ('approved', 'pending')),
    ADD COLUMN anonymous BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN ip_hash   TEXT;
ALTER TABLE profile_guestbook
    ADD CONSTRAINT profile_guestbook_author_ck CHECK (author_id IS NOT NULL OR anonymous);
CREATE INDEX profile_guestbook_pending_idx ON profile_guestbook (profile_id, id DESC) WHERE status = 'pending';
CREATE INDEX profile_guestbook_ip_idx ON profile_guestbook (ip_hash, created_at DESC) WHERE ip_hash IS NOT NULL;
