-- EXT-PKG-001 (docs/11 section 4, doc 04 section 6): declarative packages. A package is data only (D-063): themes,
-- profile layouts, emoji/sticker/sound packs, recipes and server templates. Each version is a zip kept in
-- object storage and reviewed by staff (REVIEW_PACKAGES) before it can be enabled (D-064).
CREATE TABLE packages (
    id                BIGINT PRIMARY KEY,
    kind              TEXT NOT NULL CHECK (kind IN
                      ('theme', 'profile_layout', 'emoji_pack', 'sticker_pack', 'sound_pack', 'recipe', 'server_template')),
    author_id         BIGINT NOT NULL REFERENCES accounts (id),
    -- The package id in its manifest; also the `data-theme` value of a theme. Unique across kinds.
    slug              TEXT NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,46}[a-z0-9]$'),
    name              TEXT NOT NULL,
    description       TEXT NOT NULL DEFAULT '',
    -- The newest approved version, null until one is approved.
    latest_version_id BIGINT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX packages_author_idx ON packages (author_id, created_at DESC);
CREATE INDEX packages_kind_idx ON packages (kind, id DESC) WHERE latest_version_id IS NOT NULL;

CREATE TABLE package_versions (
    id            BIGINT PRIMARY KEY,
    package_id    BIGINT NOT NULL REFERENCES packages (id) ON DELETE CASCADE,
    version       TEXT NOT NULL CHECK (version ~ '^[0-9]{1,6}\.[0-9]{1,6}\.[0-9]{1,6}$'),
    -- {name, description, author, files: [{path, size}], payload: <per kind>, warnings: []}
    manifest      JSONB NOT NULL,
    -- Object key of the uploaded zip, for the reviewer's download and the package's files.
    artifact_key  TEXT NOT NULL,
    checksum      TEXT NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
    size_bytes    BIGINT NOT NULL CHECK (size_bytes >= 0),
    review_state  TEXT NOT NULL DEFAULT 'pending'
                  CHECK (review_state IN ('pending', 'approved', 'rejected', 'withdrawn', 'revoked')),
    reviewed_by   BIGINT,
    reviewed_at   TIMESTAMPTZ,
    review_note   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (package_id, version)
);
CREATE INDEX package_versions_pending_idx ON package_versions (created_at) WHERE review_state = 'pending';
CREATE INDEX package_versions_package_idx ON package_versions (package_id, id DESC);
ALTER TABLE packages
    ADD CONSTRAINT packages_latest_version_fk
    FOREIGN KEY (latest_version_id) REFERENCES package_versions (id) ON DELETE SET NULL;

-- A server or a person has a package turned on, at a version they chose. `section` is where a theme applies
-- (`all` or a section such as `lounge`); config holds the applier's record (a recipe's last result).
CREATE TABLE package_enablements (
    scope       TEXT NOT NULL CHECK (scope IN ('guild', 'user')),
    scope_id    BIGINT NOT NULL,
    package_id  BIGINT NOT NULL REFERENCES packages (id) ON DELETE CASCADE,
    version_id  BIGINT NOT NULL REFERENCES package_versions (id),
    section     TEXT NOT NULL DEFAULT 'all' CHECK (section ~ '^[a-z]{2,16}$'),
    config      JSONB NOT NULL DEFAULT '{}'::jsonb,
    enabled_by  BIGINT NOT NULL,
    enabled_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (scope, scope_id, package_id)
);
CREATE INDEX package_enablements_version_idx ON package_enablements (version_id);
