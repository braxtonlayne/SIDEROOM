-- Per-user keyword highlights for the notification engine (LNG-NTF, G-3x). A message in a
-- channel the user can see that contains one of these words or phrases notifies like a mention.
ALTER TABLE user_settings
    ADD COLUMN notification_keywords TEXT[] NOT NULL DEFAULT '{}';
