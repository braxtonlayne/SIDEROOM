-- 0290: the shared review pipeline (REV-CORE-001, docs/specs/submissions-and-review.md).
-- One `submissions` row per thing a member wants published; the kind (`library.series`,
-- `library.items`, later cinema/app/package kinds) says what `ref_id` points at and which
-- platform permission reviews it. `submission_items` carries the per-item decisions (a
-- chapter, an episode), `submission_events` is the timeline (state changes, comments,
-- internal notes, change requests, with the reason of each decision).

CREATE TABLE submissions (
    id              BIGINT PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind ~ '^[a-z]+\.[a-z_]+$'),
    submitter_id    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    state           TEXT NOT NULL DEFAULT 'draft'
                    CHECK (state IN ('draft', 'submitted', 'in_review', 'changes_requested',
                                     'approved', 'rejected', 'published', 'withdrawn')),
    title           TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    summary         TEXT NOT NULL DEFAULT '' CHECK (char_length(summary) <= 4000),
    -- What the kind works on (the draft series for the Library kinds).
    ref_id          BIGINT,
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    flags           TEXT[] NOT NULL DEFAULT '{}',
    -- Reviewer lease (soft lock): valid while claim_expires_at is in the future.
    claimed_by      BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    claim_expires_at TIMESTAMPTZ,
    decided_by      BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    decision_reason TEXT,
    submitted_at    TIMESTAMPTZ,
    decided_at      TIMESTAMPTZ,
    published_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX submissions_submitter_idx ON submissions (submitter_id, created_at DESC);
CREATE INDEX submissions_queue_idx ON submissions (kind, submitted_at)
    WHERE state IN ('submitted', 'in_review');
CREATE INDEX submissions_ref_idx ON submissions (kind, ref_id) WHERE ref_id IS NOT NULL;

CREATE TABLE submission_items (
    id            BIGINT PRIMARY KEY,
    submission_id BIGINT NOT NULL REFERENCES submissions (id) ON DELETE CASCADE,
    ref_id        BIGINT NOT NULL,
    label         TEXT NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0,
    state         TEXT NOT NULL DEFAULT 'pending'
                  CHECK (state IN ('pending', 'approved', 'rejected', 'published')),
    note          TEXT,
    UNIQUE (submission_id, ref_id)
);

CREATE TABLE submission_events (
    id            BIGINT PRIMARY KEY,
    submission_id BIGINT NOT NULL REFERENCES submissions (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    actor_id      BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    body          TEXT,
    -- Internal notes are visible to reviewers only.
    internal      BOOLEAN NOT NULL DEFAULT false,
    data          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX submission_events_idx ON submission_events (submission_id, id);

-- Non-bounty content requests (REV-REQ-001): request a title, upvote, fulfil with a submission.
CREATE TABLE content_requests (
    id            BIGINT PRIMARY KEY,
    section       TEXT NOT NULL CHECK (section IN ('library', 'cinema')),
    requester_id  BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    title         TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    creator       TEXT,
    year          SMALLINT,
    link          TEXT,
    note          TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 2000),
    status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'fulfilled', 'declined')),
    vote_count    INTEGER NOT NULL DEFAULT 1,
    submission_id BIGINT REFERENCES submissions (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX content_requests_board_idx ON content_requests (section, status, vote_count DESC, id DESC);

CREATE TABLE content_request_votes (
    request_id BIGINT NOT NULL REFERENCES content_requests (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (request_id, user_id)
);
