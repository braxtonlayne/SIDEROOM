-- 0963: role colour gradients (J-08, LNG-SSET-032). `color` stays the primary colour;
-- the secondary and tertiary stops are optional. 0010 already created `secondary_color`
-- (never written), so this only adds the third stop and the rule that a tertiary stop needs
-- a secondary one.
ALTER TABLE roles ADD COLUMN IF NOT EXISTS secondary_color INTEGER
    CHECK (secondary_color BETWEEN 0 AND 16777215);
ALTER TABLE roles ADD COLUMN IF NOT EXISTS tertiary_color INTEGER
    CHECK (tertiary_color BETWEEN 0 AND 16777215);
ALTER TABLE roles ADD CONSTRAINT roles_tertiary_needs_secondary
    CHECK (tertiary_color IS NULL OR secondary_color IS NOT NULL);
