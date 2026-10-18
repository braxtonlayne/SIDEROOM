-- EXT-PRF-004: guestbook comments on a customised profile. Friends only (checked when posting), the owner
-- moderates (deletes any entry), an author deletes their own. Both ends cascade with the account row, which
-- account deletion removes explicitly as well.
CREATE TABLE profile_guestbook (
    id          BIGINT PRIMARY KEY,
    profile_id  BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    author_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 500),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX profile_guestbook_profile_idx ON profile_guestbook (profile_id, id DESC);
CREATE INDEX profile_guestbook_author_idx ON profile_guestbook (author_id, created_at DESC);
