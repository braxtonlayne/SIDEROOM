-- PRF-PRO-001 (D-382): the member's policy for who sees their profile, kept apart from accounts.profile_doc
-- (the doc is layout, this is policy). A missing row is the `private` preset: every tier only_me. The tiers
-- are ordered public < members < mutual_servers < friends < only_me; the application clamps a block so it is
-- never looser than the page.
CREATE TABLE profile_privacy (
    user_id                  BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    preset                   TEXT NOT NULL DEFAULT 'private'
        CHECK (preset IN ('public', 'members', 'friends', 'private', 'custom')),
    page_tier                TEXT NOT NULL DEFAULT 'only_me'
        CHECK (page_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    links_tier               TEXT NOT NULL DEFAULT 'only_me'
        CHECK (links_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    top8_tier                TEXT NOT NULL DEFAULT 'only_me'
        CHECK (top8_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    now_playing_tier         TEXT NOT NULL DEFAULT 'only_me'
        CHECK (now_playing_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    bio_tier                 TEXT NOT NULL DEFAULT 'only_me'
        CHECK (bio_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    guestbook_tier           TEXT NOT NULL DEFAULT 'only_me'
        CHECK (guestbook_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    media_tier               TEXT NOT NULL DEFAULT 'only_me'
        CHECK (media_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    ring_tier                TEXT NOT NULL DEFAULT 'only_me'
        CHECK (ring_tier IN ('public', 'members', 'mutual_servers', 'friends', 'only_me')),
    guestbook_accepts_public BOOLEAN NOT NULL DEFAULT false,
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
