import { Badge, Button, Notice, Switch, TextInput, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { errorText, keys, renameBot, resetBotToken } from '../api';
import { useT, type MessageKey } from '../../i18n';
import { usePatchApp } from '../hooks';
import { INTENTS } from '../oauth';
import type { DevApp, IntentName } from '../types';
import { PasswordConfirm, SecretDialog } from '../ui';
import { useQueryClient } from '@tanstack/react-query';

/** Review status of one privileged intent from the requested and approved sets. */
export function intentStatus(app: DevApp, key: IntentName): 'off' | 'pending' | 'approved' {
  const { requested, approved } = app.privileged_intents;
  if (!requested.includes(key)) return 'off';
  return approved.includes(key) ? 'approved' : 'pending';
}

const STATUS = {
  off: { label: 'developers.bot.statusOff', variant: 'neutral' },
  pending: { label: 'developers.bot.statusPending', variant: 'warning' },
  approved: { label: 'developers.bot.statusApproved', variant: 'success' },
} as const satisfies Record<string, { label: MessageKey; variant: string }>;

export function BotTab({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const qc = useQueryClient();
  const patch = usePatchApp(app.id);
  const [token, setToken] = useState<string | null>(null); // only while the reveal dialog is open
  const [confirmReset, setConfirmReset] = useState(false);
  const [username, setUsername] = useState(app.bot.username);
  const [renaming, setRenaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const nameError =
    username.trim().length < 2 || username.trim().length > 32
      ? t('developers.bot.nameError')
      : undefined;

  const rename = async () => {
    setRenaming(true);
    setError(null);
    try {
      await renameBot(app.id, username.trim());
      await qc.invalidateQueries({ queryKey: keys.app(app.id) });
      toast.show({ title: t('developers.bot.renamed'), variant: 'success' });
    } catch (e) {
      setError(errorText(e, t('developers.bot.renameFailed')));
    } finally {
      setRenaming(false);
    }
  };

  return (
    <div className="dev-stack">
      {error && (
        <Notice variant="danger" title={t('developers.bot.didNotWork')}>
          {error}
        </Notice>
      )}
      <section className="dev-card" aria-labelledby="dev-bot-h">
        <h2 id="dev-bot-h" className="dev-card-title">
          {t('developers.bot.userHeading')}
        </h2>
        <p className="dev-muted">{t('developers.bot.userIntro', { id: app.bot.id })}</p>
        <TextInput
          label={t('developers.bot.nameLabel')}
          value={username}
          maxLength={40}
          onChange={(e) => setUsername(e.target.value)}
          error={nameError}
        />
        <div className="dev-actions">
          <Button
            variant="secondary"
            disabled={!!nameError || username.trim() === app.bot.username}
            loading={renaming}
            onClick={() => void rename()}
          >
            {t('developers.bot.saveName')}
          </Button>
          <Button variant="danger" onClick={() => setConfirmReset(true)}>
            {t('developers.bot.resetToken')}
          </Button>
        </div>
      </section>

      <section className="dev-card" aria-labelledby="dev-intents-h">
        <h2 id="dev-intents-h" className="dev-card-title">
          {t('developers.bot.intentsHeading')}
        </h2>
        <p className="dev-muted">{t('developers.bot.intentsIntro')}</p>
        <ul className="dev-intents">
          {INTENTS.map((i) => {
            const status = intentStatus(app, i.key);
            const { requested } = app.privileged_intents;
            return (
              <li key={i.key}>
                <Switch
                  label={i.label}
                  description={i.description}
                  checked={requested.includes(i.key)}
                  disabled={patch.isPending}
                  onChange={(e) =>
                    patch.mutate(
                      {
                        privileged_intents: e.target.checked
                          ? [...requested, i.key]
                          : requested.filter((x) => x !== i.key),
                      },
                      { onError: (err) => setError(errorText(err)) },
                    )
                  }
                />
                <Badge variant={STATUS[status].variant}>{t(STATUS[status].label)}</Badge>
              </li>
            );
          })}
        </ul>
        <p className="dev-muted">{t('developers.bot.approvalNote')}</p>
      </section>

      <PasswordConfirm
        open={confirmReset}
        title={t('developers.bot.resetTitle')}
        description={t('developers.bot.resetDescription')}
        confirmLabel={t('developers.bot.resetConfirm')}
        onCancel={() => setConfirmReset(false)}
        onSubmit={async (password) => {
          const r = await resetBotToken(app.id, password);
          setConfirmReset(false);
          setToken(r.token);
        }}
      />
      <SecretDialog
        title={t('developers.bot.tokenTitle')}
        description={t('developers.bot.tokenDescription')}
        secrets={token === null ? null : [{ label: t('developers.bot.tokenTitle'), value: token }]}
        onClose={() => setToken(null)}
      />
    </div>
  );
}
