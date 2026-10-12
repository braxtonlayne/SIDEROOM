-- Operator emergency invites (`sr-cli invite create`, open question 46) sit outside the
-- per-user credit economy: redeeming one neither requires nor spends issuer credit.
-- The issuer stays recorded so the invite graph still shows who the account came from.
ALTER TABLE platform_invites ADD COLUMN emergency BOOLEAN NOT NULL DEFAULT false;
