-- 1152: Person links meet the evasion signals (phase 19 integration; MOD-SUS-003, MOD-DSR-007).
--
-- Linking an account under another Person folds an emptied Person into the target (`link_in` in
-- `admin_repo_ext.rs`). Its evasion signals must follow, like its cases and suspicion events, or the Person cannot be
-- deleted. Two kinds of row cannot simply be re-pointed: a signal between the two Persons that are being merged
-- (it would become a pair of one Person with itself) and a signal that duplicates one the target already has for
-- the same banned Person and kind. Those rows are kept, never deleted (they may carry a review and a snapshot),
-- and marked folded: `folded_at` and the id of the Person that no longer exists. The once-per-pair rule applies to
-- rows that are not folded.

ALTER TABLE evasion_signals
    ADD COLUMN folded_at TIMESTAMPTZ,
    ADD COLUMN folded_from_person_id BIGINT;

ALTER TABLE evasion_signals DROP CONSTRAINT evasion_signals_check;
ALTER TABLE evasion_signals
    ADD CONSTRAINT evasion_signals_pair_check CHECK (folded_at IS NOT NULL OR person_id <> related_person_id);

DROP INDEX evasion_signals_pair_uidx;
CREATE UNIQUE INDEX evasion_signals_pair_uidx ON evasion_signals (person_id, related_person_id, kind)
    WHERE folded_at IS NULL;
CREATE INDEX evasion_signals_related_idx ON evasion_signals (related_person_id);
