-- ENT-UI-010/025: operator-editable Entrance copy (the /no-invitation and /house-notes pages and
-- landing strings). A missing row means "use the client's built-in default".
CREATE TABLE entrance_copy (
    key         TEXT PRIMARY KEY,
    markdown    TEXT NOT NULL,
    updated_by  BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
