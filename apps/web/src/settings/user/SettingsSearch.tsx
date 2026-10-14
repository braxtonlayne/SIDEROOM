import { useId, useState, type KeyboardEvent } from 'react';
import { useT } from '../../i18n';
import { searchSettings, sectionLabel, type SettingsEntry } from './searchIndex';

/**
 * The search box above the section list (LNG-USET-001). Typing lists the settings that match, by name or by
 * the words people use for them; choosing one opens its section and scrolls to its card.
 */
export function SettingsSearch({ onPick }: { onPick: (entry: SettingsEntry) => void }) {
  const t = useT();
  const [query, setQuery] = useState('');
  const listId = useId();
  const results = searchSettings(query);
  const pick = (e: SettingsEntry) => {
    setQuery('');
    onPick(e);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && results[0]) {
      e.preventDefault();
      pick(results[0]);
    } else if (e.key === 'Escape' && query) {
      // Clear the box first; a second Escape closes Settings.
      e.preventDefault();
      e.stopPropagation();
      e.nativeEvent.stopImmediatePropagation();
      setQuery('');
    }
  };
  return (
    <div role="search" className="us-search">
      <input
        type="search"
        className="us-search-input"
        aria-label={t('settings.search.aria')}
        placeholder={t('settings.search.placeholder')}
        autoComplete="off"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKey}
        aria-controls={query ? listId : undefined}
      />
      {query.trim() !== '' && (
        <ul id={listId} className="us-search-results" aria-label={t('settings.search.results')}>
          {results.length === 0 ? (
            <li className="us-muted">{t('settings.search.noMatch', { query: query.trim() })}</li>
          ) : (
            results.map((r) => (
              <li key={`${r.section}-${r.card ?? r.label}`}>
                <button type="button" className="us-search-hit" onClick={() => pick(r)}>
                  <span>{r.label}</span>
                  <span className="us-muted">{sectionLabel(r.section)}</span>
                </button>
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}
