-- 0061: Discord split pinning out of MANAGE_MESSAGES. PIN_MESSAGES (1 << 51) is
-- granted once to every role that holds MANAGE_MESSAGES (1 << 13), so existing
-- guilds keep their pin rights (open question 95). Forward-only: never re-run.
UPDATE roles
SET permissions = permissions | (1::BIGINT << 51)
WHERE (permissions & (1::BIGINT << 13)) <> 0
  AND (permissions & (1::BIGINT << 51)) = 0;
