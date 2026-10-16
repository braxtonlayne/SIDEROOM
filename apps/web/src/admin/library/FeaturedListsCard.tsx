// Reading lists featured on the Library home (ADM-CNT-004): staff feature a public list by its id (the number in
// the list's address) and take a feature away. Needs PUBLISH_READING; a featured list must stay public.
import { Button, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { ApiProblem, useResource } from '../ui';
import * as api from './api';

export function FeaturedListsCard() {
  const t = useT();
  const res = useResource('lib-featured-lists', api.featuredLists);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const lists = res.data ?? [];
  const run = async (id: string, featured: boolean) => {
    setError(null);
    setBusy(true);
    try {
      await api.setListFeatured(id, featured);
      if (featured) setDraft('');
      await res.reload();
    } catch (e) {
      setError(api.errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="ss-card" aria-labelledby="lib-featured-h">
      <h2 id="lib-featured-h">{t('admin.library.featuredTitle')}</h2>
      <p className="ss-meta">{t('admin.library.featuredHint')}</p>
      <ApiProblem error={error ?? res.error} />
      {lists.length === 0 ? (
        !res.loading && <p className="ss-na">{t('admin.library.featuredNone')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.library.featuredList')}>
          {lists.map((l) => (
            <li key={l.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">{l.name}</div>
                <div className="ss-meta">
                  {t('admin.library.featuredMeta', { owner: l.owner_name, count: l.item_count })}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                disabled={busy}
                aria-label={t('admin.library.unfeatureLabel', { name: l.name })}
                onClick={() => void run(l.id, false)}
              >
                {t('admin.library.unfeature')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="ss-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          if (/^\d+$/.test(draft.trim())) void run(draft.trim(), true);
        }}
      >
        <TextInput
          label={t('admin.library.featureListId')}
          inputMode="numeric"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" disabled={busy || !/^\d+$/.test(draft.trim())}>
          {t('admin.library.feature')}
        </Button>
      </form>
    </section>
  );
}
