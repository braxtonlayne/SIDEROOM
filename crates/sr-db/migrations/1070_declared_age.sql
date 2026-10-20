-- M-06 (D-294): the minimum age the person affirmed when they registered ("I am at least 13").
-- Self-declaration only; nothing is verified. NULL for accounts created before the confirmation existed.
ALTER TABLE accounts ADD COLUMN age_declared SMALLINT CHECK (age_declared BETWEEN 13 AND 18);
