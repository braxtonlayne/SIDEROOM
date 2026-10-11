-- 0080: one source of truth for rules/updates channels (open question 138).
-- guilds.rules_channel_id and guilds.public_updates_channel_id are the Discord
-- guild fields and win. Values only held in guild_community are copied over
-- first, then the duplicate columns are dropped.
UPDATE guilds g
   SET rules_channel_id = COALESCE(g.rules_channel_id, gc.rules_channel_id),
       public_updates_channel_id = COALESCE(g.public_updates_channel_id, gc.updates_channel_id)
  FROM guild_community gc
 WHERE gc.guild_id = g.id;

ALTER TABLE guild_community
    DROP COLUMN rules_channel_id,
    DROP COLUMN updates_channel_id;
