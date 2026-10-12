-- 0131: Person bans with a reason category, inviter suspicion history, and the
-- account deletion request stub. docs/06 sections 2 and 5.4, doc 09, ADM-PPL-003/005/006.

CREATE TYPE ban_category AS ENUM ('serious', 'standard');

-- A platform ban is tied to the Person (doc 06 section 2): registration refuses a
-- Person with an open ban, even when none of its accounts is currently banned
-- (for example a banned Person whose only account was deleted).
CREATE TABLE person_bans (
    id          BIGINT PRIMARY KEY,
    person_id   BIGINT NOT NULL REFERENCES persons (id),
    category    ban_category NOT NULL,
    note        TEXT NOT NULL DEFAULT '',
    banned_by   BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    lifted_at   TIMESTAMPTZ,
    lifted_by   BIGINT REFERENCES accounts (id),
    lift_note   TEXT,
    CHECK ((lifted_at IS NULL) = (lifted_by IS NULL))
);
CREATE UNIQUE INDEX person_bans_open_uidx ON person_bans (person_id) WHERE lifted_at IS NULL;
CREATE INDEX person_bans_created_idx ON person_bans (id DESC);

-- History behind persons.suspicion_level (doc 06 section 5.4; D-031).
CREATE TABLE suspicion_events (
    id                 BIGINT PRIMARY KEY,
    person_id          BIGINT NOT NULL REFERENCES persons (id),
    delta              SMALLINT NOT NULL,
    level_after        INTEGER NOT NULL CHECK (level_after >= 0),
    reason             TEXT NOT NULL,
    source_account_id  BIGINT REFERENCES accounts (id),
    actor_id           BIGINT REFERENCES accounts (id),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX suspicion_events_person_idx ON suspicion_events (person_id, id DESC);

-- Deletion is user-initiated (D-029); staff can only record that a request exists.
-- Executing it is a later work package.
CREATE TABLE account_delete_requests (
    id            BIGINT PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts (id),
    requested_by  BIGINT NOT NULL REFERENCES accounts (id),
    reason        TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'cancelled', 'completed')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at   TIMESTAMPTZ
);
CREATE UNIQUE INDEX account_delete_requests_pending_uidx
    ON account_delete_requests (account_id) WHERE state = 'pending';
