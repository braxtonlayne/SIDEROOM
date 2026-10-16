-- 0875: platform bit MANAGE_ENTRANCE (1 << 24, D-239). Senior Staff gains it (the template, and the
-- role of that name on an existing Staff Server); MANAGE_FEATURES no longer covers the Entrance
-- copy keys or the hero image, and OWNER implies the new bit.
UPDATE staff_role_templates
   SET platform_permissions = platform_permissions | (1 << 24)
 WHERE name = 'Senior Staff';

UPDATE roles r
   SET platform_permissions = r.platform_permissions | (1 << 24)
  FROM guilds g
 WHERE g.id = r.guild_id AND g.is_staff_guild AND r.name = 'Senior Staff';
