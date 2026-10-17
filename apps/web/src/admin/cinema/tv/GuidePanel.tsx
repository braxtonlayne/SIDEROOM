// Imported-guide channels (TV-ADM-003, TV-ADM-004): the weekly grid editor and the guide import with its
// mapping report. The grid is saved whole (`PUT /admin/tv/channels/{id}/slots`); an import changes nothing
// until it is confirmed. Times are in the channel's time zone. Spec: docs/specs/cinema-live-tv.md sections 4, 6.
import { Badge, Button, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { type MessageKey, useT } from '../../../i18n';
import { listTitles } from '../api';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import * as tv from './tvApi';

const DAYS = [0, 1, 2, 3, 4, 5, 6] as const;

const STATUS_LABEL: Record<tv.RowStatus, MessageKey> = {
  matched: 'admin.guide.status.matched',
  ambiguous: 'admin.guide.status.ambiguous',
  missing: 'admin.guide.status.missing',
  invalid: 'admin.guide.status.invalid',
};

/** Local weekday names (2025-03-03 was a Monday), so the labels follow the viewer's language. */
export function weekdayName(day: number, style: 'long' | 'short' = 'long'): string {
  return new Intl.DateTimeFormat(undefined, { weekday: style, timeZone: 'UTC' }).format(
    new Date(Date.UTC(2025, 2, 3 + day)),
  );
}

export const minutesOf = (hhmm: string): number => {
  const [h, m] = hhmm.split(':');
  return Number(h) * 60 + Number(m);
};
export const clock = (min: number): string => {
  const m = ((min % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
};

/** Slots sorted by day and start. */
export const sortSlots = (slots: readonly tv.Slot[]): tv.Slot[] =>
  [...slots].sort((a, b) => a.weekday - b.weekday || minutesOf(a.start) - minutesOf(b.start));

/** The first pair of slots that overlap in the week (also across midnight and the turn of the week). */
export function findOverlap(slots: readonly tv.Slot[]): [tv.Slot, tv.Slot] | null {
  const WEEK = 7 * 1440;
  const span = (s: tv.Slot): [number, number] => {
    const a = s.weekday * 1440 + minutesOf(s.start);
    return [a, a + s.duration_min];
  };
  for (let i = 0; i < slots.length; i++) {
    for (let j = i + 1; j < slots.length; j++) {
      const [a0, a1] = span(slots[i]!);
      const [b0, b1] = span(slots[j]!);
      if ([-WEEK, 0, WEEK].some((shift) => a0 < b1 + shift && b0 + shift < a1)) {
        return [slots[i]!, slots[j]!];
      }
    }
  }
  return null;
}

/** The file's text (`Blob.text` where the browser has it, else a FileReader). */
const readText = (file: Blob): Promise<string> =>
  typeof file.text === 'function'
    ? file.text()
    : new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result ?? ''));
        reader.onerror = () => reject(new Error('read'));
        reader.readAsText(file);
      });

const blank = (weekday: number): tv.Slot => ({
  weekday,
  start: '20:00',
  duration_min: 30,
  show_name: '',
  title_id: null,
});

export function GuidePanel({ channel }: { channel: tv.AdminChannel }) {
  const t = useT();
  // A confirmed import replaces the grid: remount the editor so it loads the new one.
  const [rev, setRev] = useState(0);
  return (
    <>
      <GridEditor key={rev} channel={channel} />
      <ImportPanel channel={channel} onConfirmed={() => setRev((n) => n + 1)} />
      <p className="ss-meta">{t('admin.guide.zoneNote', { zone: channel.timezone })}</p>
    </>
  );
}

function GridEditor({ channel }: { channel: tv.AdminChannel }) {
  const t = useT();
  const res = useLoad<tv.Slot[]>(`tv-slots-${channel.id}`, () => tv.listSlots(channel.id));
  const [edited, setEdited] = useState<tv.Slot[] | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const { busy, error, notice, run } = useRun();
  const slots = edited ?? res.data ?? [];
  const dirty = edited !== null;
  const overlap = useMemo(() => findOverlap(slots), [slots]);

  const change = (next: tv.Slot[]) => setEdited(next);
  const patch = (i: number, p: Partial<tv.Slot>) =>
    change(slots.map((s, n) => (n === i ? { ...s, ...p } : s)));
  const sel = selected !== null ? slots[selected] : undefined;

  const save = () =>
    run(async () => {
      const saved = await tv.saveSlots(channel.id, sortSlots(slots));
      setEdited(null);
      setSelected(null);
      res.set(saved);
      return saved;
    }, t('admin.guide.saved'));

  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.guide.what')}
      />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.guide.loading')}</p>;

  return (
    <section className="ss-card" aria-label={t('admin.guide.gridTitle')}>
      <h2 className="ss-sub">{t('admin.guide.gridTitle')}</h2>
      <p className="ss-meta">{t('admin.guide.gridHelp')}</p>
      <div className="adm-week" role="group" aria-label={t('admin.guide.weekLabel')}>
        {DAYS.map((day) => {
          const here = slots
            .map((s, i) => ({ s, i }))
            .filter(({ s }) => s.weekday === day)
            .sort((a, b) => minutesOf(a.s.start) - minutesOf(b.s.start));
          return (
            <div
              key={day}
              className="adm-week-day"
              data-day={day}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                const from = Number(e.dataTransfer.getData('text/plain'));
                if (Number.isInteger(from) && slots[from]) patch(from, { weekday: day });
              }}
            >
              <h3 className="adm-week-name">{weekdayName(day)}</h3>
              <ul className="adm-week-list" aria-label={weekdayName(day)}>
                {here.map(({ s, i }) => (
                  <li key={i}>
                    <button
                      type="button"
                      className="adm-slot"
                      draggable
                      aria-pressed={selected === i}
                      data-skipped={s.title_id ? undefined : 'true'}
                      onDragStart={(e) => e.dataTransfer.setData('text/plain', String(i))}
                      onClick={() => setSelected(i)}
                    >
                      <span className="adm-mono">
                        {s.start}–{clock(minutesOf(s.start) + s.duration_min)}
                      </span>
                      <span className="adm-slot-name">
                        {s.show_name || t('admin.guide.untitled')}
                      </span>
                      {!s.title_id && <Badge variant="warning">{t('admin.guide.skipped')}</Badge>}
                    </button>
                  </li>
                ))}
              </ul>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  change([...slots, blank(day)]);
                  setSelected(slots.length);
                }}
              >
                {t('admin.guide.add', { day: weekdayName(day) })}
              </Button>
            </div>
          );
        })}
      </div>

      {sel && selected !== null && (
        <SlotForm
          key={selected}
          slot={sel}
          onChange={(p) => patch(selected, p)}
          onRemove={() => {
            change(slots.filter((_, n) => n !== selected));
            setSelected(null);
          }}
          onClose={() => setSelected(null)}
        />
      )}
      {slots.length === 0 && (
        <EmptyState
          title={t('admin.guide.empty.title')}
          description={t('admin.guide.empty.description')}
        />
      )}
      {overlap && (
        <Notice variant="danger" role="alert">
          {t('admin.guide.overlap', { a: overlap[0].show_name, b: overlap[1].show_name })}
        </Notice>
      )}
      <Banner error={error} notice={notice} />
      <div className="adm-actions">
        <Button loading={busy} disabled={!dirty || !!overlap} onClick={() => void save()}>
          {t('admin.guide.save')}
        </Button>
        {dirty && (
          <Button
            variant="secondary"
            onClick={() => {
              setEdited(null);
              setSelected(null);
            }}
          >
            {t('admin.guide.discard')}
          </Button>
        )}
      </div>
    </section>
  );
}

function SlotForm({
  slot,
  onChange,
  onRemove,
  onClose,
}: {
  slot: tv.Slot;
  onChange: (p: Partial<tv.Slot>) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<{ id: string; title: string; year?: number | null }[]>([]);
  const [searching, setSearching] = useState(false);

  const search = async () => {
    const q = query.trim() || slot.show_name.trim();
    if (!q) return;
    setSearching(true);
    try {
      const page = await listTitles({ title: q, published: 'published', limit: 8 });
      setHits(page.items.map((x) => ({ id: x.id, title: x.title, year: x.year })));
    } catch {
      setHits([]);
    } finally {
      setSearching(false);
    }
  };

  return (
    <form
      className="adm-form"
      aria-label={t('admin.guide.slotForm')}
      onSubmit={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div className="adm-cin-grid">
        <Select
          aria-label={t('admin.guide.day')}
          options={DAYS.map((d) => ({ value: String(d), label: weekdayName(d) }))}
          value={String(slot.weekday)}
          onValueChange={(v) => onChange({ weekday: Number(v) })}
        />
        <TextInput
          label={t('admin.guide.start')}
          type="time"
          value={slot.start}
          onChange={(e) => e.target.value && onChange({ start: e.target.value })}
        />
        <TextInput
          label={t('admin.guide.duration')}
          type="number"
          min={1}
          max={1440}
          value={slot.duration_min}
          onChange={(e) => onChange({ duration_min: Math.max(1, Number(e.target.value) || 1) })}
        />
        <TextInput
          label={t('admin.guide.show')}
          value={slot.show_name}
          onChange={(e) => onChange({ show_name: e.target.value })}
        />
        <TextInput
          label={t('admin.guide.season')}
          type="number"
          min={0}
          value={slot.season_number ?? ''}
          onChange={(e) =>
            onChange({ season_number: e.target.value === '' ? null : Number(e.target.value) })
          }
        />
        <TextInput
          label={t('admin.guide.episode')}
          type="number"
          min={0}
          value={slot.episode_number ?? ''}
          onChange={(e) =>
            onChange({ episode_number: e.target.value === '' ? null : Number(e.target.value) })
          }
        />
        <TextInput
          label={t('admin.guide.note')}
          value={slot.note ?? ''}
          onChange={(e) => onChange({ note: e.target.value || null })}
        />
      </div>
      <p className="ss-meta">
        {slot.title_id
          ? t('admin.guide.linked', { id: slot.title_id })
          : t('admin.guide.notLinked')}
      </p>
      <div className="adm-actions">
        <TextInput
          label={t('admin.guide.findTitle')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button variant="secondary" loading={searching} onClick={() => void search()}>
          {t('admin.guide.search')}
        </Button>
        {slot.title_id && (
          <Button variant="secondary" onClick={() => onChange({ title_id: null })}>
            {t('admin.guide.unlink')}
          </Button>
        )}
      </div>
      {hits.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.guide.results')}>
          {hits.map((h) => (
            <li key={h.id} className="ss-row">
              <div className="ss-grow">
                {h.title}
                {h.year ? ` (${h.year})` : ''}
              </div>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  onChange({ title_id: h.id, show_name: slot.show_name || h.title });
                  setHits([]);
                }}
              >
                {t('admin.guide.use', { title: h.title })}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="adm-actions">
        <Button type="submit" variant="secondary">
          {t('admin.guide.done')}
        </Button>
        <Button variant="danger" onClick={onRemove}>
          {t('admin.guide.remove')}
        </Button>
      </div>
    </form>
  );
}

function ImportPanel({
  channel,
  onConfirmed,
}: {
  channel: tv.AdminChannel;
  onConfirmed: () => void;
}) {
  const t = useT();
  const [report, setReport] = useState<tv.GuideImport | null>(null);
  const [choice, setChoice] = useState<Record<number, string>>({});
  const upload = useRun();
  const confirm = useRun();

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    const format = file.name.toLowerCase().endsWith('.json') ? 'json' : 'csv';
    const content = await readText(file);
    const r = await upload.run(() => tv.importGuide(channel.id, format, content));
    if (r) {
      setReport(r);
      setChoice({});
    }
  };

  const rows = report?.report.rows ?? [];
  return (
    <section className="ss-card" aria-label={t('admin.guide.importTitle')}>
      <h2 className="ss-sub">{t('admin.guide.importTitle')}</h2>
      <p className="ss-meta">{t('admin.guide.importHelp')}</p>
      <div className="adm-actions">
        <a className="ss-link" href={tv.templateUrl('csv')} download>
          {t('admin.guide.templateCsv')}
        </a>
        <a className="ss-link" href={tv.templateUrl('json')} download>
          {t('admin.guide.templateJson')}
        </a>
      </div>
      <label className="ss-field">
        <span>{t('admin.guide.file')}</span>
        <input
          type="file"
          accept=".csv,.json,text/csv,application/json"
          onChange={(e) => void onFile(e.target.files?.[0])}
        />
      </label>
      <Banner error={upload.error} notice={null} />
      {report && (
        <>
          <p role="status">
            {t('admin.guide.counts', {
              matched: report.report.matched,
              ambiguous: report.report.ambiguous,
              missing: report.report.missing,
              invalid: report.report.invalid,
            })}
          </p>
          <table className="adm-table" aria-label={t('admin.guide.reportLabel')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.guide.col.line')}</th>
                <th scope="col">{t('admin.guide.col.when')}</th>
                <th scope="col">{t('admin.guide.col.show')}</th>
                <th scope="col">{t('admin.guide.col.status')}</th>
                <th scope="col">{t('admin.guide.col.title')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.line}>
                  <td>{r.line}</td>
                  <td>
                    {r.status === 'invalid' && !r.start
                      ? '–'
                      : `${weekdayName(r.weekday, 'short')} ${r.start} · ${r.duration_min}`}
                  </td>
                  <td>{r.show || '–'}</td>
                  <td>
                    <Badge
                      variant={
                        r.status === 'matched'
                          ? 'success'
                          : r.status === 'invalid'
                            ? 'danger'
                            : 'warning'
                      }
                    >
                      {t(STATUS_LABEL[r.status])}
                    </Badge>
                    {r.message && <div className="ss-meta">{r.message}</div>}
                  </td>
                  <td>
                    {r.status === 'matched' && r.candidates[0]?.title}
                    {r.status === 'ambiguous' && (
                      <Select
                        aria-label={t('admin.guide.pick', { line: r.line })}
                        options={[
                          { value: '', label: t('admin.guide.skipSlot') },
                          ...r.candidates.map((c) => ({
                            value: c.title_id,
                            label: `${c.title}${c.year ? ` (${c.year})` : ''}`,
                          })),
                        ]}
                        value={choice[r.line] ?? ''}
                        onValueChange={(v) => setChoice({ ...choice, [r.line]: v })}
                      />
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <Banner error={confirm.error} notice={confirm.notice} />
          <div className="adm-actions">
            <Button
              loading={confirm.busy}
              disabled={report.confirmed}
              onClick={() =>
                void confirm
                  .run(
                    () =>
                      tv.confirmImport(
                        channel.id,
                        report.id,
                        Object.entries(choice).map(([line, id]) => ({
                          line: Number(line),
                          title_id: id || null,
                        })),
                      ),
                    t('admin.guide.confirmed'),
                  )
                  .then((r) => {
                    if (!r) return;
                    setReport({ ...report, confirmed: true });
                    onConfirmed();
                  })
              }
            >
              {t('admin.guide.confirm')}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
