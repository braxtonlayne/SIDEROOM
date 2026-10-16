-- 0962: server traits (J-07, LNG-SSET-007). `guilds.traits` has existed since 0010 but nothing
-- wrote it; this fixes its shape: a JSON array of at most five entries. The entry shape
-- ({emoji_id?, emoji_name?, label}) is validated in sr-chat.
UPDATE guilds SET traits = '[]'::jsonb WHERE jsonb_typeof(traits) <> 'array';
ALTER TABLE guilds ADD CONSTRAINT guilds_traits_shape
    CHECK (jsonb_typeof(traits) = 'array' AND jsonb_array_length(traits) <= 5);
