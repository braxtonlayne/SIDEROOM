// Library catalog editor (ADM-CNT-003): series list, series form, editions, chapters, publish and merge.
// Page reorder and replace need the upload pipeline and are not part of this screen yet.
import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  Drawer,
  Notice,
  Select,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { useT } from '../../i18n';
import { DataTable, type Column } from '../DataTable';
import {
  ApiProblem,
  Async,
  FormDialog,
  SectionHeader,
  fmtMs,
  useAdminViewer,
  usePaged,
  useResource,
} from '../ui';
import * as api from './api';

const PAGE = 50;
const list = (s: string) =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);

export function LibraryCatalogSection() {
  const t = useT();
  const [draft, setDraft] = useState({ title: '', published: '' });
  const [applied, setApplied] = useState({ title: '', published: '' });
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const paged = usePaged<api.Series, number>(
    `lib-series:${applied.title}:${applied.published}`,
    async (offset) => {
      const o = offset ?? 0;
      const p = await api.listSeries({
        title: applied.title,
        ...(applied.published ? { published: applied.published === 'yes' } : {}),
        limit: PAGE,
        offset: o,
      });
      const seen = o + p.items.length;
      return { items: p.items, next: seen < p.total ? seen : null };
    },
  );
  const columns: Column<api.Series>[] = [
    {
      id: 'title',
      header: t('admin.library.colTitle'),
      locked: true,
      csv: (s) => s.title,
      cell: (s) => <strong>{s.title}</strong>,
    },
    { id: 'kind', header: t('admin.library.colKind'), csv: (s) => s.kind, cell: (s) => s.kind },
    {
      id: 'status',
      header: t('admin.library.colStatus'),
      csv: (s) => s.status,
      cell: (s) => s.status,
    },
    {
      id: 'published',
      header: t('admin.library.colPublished'),
      csv: (s) => s.published,
      cell: (s) =>
        s.merged_into ? (
          <Badge>{t('admin.library.merged')}</Badge>
        ) : s.published ? (
          <Badge variant="success">{t('admin.library.published')}</Badge>
        ) : (
          <Badge>{t('admin.library.draft')}</Badge>
        ),
    },
    {
      id: 'chapters',
      header: t('admin.library.colChapters'),
      csv: (s) => s.chapter_count,
      cell: (s) => s.chapter_count,
    },
    {
      id: 'followers',
      header: t('admin.library.colFollowers'),
      defaultHidden: true,
      csv: (s) => s.follower_count,
      cell: (s) => s.follower_count,
    },
    {
      id: 'updated',
      header: t('admin.library.colUpdated'),
      csv: (s) => new Date(s.updated_at_ms).toISOString(),
      cell: (s) => fmtMs(s.updated_at_ms),
    },
  ];
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setApplied({ ...draft, title: draft.title.trim() });
  };

  return (
    <>
      <SectionHeader
        title={t('admin.library.title')}
        description={t('admin.library.description')}
        actions={<Button onClick={() => setEditing('new')}>{t('admin.library.newSeries')}</Button>}
      />
      <DataTable
        tableId="library-series"
        label={t('admin.library.tableLabel')}
        columns={columns}
        paged={paged}
        rowKey={(s) => s.id}
        emptyTitle={t('admin.library.emptyTitle')}
        emptyDescription={t('admin.library.emptyDescription')}
        actions={(s) => (
          <Button
            size="sm"
            variant="secondary"
            aria-label={t('admin.library.editAria', { title: s.title })}
            onClick={() => setEditing(s.id)}
          >
            {t('admin.library.edit')}
          </Button>
        )}
        filters={
          <form
            className="ss-toolbar"
            role="search"
            aria-label={t('admin.library.filterForm')}
            onSubmit={submit}
          >
            <TextInput
              label={t('admin.library.titleContains')}
              type="search"
              value={draft.title}
              onChange={(e) => setDraft({ ...draft, title: e.target.value })}
            />
            <Select
              aria-label={t('admin.library.filterPublished')}
              options={[
                { value: '', label: t('admin.library.anyVisibility') },
                { value: 'yes', label: t('admin.library.published') },
                { value: 'no', label: t('admin.library.draft') },
              ]}
              value={draft.published}
              onValueChange={(published) => {
                setDraft({ ...draft, published });
                setApplied({ title: draft.title.trim(), published });
              }}
            />
            <Button type="submit">{t('admin.library.search')}</Button>
          </form>
        }
      />
      {editing && (
        <SeriesDrawer
          id={editing}
          onClose={() => setEditing(null)}
          onChanged={() => paged.reload()}
          onCreated={(id) => {
            setEditing(id);
            paged.reload();
          }}
        />
      )}
    </>
  );
}

const blank: api.SeriesInput = {
  kind: 'manga',
  title: '',
  alt_titles: [],
  authors: [],
  artists: [],
  status: 'ongoing',
  year: null,
  synopsis: '',
  genres: [],
  tags: [],
  reading_direction: 'rtl',
  age_restricted: false,
};

function SeriesDrawer({
  id,
  onClose,
  onChanged,
  onCreated,
}: {
  id: string | 'new';
  onClose: () => void;
  onChanged: () => void;
  onCreated: (id: string) => void;
}) {
  const t = useT();
  const res = useResource(`lib-series:${id}`, async (): Promise<api.Series | null> =>
    id === 'new' ? null : api.getSeries(id),
  );
  return (
    <Drawer
      open
      onClose={onClose}
      title={
        id === 'new' ? t('admin.library.newSeries') : (res.data?.title ?? t('admin.library.edit'))
      }
    >
      <Async res={res} label={t('admin.library.loading')}>
        {(series) => (
          <SeriesEditor
            key={series?.id ?? 'new'}
            series={series}
            onSaved={async (s) => {
              if (!series) onCreated(s.id);
              else await res.reload();
              onChanged();
            }}
            onGone={() => {
              onChanged();
              onClose();
            }}
          />
        )}
      </Async>
    </Drawer>
  );
}

const toInput = (s: api.Series | null): api.SeriesInput =>
  s
    ? {
        kind: s.kind,
        title: s.title,
        alt_titles: s.alt_titles,
        authors: s.authors,
        artists: s.artists,
        status: s.status,
        year: s.year ?? null,
        synopsis: s.synopsis,
        genres: s.genres,
        tags: s.tags,
        reading_direction: s.reading_direction,
        age_restricted: s.age_restricted,
      }
    : blank;

type Pending = 'merge' | 'delete' | null;

function SeriesEditor({
  series,
  onSaved,
  onGone,
}: {
  series: api.Series | null;
  onSaved: (s: api.Series) => Promise<void>;
  onGone: () => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  // Merging deletes the source series, so it needs both bits (D-172, G-26).
  const canMerge = viewer.can('PUBLISH_READING') && viewer.can('DELETE_CONTENT');
  const [f, setF] = useState(toInput(series));
  const [lists, setLists] = useState({
    alt: f.alt_titles.join(', '),
    authors: f.authors.join(', '),
    artists: f.artists.join(', '),
    genres: f.genres.join(', '),
    tags: f.tags.join(', '),
  });
  const [year, setYear] = useState(f.year === null ? '' : String(f.year));
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<Pending>(null);
  const [target, setTarget] = useState('');

  const run = async (fn: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      if (done) setNotice(done);
      return true;
    } catch (e) {
      setError(api.errorText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const save = (e: FormEvent) => {
    e.preventDefault();
    const body: api.SeriesInput = {
      ...f,
      title: f.title.trim(),
      alt_titles: list(lists.alt),
      authors: list(lists.authors),
      artists: list(lists.artists),
      genres: list(lists.genres),
      tags: list(lists.tags),
      year: year.trim() === '' ? null : Number(year),
    };
    void run(async () => {
      const s = series ? await api.updateSeries(series.id, body) : await api.createSeries(body);
      await onSaved(s);
    }, t('admin.library.saved'));
  };
  const validYear = year.trim() === '' || (Number.isInteger(Number(year)) && Number(year) > 0);

  return (
    <div className="adm-form">
      <ApiProblem error={error} />
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <form className="adm-form" onSubmit={save} aria-label={t('admin.library.formLabel')}>
        <TextInput
          label={t('admin.library.fieldTitle')}
          required
          value={f.title}
          onChange={(e) => setF({ ...f, title: e.target.value })}
        />
        <Select
          aria-label={t('admin.library.fieldKind')}
          options={api.SERIES_KINDS.map((k) => ({ value: k, label: k }))}
          value={f.kind}
          onValueChange={(kind) => setF({ ...f, kind: kind as api.SeriesKind })}
        />
        <Select
          aria-label={t('admin.library.fieldStatus')}
          options={api.SERIES_STATUSES.map((k) => ({ value: k, label: k }))}
          value={f.status}
          onValueChange={(status) => setF({ ...f, status: status as api.SeriesStatus })}
        />
        <Select
          aria-label={t('admin.library.fieldDirection')}
          options={api.DIRECTIONS.map((k) => ({ value: k, label: k }))}
          value={f.reading_direction}
          onValueChange={(d) => setF({ ...f, reading_direction: d as api.Direction })}
        />
        <TextInput
          label={t('admin.library.fieldYear')}
          inputMode="numeric"
          value={year}
          error={validYear ? undefined : t('admin.library.yearInvalid')}
          onChange={(e) => setYear(e.target.value)}
        />
        <TextInput
          label={t('admin.library.fieldAltTitles')}
          hint={t('admin.library.commaHint')}
          value={lists.alt}
          onChange={(e) => setLists({ ...lists, alt: e.target.value })}
        />
        <TextInput
          label={t('admin.library.fieldAuthors')}
          hint={t('admin.library.commaHint')}
          value={lists.authors}
          onChange={(e) => setLists({ ...lists, authors: e.target.value })}
        />
        <TextInput
          label={t('admin.library.fieldArtists')}
          hint={t('admin.library.commaHint')}
          value={lists.artists}
          onChange={(e) => setLists({ ...lists, artists: e.target.value })}
        />
        <TextInput
          label={t('admin.library.fieldGenres')}
          hint={t('admin.library.commaHint')}
          value={lists.genres}
          onChange={(e) => setLists({ ...lists, genres: e.target.value })}
        />
        <TextInput
          label={t('admin.library.fieldTags')}
          hint={t('admin.library.commaHint')}
          value={lists.tags}
          onChange={(e) => setLists({ ...lists, tags: e.target.value })}
        />
        <TextArea
          label={t('admin.library.fieldSynopsis')}
          rows={4}
          value={f.synopsis}
          onChange={(e) => setF({ ...f, synopsis: e.target.value })}
        />
        <Checkbox
          label={t('admin.library.fieldAgeRestricted')}
          checked={f.age_restricted}
          onChange={(e) => setF({ ...f, age_restricted: e.target.checked })}
        />
        <div className="adm-actions">
          <Button type="submit" loading={busy} disabled={!f.title.trim() || !validYear}>
            {series ? t('admin.library.save') : t('admin.library.create')}
          </Button>
        </div>
      </form>

      {series && (
        <>
          <div className="adm-actions" role="group" aria-label={t('admin.library.seriesActions')}>
            <Button
              variant="secondary"
              loading={busy}
              onClick={() =>
                void run(
                  async () => {
                    await api.setSeriesPublished(series.id, !series.published);
                    await onSaved(series);
                  },
                  series.published
                    ? t('admin.library.unpublished')
                    : t('admin.library.nowPublished'),
                )
              }
            >
              {series.published ? t('admin.library.unpublish') : t('admin.library.publish')}
            </Button>
            {canMerge && (
              <Button variant="secondary" onClick={() => setPending('merge')}>
                {t('admin.library.merge')}
              </Button>
            )}
            <Button variant="danger" onClick={() => setPending('delete')}>
              {t('admin.library.deleteSeries')}
            </Button>
          </div>
          <Chapters seriesId={series.id} onChanged={() => void onSaved(series)} />
          <FormDialog
            open={pending === 'merge'}
            title={t('admin.library.mergeTitle', { title: series.title })}
            description={t('admin.library.mergeDescription')}
            confirmLabel={t('admin.library.merge')}
            danger
            valid={/^\d+$/.test(target.trim())}
            onCancel={() => setPending(null)}
            onConfirm={async () => {
              await api.mergeSeries(series.id, target.trim());
              setPending(null);
              onGone();
            }}
          >
            <TextInput
              label={t('admin.library.mergeTarget')}
              hint={t('admin.library.mergeTargetHint')}
              inputMode="numeric"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
            />
          </FormDialog>
          <ConfirmDialog
            open={pending === 'delete'}
            danger
            title={t('admin.library.deleteTitle', { title: series.title })}
            description={t('admin.library.deleteDescription')}
            confirmLabel={t('admin.library.deleteSeries')}
            loading={busy}
            onCancel={() => setPending(null)}
            onConfirm={() =>
              void run(async () => {
                await api.deleteSeries(series.id);
                setPending(null);
                onGone();
              })
            }
          />
        </>
      )}
    </div>
  );
}

function Chapters({ seriesId, onChanged }: { seriesId: string; onChanged: () => void }) {
  const t = useT();
  const res = useResource(`lib-chapters:${seriesId}`, () => api.listChapters(seriesId));
  const [error, setError] = useState<string | null>(null);
  const [addEdition, setAddEdition] = useState(false);
  const [addChapterTo, setAddChapterTo] = useState<api.Edition | null>(null);
  const [del, setDel] = useState<api.Chapter | null>(null);
  const [lang, setLang] = useState('en');
  const [label, setLabel] = useState('');
  const [num, setNum] = useState('');
  const [chTitle, setChTitle] = useState('');
  const [group, setGroup] = useState('');

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await res.reload();
      onChanged();
      return true;
    } catch (e) {
      setError(api.errorText(e));
      return false;
    }
  };

  return (
    <section className="ss-card" aria-labelledby="lib-ch-h">
      <h2 id="lib-ch-h">{t('admin.library.chaptersTitle')}</h2>
      <ApiProblem error={error} />
      <Async res={res} label={t('admin.library.loadingChapters')}>
        {(cl) => (
          <>
            {cl.editions.length === 0 && <p className="ss-na">{t('admin.library.noEditions')}</p>}
            {cl.editions.map((ed) => {
              const chapters = cl.chapters
                .filter((c) => c.edition_id === ed.id)
                .sort((a, b) => a.number - b.number);
              return (
                <div key={ed.id}>
                  <h3>
                    {ed.label} <span className="ss-meta">({ed.language})</span>{' '}
                    {ed.is_default && <Badge>{t('admin.library.defaultEdition')}</Badge>}
                  </h3>
                  <ul
                    className="ss-list"
                    aria-label={t('admin.library.chaptersOf', { label: ed.label })}
                  >
                    {chapters.map((c) => (
                      <li key={c.id} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">
                            {t('admin.library.chapterName', { n: c.number })}
                            {c.title ? `: ${c.title}` : ''}{' '}
                            {c.published ? (
                              <Badge variant="success">{t('admin.library.published')}</Badge>
                            ) : (
                              <Badge>{t('admin.library.draft')}</Badge>
                            )}
                          </div>
                          <div className="ss-meta">
                            {t('admin.library.pageCount', { count: c.page_count })} · {c.format}
                            {c.release_group ? ` · ${c.release_group}` : ''}
                          </div>
                        </div>
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={
                            c.published
                              ? t('admin.library.unpublishChapterAria', { n: c.number })
                              : t('admin.library.publishChapterAria', { n: c.number })
                          }
                          onClick={() =>
                            void act(() => api.setChapterPublished(c.id, !c.published))
                          }
                        >
                          {c.published ? t('admin.library.unpublish') : t('admin.library.publish')}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('admin.library.deleteChapterAria', { n: c.number })}
                          onClick={() => setDel(c)}
                        >
                          {t('admin.library.delete')}
                        </Button>
                      </li>
                    ))}
                  </ul>
                  <div className="adm-actions">
                    <Button size="sm" variant="secondary" onClick={() => setAddChapterTo(ed)}>
                      {t('admin.library.addChapter')}
                    </Button>
                  </div>
                </div>
              );
            })}
            <div className="adm-actions">
              <Button variant="secondary" onClick={() => setAddEdition(true)}>
                {t('admin.library.addEdition')}
              </Button>
            </div>
            <FormDialog
              open={addEdition}
              title={t('admin.library.addEdition')}
              confirmLabel={t('admin.library.add')}
              valid={lang.trim() !== '' && label.trim() !== ''}
              onCancel={() => setAddEdition(false)}
              onConfirm={async () => {
                const ok = await act(() =>
                  api.createEdition(seriesId, {
                    language: lang.trim(),
                    label: label.trim(),
                    is_default: cl.editions.length === 0,
                  }),
                );
                if (ok) {
                  setAddEdition(false);
                  setLabel('');
                }
              }}
            >
              <TextInput
                label={t('admin.library.editionLanguage')}
                value={lang}
                onChange={(e) => setLang(e.target.value)}
              />
              <TextInput
                label={t('admin.library.editionLabel')}
                value={label}
                onChange={(e) => setLabel(e.target.value)}
              />
            </FormDialog>
            <FormDialog
              open={!!addChapterTo}
              title={t('admin.library.addChapterTo', { label: addChapterTo?.label ?? '' })}
              confirmLabel={t('admin.library.add')}
              valid={num.trim() !== '' && Number.isFinite(Number(num))}
              onCancel={() => setAddChapterTo(null)}
              onConfirm={async () => {
                if (!addChapterTo) return;
                const ok = await act(() =>
                  api.createChapter(addChapterTo.id, {
                    number: Number(num),
                    title: chTitle.trim() || null,
                    format: 'images',
                    release_group: group.trim() || null,
                  }),
                );
                if (ok) {
                  setAddChapterTo(null);
                  setNum('');
                  setChTitle('');
                }
              }}
            >
              <TextInput
                label={t('admin.library.chapterNumber')}
                inputMode="decimal"
                value={num}
                onChange={(e) => setNum(e.target.value)}
              />
              <TextInput
                label={t('admin.library.chapterTitle')}
                value={chTitle}
                onChange={(e) => setChTitle(e.target.value)}
              />
              <TextInput
                label={t('admin.library.releaseGroup')}
                value={group}
                onChange={(e) => setGroup(e.target.value)}
              />
            </FormDialog>
            <ConfirmDialog
              open={!!del}
              danger
              title={t('admin.library.deleteChapterTitle', { n: del?.number ?? 0 })}
              description={t('admin.library.deleteChapterDescription')}
              confirmLabel={t('admin.library.delete')}
              onCancel={() => setDel(null)}
              onConfirm={() => {
                if (!del) return;
                void act(() => api.deleteChapter(del.id)).then(() => setDel(null));
              }}
            />
          </>
        )}
      </Async>
    </section>
  );
}
