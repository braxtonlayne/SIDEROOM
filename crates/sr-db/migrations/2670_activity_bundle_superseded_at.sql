-- SEC-AUD-139 (D-630): when a bundle was superseded, so the object cleanup can leave a just-superseded bundle's
-- files in place for a grace period (members who opened the Activity before the change still load them).
-- Rows superseded before this migration stay NULL, which means "long ago".
ALTER TABLE application_activity_bundles ADD COLUMN superseded_at TIMESTAMPTZ;

CREATE FUNCTION application_activity_bundles_stamp_superseded() RETURNS trigger AS $$
BEGIN
    IF NEW.state = 'superseded' AND OLD.state IS DISTINCT FROM 'superseded' THEN
        NEW.superseded_at := now();
    ELSIF NEW.state <> 'superseded' AND OLD.state = 'superseded' THEN
        NEW.superseded_at := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER application_activity_bundles_stamp_superseded
    BEFORE UPDATE ON application_activity_bundles
    FOR EACH ROW EXECUTE FUNCTION application_activity_bundles_stamp_superseded();
