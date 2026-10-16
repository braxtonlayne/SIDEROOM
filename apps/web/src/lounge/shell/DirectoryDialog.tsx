import { Avatar, Button, EmptyState, Modal, Spinner, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { cdnUrl } from '../../store/cdn';
import { useGuilds } from '../data';
import { useGotoGuild } from '../AddServerDialog';
import { TraitChips } from '../components/TraitChips';
import {
  joinListedGuild,
  listDirectoryCategories,
  searchDirectory,
  type DirectoryGuild,
} from './directory';

const PAGE = 24;
type Load = { kind: 'loading' } | { kind: 'error' } | { kind: 'ready'; items: DirectoryGuild[] };

/** Category ids are machine values; the label is the id, title-cased. */
const prettyCategory = (id: string) =>
  id.replace(/[-_]+/g, ' ').replace(/^./, (c) => c.toUpperCase());

function Card({ g, onJoined }: { g: DirectoryGuild; onJoined: (id: string) => void }) {
  const t = useT();
  const goto = useGotoGuild();
  const mine = useGuilds().some((x) => x.id === g.id);
  const joined = g.joined || mine;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const join = async () => {
    setBusy(true);
    setError(null);
    try {
      await joinListedGuild(g.id);
      onJoined(g.id);
      goto(g.id);
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.directory.joinError'));
      setBusy(false);
    }
  };
  const banner = cdnUrl('icons', g.id, g.banner, { size: 512 });
  return (
    <li className="lng-dir-card" aria-label={g.name}>
      <div
        className="lng-dir-banner"
        aria-hidden="true"
        style={banner ? { backgroundImage: `url(${banner})` } : undefined}
      />
      <div className="lng-dir-body">
        <Avatar name={g.name} src={cdnUrl('icons', g.id, g.icon, { size: 64 })} size={56} />
        <h3 className="lng-dir-name">
          {g.name}
          {g.featured && <span className="lng-tag">{t('lounge.directory.featured')}</span>}
        </h3>
        <p className="lng-dir-desc">{g.description || t('lounge.directory.noDescription')}</p>
        <TraitChips traits={g.traits} label={t('lounge.directory.traits', { name: g.name })} />
        <p className="lng-muted lng-dir-meta">
          {g.approximate_presence_count != null && (
            <span>{t('lounge.directory.online', { count: g.approximate_presence_count })}</span>
          )}
          <span>{t('lounge.directory.members', { count: g.approximate_member_count })}</span>
        </p>
        {joined ? (
          <Button size="sm" variant="secondary" onClick={() => goto(g.id)}>
            {t('lounge.directory.open')}
          </Button>
        ) : (
          <Button size="sm" loading={busy} onClick={() => void join()}>
            {t('lounge.directory.join')}
          </Button>
        )}
        {error && (
          <p role="alert" className="lng-failed">
            {error}
          </p>
        )}
      </div>
    </li>
  );
}

/** The Server Directory (LNG-RAIL-011): search and browse listed servers, join one without an invite. */
export function DirectoryDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [query, setQuery] = useState('');
  const [debounced, setDebounced] = useState('');
  const [category, setCategory] = useState('');
  const [categories, setCategories] = useState<string[]>([]);
  const [load, setLoad] = useState<Load>({ kind: 'loading' });
  const [more, setMore] = useState<'idle' | 'busy' | 'done'>('idle');
  const seq = useRef(0);

  useEffect(() => {
    let live = true;
    listDirectoryCategories().then(
      (c) => live && setCategories(c),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(query), 250);
    return () => clearTimeout(id);
  }, [query]);

  useEffect(() => {
    const mine = ++seq.current;
    setLoad({ kind: 'loading' });
    setMore('idle');
    searchDirectory({ query: debounced, category, limit: PAGE }).then(
      (items) => {
        if (mine !== seq.current) return;
        setLoad({ kind: 'ready', items });
        if (items.length < PAGE) setMore('done');
      },
      () => mine === seq.current && setLoad({ kind: 'error' }),
    );
  }, [debounced, category]);

  const loadMore = async () => {
    if (load.kind !== 'ready') return;
    setMore('busy');
    try {
      const next = await searchDirectory({
        query: debounced,
        category,
        limit: PAGE,
        offset: load.items.length,
      });
      setLoad({ kind: 'ready', items: [...load.items, ...next] });
      setMore(next.length < PAGE ? 'done' : 'idle');
    } catch {
      setMore('idle');
    }
  };

  const markJoined = (id: string) =>
    setLoad((l) =>
      l.kind === 'ready'
        ? { kind: 'ready', items: l.items.map((g) => (g.id === id ? { ...g, joined: true } : g)) }
        : l,
    );

  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={t('lounge.directory.title')}
      description={t('lounge.directory.subtitle')}
    >
      <div className="lng-dir">
        <TextInput
          type="search"
          label={t('lounge.directory.searchLabel')}
          placeholder={t('lounge.directory.searchPlaceholder')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {categories.length > 0 && (
          <div role="group" aria-label={t('lounge.directory.categories')} className="lng-dir-cats">
            {['', ...categories].map((c) => (
              <button
                key={c || 'all'}
                type="button"
                className="lng-dir-cat"
                aria-pressed={category === c}
                onClick={() => setCategory(c)}
              >
                {c ? prettyCategory(c) : t('lounge.directory.all')}
              </button>
            ))}
          </div>
        )}
        {load.kind === 'loading' && (
          <p role="status">
            <Spinner label={t('lounge.directory.loading')} />
          </p>
        )}
        {load.kind === 'error' && <p role="alert">{t('lounge.directory.loadError')}</p>}
        {load.kind === 'ready' &&
          (load.items.length === 0 ? (
            <EmptyState
              title={t('lounge.directory.emptyTitle')}
              description={t('lounge.directory.emptyBody')}
            />
          ) : (
            <>
              <ul className="lng-dir-grid" aria-label={t('lounge.directory.results')}>
                {load.items.map((g) => (
                  <Card key={g.id} g={g} onJoined={markJoined} />
                ))}
              </ul>
              {more !== 'done' && (
                <Button
                  variant="secondary"
                  loading={more === 'busy'}
                  onClick={() => void loadMore()}
                >
                  {t('lounge.directory.loadMore')}
                </Button>
              )}
            </>
          ))}
      </div>
    </Modal>
  );
}
