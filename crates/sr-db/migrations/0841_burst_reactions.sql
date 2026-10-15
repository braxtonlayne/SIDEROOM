-- LNG-EXP-012: burst ("super") reactions. A member may react with the same emoji both normally
-- and as a burst, so `burst` joins the primary key (the column itself exists since 0013).
ALTER TABLE reactions DROP CONSTRAINT reactions_pkey;
ALTER TABLE reactions ADD PRIMARY KEY (message_id, emoji_key, user_id, burst);
