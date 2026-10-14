// Per-channel operations: the generated schedule with publish and "simulate now" (TV-ADM-005), the manual
// override / now control, and the what-aired report.
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { DAY, addDays, startOfDay } from '../../../cinema/tv/model';
import { ReasonDialog, fmtMs } from '../../ui';
import { NotAvailableYet } from '../AssetPanel';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import * as tv from './tvApi';

const kindLabel = (k: string) =>
  k === 'filler' ? 'Filler' : k.charAt(0).toUpperCase() + k.slice(1);
const clock = (ms: number, tz?: string) =>
  new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
    ...(tz ? { timeZone: tz } : {}),
  }).format(ms);
const dayHead = (ms: number) =>
  new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'short', day: 'numeric' }).format(
    ms,
  );

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

function BlockList({ blocks, atMs }: { blocks: tv.PreviewBlock[]; atMs: number | null }) {
  const groups = useMemo(() => groupByDay(blocks), [blocks]);
  return (
    <div className="adm-cin-schedule">
      {groups.map((g) => (
        <section key={g.day} aria-label={dayHead(g.day)}>
          <h2 className="ss-sub">{dayHead(g.day)}</h2>
          <ol className="ss-list">
            {g.blocks.map((b, i) => {
              const onAir = atMs !== null && b.start_ms <= atMs && atMs < b.end_ms;
              return (
                <li
                  key={`${b.start_ms}-${i}`}
                  className="ss-row"
                  aria-current={onAir ? 'time' : undefined}
                >
                  <div className="ss-grow">
                    <div className="ss-name">
                      <span className="adm-mono">
                        {clock(b.start_ms)} to {clock(b.end_ms)}
                      </span>{' '}
                      {b.title}
                      {b.episode_label ? ` · ${b.episode_label}` : ''}{' '}
                      {onAir && <Badge variant="accent">on air at that time</Badge>}
                    </div>
                    <div className="ss-meta">
                      {kindLabel(b.kind)}
                      {b.source ? ` · ${b.source}` : ''}
                    </div>
                  </div>
                </li>
              );
            })}
          </ol>
        </section>
      ))}
    </div>
  );
}

/* ----------------------------------------------------------- schedule */

export function SchedulePanel({ channel }: { channel: tv.AdminChannel }) {
  const [simulate, setSimulate] = useState('');
  const [days, setDays] = useState('7');
  const [confirm, setConfirm] = useState(false);
  const from = startOfDay(Date.now());
  const to = addDays(from, Number(days));
  const at = parseLocalInput(simulate);
  const { res, unavailable } = useChannelLoad(`tv-sched-${channel.id}-${days}-${at ?? ''}`, () =>
    tv.previewSchedule(channel.id, from, to, at ?? undefined),
  );
  const pub = useRun();

  if (unavailable) return <NotAvailableYet what="The schedule preview" />;
  return (
    <>
      <div className="adm-cin-grid">
        <Select
          aria-label="Preview range"
          options={[
            { value: '1', label: 'Today' },
            { value: '3', label: 'Next 3 days' },
            { value: '7', label: 'Next 7 days' },
          ]}
          value={days}
          onValueChange={setDays}
        />
        <TextInput
          label="Simulate now"
          type="datetime-local"
          hint="See what would be on air at that moment. Nothing changes for viewers."
          value={simulate}
          onChange={(e) => setSimulate(e.target.value)}
        />
      </div>
      <Banner error={pub.error} notice={pub.notice} />
      {res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what="the schedule"
        />
      )}
      {!res.data && !res.error && <p className="ss-loading">Generating the preview…</p>}
      {res.data && (
        <>
          <p className="ss-meta" role="status">
            {res.data.published
              ? 'This schedule is published.'
              : 'Draft preview: viewers do not see this yet.'}
            {channel.published_until_ms
              ? ` Published through ${fmtMs(channel.published_until_ms)}.`
              : ''}
          </p>
          {res.data.warnings?.map((w) => (
            <p key={w} className="ss-meta" role="note">
              {w}
            </p>
          ))}
          {res.data.blocks.length === 0 ? (
            <EmptyState
              title="Nothing scheduled"
              description="The rules produce no programmes for this range. Check the content pool."
            />
          ) : (
            <BlockList blocks={res.data.blocks} atMs={at} />
          )}
          <div className="adm-actions">
            <Button variant="secondary" onClick={() => void res.reload()}>
              Regenerate
            </Button>
            <Button disabled={res.data.blocks.length === 0} onClick={() => setConfirm(true)}>
              Publish schedule
            </Button>
          </div>
        </>
      )}
      <ConfirmDialog
        open={confirm}
        title={`Publish the schedule for ${channel.name}`}
        description={`Viewers see this schedule from now until ${new Date(to).toLocaleDateString()}. Anything already published for these days is replaced.`}
        confirmLabel="Publish"
        loading={pub.busy}
        onCancel={() => setConfirm(false)}
        onConfirm={() =>
          void pub
            .run(async () => {
              await tv.publishSchedule(channel.id, from, to);
              await res.reload();
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
    tv.getOverride(channel.id),
  );
  const [dialog, setDialog] = useState<'skip' | 'put' | 'clear' | null>(null);
  const [q, setQ] = useState('');
  const [found, setFound] = useState<{ id: string; title: string; year?: number | null }[]>([]);
  const [picked, setPicked] = useState<{ id: string; title: string } | null>(null);
  const { busy, error, notice, run } = useRun();

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
  const done = async (fn: () => Promise<unknown>, message: string) => {
    const ok = await run(async () => {
      await fn();
      await res.reload();
      return true;
    }, message);
    if (ok) {
      setDialog(null);
      setPicked(null);
      setQ('');
    }
    return ok;
  };
  return (
    <>
      <Banner error={error} notice={notice} />
      <dl className="adm-kv" aria-label="On air now">
        <dt>On air</dt>
        <dd>
          {st.now
            ? `${st.now.title}${st.now.episode_label ? ` · ${st.now.episode_label}` : ''}`
            : 'Off the air'}
        </dd>
        <dt>Source</dt>
        <dd>
          {st.override ? (
            <>
              <Badge variant="warning">manual override</Badge> since {fmtMs(st.override.started_ms)}
            </>
          ) : (
            'Schedule'
          )}
        </dd>
      </dl>
      <div className="adm-actions">
        <Button variant="secondary" onClick={() => setDialog('skip')}>
          Skip to the next programme
        </Button>
        {st.override && (
          <Button variant="secondary" onClick={() => setDialog('clear')}>
            Return to the schedule
          </Button>
        )}
      </div>

      <section aria-label="Put a title on air" className="ss-card">
        <h2 className="ss-sub">Put a title on air now</h2>
        <TextInput
          label="Find a title"
          hint="Viewers switch to it immediately and the schedule resumes when it ends."
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setPicked(null);
          }}
        />
        {found.length > 0 && !picked && (
          <ul className="ss-list" aria-label="Matching titles">
            {found.map((t) => (
              <li key={t.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {t.title}
                    {t.year ? ` (${t.year})` : ''}
                  </div>
                </div>
                <Button size="sm" variant="secondary" onClick={() => setPicked(t)}>
                  Choose {t.title}
                </Button>
              </li>
            ))}
          </ul>
        )}
        {picked && (
          <div className="adm-actions">
            <span>
              Selected: <strong>{picked.title}</strong>
            </span>
            <Button onClick={() => setDialog('put')}>Put on air now</Button>
          </div>
        )}
      </section>

      <ReasonDialog
        open={dialog === 'skip'}
        title={`Skip ${st.now?.title ?? 'the current programme'}`}
        description="Everyone watching moves to the next programme."
        confirmLabel="Skip"
        reasonOptional
        onCancel={() => setDialog(null)}
        onConfirm={(reason) =>
          done(() => tv.skipProgramme(channel.id, reason || undefined), 'Skipped.')
        }
      />
      <ReasonDialog
        open={dialog === 'put' && !!picked}
        title={`Put ${picked?.title ?? ''} on air`}
        description={`Replaces what is on ${channel.name} right now.`}
        confirmLabel="Put on air"
        reasonOptional
        onCancel={() => setDialog(null)}
        onConfirm={(reason) =>
          done(
            () =>
              tv.setOverride(channel.id, { title_id: picked!.id, ...(reason ? { reason } : {}) }),
            'On air.',
          )
        }
      />
      <ConfirmDialog
        open={dialog === 'clear'}
        title="Return to the schedule"
        description="The manual override ends and the channel follows its schedule again."
        confirmLabel="Return to schedule"
        loading={busy}
        onCancel={() => setDialog(null)}
        onConfirm={() => void done(() => tv.clearOverride(channel.id), 'Back on schedule.')}
      />
    </>
  );
}

/* ------------------------------------------------------------- what aired */

export function AiredReport({ channel }: { channel: tv.AdminChannel }) {
  const [range, setRange] = useState('1');
  const to = Date.now();
  const from = range === '1' ? to - DAY : to - Number(range) * DAY;
  const { res, unavailable } = useChannelLoad(`tv-aired-${channel.id}-${range}`, () =>
    tv.getAired(channel.id, from, to),
  );
  if (unavailable) return <NotAvailableYet what="The what-aired report" />;
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
              <th scope="col">Peak viewers</th>
            </tr>
          </thead>
          <tbody>
            {[...res.data.items]
              .sort((a, b) => b.start_ms - a.start_ms)
              .map((a, i) => (
                <tr key={`${a.start_ms}-${i}`}>
                  <td>
                    {fmtMs(a.start_ms)} to {clock(a.end_ms)}
                  </td>
                  <td>
                    {a.title}
                    {a.episode_label ? ` · ${a.episode_label}` : ''}
                  </td>
                  <td>{a.source ?? 'schedule'}</td>
                  <td>{a.peak_viewers ?? 'n/a'}</td>
                </tr>
              ))}
          </tbody>
        </table>
      )}
    </>
  );
}
