-- ADM-GRAPH-004: staff can freeze one account's invite creation by hand. It sits next to the
-- automatic freeze at a suspicion threshold and is shown to the member with the same generic
-- wording (doc 06 section 5.4).
ALTER TABLE accounts ADD COLUMN invites_frozen_at TIMESTAMPTZ;
