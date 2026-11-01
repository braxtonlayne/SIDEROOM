import { useMemo, useState } from 'react';
import { formatDate, formatNumber, useT } from '../i18n';
import { RoomsNav } from '../study/RoomsNav';
import { StudyAreas } from '../study/StudyAreas';
import '../study/study.css';
import { FIXTURE_SHELVES, type FixtureKind, type FixtureShelf } from './fixtures';
import { previewHint } from './state';
import { rowsFor, shelvesFor, type KindFilter, type SourceFilter } from './studyModel';
import './showroom.css';

const SOURCES: SourceFilter[] = ['all', 'mine', 'friend', 'server'];
const KINDS: KindFilter[] = ['all', 'film', 'book', 'comic', 'music', 'doc'];

/**
 * The Study with sample shelves (UX-SHOW-001): My Files, several friends' shelves and several servers' shelves, with the
 * source filter and the local metadata view. Everything is local state over fixtures: no query, no request. Pin, open
 * and edit only show the preview hint.
 */
export default function PreviewStudy() {
  const t = useT();
  const [source, setSource] = useState<SourceFilter>('all');
  const [shelfId, setShelfId] = useState<string | null>(null);
  const [kind, setKind] = useState<KindFilter>('all');
  const [picked, setPicked] = useState<string | null>(null);

  const rows = useMemo(
    () => rowsFor(FIXTURE_SHELVES, source, shelfId, kind),
    [source, shelfId, kind],
  );
  const detail = rows.find((r) => r.item.id === picked) ?? null;
  const noop = () => previewHint(t('showroom.hint.study'));

  const chooseSource = (s: SourceFilter) => {
    setSource(s);
    setShelfId(null);
    setPicked(null);
  };
  const sourceLabel = (s: SourceFilter) => t(`showroom.study.source.${s}` as const);
  const kindLabel = (k: KindFilter) => t(`showroom.study.kind.${k}` as const);
  const owner = (s: FixtureShelf) =>
    s.source === 'mine'
      ? t('showroom.study.fromMine')
      : s.source === 'friend'
        ? t('showroom.study.fromFriend', { name: s.owner ?? '' })
        : t('showroom.study.fromServer', { name: s.owner ?? '' });

  const group = (src: FixtureShelf['source'], title: string) => {
    const list = shelvesFor(FIXTURE_SHELVES, src);
    return (
      list.length > 0 && (
        <section className="stu-group" aria-label={title}>
          <h2>{title}</h2>
          <ul>
            {list.map((s) => (
              <li key={s.id}>
                <button
                  type="button"
                  className="stu-entry"
                  aria-current={shelfId === s.id ? 'true' : undefined}
                  onClick={() => {
                    setSource('all');
                    setShelfId(s.id);
                    setPicked(null);
                  }}
                >
                  {s.source === 'mine' ? t('study.side.mine') : (s.owner ?? s.label)}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )
    );
  };

  return (
    <>
      <header className="stu-bar">
        <RoomsNav current="study" className="stu-bar-nav" linkClassName="stu-bar-link" />
        <StudyAreas current="shelves" className="stu-bar-nav" linkClassName="stu-bar-link" />
      </header>
      <div className="stu-page" data-testid="showroom-study">
        <nav className="stu-side" aria-label={t('study.side.label')}>
          <h1>{t('study.title')}</h1>
          {group('mine', t('study.side.mineGroup'))}
          {group('friend', t('study.side.friends'))}
          {group('server', t('study.side.guilds'))}
        </nav>
        <main className="stu-main">
          <div className="srm-filters" role="group" aria-label={t('showroom.study.sourceFilter')}>
            {SOURCES.map((s) => (
              <button
                key={s}
                type="button"
                className="srm-chip"
                aria-pressed={source === s && shelfId === null}
                onClick={() => chooseSource(s)}
              >
                {sourceLabel(s)}
              </button>
            ))}
          </div>
          <div className="srm-filters" role="group" aria-label={t('showroom.study.kindFilter')}>
            {KINDS.map((k) => (
              <button
                key={k}
                type="button"
                className="srm-chip"
                aria-pressed={kind === k}
                onClick={() => setKind(k)}
              >
                {kindLabel(k)}
              </button>
            ))}
          </div>
          <p className="srm-count" role="status">
            {t('showroom.study.count', { n: rows.length })}
          </p>
          {rows.length === 0 && <p className="stu-muted">{t('showroom.study.empty')}</p>}
          <ul className="srm-rows" aria-label={t('showroom.study.items')}>
            {rows.map(({ shelf, item }) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="srm-row"
                  aria-pressed={picked === item.id}
                  onClick={() => setPicked(item.id)}
                >
                  <span className="srm-row-title">{item.title}</span>
                  <span className="srm-row-meta">
                    {owner(shelf)} · {kindLabel(item.kind as FixtureKind)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {detail && (
            <aside className="srm-detail" aria-label={t('showroom.study.detail')}>
              <h2>{detail.item.title}</h2>
              <p className="srm-local">{t('showroom.study.localNote')}</p>
              <dl>
                <dt>{t('showroom.study.fromLabel')}</dt>
                <dd>{owner(detail.shelf)}</dd>
                <dt>{t('showroom.study.size')}</dt>
                <dd>
                  {t('showroom.study.sizeMb', {
                    mb: formatNumber(Math.round(detail.item.bytes / (1024 * 1024))),
                  })}
                </dd>
                <dt>{t('showroom.study.added')}</dt>
                <dd>{formatDate(new Date(detail.item.added))}</dd>
                <dt>{t('showroom.study.tags')}</dt>
                <dd>{detail.item.tags.join(', ')}</dd>
                <dt>{t('showroom.study.note')}</dt>
                <dd>{detail.item.note || t('showroom.study.noNote')}</dd>
              </dl>
              <div className="srm-actions">
                <button type="button" onClick={noop}>
                  {t('showroom.study.open')}
                </button>
                <button type="button" onClick={noop}>
                  {t('showroom.study.pin')}
                </button>
                <button type="button" onClick={noop}>
                  {t('showroom.study.editNote')}
                </button>
              </div>
            </aside>
          )}
        </main>
      </div>
    </>
  );
}
