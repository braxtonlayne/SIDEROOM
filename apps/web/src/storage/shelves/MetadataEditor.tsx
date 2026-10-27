import { Button, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { lookup, lookupProviders, type LookupCandidate } from '../../review/apiP13';
import { storageErrorText } from '../../settings/user/storage/errors';
import {
  patchShelfItem,
  patchShelfPart,
  shelfKeys,
  type ShelfDetail,
  type ShelfPatch,
  type ShelfProvider,
} from './api';
import './shelves.css';

const list = (s: string) =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);

const PROVIDERS: ShelfProvider[] = ['tmdb', 'anilist', 'mangadex', 'mangaupdates', 'comicvine'];
const isProvider = (p: string): p is ShelfProvider => (PROVIDERS as string[]).includes(p);

/**
 * The metadata editor of a shelf item (BYOS-CAT-002): title, year, tags, genres, synopsis, artwork from the
 * bucket, and for a series its episodes and chapters. "Fetch details" asks the server's lookup providers (the same
 * ones as the submission editor, off unless the operator switched them on); the member picks a match and the
 * fields are filled in, with the provider's attribution kept on the item. Every saved field becomes an override a
 * re-index leaves alone.
 */
export function MetadataEditor({ detail, onSaved }: { detail: ShelfDetail; onSaved?: () => void }) {
  const t = useT();
  const qc = useQueryClient();
  const { item } = detail;
  const section = item.section;
  const [title, setTitle] = useState(item.title);
  const [year, setYear] = useState(item.year ? String(item.year) : '');
  const [tags, setTags] = useState(item.tags.join(', '));
  const [genres, setGenres] = useState(item.genres.join(', '));
  const [synopsis, setSynopsis] = useState(item.synopsis);
  const [poster, setPoster] = useState('');
  const [artist, setArtist] = useState('');
  const [album, setAlbum] = useState('');
  const [picked, setPicked] = useState<LookupCandidate | null>(null);
  const [matches, setMatches] = useState<LookupCandidate[] | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const lookupSection = section === 'cinema' ? 'cinema' : 'library';

  const providers = useQuery({
    queryKey: ['shelves', 'providers', lookupSection],
    queryFn: () => lookupProviders(lookupSection),
    enabled: section !== 'music',
    retry: false,
  });
  const fetchDetails = useMutation({
    mutationFn: async () => {
      const p = providers.data?.providers[0];
      if (!p) return [];
      const y = Number(year);
      const res = await lookup({
        provider: p.id,
        q: title,
        section: lookupSection,
        ...(Number.isInteger(y) && y > 0 ? { year: y } : {}),
      });
      return res.candidates;
    },
    onSuccess: (c) => {
      setMatches(c);
      setMessage(c.length ? null : { ok: false, text: t('shelves.editor.fetchNone') });
    },
    onError: () => setMessage({ ok: false, text: t('shelves.editor.fetchFailed') }),
  });

  const adopt = (c: LookupCandidate) => {
    setPicked(c);
    setTitle(c.title);
    if (c.year) setYear(String(c.year));
    if (c.synopsis) setSynopsis(c.synopsis);
    if (c.genres.length) setGenres(c.genres.join(', '));
    setMatches(null);
  };

  const save = useMutation({
    mutationFn: () => {
      const body: ShelfPatch = {};
      if (title.trim() !== item.title) body.title = title.trim();
      const y = year.trim() === '' ? null : Number(year);
      if (y !== item.year) body.year = y;
      if (tags !== item.tags.join(', ')) body.tags = list(tags);
      if (section !== 'music') {
        if (genres !== item.genres.join(', ')) body.genres = list(genres);
        if (synopsis !== item.synopsis) body.synopsis = synopsis;
      }
      if (poster.trim()) body.poster_key = poster.trim();
      if (section === 'music') {
        if (artist.trim()) body.artist = artist.trim();
        if (album.trim()) body.album = album.trim();
      }
      if (picked && isProvider(picked.provider)) {
        body.details = { provider: picked.provider, external_id: picked.external_id };
      }
      return patchShelfItem(section, item.id, body);
    },
    onSuccess: (d) => {
      qc.setQueryData(shelfKeys.item(section, item.id), d);
      void qc.invalidateQueries({ queryKey: ['shelves', section] });
      setMessage({ ok: true, text: t('shelves.editor.saved') });
      setPicked(null);
      onSaved?.();
    },
    onError: (e) =>
      setMessage({
        ok: false,
        text: e instanceof ApiError ? storageErrorText(e) : t('shelves.editor.failed'),
      }),
  });

  return (
    <form
      className="shv-editor"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate();
      }}
    >
      <TextInput
        label={t('shelves.editor.title')}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      <TextInput
        label={t('shelves.editor.year')}
        inputMode="numeric"
        value={year}
        onChange={(e) => setYear(e.target.value)}
      />
      {section === 'music' ? (
        <>
          <TextInput
            label={t('shelves.editor.artist')}
            value={artist}
            placeholder={item.subtitle}
            onChange={(e) => setArtist(e.target.value)}
          />
          <TextInput
            label={t('shelves.editor.album')}
            value={album}
            onChange={(e) => setAlbum(e.target.value)}
          />
        </>
      ) : (
        <>
          <TextInput
            label={t('shelves.editor.genres')}
            value={genres}
            onChange={(e) => setGenres(e.target.value)}
          />
          <TextArea
            label={t('shelves.editor.synopsis')}
            value={synopsis}
            onChange={(e) => setSynopsis(e.target.value)}
          />
        </>
      )}
      <TextInput
        label={t('shelves.editor.tags')}
        value={tags}
        onChange={(e) => setTags(e.target.value)}
      />
      <TextInput
        label={t('shelves.editor.poster')}
        hint={t('shelves.editor.posterHint')}
        value={poster}
        onChange={(e) => setPoster(e.target.value)}
      />

      {section !== 'music' && (
        <div className="shv-field">
          {providers.data && !providers.data.enabled ? (
            <p className="shv-muted">{t('shelves.editor.fetchOff')}</p>
          ) : (
            <Button
              type="button"
              variant="secondary"
              loading={fetchDetails.isPending}
              disabled={!providers.data?.enabled}
              onClick={() => fetchDetails.mutate()}
            >
              {fetchDetails.isPending ? t('shelves.editor.fetching') : t('shelves.editor.fetch')}
            </Button>
          )}
          {matches && matches.length > 0 && (
            <ul className="shv-candidates" aria-label={t('shelves.editor.candidates')}>
              {matches.map((c) => (
                <li key={`${c.provider}:${c.external_id}`} className="shv-part">
                  <span>
                    {c.title}
                    {c.year ? ` (${c.year})` : ''}
                  </span>
                  <Button type="button" size="sm" variant="secondary" onClick={() => adopt(c)}>
                    {t('shelves.editor.useThis')}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {message && <Notice variant={message.ok ? 'success' : 'danger'}>{message.text}</Notice>}
      <div className="shv-actions">
        <Button type="submit" loading={save.isPending}>
          {t('shelves.editor.save')}
        </Button>
      </div>

      {detail.parts.length > 0 && <PartsEditor detail={detail} />}
    </form>
  );
}

/** Retitle, renumber or move one episode or chapter. */
function PartsEditor({ detail }: { detail: ShelfDetail }) {
  const t = useT();
  const qc = useQueryClient();
  const { item } = detail;
  return (
    <fieldset className="shv-field">
      <legend>{t('shelves.editor.parts')}</legend>
      <ul className="shv-parts">
        {detail.parts.map((p) => (
          <PartRow
            key={p.id}
            initial={{ title: p.title, number: p.number, season: p.season }}
            label={`${item.section === 'cinema' ? `S${p.season ?? 1}E` : '#'}${p.number}`}
            showSeason={item.section === 'cinema'}
            onSave={async (body) => {
              const d = await patchShelfPart(item.section, item.id, p.id, body);
              qc.setQueryData(shelfKeys.item(item.section, item.id), d);
            }}
          />
        ))}
      </ul>
    </fieldset>
  );
}

function PartRow({
  initial,
  label,
  onSave,
  showSeason,
}: {
  initial: { title: string; number: number; season: number | undefined };
  label: string;
  onSave: (b: { title: string; number: number; season?: number }) => Promise<void>;
  showSeason: boolean;
}) {
  const t = useT();
  const [title, setTitle] = useState(initial.title);
  const [number, setNumber] = useState(String(initial.number));
  const [season, setSeason] = useState(String(initial.season ?? 1));
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  return (
    <li className="shv-part">
      <TextInput
        label={t('shelves.editor.partTitle', { label })}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      {showSeason && (
        <TextInput
          label={t('shelves.editor.partSeason', { label })}
          inputMode="numeric"
          value={season}
          onChange={(e) => setSeason(e.target.value)}
        />
      )}
      <TextInput
        label={t('shelves.editor.partNumber', { label })}
        inputMode="decimal"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <Button
        type="button"
        size="sm"
        variant="secondary"
        loading={busy}
        onClick={async () => {
          setBusy(true);
          setErr(null);
          try {
            await onSave({
              title,
              number: Number(number),
              ...(showSeason ? { season: Number(season) } : {}),
            });
          } catch (e) {
            setErr(e instanceof ApiError ? storageErrorText(e) : t('shelves.editor.failed'));
          } finally {
            setBusy(false);
          }
        }}
      >
        {t('shelves.editor.partSave', { label })}
      </Button>
      {err && <Notice variant="danger">{err}</Notice>}
    </li>
  );
}
