-- 0701: "I'll upload this" on the requests board (REV-REQ-002). One member at a time can say
-- they are working on an open request, with a short note; only they (or section reviewers)
-- can release it.
ALTER TABLE content_requests
    ADD COLUMN claimed_by BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    ADD COLUMN claim_note TEXT CHECK (claim_note IS NULL OR char_length(claim_note) <= 500),
    ADD COLUMN claimed_at TIMESTAMPTZ;
