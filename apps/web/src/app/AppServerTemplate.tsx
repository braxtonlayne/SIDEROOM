import { Button, TextInput, ThemeProvider } from '@sideroom/ui';
import { useNavigate, useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch, apiPost } from '../api/client';
import { getMe } from '../api/entrance';
import { useT } from '../i18n/react';
import '../components/entrance.css';

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

const KIND_PREFIX: Record<number, string> = { 0: '#', 5: '#', 15: '#' };

/**
 * Public page for a server template (LNG-SSET-071), `/template/:code`. The preview needs a session (the API
 * answers 401 otherwise), so a signed-out visitor is sent to log in and comes back here.
 */
export function AppServerTemplate() {
  const t = useT();
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
          ? t('entrance.template.tooFast')
          : e instanceof ApiError && e.status === 400
            ? t('entrance.template.badName')
            : t('entrance.template.failed'),
      );
      setCreating(false);
    }
  };

  const nameOk = name.trim().length >= 2 && name.trim().length <= 100;

  return (
    <ThemeProvider section="lounge" className="ent-invite-page">
      <main className="ent-invite-card" aria-busy={state.kind === 'loading'}>
        {state.kind === 'loading' && <p role="status">{t('entrance.template.loading')}</p>}
        {state.kind === 'invalid' && <p role="alert">{t('entrance.template.invalid')}</p>}
        {state.kind === 'login' && (
          <>
            <h1>{t('entrance.template.loginTitle')}</h1>
            <a
              href={`/login?next=${encodeURIComponent(`/template/${code}`)}`}
              className="ent-invite-cta"
            >
              {t('entrance.template.login')}
            </a>
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
  const t = useT();
  const tpl = state.template;
  const channels = [...(tpl.serialized_source_guild.channels ?? [])].sort(
    (a, b) => a.position - b.position,
  );
  const categories = channels.filter((c) => c.kind === 4);
  const roles = tpl.serialized_source_guild.roles?.length ?? 0;
  const by = tpl.creator?.global_name || tpl.creator?.username;
  return (
    <>
      <p>{t('entrance.template.start')}</p>
      <h1>{tpl.name}</h1>
      {tpl.description && <p>{tpl.description}</p>}
      <p>
        {t('entrance.template.stats', {
          channels: channels.length - categories.length,
          roles,
          uses: tpl.usage_count,
        })}
        {by ? t('entrance.template.by', { name: by }) : ''}
      </p>
      <ul
        aria-label={t('entrance.template.channelsLabel')}
        style={{ textAlign: 'start', margin: 0 }}
      >
        {channels
          .filter((c) => c.kind !== 4)
          .slice(0, 12)
          .map((c) => (
            <li key={c.local_id}>
              {c.kind === 2 ? t('entrance.template.voicePrefix') : (KIND_PREFIX[c.kind] ?? '')}
              {c.name}
            </li>
          ))}
      </ul>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (nameOk) void create();
        }}
        style={{ display: 'flex', flexDirection: 'column', gap: 12, textAlign: 'start' }}
      >
        <TextInput
          label={t('entrance.template.name')}
          value={name}
          maxLength={100}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" loading={creating} disabled={!nameOk}>
          {t('entrance.template.create')}
        </Button>
        {error && <p role="alert">{error}</p>}
      </form>
    </>
  );
}
