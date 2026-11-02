-- 2320: FU-8 (FU-UTL-3), the list of who said they would come is purged after the event.
--
-- `invites.card_claimants_retention_days` (default 30, 0 = never) after a card finishes (`closed_at`) or is canceled
-- (`canceled_at`), the `cards.purge_claimants` job deletes its `invite_card_claims` rows: who claimed a seat is personal
-- data with no use once the event is over. The card stays, and so does how many went: the job writes that number to
-- `going_final` (and the time to `claimants_purged_at`) before it deletes, and a purged card shows it instead of the list.
ALTER TABLE invite_cards
    ADD COLUMN claimants_purged_at TIMESTAMPTZ,
    ADD COLUMN going_final INTEGER CHECK (going_final IS NULL OR going_final >= 0);
-- The job's scan: cards that are over and not purged yet.
CREATE INDEX invite_cards_unpurged_idx ON invite_cards (LEAST(closed_at, canceled_at))
    WHERE claimants_purged_at IS NULL AND (closed_at IS NOT NULL OR canceled_at IS NOT NULL);
