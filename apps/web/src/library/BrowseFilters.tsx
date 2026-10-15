// Chip pickers for the Browse filter drawer (LIB-BRW-001): genres cycle off, included, excluded; tags are a
// token field. Both read and write the comma-separated strings the URL carries.
import { Button, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { label } from './ui';

export const csvList = (v: string | undefined): string[] =>
  (v ?? '')
    .split(',')
    .map((x) => x.trim().toLowerCase())
    .filter(Boolean);

const join = (l: string[]) => [...new Set(l)].join(',');

/** Genres seen in the catalog plus the common ones; the order is stable so chips do not jump. */
export function genreOptions(seen: string[], defaults: string[], chosen: string[]) {
  return [...new Set([...chosen, ...defaults, ...seen])];
}

export type GenreState = 'off' | 'include' | 'exclude';

const STATE_KEY = {
  off: 'library.browse.genreStateOff',
  include: 'library.browse.genreStateInclude',
  exclude: 'library.browse.genreStateExclude',
} as const;

export const nextState = (s: GenreState): GenreState =>
  s === 'off' ? 'include' : s === 'include' ? 'exclude' : 'off';

export function GenrePicker({
  include,
  exclude,
  options,
  onChange,
}: {
  include: string;
  exclude: string;
  options: string[];
  onChange: (next: { genres: string; exclude: string }) => void;
}) {
  const t = useT();
  const [custom, setCustom] = useState('');
  const inc = csvList(include);
  const exc = csvList(exclude);
  const all = genreOptions(options, [], [...inc, ...exc]);
  const stateOf = (g: string): GenreState =>
    inc.includes(g) ? 'include' : exc.includes(g) ? 'exclude' : 'off';
  const set = (g: string, s: GenreState) =>
    onChange({
      genres: join(s === 'include' ? [...inc, g] : inc.filter((x) => x !== g)),
      exclude: join(s === 'exclude' ? [...exc, g] : exc.filter((x) => x !== g)),
    });
  const add = () => {
    const g = custom.trim().toLowerCase();
    if (g) set(g, 'include');
    setCustom('');
  };
  return (
    <fieldset className="lib-genre-picker">
      <legend>{t('library.browse.genres')}</legend>
      <p className="lib-muted">{t('library.browse.genresCycleHint')}</p>
      <ul className="lib-genre-chips">
        {all.map((g) => {
          const s = stateOf(g);
          return (
            <li key={g}>
              <button
                type="button"
                className="lib-genre-chip lib-tri-chip"
                data-state={s}
                aria-pressed={s === 'off' ? false : s === 'include' ? true : 'mixed'}
                aria-label={t(STATE_KEY[s], { genre: label(g) })}
                onClick={() => set(g, nextState(s))}
              >
                {s === 'exclude' ? '− ' : s === 'include' ? '+ ' : ''}
                {label(g)}
              </button>
            </li>
          );
        })}
      </ul>
      <div className="lib-token-add">
        <TextInput
          label={t('library.browse.addGenre')}
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button variant="secondary" size="sm" type="button" onClick={add} disabled={!custom.trim()}>
          {t('library.browse.add')}
        </Button>
      </div>
    </fieldset>
  );
}

export function TagField({ value, onChange }: { value: string; onChange: (next: string) => void }) {
  const t = useT();
  const [draft, setDraft] = useState('');
  const tags = csvList(value);
  const add = () => {
    const x = draft.trim().toLowerCase();
    if (x) onChange(join([...tags, x]));
    setDraft('');
  };
  return (
    <fieldset className="lib-genre-picker">
      <legend>{t('library.browse.tags')}</legend>
      <ul className="lib-genre-chips">
        {tags.map((g) => (
          <li key={g}>
            <button
              type="button"
              className="lib-genre-chip"
              aria-label={t('library.browse.removeTag', { tag: label(g) })}
              onClick={() => onChange(join(tags.filter((x) => x !== g)))}
            >
              {label(g)} {'×'}
            </button>
          </li>
        ))}
      </ul>
      <div className="lib-token-add">
        <TextInput
          label={t('library.browse.addTag')}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button variant="secondary" size="sm" type="button" onClick={add} disabled={!draft.trim()}>
          {t('library.browse.add')}
        </Button>
      </div>
    </fieldset>
  );
}
