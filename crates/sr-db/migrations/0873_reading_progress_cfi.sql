-- 0873: EPUB reading progress is the EPUB CFI stored beside the page index (D-225, H-16). The `cfi`
-- column exists since 0201; this adds the length check (at most 1024 characters, the API bound).
-- Rows written before the API limit was tightened are trimmed to NULL rather than failing the migration.
UPDATE reading_progress SET cfi = NULL WHERE cfi IS NOT NULL AND char_length(cfi) > 1024;
ALTER TABLE reading_progress
    ADD CONSTRAINT reading_progress_cfi_len CHECK (cfi IS NULL OR char_length(cfi) <= 1024);
