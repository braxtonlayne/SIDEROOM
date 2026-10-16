-- DEV-PORT-003: application emoji (Emojis tab) and Rich Presence art assets.
CREATE TABLE application_emojis (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    animated       BOOLEAN NOT NULL DEFAULT false,
    media_key      TEXT NOT NULL,
    size_bytes     BIGINT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX application_emojis_name_idx ON application_emojis (application_id, lower(name));

CREATE TABLE application_assets (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    kind           SMALLINT NOT NULL CHECK (kind IN (1, 2)),
    media_key      TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (application_id, name)
);
