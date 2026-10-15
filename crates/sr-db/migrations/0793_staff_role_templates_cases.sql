-- 0793: platform bits MANAGE_CASES (1 << 22) and MANAGE_TAKEDOWNS (1 << 23) (D-210, MOD-2).
-- Moderator gains MANAGE_CASES; Senior Staff gains both. The same names on an existing Staff
-- Server get the bits ORed in, so a running installation keeps working without a manual edit.
UPDATE staff_role_templates
   SET platform_permissions = platform_permissions | (1 << 22)
 WHERE name = 'Moderator';
UPDATE staff_role_templates
   SET platform_permissions = platform_permissions | (1 << 22) | (1 << 23)
 WHERE name = 'Senior Staff';

UPDATE roles r
   SET platform_permissions = r.platform_permissions | (1 << 22)
  FROM guilds g
 WHERE g.id = r.guild_id AND g.is_staff_guild AND r.name = 'Moderator';
UPDATE roles r
   SET platform_permissions = r.platform_permissions | (1 << 22) | (1 << 23)
  FROM guilds g
 WHERE g.id = r.guild_id AND g.is_staff_guild AND r.name = 'Senior Staff';
