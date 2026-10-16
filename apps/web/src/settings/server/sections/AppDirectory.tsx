import { Avatar, Badge, Button, EmptyState, Modal, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../../api/client';
import { useT } from '../../../i18n';
import { cdnUrl } from '../../../store';
import { useResource, type Viewer } from '../hooks';
import { Async } from '../ui';

/** The App Directory (DEV-BOT-012): released apps with a public bot, as the API lists them. */
export interface DirectoryApp {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  bot_id: string;
  tags: string[];
  installs: number;
}

export interface DirectoryPage {
  items: DirectoryApp[];
  has_more: boolean;
}

export interface DirectoryAppPage extends DirectoryApp {
  terms_of_service_url: string | null;
  privacy_policy_url: string | null;
  install_scopes: string[];
  install_permissions: string;
  commands: { name: string; description: string; type: number }[];
}

const PAGE_SIZE = 12;

export const searchDirectory = (q: { query: string; tag: string; page: number }) => {
  const p = new URLSearchParams({ per_page: String(PAGE_SIZE), page: String(q.page) });
  if (q.query.trim()) p.set('query', q.query.trim());
  if (q.tag) p.set('tag', q.tag);
  return apiFetch<DirectoryPage>(`/app-directory?${p.toString()}`, { retries: 0 });
};
export const directoryTags = () =>
  apiFetch<{ tag: string; count: number }[]>('/app-directory/tags', { retries: 0 });
export const directoryApp = (id: string) =>
  apiFetch<DirectoryAppPage>(`/app-directory/${encodeURIComponent(id)}`, { retries: 0 });

/** What "Add App" asks for: the application's own install parameters, `bot` and commands by default. */
export function addAppSearch(app: DirectoryAppPage, guildId: string) {
  const scopes = app.install_scopes.length ? app.install_scopes : ['bot', 'applications.commands'];
  return {
    client_id: app.id,
    scope: scopes.join(' '),
    permissions: app.install_permissions,
    guild_id: guildId,
  };
}

const iconOf = (a: DirectoryApp) =>
  a.icon ? cdnUrl('avatars', a.bot_id, a.icon, { size: 64 }) : null;

/** Search, categories (the apps' tags), and an app page with "Add App" for this server. */
export function AppDirectory({ guildId, viewer }: { guildId: string; viewer: Viewer }) {
  const t = useT();
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const [tag, setTag] = useState('');
  const [page, setPage] = useState(0);
  const [open, setOpen] = useState<string | null>(null);

  // Search as the person types, after a short pause.
  useEffect(() => {
    const id = setTimeout(() => {
      setQuery(text);
      setPage(0);
    }, 250);
    return () => clearTimeout(id);
  }, [text]);

  const tags = useResource('directory:tags', directoryTags);
  const res = useResource(`directory:${query}:${tag}:${page}`, () =>
    searchDirectory({ query, tag, page }),
  );

  return (
    <div className="ss-form" style={{ maxWidth: 'none' }}>
      <p className="ss-sub">{t('settings.appDirectory.intro')}</p>
      <TextInput
        label={t('settings.appDirectory.search')}
        type="search"
        value={text}
        maxLength={100}
        onChange={(e) => setText(e.target.value)}
      />
      {tags.data && tags.data.length > 0 && (
        <div role="group" aria-label={t('settings.appDirectory.categories')} className="ss-actions">
          <Button
            size="sm"
            variant={tag === '' ? 'primary' : 'ghost'}
            aria-pressed={tag === ''}
            onClick={() => {
              setTag('');
              setPage(0);
            }}
          >
            {t('settings.appDirectory.all')}
          </Button>
          {tags.data.map((c) => (
            <Button
              key={c.tag}
              size="sm"
              variant={tag === c.tag ? 'primary' : 'ghost'}
              aria-pressed={tag === c.tag}
              onClick={() => {
                setTag(c.tag);
                setPage(0);
              }}
            >
              {c.tag}
            </Button>
          ))}
        </div>
      )}
      <Async res={res} label={t('settings.appDirectory.loading')}>
        {(d) =>
          d.items.length === 0 ? (
            <EmptyState
              title={t('settings.appDirectory.emptyTitle')}
              description={t('settings.appDirectory.emptyDescription')}
            />
          ) : (
            <>
              <ul className="ss-list" aria-label={t('settings.appDirectory.listLabel')}>
                {d.items.map((a) => (
                  <li key={a.id} className="ss-row">
                    <Avatar name={a.name} src={iconOf(a)} size={40} />
                    <div className="ss-grow">
                      <strong>{a.name}</strong>
                      <p className="ss-sub">
                        {a.description || t('settings.appDirectory.noDescription')}
                      </p>
                      <p className="ss-sub">
                        {t('settings.appDirectory.installs', { n: a.installs })}
                      </p>
                      {a.tags.map((x) => (
                        <Badge key={x}>{x}</Badge>
                      ))}
                    </div>
                    <Button
                      variant="secondary"
                      aria-label={t('settings.appDirectory.viewLabel', { name: a.name })}
                      onClick={() => setOpen(a.id)}
                    >
                      {t('settings.appDirectory.view')}
                    </Button>
                  </li>
                ))}
              </ul>
              <div className="ss-actions">
                <Button variant="ghost" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
                  {t('settings.appDirectory.previous')}
                </Button>
                <Button
                  variant="ghost"
                  disabled={!d.has_more}
                  onClick={() => setPage((p) => p + 1)}
                >
                  {t('settings.appDirectory.next')}
                </Button>
              </div>
            </>
          )
        }
      </Async>
      {open && (
        <AppPage id={open} guildId={guildId} viewer={viewer} onClose={() => setOpen(null)} />
      )}
    </div>
  );
}

function AppPage({
  id,
  guildId,
  viewer,
  onClose,
}: {
  id: string;
  guildId: string;
  viewer: Viewer;
  onClose: () => void;
}) {
  const t = useT();
  const res = useResource(`directory:app:${id}`, () => directoryApp(id));
  const canAdd = viewer.can('MANAGE_GUILD');
  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={res.data?.name ?? t('settings.appDirectory.appTitle')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.appDirectory.close')}
          </Button>
          {res.data && canAdd && (
            <a
              href={`/oauth2/authorize?${new URLSearchParams(addAppSearch(res.data, guildId)).toString()}`}
              className="ss-btn-link"
            >
              {t('settings.appDirectory.addApp')}
            </a>
          )}
        </>
      }
    >
      <Async res={res} label={t('settings.appDirectory.loading')}>
        {(a) => (
          <div className="ss-form">
            <p>{a.description || t('settings.appDirectory.noDescription')}</p>
            {!canAdd && <p className="ss-sub">{t('settings.appDirectory.needsManageServer')}</p>}
            <h3>{t('settings.appDirectory.commands')}</h3>
            {a.commands.length === 0 ? (
              <p className="ss-sub">{t('settings.appDirectory.noCommands')}</p>
            ) : (
              <ul aria-label={t('settings.appDirectory.commandsLabel')}>
                {a.commands.map((c) => (
                  <li key={`${c.type}:${c.name}`}>
                    <code>{c.type === 1 ? `/${c.name}` : c.name}</code>{' '}
                    <span className="ss-sub">{c.description}</span>
                  </li>
                ))}
              </ul>
            )}
            <p className="ss-sub">
              {a.privacy_policy_url && (
                <a href={a.privacy_policy_url} target="_blank" rel="noreferrer noopener">
                  {t('settings.appDirectory.privacy')}
                </a>
              )}{' '}
              {a.terms_of_service_url && (
                <a href={a.terms_of_service_url} target="_blank" rel="noreferrer noopener">
                  {t('settings.appDirectory.terms')}
                </a>
              )}
            </p>
          </div>
        )}
      </Async>
    </Modal>
  );
}
