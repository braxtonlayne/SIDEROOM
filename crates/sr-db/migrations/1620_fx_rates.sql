-- UTL-CALC-001 (wave 32): the currency converter's rates.
--
-- The server fetches the European Central Bank daily reference rates (EUR base, no key) and keeps the newest
-- set here; the browser reads them through GET /tools/fx and does the arithmetic, so no client ever calls a
-- third party. One row per currency: `rate` is units of the currency per one euro.
CREATE TABLE fx_rates (
    currency   TEXT PRIMARY KEY CHECK (currency ~ '^[A-Z]{3}$' AND currency <> 'EUR'),
    rate       DOUBLE PRECISION NOT NULL CHECK (rate > 0 AND rate < 1e9),
    -- The ECB reference date of the set this row came from.
    as_of      DATE NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row: how the last attempts went, for the staleness note and the operator's logs.
CREATE TABLE fx_fetch_state (
    id              SMALLINT PRIMARY KEY CHECK (id = 1),
    last_attempt_at TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    last_error      TEXT NOT NULL DEFAULT '' CHECK (char_length(last_error) <= 500),
    failures        INTEGER NOT NULL DEFAULT 0 CHECK (failures >= 0)
);
INSERT INTO fx_fetch_state (id) VALUES (1);
