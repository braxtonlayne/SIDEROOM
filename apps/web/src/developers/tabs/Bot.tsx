import { Badge, Button, Notice, Switch, TextInput, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { errorText, keys, renameBot, resetBotToken } from '../api';
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
  off: { label: 'Off', variant: 'neutral' },
  pending: { label: 'Waiting for approval', variant: 'warning' },
  approved: { label: 'Approved', variant: 'success' },
} as const;

export function BotTab({ app }: { app: DevApp }) {
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
      ? 'Use 2 to 32 characters.'
      : undefined;

  const rename = async () => {
    setRenaming(true);
    setError(null);
    try {
      await renameBot(app.id, username.trim());
      await qc.invalidateQueries({ queryKey: keys.app(app.id) });
      toast.show({ title: 'Bot renamed.', variant: 'success' });
    } catch (e) {
      setError(errorText(e, 'Could not rename the bot.'));
    } finally {
      setRenaming(false);
    }
  };

  return (
    <div className="dev-stack">
      {error && (
        <Notice variant="danger" title="That did not work">
          {error}
        </Notice>
      )}
      <section className="dev-card" aria-labelledby="dev-bot-h">
        <h2 id="dev-bot-h" className="dev-card-title">
          Bot user
        </h2>
        <p className="dev-muted">
          Every application comes with a bot account (ID {app.bot.id}). Its token is stored hashed
          and cannot be shown again. If it leaks, or you lost it, reset it.
        </p>
        <TextInput
          label="Bot name"
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
            Save bot name
          </Button>
          <Button variant="danger" onClick={() => setConfirmReset(true)}>
            Reset token
          </Button>
        </div>
      </section>

      <section className="dev-card" aria-labelledby="dev-intents-h">
        <h2 id="dev-intents-h" className="dev-card-title">
          Privileged gateway intents
        </h2>
        <p className="dev-muted">
          While the app is in development these work in your test servers. Once released, only
          intents staff have approved are allowed; asking for one outside that closes the connection
          with code 4014.
        </p>
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
                <Badge variant={STATUS[status].variant}>{STATUS[status].label}</Badge>
              </li>
            );
          })}
        </ul>
        <p className="dev-muted">
          Approval happens with a version review: use the Review tab to submit one.
        </p>
      </section>

      <PasswordConfirm
        open={confirmReset}
        title="Reset the bot token?"
        description="The current token stops working immediately; a connected bot is dropped at its next reconnect. You will get a new token to copy. Confirm with your password."
        confirmLabel="Yes, reset it"
        onCancel={() => setConfirmReset(false)}
        onSubmit={async (password) => {
          const r = await resetBotToken(app.id, password);
          setConfirmReset(false);
          setToken(r.token);
        }}
      />
      <SecretDialog
        title="Bot token"
        description="Your bot uses this to log in. Treat it like a password."
        secrets={token === null ? null : [{ label: 'Bot token', value: token }]}
        onClose={() => setToken(null)}
      />
    </div>
  );
}
