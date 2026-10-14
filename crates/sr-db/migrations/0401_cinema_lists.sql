-- 0401: Cinema lists (CIN-MY-002): the saved "My list" (kind 'saved', one per account) and custom lists.

CREATE TABLE cinema_lists (
    id           BIGINT PRIMARY KEY,
    owner_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind         TEXT NOT NULL DEFAULT 'custom' CHECK (kind IN ('saved', 'custom')),
    name         TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description  TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 500),
    visibility   TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'friends', 'public')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cinema_lists_owner_idx ON cinema_lists (owner_id, created_at);
CREATE UNIQUE INDEX cinema_lists_saved_idx ON cinema_lists (owner_id) WHERE kind = 'saved';

CREATE TABLE cinema_list_items (
    list_id    BIGINT NOT NULL REFERENCES cinema_lists (id) ON DELETE CASCADE,
    title_id   BIGINT NOT NULL CHECK (title_id > 0),
    position   BIGINT NOT NULL,
    note       TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 500),
    added_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (list_id, title_id)
);
CREATE INDEX cinema_list_items_order_idx ON cinema_list_items (list_id, position);
CREATE INDEX cinema_list_items_title_idx ON cinema_list_items (title_id);
