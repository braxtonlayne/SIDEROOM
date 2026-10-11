-- 0050: channel scope and inheritance for feature_enablements (docs/11 section 1.1
-- scopes Platform | Guild | Channel | User). Extends 0003; never edits it.
--
-- `enabled` becomes nullable: NULL means "inherit from the parent scope".
-- The inherit flag previously stored as `config.inherit = true` (with
-- `enabled = false`) moves to the NULL state and the key is dropped.
ALTER TYPE enablement_scope ADD VALUE IF NOT EXISTS 'channel';

ALTER TABLE feature_enablements ALTER COLUMN enabled DROP NOT NULL;

UPDATE feature_enablements
SET enabled = NULL,
    config = config - 'inherit'
WHERE config ->> 'inherit' = 'true';
