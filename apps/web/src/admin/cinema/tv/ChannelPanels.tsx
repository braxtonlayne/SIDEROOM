// Per-channel operations (spec cinema-live-tv section 8.5): the schedule (preview from the rules, the stored
// blocks, publish, "simulate now"), the now control and manual placement, and the what-aired report.
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { DAY, formatDuration, startOfDay } from '../../../cinema/tv/model';
import type { TvBlock } from '../../../cinema/tv/types';
import { fmtMs } from '../../ui';
import { NotAvailableYet } from '../AssetPanel';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import * as tv from './tvApi';

const kindLabel = (k: string) =>
  k === 'filler' ? 'Filler' : k.charAt(0).toUpperCase() + k.slice(1);
const clock = (ms: number) =>
  new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(ms);
const dayHead = (ms: number) =>
  new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'short', day: 'numeric' }).format(
    ms,
  );
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
  s === 'override' ? 'override' : s === 'manual' ? 'manual' : 'generated';

function BlockList({ blocks }: { blocks: TvBlock[] }) {
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
                      {clock(b.start_ms)} to {clock(b.end_ms)}
                    </span>{' '}
                    {blockLine(b)}{' '}
                    {b.source && b.source !== 'generated' && (
                      <Badge variant="warning">{sourceLabel(b.source)}</Badge>
                    )}
                  </div>
                  <div className="ss-meta">
                    {kindLabel(b.kind)}
                    {b.age_restricted ? ' · 18+' : ''}
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
  const [q, setQ] = useState('');
  const [found, setFound] = useState<Found[]>([]);
  useEffect(() => {
    if (q.trim().length < 2) {
      setFound([]);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      tv.listTitles({ title: q.trim(), published: 'true', limit: 8 })
        .then((p) => live && setFound(p.items))
        .catch(() => live && setFound([]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
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
        <ul className="ss-list" aria-label={`${label}: matches`}>
          {found.map((t) => (
            <li key={t.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t.title}
                  {t.year ? ` (${t.year})` : ''}
                </div>
              </div>
              <Button size="sm" variant="secondary" onClick={() => onPick(t)}>
                Choose {t.title}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {picked && (
        <p className="ss-meta" role="status">
          Selected: <strong>{picked.title}</strong>
        </p>
      )}
    </>
  );
}

/* ----------------------------------------------------------- schedule */

export function SchedulePanel({ channel }: { channel: tv.AdminChannel }) {
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

  if (sched.unavailable) return <NotAvailableYet what="The schedule preview" />;
  const data = sched.res.data;
  return (
    <>
      <div className="adm-cin-grid">
        <Select
          aria-label="Schedule source"
          options={[
            { value: 'preview', label: 'Preview from the current rules' },
            { value: 'stored', label: 'Stored schedule (what viewers get)' },
          ]}
          value={source}
          onValueChange={setSource}
        />
        <Select
          aria-label="Range"
          options={[
            { value: '1', label: 'Next 24 hours' },
            { value: '3', label: 'Next 3 days' },
            { value: '7', label: 'Next 7 days' },
          ]}
          value={days}
          onValueChange={setDays}
        />
        <TextInput
          label="Simulate now"
          type="datetime-local"
          hint="See what the stored schedule has on air at that moment. Nothing changes for viewers."
          value={simulate}
          onChange={(e) => setSimulate(e.target.value)}
        />
      </div>
      {at !== null && sim.res.data && (
        <section aria-label="Simulated now" className="ss-card">
          <p className="ss-name">
            At {fmtMs(sim.res.data.at_ms)}:{' '}
            {sim.res.data.current ? blockLine(sim.res.data.current) : 'nothing is on air'}
            {sim.res.data.offset_ms !== null && sim.res.data.current
              ? ` (${formatDuration(Math.round(sim.res.data.offset_ms / 60_000))} in)`
              : ''}
          </p>
          <p className="ss-meta">
            Next:{' '}
            {sim.res.data.next
              ? `${clock(sim.res.data.next.start_ms)} ${blockLine(sim.res.data.next)}`
              : 'nothing'}
          </p>
        </section>
      )}
      <Banner error={pub.error} notice={pub.notice} />
      {published && (
        <p className="ss-meta" role="status">
          Wrote {published.written} blocks and removed {published.removed} through{' '}
          {fmtMs(published.to_ms)}.
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
          what="the schedule"
        />
      )}
      {!data && !sched.res.error && <p className="ss-loading">Loading the schedule…</p>}
      {data && (
        <>
          <p className="ss-meta" role="status">
            {source === 'preview'
              ? 'Draft preview generated from the current rules: viewers do not see this until you publish.'
              : 'These blocks are stored: this is what viewers get.'}
          </p>
          {data.warnings.map((w) => (
            <p key={w} className="ss-meta" role="note">
              {w}
            </p>
          ))}
          {data.blocks.length === 0 ? (
            <EmptyState
              title="Nothing scheduled"
              description={
                manual
                  ? 'Place programmes from the Now control tab.'
                  : 'The rules produce no programmes for this range. Check the content pools.'
              }
            />
          ) : (
            <BlockList blocks={data.blocks} />
          )}
          <div className="adm-actions">
            <Button variant="secondary" onClick={() => void sched.res.reload()}>
              Refresh
            </Button>
            {!manual && (
              <>
                <Select
                  aria-label="Publish horizon"
                  options={[
                    { value: '24', label: 'Next 24 hours' },
                    { value: '72', label: 'Next 3 days' },
                    { value: '168', label: 'Next 7 days' },
                    { value: '336', label: 'Next 14 days' },
                  ]}
                  value={horizon}
                  onValueChange={setHorizon}
                />
                <Button onClick={() => setConfirm(true)}>Publish schedule</Button>
              </>
            )}
          </div>
        </>
      )}
      <ConfirmDialog
        open={confirm}
        title={`Publish the schedule for ${channel.name}`}
        description={`Generated programmes that have not started are replaced by a fresh schedule from the programme on air now, for the next ${Number(horizon) / 24} days. Manual placements and overrides are kept.`}
        confirmLabel="Publish"
        loading={pub.busy}
        onCancel={() => setConfirm(false)}
        onConfirm={() =>
          void pub
            .run(async () => {
              setPublished(await tv.publishSchedule(channel.id, Number(horizon)));
              await sched.res.reload();
              return true;
            }, 'Schedule published.')
            .then(() => setConfirm(false))
        }
      />
    </>
  );
}

/* ------------------------------------------------------------ now control */

export function NowControl({ channel }: { channel: tv.AdminChannel }) {
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

  if (unavailable) return <NotAvailableYet what="Now control" />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what="Now control"
      />
    );
  if (!res.data) return <p className="ss-loading">Loading what is on air…</p>;
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
      <dl className="adm-kv" aria-label="On air now">
        <dt>On air</dt>
        <dd>{cur ? blockLine(cur) : 'Off the air'}</dd>
        <dt>Source</dt>
        <dd>
          {cur?.source && cur.source !== 'generated' ? (
            <Badge variant="warning">{sourceLabel(cur.source)}</Badge>
          ) : (
            'Schedule'
          )}
        </dd>
        <dt>Next</dt>
        <dd>
          {st.next ? `${clock(st.next.start_ms)} ${blockLine(st.next)}` : 'Nothing scheduled'}
        </dd>
        <dt>Viewers</dt>
        <dd>{st.viewers}</dd>
      </dl>
      {removable && cur && (
        <div className="adm-actions">
          <Button variant="secondary" onClick={() => setDialog('remove')}>
            Remove this {sourceLabel(cur.source)} block
          </Button>
        </div>
      )}

      <section aria-label="Put a title on air" className="ss-card">
        <h2 className="ss-sub">Put a title on air now</h2>
        <TitlePicker label="Find a title" picked={picked} onPick={setPicked} />
        <div className="adm-cin-grid">
          <TextInput
            label="Episode id (series and anime)"
            value={episode}
            onChange={(e) => setEpisode(e.target.value)}
          />
          <TextInput
            label="Start this many minutes in"
            type="number"
            min={0}
            value={offset}
            onChange={(e) => setOffset(e.target.value)}
          />
        </div>
        <div className="adm-actions">
          <Button disabled={!picked} onClick={() => setDialog('now')}>
            Put on air now
          </Button>
        </div>
      </section>

      <section aria-label="Place a programme" className="ss-card">
        <h2 className="ss-sub">Place a programme at a time</h2>
        <p className="ss-meta">
          Whatever it overlaps is trimmed, split or removed. On a rule-based channel the block is an
          override.
        </p>
        <TitlePicker label="Find a title to place" picked={placePicked} onPick={setPlacePicked} />
        <div className="adm-cin-grid">
          <TextInput
            label="Starts at"
            type="datetime-local"
            value={placeStart}
            onChange={(e) => setPlaceStart(e.target.value)}
          />
          <TextInput
            label="Episode id for the placed programme"
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
              }, 'Programme placed.')
            }
          >
            Place programme
          </Button>
        </div>
      </section>

      <ConfirmDialog
        open={dialog === 'now' && !!picked}
        title={`Put ${picked?.title ?? ''} on air`}
        description={`Replaces what is on ${channel.name} right now; the schedule continues after it. Everyone watching switches over.`}
        confirmLabel="Put on air"
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
          }, 'On air.')
        }
      />
      <ConfirmDialog
        open={dialog === 'remove'}
        danger
        title="Remove this block"
        description={`Removes ${cur ? blockLine(cur) : 'the block'} from the schedule. The gap is filled by the channel's filler policy.`}
        confirmLabel="Remove block"
        loading={busy}
        onCancel={() => setDialog(null)}
        onConfirm={() => void done(() => tv.deleteBlock(cur!.id), 'Block removed.')}
      />
    </>
  );
}

/* ------------------------------------------------------------- what aired */

export function AiredReport({ channel }: { channel: tv.AdminChannel }) {
  const [range, setRange] = useState('1');
  const to = Date.now();
  const from = to - Number(range) * DAY;
  const { res, unavailable } = useChannelLoad(`tv-aired-${channel.id}-${range}`, () =>
    tv.getReport(from, to, channel.id),
  );
  if (unavailable) return <NotAvailableYet what="The what-aired report" />;
  const mine = res.data?.channels.find((c) => c.channel_id === channel.id);
  return (
    <>
      <Select
        aria-label="Report range"
        options={[
          { value: '1', label: 'Last 24 hours' },
          { value: '3', label: 'Last 3 days' },
          { value: '7', label: 'Last 7 days' },
        ]}
        value={range}
        onValueChange={setRange}
      />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what="the report"
        />
      )}
      {!res.data && !res.error && <p className="ss-loading">Loading the report…</p>}
      {res.data && mine && (
        <p className="ss-meta" role="status">
          {mine.programmes} programmes, {mine.overrides} overrides,{' '}
          {formatDuration(Math.round(mine.aired_ms / 60_000))} on air. Viewer figures are not kept.
        </p>
      )}
      {res.data && res.data.items.length === 0 && (
        <EmptyState
          title="Nothing aired"
          description="No programmes were recorded for this range."
        />
      )}
      {res.data && res.data.items.length > 0 && (
        <table className="adm-table" aria-label="What aired">
          <thead>
            <tr>
              <th scope="col">Time</th>
              <th scope="col">Programme</th>
              <th scope="col">Source</th>
              <th scope="col">Aired</th>
            </tr>
          </thead>
          <tbody>
            {[...res.data.items]
              .sort((a, b) => b.block.start_ms - a.block.start_ms)
              .map((a, i) => (
                <tr key={`${a.block.id}-${i}`}>
                  <td>
                    {fmtMs(a.block.start_ms)} to {clock(a.block.end_ms)}
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
