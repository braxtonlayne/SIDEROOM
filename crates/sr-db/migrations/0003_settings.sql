-- 0003: platform settings and feature enablements. docs/11 section 2.
CREATE TABLE platform_settings (
    key             TEXT PRIMARY KEY,
    value           JSONB NOT NULL,
    schema_version  INTEGER NOT NULL DEFAULT 1 CHECK (schema_version >= 1),
    updated_by      BIGINT REFERENCES accounts (id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TYPE enablement_scope AS ENUM ('platform', 'guild', 'user');

-- scope_id is 0 for the platform scope so the unique key has no NULLs.
CREATE TABLE feature_enablements (
    id          BIGINT PRIMARY KEY,
    feature     TEXT NOT NULL,
    scope       enablement_scope NOT NULL,
    scope_id    BIGINT NOT NULL DEFAULT 0,
    enabled     BOOLEAN NOT NULL,
    config      JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_by  BIGINT REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (feature, scope, scope_id),
    CHECK ((scope = 'platform') = (scope_id = 0))
);
