// "Naming guide" with a tester (REV-UP-008): paste file names and see how the importer reads
// them (docs/specs/ingest-naming-conventions.md). Nothing is stored.
import { Button, Collapsible, Select, TextArea } from '@sideroom/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import * as rv from './api';
import type { NamingItem } from './types';
import './review.css';

const ROLE: Record<string, MessageKey> = {
  video: 'review.naming.role.video',
  subtitle: 'review.naming.role.subtitle',
  artwork: 'review.naming.role.artwork',
  unknown: 'review.naming.role.unknown',
};
const LIB_ROLE: Record<string, MessageKey> = {
  page: 'review.naming.libRole.page',
  archive: 'review.naming.libRole.archive',
  document: 'review.naming.libRole.document',
  series_json: 'review.naming.libRole.seriesJson',
  comic_info: 'review.naming.libRole.comicInfo',
  folder: 'review.naming.libRole.folder',
  unknown: 'review.naming.libRole.unknown',
};

function describe(it: NamingItem, t: ReturnType<typeof useT>): string {
  const bits: string[] = [];
  if (it.cinema) {
    const c = it.cinema;
    bits.push(t(ROLE[c.role] ?? 'review.naming.role.unknown'));
    if (c.title) bits.push(c.year ? `${c.title} (${c.year})` : c.title);
    if (c.season !== null && c.episodes.length > 0)
      bits.push(t('review.naming.episode', { season: c.season, episodes: c.episodes.join(', ') }));
    if (c.episode_title) bits.push(c.episode_title);
    if (c.variant) bits.push(c.variant);
    if (c.language) bits.push(c.language);
  } else if (it.library) {
    const l = it.library;
    bits.push(t(LIB_ROLE[l.role] ?? 'review.naming.libRole.unknown'));
    if (l.series) bits.push(l.series);
    if (l.volume !== null) bits.push(t('review.naming.volume', { n: l.volume }));
    if (l.chapter !== null) bits.push(t('review.naming.chapter', { n: l.chapter }));
    if (l.page !== null) bits.push(t('review.naming.page', { n: l.page }));
    if (l.title) bits.push(l.title);
    if (l.group) bits.push(`[${l.group}]`);
  }
  return bits.join(' · ');
}

export function NamingGuide({ section = 'library' }: { section?: 'library' | 'cinema' }) {
  const t = useT();
  const [kind, setKind] = useState<string>(section);
  const [text, setText] = useState('');
  const parse = useMutation({
    mutationFn: () =>
      rv.parseNames(
        kind,
        text
          .split('\n')
          .map((l) => l.trim())
          .filter(Boolean)
          .slice(0, 200),
      ),
  });
  return (
    <Collapsible title={t('review.naming.title')} headingLevel={2}>
      <div className="rv-stack">
        <p className="rv-muted" style={{ margin: 0 }}>
          {kind === 'library' ? t('review.naming.helpLibrary') : t('review.naming.helpCinema')}
        </p>
        <Select
          aria-label={t('review.naming.section')}
          options={[
            { value: 'library', label: t('review.naming.sectionLibrary') },
            { value: 'cinema', label: t('review.naming.sectionCinema') },
          ]}
          value={kind}
          onValueChange={setKind}
        />
        <TextArea
          label={t('review.naming.names')}
          hint={t('review.naming.namesHint')}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="rv-row">
          <Button
            variant="secondary"
            disabled={!text.trim()}
            loading={parse.isPending}
            onClick={() => parse.mutate()}
          >
            {t('review.naming.test')}
          </Button>
        </div>
        {parse.isError && <p role="alert">{t('review.naming.error')}</p>}
        {parse.data && (
          <table className="rv-table" aria-label={t('review.naming.results')}>
            <thead>
              <tr>
                <th scope="col">{t('review.naming.colName')}</th>
                <th scope="col">{t('review.naming.colParsed')}</th>
              </tr>
            </thead>
            <tbody>
              {parse.data.items.map((it, i) => (
                <tr key={`${i}-${it.name}`}>
                  <td>{it.name}</td>
                  <td>
                    {describe(it, t)}
                    {[...(it.cinema?.warnings ?? []), ...(it.library?.warnings ?? [])].map((w) => (
                      <div key={w} className="rv-muted">
                        {w}
                      </div>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </Collapsible>
  );
}
