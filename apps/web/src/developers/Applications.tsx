import { Badge, Button, Checkbox, EmptyState, Modal, TextInput, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { cdnUrl } from '../store';
import { createApp, errorText, isUnavailable, keys, listApps } from './api';
import { LoadError, Loading, NotAvailable, SecretDialog, type Secret } from './ui';
import type { DevApp, ReleaseState } from './types';

export const STATE_LABEL: Record<ReleaseState, string> = {
  development: 'In development',
  in_review: 'In review',
  released: 'Released',
  suspended: 'Suspended',
};
const STATE_VARIANT = {
  development: 'neutral',
  in_review: 'warning',
  released: 'success',
  suspended: 'danger',
} as const;

export function StateBadge({ state }: { state: ReleaseState }) {
  return <Badge variant={STATE_VARIANT[state]}>{STATE_LABEL[state]}</Badge>;
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
      ? 'Use 2 to 32 characters.'
      : undefined;

  const create = useMutation({
    mutationFn: () => createApp(trimmed),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: keys.apps });
      toast.show({ title: `Created ${r.application.name}.`, variant: 'success' });
      setCreated({
        id: r.application.id,
        secrets: [
          { label: 'Client secret', value: r.client_secret },
          { label: 'Bot token', value: r.bot_token },
        ],
      });
    },
    onError: (e) => setError(errorText(e, 'Could not create the application.')),
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
        title="New application"
        description="Give it a name people will recognise. You can change everything else later."
        footer={
          <>
            <Button variant="ghost" onClick={reset}>
              Cancel
            </Button>
            <Button
              loading={create.isPending}
              disabled={trimmed.length < 2 || trimmed.length > 32 || !agree}
              onClick={() => {
                setError(null);
                create.mutate();
              }}
            >
              Create
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
            label="Name"
            value={name}
            maxLength={40}
            onChange={(e) => setName(e.target.value)}
            error={nameError}
            autoComplete="off"
          />
          <Checkbox
            label="I agree to the developer terms and will keep my secrets private."
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
        title="Save your credentials"
        description="Your app and its bot are ready. These two values are shown only now."
        secrets={created?.secrets ?? null}
        onClose={finish}
      />
    </>
  );
}

export function ApplicationsPage() {
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);
  const apps = useQuery({ queryKey: keys.apps, queryFn: listApps });

  let body;
  if (apps.isPending) body = <Loading what="your applications" />;
  else if (apps.error instanceof ApiError && apps.error.status === 401)
    body = (
      <EmptyState
        title="Sign in to build apps"
        description="Applications belong to your account."
        action={
          <Button
            onClick={() =>
              void navigate({ to: '/login', search: { next: '/developers/applications' } })
            }
          >
            Log in
          </Button>
        }
      />
    );
  else if (isUnavailable(apps.error))
    body = (
      <NotAvailable>
        This server does not have the developer platform switched on yet, so applications cannot be
        created or listed. The documentation is still available.
      </NotAvailable>
    );
  else if (apps.error)
    body = (
      <LoadError what="your applications" error={apps.error} onRetry={() => void apps.refetch()} />
    );
  else if (apps.data.length === 0)
    body = (
      <EmptyState
        title="No applications yet"
        description="Create one to get an OAuth2 client, a bot user and slash commands."
        action={<Button onClick={() => setCreating(true)}>New application</Button>}
      />
    );
  else
    body = (
      <ul className="dev-app-grid" aria-label="Applications">
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
                <span className="dev-muted">Bot: {a.bot.username}</span>
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
        <h1 className="dev-title">Applications</h1>
        {canCreate && apps.data && apps.data.length > 0 && (
          <Button onClick={() => setCreating(true)}>New application</Button>
        )}
      </div>
      {body}
      <NewApplicationModal open={creating} onClose={() => setCreating(false)} />
    </>
  );
}
