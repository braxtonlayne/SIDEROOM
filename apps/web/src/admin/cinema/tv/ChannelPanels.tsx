// Per-channel operations (spec cinema-live-tv section 8.5): the schedule (preview from the rules, the stored
// blocks, publish, "simulate now"), the now control and manual placement, and the what-aired report.
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { DAY, formatDuration, startOfDay } from '../../../cinema/tv/model';
import type { TvBlock } from '../../../cinema/tv/types';
import { formatDate, formatTime, t, useT } from '../../../i18n';
import { fmtMs } from '../../ui';
import { NotAvailableYet } from '../AssetPanel';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import * as tv from './tvApi';

const kindLabel = (k: string) =>
  k === 'filler' ? t('admin.channelPanels.filler') : k.charAt(0).toUpperCase() + k.slice(1);
const clock = (ms: number) => formatTime(ms, { hour: 'numeric', minute: '2-digit' });
const dayHead = (ms: number) => formatDate(ms, { weekday: 'long', month: 'short', day: 'numeric' });
const blockLine = (b: TvBlock) => `${b.title}${b.episode_label ? ` · ${b.episode_label}` : ''}`;

/** `datetime-local` value (local time) to unix ms, or null. */
export const parseLocalInput = (v: string): number | null => {
  if (!v) return null;
  const ms = new Date(v).getTime();
  return Number.isFinite(ms) ? ms : null;
};

/** Groups blocks by local day, in order. */
export function groupByDay<T extends { start_ms: number }>(
  blocks: T[],
): { day: number; blocks: T[] }[] {
  const out: { day: number; blocks: T[] }[] = [];
  for (const b of blocks) {
    const day = startOfDay(b.start_ms);
    const last = out[out.length - 1];
    if (last && last.day === day) last.blocks.push(b);
    else out.push({ day, blocks: [b] });
  }
  return out;
}

function useChannelLoad<T>(key: string, fetcher: () => Promise<T>) {
  const [unavailable, setUnavailable] = useState(false);
  const res = useLoad<T | null>(key, async () => {
    try {
      return await fetcher();
    } catch (e) {
      if (tv.isUnavailable(e)) {
        setUnavailable(true);
        return null;
      }
      throw e;
    }
  });
  return { res, unavailable };
}

const sourceLabel = (s: TvBlock['source']) =>
  s === 'override'
    ? t('admin.channelPanels.source.override')
    : s === 'manual'
      ? t('admin.channelPanels.source.manual')
      : t('admin.channelPanels.source.generated');

function BlockList({ blocks }: { blocks: TvBlock[] }) {
  const t = useT();
  const groups = useMemo(() => groupByDay(blocks), [blocks]);
  return (
    <div className="adm-cin-schedule">
      {groups.map((g) => (
        <section key={g.day} aria-label={dayHead(g.day)}>
          <h2 className="ss-sub">{dayHead(g.day)}</h2>
          <ol className="ss-list">
            {g.blocks.map((b, i) => (
              <li key={`${b.id}-${i}`} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    <span className="adm-mono">
                      {t('admin.channelPanels.timeRange', {
                        start: clock(b.start_ms),
                        end: clock(b.end_ms),
                      })}
                    </span>{' '}
                    {blockLine(b)}{' '}
                    {b.source && b.source !== 'generated' && (
                      <Badge variant="warning">{sourceLabel(b.source)}</Badge>
                    )}
                  </div>
                  <div className="ss-meta">
                    {kindLabel(b.kind)}
                    {b.age_restricted ? ` · ${t('admin.channelPanels.age18')}` : ''}
                  </div>
                </div>
              </li>
            ))}
          </ol>
        </section>
      ))}
    </div>
  );
}

/* ----------------------------------------------------------- title picker */

type Found = { id: string; title: string; year?: number | null };

/** Search the catalog and pick one title (shared by the now control and manual placement). */
function TitlePicker({
  label,
  picked,
  onPick,
}: {
  label: string;
  picked: Found | null;
  onPick: (t: Found | null) => void;
}) {
  const t = useT();
  const [q, setQ] = useState('');
  const [found, setFound] = useState<Found[]>([]);
  useEffect(() => {
    if (q.trim().length < 2) {
      setFound([]);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      tv.listTitles({ title: q.trim(), published: 'true', limit: 8 })
        .then((p) => live && setFound(p.items))
        .catch(() => live && setFound([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [q]);
  return (
    <>
      <TextInput
        label={label}
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          onPick(null);
        }}
      />
      {found.length > 0 && !picked && (
        <ul className="ss-list" aria-label={t('admin.channelPanels.picker.matches', { label })}>
          {found.map((f) => (
            <li key={f.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {f.title}
                  {f.year ? ` (${f.year})` : ''}
                </div>
              </div>
              <Button size="sm" variant="secondary" onClick={() => onPick(f)}>
                {t('admin.channelPanels.picker.choose', { title: f.title })}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {picked && (
        <p className="ss-meta" role="status">
          {t('admin.channelPanels.picker.selected')} <strong>{picked.title}</strong>
        </p>
      )}
    </>
  );
}

/* ----------------------------------------------------------- schedule */

export function SchedulePanel({ channel }: { channel: tv.AdminChannel }) {
  const t = useT();
  const [source, setSource] = useState('preview');
  const [days, setDays] = useState('1');
  const [simulate, setSimulate] = useState('');
  const [horizon, setHorizon] = useState('168');
  const [confirm, setConfirm] = useState(false);
  const pub = useRun();
  const [published, setPublished] = useState<tv.Published | null>(null);
  const from = Date.now();
  const to = from + Number(days) * DAY;
  const at = parseLocalInput(simulate);
  const manual = channel.kind === 'manual';

  const sched = useChannelLoad(`tv-sched-${channel.id}-${source}-${days}`, async () =>
    source === 'preview'
      ? await tv.previewSchedule(channel.id, from, to)
      : { blocks: await tv.storedSchedule(channel.id, from, to), warnings: [] as string[] },
  );
  const sim = useChannelLoad<tv.NowState | null>(`tv-sim-${channel.id}-${at ?? ''}`, async () =>
    at === null ? null : await tv.getNowAt(channel.id, at),
  );

  if (sched.unavailable)
    return <NotAvailableYet what={t('admin.channelPanels.sched.whatPreview')} />;
  const data = sched.res.data;
  return (
    <>
      <div className="adm-cin-grid">
        <Select
          aria-label={t('admin.channelPanels.sched.source')}
          options={[
            { value: 'preview', label: t('admin.channelPanels.sched.sourcePreview') },
            { value: 'stored', label: t('admin.channelPanels.sched.sourceStored') },
          ]}
          value={source}
          onValueChange={setSource}
        />
        <Select
          aria-label={t('admin.channelPanels.sched.range')}
          options={[
            { value: '1', label: t('admin.channelPanels.next24h') },
            { value: '3', label: t('admin.channelPanels.next3d') },
            { value: '7', label: t('admin.channelPanels.next7d') },
          ]}
          value={days}
          onValueChange={setDays}
        />
        <TextInput
          label={t('admin.channelPanels.sched.simulate')}
          type="datetime-local"
          hint={t('admin.channelPanels.sched.simulateHint')}
          value={simulate}
          onChange={(e) => setSimulate(e.target.value)}
        />
      </div>
      {at !== null && sim.res.data && (
        <section aria-label={t('admin.channelPanels.sched.simulated')} className="ss-card">
          <p className="ss-name">
            {t('admin.channelPanels.sched.at', { time: fmtMs(sim.res.data.at_ms) })}{' '}
            {sim.res.data.current
              ? blockLine(sim.res.data.current)
              : t('admin.channelPanels.sched.nothingOnAir')}
            {sim.res.data.offset_ms !== null && sim.res.data.current
              ? ` ${t('admin.channelPanels.sched.offsetIn', { duration: formatDuration(Math.round(sim.res.data.offset_ms / 60_000)) })}`
              : ''}
          </p>
          <p className="ss-meta">
            {t('admin.channelPanels.sched.next')}{' '}
            {sim.res.data.next
              ? `${clock(sim.res.data.next.start_ms)} ${blockLine(sim.res.data.next)}`
              : t('admin.channelPanels.sched.nothing')}
          </p>
        </section>
      )}
      <Banner error={pub.error} notice={pub.notice} />
      {published && (
        <p className="ss-meta" role="status">
          {t('admin.channelPanels.sched.wrote', {
            written: published.written,
            removed: published.removed,
            time: fmtMs(published.to_ms),
          })}
        </p>
      )}
      {published?.warnings.map((w) => (
        <p key={w} className="ss-meta" role="note">
          {w}
        </p>
      ))}
      {sched.res.error && !data && (
        <LoadProblem
          error={sched.res.error}
          status={sched.res.status}
          onRetry={() => void sched.res.reload()}
          what={t('admin.channelPanels.sched.what')}
        />
      )}
      {!data && !sched.res.error && (
        <p className="ss-loading">{t('admin.channelPanels.sched.loading')}</p>
      )}
      {data && (
        <>
          <p className="ss-meta" role="status">
            {source === 'preview'
              ? t('admin.channelPanels.sched.draftNote')
              : t('admin.channelPanels.sched.storedNote')}
          </p>
          {data.warnings.map((w) => (
            <p key={w} className="ss-meta" role="note">
              {w}
            </p>
          ))}
          {data.blocks.length === 0 ? (
            <EmptyState
              title={t('admin.channelPanels.sched.empty.title')}
              description={
                manual
                  ? t('admin.channelPanels.sched.empty.manual')
                  : t('admin.channelPanels.sched.empty.rules')
              }
            />
          ) : (
            <BlockList blocks={data.blocks} />
          )}
          <div className="adm-actions">
            <Button variant="secondary" onClick={() => void sched.res.reload()}>
              {t('admin.channelPanels.sched.refresh')}
            </Button>
            {!manual && (
              <>
                <Select
                  aria-label={t('admin.channelPanels.sched.horizon')}
                  options={[
                    { value: '24', label: t('admin.channelPanels.next24h') },
                    { value: '72', label: t('admin.channelPanels.next3d') },
                    { value: '168', label: t('admin.channelPanels.next7d') },
                    { value: '336', label: t('admin.channelPanels.next14d') },
                  ]}
                  value={horizon}
                  onValueChange={setHorizon}
                />
                <Button onClick={() => setConfirm(true)}>
                  {t('admin.channelPanels.sched.publish')}
                </Button>
              </>
            )}
          </div>
        </>
      )}
      <ConfirmDialog
        open={confirm}
        title={t('admin.channelPanels.sched.publishTitle', { name: channel.name })}
        description={t('admin.channelPanels.sched.publishDescription', {
          days: Number(horizon) / 24,
        })}
        confirmLabel={t('admin.channelPanels.sched.publishConfirm')}
        loading={pub.busy}
        onCancel={() => setConfirm(false)}
        onConfirm={() =>
          void pub
            .run(async () => {
              setPublished(await tv.publishSchedule(channel.id, Number(horizon)));
              await sched.res.reload();
              return true;
            }, t('admin.channelPanels.sched.published'))
            .then(() => setConfirm(false))
        }
      />
    </>
  );
}

/* ------------------------------------------------------------ now control */

export function NowControl({ channel }: { channel: tv.AdminChannel }) {
  const t = useT();
  const { res, unavailable } = useChannelLoad(`tv-now-${channel.id}`, () =>
    tv.getNowAt(channel.id),
  );
  const [picked, setPicked] = useState<Found | null>(null);
  const [episode, setEpisode] = useState('');
  const [offset, setOffset] = useState('');
  const [placePicked, setPlacePicked] = useState<Found | null>(null);
  const [placeEpisode, setPlaceEpisode] = useState('');
  const [placeStart, setPlaceStart] = useState('');
  const [dialog, setDialog] = useState<'now' | 'remove' | null>(null);
  const { busy, error, notice, run } = useRun();

  if (unavailable) return <NotAvailableYet what={t('admin.channelPanels.now.what')} />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.channelPanels.now.what')}
      />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.channelPanels.now.loading')}</p>;
  const st = res.data;
  const cur = st.current;
  const removable = cur && cur.source && cur.source !== 'generated';
  const done = async (fn: () => Promise<unknown>, message: string) => {
    const ok = await run(async () => {
      await fn();
      await res.reload();
      return true;
    }, message);
    if (ok) setDialog(null);
    return ok;
  };
  const startMs = parseLocalInput(placeStart);
  const offsetMin = offset.trim() === '' ? null : Number(offset);
  return (
    <>
      <Banner error={error} notice={notice} />
      <dl className="adm-kv" aria-label={t('admin.channelPanels.now.aria')}>
        <dt>{t('admin.channelPanels.now.onAir')}</dt>
        <dd>{cur ? blockLine(cur) : t('admin.channelPanels.now.offAir')}</dd>
        <dt>{t('admin.channelPanels.now.source')}</dt>
        <dd>
          {cur?.source && cur.source !== 'generated' ? (
            <Badge variant="warning">{sourceLabel(cur.source)}</Badge>
          ) : (
            t('admin.channelPanels.now.schedule')
          )}
        </dd>
        <dt>{t('admin.channelPanels.now.next')}</dt>
        <dd>
          {st.next
            ? `${clock(st.next.start_ms)} ${blockLine(st.next)}`
            : t('admin.channelPanels.now.nothingScheduled')}
        </dd>
        <dt>{t('admin.channelPanels.now.viewers')}</dt>
        <dd>{st.viewers}</dd>
      </dl>
      {removable && cur && (
        <div className="adm-actions">
          <Button variant="secondary" onClick={() => setDialog('remove')}>
            {t('admin.channelPanels.now.removeBlock', { source: sourceLabel(cur.source) })}
          </Button>
        </div>
      )}

      <section aria-label={t('admin.channelPanels.now.put.aria')} className="ss-card">
        <h2 className="ss-sub">{t('admin.channelPanels.now.put.heading')}</h2>
        <TitlePicker
          label={t('admin.channelPanels.now.put.find')}
          picked={picked}
          onPick={setPicked}
        />
        <div className="adm-cin-grid">
          <TextInput
            label={t('admin.channelPanels.now.put.episode')}
            value={episode}
            onChange={(e) => setEpisode(e.target.value)}
          />
          <TextInput
            label={t('admin.channelPanels.now.put.offset')}
            type="number"
            min={0}
            value={offset}
            onChange={(e) => setOffset(e.target.value)}
          />
        </div>
        <div className="adm-actions">
          <Button disabled={!picked} onClick={() => setDialog('now')}>
            {t('admin.channelPanels.now.put.button')}
          </Button>
        </div>
      </section>

      <section aria-label={t('admin.channelPanels.now.place.aria')} className="ss-card">
        <h2 className="ss-sub">{t('admin.channelPanels.now.place.heading')}</h2>
        <p className="ss-meta">{t('admin.channelPanels.now.place.note')}</p>
        <TitlePicker
          label={t('admin.channelPanels.now.place.find')}
          picked={placePicked}
          onPick={setPlacePicked}
        />
        <div className="adm-cin-grid">
          <TextInput
            label={t('admin.channelPanels.now.place.startsAt')}
            type="datetime-local"
            value={placeStart}
            onChange={(e) => setPlaceStart(e.target.value)}
          />
          <TextInput
            label={t('admin.channelPanels.now.place.episode')}
            value={placeEpisode}
            onChange={(e) => setPlaceEpisode(e.target.value)}
          />
        </div>
        <div className="adm-actions">
          <Button
            disabled={!placePicked || startMs === null}
            loading={busy}
            onClick={() =>
              void done(async () => {
                await tv.placeBlock(channel.id, {
                  title_id: placePicked!.id,
                  ...(placeEpisode.trim() ? { episode_id: placeEpisode.trim() } : {}),
                  start_ms: startMs!,
                });
                setPlacePicked(null);
                setPlaceStart('');
              }, t('admin.channelPanels.now.place.placed'))
            }
          >
            {t('admin.channelPanels.now.place.button')}
          </Button>
        </div>
      </section>

      <ConfirmDialog
        open={dialog === 'now' && !!picked}
        title={t('admin.channelPanels.now.dialog.title', { title: picked?.title ?? '' })}
        description={t('admin.channelPanels.now.dialog.description', { name: channel.name })}
        confirmLabel={t('admin.channelPanels.now.dialog.confirm')}
        loading={busy}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          void done(async () => {
            await tv.putNow(channel.id, {
              title_id: picked!.id,
              ...(episode.trim() ? { episode_id: episode.trim() } : {}),
              ...(offsetMin !== null && Number.isFinite(offsetMin) && offsetMin >= 0
                ? { offset_ms: Math.round(offsetMin * 60_000) }
                : {}),
            });
            setPicked(null);
          }, t('admin.channelPanels.now.dialog.done'))
        }
      />
      <ConfirmDialog
        open={dialog === 'remove'}
        danger
        title={t('admin.channelPanels.now.remove.title')}
        description={t('admin.channelPanels.now.remove.description', {
          what: cur ? blockLine(cur) : t('admin.channelPanels.now.remove.theBlock'),
        })}
        confirmLabel={t('admin.channelPanels.now.remove.confirm')}
        loading={busy}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          void done(() => tv.deleteBlock(cur!.id), t('admin.channelPanels.now.remove.done'))
        }
      />
    </>
  );
}

/* ------------------------------------------------------------- what aired */

export function AiredReport({ channel }: { channel: tv.AdminChannel }) {
  const t = useT();
  const [range, setRange] = useState('1');
  const to = Date.now();
  const from = to - Number(range) * DAY;
  const { res, unavailable } = useChannelLoad(`tv-aired-${channel.id}-${range}`, () =>
    tv.getReport(from, to, channel.id),
  );
  if (unavailable) return <NotAvailableYet what={t('admin.channelPanels.aired.whatReport')} />;
  const mine = res.data?.channels.find((c) => c.channel_id === channel.id);
  return (
    <>
      <Select
        aria-label={t('admin.channelPanels.aired.range')}
        options={[
          { value: '1', label: t('admin.channelPanels.aired.last24h') },
          { value: '3', label: t('admin.channelPanels.aired.last3d') },
          { value: '7', label: t('admin.channelPanels.aired.last7d') },
        ]}
        value={range}
        onValueChange={setRange}
      />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what={t('admin.channelPanels.aired.what')}
        />
      )}
      {!res.data && !res.error && (
        <p className="ss-loading">{t('admin.channelPanels.aired.loading')}</p>
      )}
      {res.data && mine && (
        <p className="ss-meta" role="status">
          {t('admin.channelPanels.aired.summary', {
            programmes: mine.programmes,
            overrides: mine.overrides,
            duration: formatDuration(Math.round(mine.aired_ms / 60_000)),
          })}
        </p>
      )}
      {res.data && res.data.items.length === 0 && (
        <EmptyState
          title={t('admin.channelPanels.aired.empty.title')}
          description={t('admin.channelPanels.aired.empty.description')}
        />
      )}
      {res.data && res.data.items.length > 0 && (
        <table className="adm-table" aria-label={t('admin.channelPanels.aired.table')}>
          <thead>
            <tr>
              <th scope="col">{t('admin.channelPanels.aired.col.time')}</th>
              <th scope="col">{t('admin.channelPanels.aired.col.programme')}</th>
              <th scope="col">{t('admin.channelPanels.aired.col.source')}</th>
              <th scope="col">{t('admin.channelPanels.aired.col.aired')}</th>
            </tr>
          </thead>
          <tbody>
            {[...res.data.items]
              .sort((a, b) => b.block.start_ms - a.block.start_ms)
              .map((a, i) => (
                <tr key={`${a.block.id}-${i}`}>
                  <td>
                    {t('admin.channelPanels.timeRange', {
                      start: fmtMs(a.block.start_ms),
                      end: clock(a.block.end_ms),
                    })}
                  </td>
                  <td>{blockLine(a.block)}</td>
                  <td>{sourceLabel(a.block.source)}</td>
                  <td>{formatDuration(Math.max(1, Math.round(a.aired_ms / 60_000)))}</td>
                </tr>
              ))}
          </tbody>
        </table>
      )}
    </>
  );
}
