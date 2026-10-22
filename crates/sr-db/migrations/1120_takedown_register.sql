-- 1120: Phase 7 step 2, the takedown register (MOD-TKD-001 to -011; D-293, D-315;
-- docs/specs/moderation-compliance.md section 5).
--
-- A notice is entered by staff (there is no public form). Its items name the content it covers; an item is
-- held through the hold API (`moderation_holds`) and then made unavailable. Notices and their events are kept
-- forever (D-212), so deleting either is refused by trigger and the events are append-only.

-- ---- holds on library and cinema rows ---------------------------------------------------------------------------
-- The hold scope list grows by the four catalogue rows a takedown can freeze. A held row cannot be deleted (a
-- trigger refuses it), so the stored objects its row points at cannot be queued for collection either.
ALTER TABLE moderation_holds DROP CONSTRAINT moderation_holds_scope_kind_check;
ALTER TABLE moderation_holds ADD CONSTRAINT moderation_holds_scope_kind_check
    CHECK (scope_kind IN ('message', 'attachment', 'submission', 'user_messages',
                          'library_series', 'library_chapter', 'cinema_title', 'cinema_episode'));

ALTER TABLE reading_series   ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
ALTER TABLE reading_chapters ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
ALTER TABLE video_titles     ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
ALTER TABLE video_episodes   ADD COLUMN hold_id BIGINT REFERENCES moderation_holds (id);
CREATE INDEX reading_series_hold_idx   ON reading_series (hold_id)   WHERE hold_id IS NOT NULL;
CREATE INDEX reading_chapters_hold_idx ON reading_chapters (hold_id) WHERE hold_id IS NOT NULL;
CREATE INDEX video_titles_hold_idx     ON video_titles (hold_id)     WHERE hold_id IS NOT NULL;
CREATE INDEX video_episodes_hold_idx   ON video_episodes (hold_id)   WHERE hold_id IS NOT NULL;

CREATE FUNCTION moderation_refuse_held_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.hold_id IS NOT NULL THEN
        RAISE EXCEPTION '% % is under a moderation hold and cannot be deleted', TG_TABLE_NAME, OLD.id
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER reading_series_held_delete BEFORE DELETE ON reading_series
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();
CREATE TRIGGER reading_chapters_held_delete BEFORE DELETE ON reading_chapters
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();
CREATE TRIGGER video_titles_held_delete BEFORE DELETE ON video_titles
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();
CREATE TRIGGER video_episodes_held_delete BEFORE DELETE ON video_episodes
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_delete();

-- The pages of a held chapter are its stored objects: replacing them would queue the old files for collection.
CREATE FUNCTION moderation_refuse_held_pages_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM reading_chapters c WHERE c.id = OLD.chapter_id AND c.hold_id IS NOT NULL) THEN
        RAISE EXCEPTION 'the pages of chapter % are under a moderation hold', OLD.chapter_id
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER reading_pages_held_delete BEFORE DELETE ON reading_pages
    FOR EACH ROW EXECUTE FUNCTION moderation_refuse_held_pages_delete();

-- ---- the register -----------------------------------------------------------------------------------------------
CREATE TABLE takedown_notices (
    id                   BIGINT PRIMARY KEY,
    kind                 TEXT NOT NULL CHECK (kind IN ('copyright', 'illegal_content')),
    -- received -> triage -> removed | rejected; removed -> counter_noticed -> restored; removed -> withdrawn.
    state                TEXT NOT NULL DEFAULT 'received'
                         CHECK (state IN ('received', 'triage', 'rejected', 'removed', 'counter_noticed',
                                          'restored', 'withdrawn')),
    notifier_name        TEXT NOT NULL CHECK (char_length(notifier_name) BETWEEN 1 AND 200),
    notifier_contact     TEXT NOT NULL CHECK (char_length(notifier_contact) BETWEEN 1 AND 300),
    -- The claimed work (copyright) or the legal basis (illegal content), as the notifier wrote it.
    legal_basis          TEXT NOT NULL CHECK (char_length(legal_basis) BETWEEN 1 AND 4000),
    -- The exact URLs or ids the notifier gave.
    locations            TEXT[] NOT NULL DEFAULT '{}',
    original_text        TEXT NOT NULL CHECK (char_length(original_text) BETWEEN 1 AND 40000),
    received_at          TIMESTAMPTZ NOT NULL,
    entered_by           BIGINT NOT NULL REFERENCES accounts (id),
    -- Triage target: one business day after receipt (spec 5, step 2).
    triage_due_at        TIMESTAMPTZ NOT NULL,
    case_id              BIGINT REFERENCES moderation_cases (id),
    rejected_at          TIMESTAMPTZ,
    rejection_reason     TEXT NOT NULL DEFAULT '',
    -- The takedown was upheld: the content is held and unavailable. This is the strike moment.
    removed_at           TIMESTAMPTZ,
    removed_by           BIGINT REFERENCES accounts (id),
    counter_noticed_at   TIMESTAMPTZ,
    counter_notice_by    BIGINT REFERENCES accounts (id),
    counter_notice_text  TEXT NOT NULL DEFAULT '' CHECK (char_length(counter_notice_text) <= 20000),
    restore_after        TIMESTAMPTZ,
    -- Staff noted that the notifier filed in court: the item stays down and the job never restores it.
    court_filing_noted   BOOLEAN NOT NULL DEFAULT FALSE,
    court_filing_at      TIMESTAMPTZ,
    court_filing_by      BIGINT REFERENCES accounts (id),
    restored_at          TIMESTAMPTZ,
    -- NULL when the housekeeping job restored it.
    restored_by          BIGINT REFERENCES accounts (id),
    restore_reason       TEXT NOT NULL DEFAULT '',
    withdrawn_at         TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (state <> 'counter_noticed' OR restore_after IS NOT NULL)
);
CREATE INDEX takedown_notices_queue_idx ON takedown_notices (state, received_at DESC);
CREATE INDEX takedown_notices_restore_idx ON takedown_notices (restore_after)
    WHERE state = 'counter_noticed' AND NOT court_filing_noted;
CREATE INDEX takedown_notices_case_idx ON takedown_notices (case_id) WHERE case_id IS NOT NULL;

-- One row per piece of content a notice covers.
CREATE TABLE takedown_items (
    id                  BIGINT PRIMARY KEY,
    notice_id           BIGINT NOT NULL REFERENCES takedown_notices (id),
    kind                TEXT NOT NULL CHECK (kind IN ('message', 'attachment', 'library_series', 'library_chapter',
                                                     'cinema_title', 'cinema_episode')),
    target_id           BIGINT NOT NULL,
    label               TEXT NOT NULL DEFAULT '' CHECK (char_length(label) <= 300),
    -- The uploader or author, when the content has one; the strike counts against their Person.
    subject_account_id  BIGINT REFERENCES accounts (id),
    subject_person_id   BIGINT REFERENCES persons (id),
    -- identified -> held -> disabled -> restored
    state               TEXT NOT NULL DEFAULT 'identified'
                        CHECK (state IN ('identified', 'held', 'disabled', 'restored')),
    hold_id             BIGINT REFERENCES moderation_holds (id),
    -- What the content looked like before, so a restore puts back exactly that.
    prior               JSONB NOT NULL DEFAULT '{}'::jsonb,
    disabled_at         TIMESTAMPTZ,
    restored_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (notice_id, kind, target_id)
);
CREATE INDEX takedown_items_subject_idx ON takedown_items (subject_person_id) WHERE subject_person_id IS NOT NULL;
CREATE INDEX takedown_items_target_idx ON takedown_items (kind, target_id);

-- The timeline of a notice (every state change; the access log carries the staff side).
CREATE TABLE takedown_events (
    id          BIGINT PRIMARY KEY,
    notice_id   BIGINT NOT NULL REFERENCES takedown_notices (id),
    actor_id    BIGINT REFERENCES accounts (id),
    kind        TEXT NOT NULL CHECK (char_length(kind) BETWEEN 1 AND 40),
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX takedown_events_notice_idx ON takedown_events (notice_id, id);

CREATE FUNCTION takedown_keep_forever() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% rows are kept forever (D-212)', TG_TABLE_NAME USING ERRCODE = 'restrict_violation';
END $$;
CREATE TRIGGER takedown_notices_keep BEFORE DELETE ON takedown_notices
    FOR EACH ROW EXECUTE FUNCTION takedown_keep_forever();
-- An item named by mistake can be taken back while it is only identified; once held it stays.
CREATE FUNCTION takedown_items_keep() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.state <> 'identified' THEN
        RAISE EXCEPTION 'takedown_items rows are kept forever once held (D-212)' USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER takedown_items_keep BEFORE DELETE ON takedown_items
    FOR EACH ROW EXECUTE FUNCTION takedown_items_keep();
CREATE TRIGGER takedown_events_keep BEFORE UPDATE OR DELETE ON takedown_events
    FOR EACH ROW EXECUTE FUNCTION takedown_keep_forever();

-- The review cases the strike counter opened, so one Person has at most one open review at a time.
CREATE TABLE takedown_strike_reviews (
    id          BIGINT PRIMARY KEY,
    person_id   BIGINT NOT NULL REFERENCES persons (id),
    case_id     BIGINT NOT NULL REFERENCES moderation_cases (id),
    strikes     INTEGER NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX takedown_strike_reviews_person_idx ON takedown_strike_reviews (person_id, id DESC);

-- ---- the media path ---------------------------------------------------------------------------------------------
-- Stored objects a takedown blocks. `key` is one object, `prefix` every object under it (ends with '/'), and
-- `hls_asset` a Cinema asset by id (its playlists and segments live under several keys). The CDN route reads
-- these (cached for a short time) before it serves, and answers 451.
CREATE TABLE blocked_objects (
    id          BIGINT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('key', 'prefix', 'hls_asset')),
    value       TEXT NOT NULL CHECK (char_length(value) BETWEEN 1 AND 1024),
    notice_id   BIGINT NOT NULL REFERENCES takedown_notices (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (kind, value, notice_id)
);
