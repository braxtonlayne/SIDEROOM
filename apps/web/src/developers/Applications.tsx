import { Badge, Button, Checkbox, EmptyState, Modal, TextInput, useToast } from '@sideroom/ui';
import { exitToLogin } from '../lib/enterApp';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { t, useT, type MessageKey } from '../i18n';
import { cdnUrl } from '../store';
import { createApp, errorText, isUnavailable, keys, listApps } from './api';
import { LoadError, Loading, NotAvailable, SecretDialog, type Secret } from './ui';
import type { DevApp, ReleaseState } from './types';

const STATE_LABEL_KEY = {
  development: 'developers.applications.stateDevelopment',
  in_review: 'developers.applications.stateInReview',
  released: 'developers.applications.stateReleased',
  suspended: 'developers.applications.stateSuspended',
} as const satisfies Record<ReleaseState, MessageKey>;

/** Resolved at call time (the locale is not known at module load). */
export function stateLabel(state: ReleaseState): string {
  return t(STATE_LABEL_KEY[state]);
}
const STATE_VARIANT = {
  development: 'neutral',
  in_review: 'warning',
  released: 'success',
  suspended: 'danger',
} as const;

export function StateBadge({ state }: { state: ReleaseState }) {
  const tt = useT();
  return <Badge variant={STATE_VARIANT[state]}>{tt(STATE_LABEL_KEY[state])}</Badge>;
}

export function AppIcon({
  app,
  size = 48,
}: {
  app: Pick<DevApp, 'id' | 'name' | 'icon'>;
  size?: number;
}) {
  const src = cdnUrl('avatars', app.id, app.icon, { size: 128 });
  return src ? (
    <img className="dev-app-icon" src={src} alt="" width={size} height={size} />
  ) : (
    <span
      className="dev-app-icon dev-app-icon-fallback"
      aria-hidden="true"
      style={{ width: size, height: size }}
    >
      {app.name.slice(0, 1).toUpperCase()}
    </span>
  );
}

function NewApplicationModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const tt = useT();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState('');
  const [agree, setAgree] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Shown once, then dropped when the dialog closes.
  const [created, setCreated] = useState<{ id: string; secrets: Secret[] } | null>(null);
  const trimmed = name.trim();
  const nameError =
    name !== '' && (trimmed.length < 2 || trimmed.length > 32)
      ? tt('developers.applications.nameError')
      : undefined;

  const create = useMutation({
    mutationFn: () => createApp(trimmed),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: keys.apps });
      toast.show({
        title: tt('developers.applications.created', { name: r.application.name }),
        variant: 'success',
      });
      setCreated({
        id: r.application.id,
        secrets: [
          { label: tt('developers.applications.clientSecret'), value: r.client_secret },
          { label: tt('developers.applications.botToken'), value: r.bot_token },
        ],
      });
    },
    onError: (e) => setError(errorText(e, tt('developers.applications.createFailed'))),
  });
  const reset = () => {
    setName('');
    setAgree(false);
    setError(null);
    onClose();
  };

  const finish = () => {
    const id = created?.id;
    setCreated(null); // drop the secrets
    reset();
    if (id) void navigate({ to: '/developers/applications/$appId', params: { appId: id } });
  };

  return (
    <>
      <Modal
        open={open && created === null}
        onClose={reset}
        title={tt('developers.applications.newTitle')}
        description={tt('developers.applications.newDescription')}
        footer={
          <>
            <Button variant="ghost" onClick={reset}>
              {tt('developers.applications.cancel')}
            </Button>
            <Button
              loading={create.isPending}
              disabled={trimmed.length < 2 || trimmed.length > 32 || !agree}
              onClick={() => {
                setError(null);
                create.mutate();
              }}
            >
              {tt('developers.applications.create')}
            </Button>
          </>
        }
      >
        <form
          className="dev-form"
          onSubmit={(e) => {
            e.preventDefault();
          }}
        >
          <TextInput
            label={tt('developers.applications.nameLabel')}
            value={name}
            maxLength={40}
            onChange={(e) => setName(e.target.value)}
            error={nameError}
            autoComplete="off"
          />
          <Checkbox
            label={tt('developers.applications.agree')}
            checked={agree}
            onChange={(e) => setAgree(e.target.checked)}
          />
          {error && (
            <p className="dev-error" role="alert">
              {error}
            </p>
          )}
        </form>
      </Modal>
      <SecretDialog
        title={tt('developers.applications.saveTitle')}
        description={tt('developers.applications.saveDescription')}
        secrets={created?.secrets ?? null}
        onClose={finish}
      />
    </>
  );
}

export function ApplicationsPage() {
  const tt = useT();
  const [creating, setCreating] = useState(false);
  const apps = useQuery({ queryKey: keys.apps, queryFn: listApps });

  let body;
  if (apps.isPending) body = <Loading what={tt('developers.applications.yourApplications')} />;
  else if (apps.error instanceof ApiError && apps.error.status === 401)
    body = (
      <EmptyState
        title={tt('developers.applications.signInTitle')}
        description={tt('developers.applications.signInDescription')}
        action={
          <Button onClick={() => exitToLogin('/developers/applications')}>
            {tt('developers.applications.logIn')}
          </Button>
        }
      />
    );
  else if (isUnavailable(apps.error))
    body = <NotAvailable>{tt('developers.applications.notAvailable')}</NotAvailable>;
  else if (apps.error)
    body = (
      <LoadError
        what={tt('developers.applications.yourApplications')}
        error={apps.error}
        onRetry={() => void apps.refetch()}
      />
    );
  else if (apps.data.length === 0)
    body = (
      <EmptyState
        title={tt('developers.applications.emptyTitle')}
        description={tt('developers.applications.emptyDescription')}
        action={
          <Button onClick={() => setCreating(true)}>
            {tt('developers.applications.newButton')}
          </Button>
        }
      />
    );
  else
    body = (
      <ul className="dev-app-grid" aria-label={tt('developers.applications.listLabel')}>
        {apps.data.map((a) => (
          <li key={a.id}>
            <Link
              to="/developers/applications/$appId"
              params={{ appId: a.id }}
              className="dev-app-card"
            >
              <AppIcon app={a} />
              <span className="dev-app-card-text">
                <strong>{a.name}</strong>
                <span className="dev-muted">
                  {tt('developers.applications.botLine', { username: a.bot.username })}
                </span>
              </span>
              <StateBadge state={a.release_state} />
            </Link>
          </li>
        ))}
      </ul>
    );

  const canCreate = apps.data !== undefined || apps.isPending;
  return (
    <>
      <div className="dev-head">
        <h1 className="dev-title">{tt('developers.applications.heading')}</h1>
        {canCreate && apps.data && apps.data.length > 0 && (
          <Button onClick={() => setCreating(true)}>
            {tt('developers.applications.newButton')}
          </Button>
        )}
      </div>
      {body}
      <NewApplicationModal open={creating} onClose={() => setCreating(false)} />
    </>
  );
}
