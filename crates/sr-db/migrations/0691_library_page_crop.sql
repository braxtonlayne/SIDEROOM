-- 0691: Detected uniform borders of a page, as {"left","top","right","bottom"} in pixels of the
-- full-size image (LIB-ING-007). Data only: the reader decides whether to crop. NULL = none found
-- or not analysed (pages ingested before this migration).
ALTER TABLE reading_pages ADD COLUMN crop JSONB;
