-- 1040: the Live TV chain anchor (D-285). The slate counts on from the end of the last viewed programme of a
-- channel: its end, the media sequence and discontinuity sequence of the segment after it, and the block that
-- anchors it. Null until the channel first airs a viewed programme; no backfill (an existing channel seeds
-- once more, as a new one would).
ALTER TABLE tv_channels
    ADD COLUMN chain_end_ms   BIGINT,
    ADD COLUMN chain_seq      BIGINT,
    ADD COLUMN chain_disc     BIGINT,
    ADD COLUMN chain_block_id BIGINT;
