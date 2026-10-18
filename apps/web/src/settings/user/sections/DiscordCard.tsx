import { Button, Notice, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import { discordLinkStart, discordStatus, discordUnlink, errorText, useResource } from '../api';
import { Card, Loaded } from '../parts';
import { PasswordPrompt } from './Account';

/** What the server puts in `?error=` when it sends the browser back from Discord. */
const RETURN_ERRORS: Record<string, MessageKey> = {
  discord_taken: 'settings.discord.errorTaken',
  discord_already: 'settings.discord.errorAlready',
  discord_failed: 'settings.discord.errorFailed',
};

function returnError(): MessageKey | null {
  try {
    const code = new URLSearchParams(window.location.search).get('error');
    return code ? (RETURN_ERRORS[code] ?? null) : null;
  } catch {
    return null;
  }
}

/** Account > Sign in with Discord: link or unlink the Discord identity used to sign in (ENT-AUTH-011). */
export function DiscordCard() {
  const t = useT();
  const toast = useToast();
  const status = useResource(discordStatus);
  const [unlinking, setUnlinking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const returned = returnError();
  const start = async () => {
    setError(null);
    setBusy(true);
    try {
      const { url } = await discordLinkStart();
      window.location.assign(url);
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };
  return (
    <Loaded res={status} what={t('settings.discord.what')}>
      {(s) =>
        // Hidden on servers without Discord sign-in, unless the account still has a link to remove.
        !s.enabled && !s.linked ? null : (
          <Card
            id="us-discord"
            title={t('settings.discord.title')}
            description={t('settings.discord.desc')}
            actions={
              s.linked ? (
                <Button variant="secondary" onClick={() => setUnlinking(true)}>
                  {t('settings.discord.unlink')}
                </Button>
              ) : (
                <Button onClick={() => void start()} disabled={busy}>
                  {t('settings.discord.link')}
                </Button>
              )
            }
          >
            <p>
              {t('settings.account.status')}{' '}
              <span className="us-badge" data-tone={s.linked ? 'good' : undefined}>
                {s.linked ? t('settings.discord.linked') : t('settings.discord.notLinked')}
              </span>
            </p>
            {returned && (
              <Notice variant="danger" role="alert">
                {t(returned)}
              </Notice>
            )}
            {error && (
              <Notice variant="danger" role="alert">
                {error}
              </Notice>
            )}
            <PasswordPrompt
              open={unlinking}
              danger
              title={t('settings.discord.unlinkTitle')}
              description={t('settings.discord.unlinkDesc')}
              confirmLabel={t('settings.discord.unlinkConfirm')}
              onClose={() => setUnlinking(false)}
              onSubmit={async (pw) => {
                await discordUnlink(pw);
                setUnlinking(false);
                toast.show({ title: t('settings.discord.unlinked'), duration: 3000 });
                status.reload();
              }}
            />
          </Card>
        )
      }
    </Loaded>
  );
}
