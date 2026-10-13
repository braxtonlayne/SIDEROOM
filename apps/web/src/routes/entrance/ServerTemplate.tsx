import { Button, TextInput, ThemeProvider } from '@sideroom/ui';
import { Link, useNavigate, useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch, apiPost } from '../../api/client';
import { getMe } from '../../api/entrance';
import '../../components/entrance.css';

interface SnapChannel {
  local_id: number;
  kind: number;
  name: string;
  position: number;
  parent_local_id: number | null;
}
interface TemplatePreview {
  code: string;
  name: string;
  description: string | null;
  usage_count: number;
  creator?: { username?: string; global_name?: string | null };
  serialized_source_guild: { roles?: unknown[]; channels?: SnapChannel[] };
}
interface CreatedGuild {
  id: string;
  channels?: { id: string; type: number }[];
}

type State =
  | { kind: 'loading' }
  | { kind: 'invalid' }
  | { kind: 'login' }
  | { kind: 'ready'; template: TemplatePreview };

const KIND_PREFIX: Record<number, string> = { 0: '#', 2: 'Voice: ', 5: '#', 15: '#' };

/**
 * Public page for a server template (LNG-SSET-071), `/template/:code`. The preview needs a session (the API
 * answers 401 otherwise), so a signed-out visitor is sent to log in and comes back here.
 */
export function ServerTemplate() {
  const { code } = useParams({ strict: false }) as { code: string };
  const navigate = useNavigate();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [name, setName] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    getMe()
      .then(() =>
        apiFetch<TemplatePreview>(`/guilds/templates/${encodeURIComponent(code)}`, { retries: 0 }),
      )
      .then((template) => {
        if (!live) return;
        setName(template.name);
        setState({ kind: 'ready', template });
      })
      .catch((e: unknown) => {
        if (!live) return;
        setState(
          e instanceof ApiError && e.status === 401 ? { kind: 'login' } : { kind: 'invalid' },
        );
      });
    return () => {
      live = false;
    };
  }, [code]);

  const create = async () => {
    setCreating(true);
    setError(null);
    try {
      const g = await apiPost<CreatedGuild>(`/guilds/templates/${encodeURIComponent(code)}`, {
        name: name.trim(),
      });
      const first = g.channels?.find((c) => c.type === 0);
      if (first)
        await navigate({
          to: '/channels/$guildId/$channelId',
          params: { guildId: g.id, channelId: first.id },
        });
      else await navigate({ to: '/channels/$guildId', params: { guildId: g.id } });
    } catch (e) {
      setError(
        e instanceof ApiError && e.status === 429
          ? 'You are doing that too quickly. Wait a moment and try again.'
          : e instanceof ApiError && e.status === 400
            ? 'Server names are 2 to 100 characters.'
            : 'Could not create the server. Try again.',
      );
      setCreating(false);
    }
  };

  const nameOk = name.trim().length >= 2 && name.trim().length <= 100;

  return (
    <ThemeProvider section="lounge" className="ent-invite-page">
      <main className="ent-invite-card" aria-busy={state.kind === 'loading'}>
        {state.kind === 'loading' && <p role="status">Loading template…</p>}
        {state.kind === 'invalid' && (
          <p role="alert">This template does not exist or was deleted.</p>
        )}
        {state.kind === 'login' && (
          <>
            <h1>Log in to use this template</h1>
            <Link to="/login" search={{ next: `/template/${code}` }} className="ent-invite-cta">
              Log in
            </Link>
          </>
        )}
        {state.kind === 'ready' && (
          <Preview state={state} {...{ name, setName, nameOk, creating, error, create }} />
        )}
      </main>
    </ThemeProvider>
  );
}

function Preview({
  state,
  name,
  setName,
  nameOk,
  creating,
  error,
  create,
}: {
  state: { template: TemplatePreview };
  name: string;
  setName: (v: string) => void;
  nameOk: boolean;
  creating: boolean;
  error: string | null;
  create: () => Promise<void>;
}) {
  const t = state.template;
  const channels = [...(t.serialized_source_guild.channels ?? [])].sort(
    (a, b) => a.position - b.position,
  );
  const categories = channels.filter((c) => c.kind === 4);
  const roles = t.serialized_source_guild.roles?.length ?? 0;
  const by = t.creator?.global_name || t.creator?.username;
  return (
    <>
      <p>Start a server from the template</p>
      <h1>{t.name}</h1>
      {t.description && <p>{t.description}</p>}
      <p>
        {channels.length - categories.length} channels · {roles} roles · used {t.usage_count}{' '}
        {t.usage_count === 1 ? 'time' : 'times'}
        {by ? ` · by ${by}` : ''}
      </p>
      <ul aria-label="Channels in this template" style={{ textAlign: 'left', margin: 0 }}>
        {channels
          .filter((c) => c.kind !== 4)
          .slice(0, 12)
          .map((c) => (
            <li key={c.local_id}>
              {KIND_PREFIX[c.kind] ?? ''}
              {c.name}
            </li>
          ))}
      </ul>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (nameOk) void create();
        }}
        style={{ display: 'flex', flexDirection: 'column', gap: 12, textAlign: 'left' }}
      >
        <TextInput
          label="Server name"
          value={name}
          maxLength={100}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" loading={creating} disabled={!nameOk}>
          Create server
        </Button>
        {error && <p role="alert">{error}</p>}
      </form>
    </>
  );
}
