// Skip-marker editor (CIN-ADM-001): intro, recap and credits windows for a film or an episode. Numeric inputs are
// the source of truth; a timeline shows the windows, and when the asset can be played a preview video (the
// player's engine, loaded on demand) lets staff scrub and copy the current time into a field.
import { Button, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { t, useT } from '../../i18n';
import { useAdminViewer } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, Section, fmtClock, useLoad, useRun } from './hooks';

const KINDS: api.MarkerKind[] = ['intro', 'recap', 'credits'];
const DAY_MS = 24 * 3600 * 1000;

interface Row {
  start: string;
  end: string;
}
type Rows = Record<api.MarkerKind, Row>;
const emptyRows = (): Rows => ({
  intro: { start: '', end: '' },
  recap: { start: '', end: '' },
  credits: { start: '', end: '' },
});

const secStr = (ms: number) => String(Math.round(ms / 100) / 10);
const rowsFrom = (markers: api.Marker[]): Rows => {
  const r = emptyRows();
  for (const m of markers) r[m.kind] = { start: secStr(m.start_ms), end: secStr(m.end_ms) };
  return r;
};
const sec = (s: string): number | null => {
  if (!s.trim()) return null;
  const n = Number(s);
  return Number.isFinite(n) && n >= 0 ? Math.round(n * 1000) : null;
};

/** Per-kind problems, or null when the rows can be saved (both fields empty means "no window"). */
export function validateRows(rows: Rows): Partial<Record<api.MarkerKind, string>> {
  const out: Partial<Record<api.MarkerKind, string>> = {};
  for (const k of KINDS) {
    const { start, end } = rows[k];
    if (!start.trim() && !end.trim()) continue;
    const s = sec(start);
    const e = sec(end);
    if (s === null || e === null) out[k] = t('admin.markersPanel.errBoth');
    else if (e <= s) out[k] = t('admin.markersPanel.errOrder');
    else if (e > DAY_MS) out[k] = t('admin.markersPanel.errDay');
  }
  return out;
}

export function rowsToMarkers(rows: Rows): api.MarkerInput[] {
  return KINDS.flatMap((k) => {
    const s = sec(rows[k].start);
    const e = sec(rows[k].end);
    return s !== null && e !== null && rows[k].start.trim()
      ? [{ kind: k, start_ms: s, end_ms: e }]
      : [];
  });
}

interface Props {
  detail: api.AdminTitleDetail;
  reload: () => Promise<void>;
}

export function MarkersPanel({ detail, reload }: Props) {
  const t = useT();
  const episodic = detail.title.kind !== 'movie' && detail.episodes.length > 0;
  const episodes = useMemo(
    () =>
      [...detail.episodes].sort((a, b) => a.season_number - b.season_number || a.number - b.number),
    [detail.episodes],
  );
  const [target, setTarget] = useState<string>(episodic ? episodes[0]!.id : 'title');
  if (detail.title.kind !== 'movie' && !episodic)
    return (
      <EmptyState
        title={t('admin.markersPanel.noEpisodes.title')}
        description={t('admin.markersPanel.noEpisodes.description')}
      />
    );
  const ep = episodes.find((e) => e.id === target);
  return (
    <Section title={t('admin.markersPanel.title')}>
      {episodic && (
        <Select
          aria-label={t('admin.markersPanel.episode')}
          value={target}
          onValueChange={setTarget}
          options={episodes.map((e) => ({
            value: e.id,
            label: `S${e.season_number} E${e.number}${e.title ? ` · ${e.title}` : ''}`,
          }))}
        />
      )}
      <MarkerEditor
        key={target}
        titleId={detail.title.id}
        episodeId={ep?.id ?? null}
        assetId={(ep ? ep.asset_id : detail.title.asset_id) ?? null}
        runtimeMin={(ep ? ep.runtime_min : detail.title.runtime_min) ?? null}
        initial={ep ? undefined : detail.markers}
        reload={reload}
      />
    </Section>
  );
}

function MarkerEditor({
  titleId,
  episodeId,
  assetId,
  runtimeMin,
  initial,
  reload,
}: {
  titleId: string;
  episodeId: string | null;
  assetId: string | null;
  runtimeMin: number | null;
  initial: api.Marker[] | undefined;
  reload: () => Promise<void>;
}) {
  const t = useT();
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const loaded = useLoad<api.Marker[]>(`markers:${episodeId ?? titleId}`, async () =>
    episodeId ? (await api.getEpisodeMarkers(episodeId)).markers : (initial ?? []),
  );
  const [rows, setRows] = useState<Rows>(emptyRows);
  const [dirty, setDirty] = useState(false);
  const [duration, setDuration] = useState(0);
  const [now, setNow] = useState(0);
  const [ready, setReady] = useState(false);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const { busy, error, notice, run } = useRun();

  useEffect(() => {
    if (loaded.data && !dirty) setRows(rowsFrom(loaded.data));
  }, [loaded.data, dirty]);

  const problems = validateRows(rows);
  const bad = Object.keys(problems).length > 0;
  const total = duration > 0 ? duration * 1000 : runtimeMin ? runtimeMin * 60_000 : 0;
  const set = (k: api.MarkerKind, f: keyof Row, v: string) => {
    setDirty(true);
    setRows((r) => ({ ...r, [k]: { ...r[k], [f]: v } }));
  };

  const save = () =>
    void run(async () => {
      const markers = rowsToMarkers(rows);
      const saved = episodeId
        ? await api.putEpisodeMarkers(episodeId, markers)
        : await api.putTitleMarkers(titleId, markers);
      setRows(rowsFrom(saved.markers));
      setDirty(false);
      await reload();
      return true;
    }, t('admin.markersPanel.saved'));

  if (loaded.error && !loaded.data)
    return (
      <LoadProblem
        error={loaded.error}
        status={loaded.status}
        onRetry={() => void loaded.reload()}
        what={t('admin.markersPanel.whatMarkers')}
      />
    );

  const segs = KINDS.flatMap((k) => {
    const s = sec(rows[k].start);
    const e = sec(rows[k].end);
    return total > 0 && s !== null && e !== null && e > s
      ? [
          {
            k,
            left: Math.min(100, (s / total) * 100),
            width: Math.min(100, ((e - s) / total) * 100),
          },
        ]
      : [];
  });

  return (
    <div className="adm-cin-markers">
      <div className="adm-cin-timeline" role="img" aria-label={t('admin.markersPanel.timeline')}>
        {segs.map((s) => (
          <span
            key={s.k}
            className={`adm-cin-seg adm-cin-seg-${s.k}`}
            style={{ left: `${s.left}%`, width: `${s.width}%` }}
            title={s.k}
          />
        ))}
        {total > 0 && duration > 0 && (
          <span
            className="adm-cin-playhead"
            style={{ left: `${Math.min(100, (now / duration) * 100)}%` }}
          />
        )}
      </div>
      <Preview
        assetId={assetId}
        videoRef={videoRef}
        onDuration={setDuration}
        onTime={setNow}
        onReady={setReady}
      />
      <div className="adm-table-wrap">
        <table className="adm-table" aria-label={t('admin.markersPanel.windows')}>
          <thead>
            <tr>
              <th scope="col">{t('admin.markersPanel.col.window')}</th>
              <th scope="col">{t('admin.markersPanel.col.start')}</th>
              <th scope="col">{t('admin.markersPanel.col.end')}</th>
              <th scope="col">{t('admin.markersPanel.col.length')}</th>
            </tr>
          </thead>
          <tbody>
            {KINDS.map((k) => {
              const s = sec(rows[k].start);
              const e = sec(rows[k].end);
              return (
                <tr key={k}>
                  <th scope="row">{k}</th>
                  <td>
                    <TimeField
                      label={t('admin.markersPanel.startLabel', { kind: k })}
                      value={rows[k].start}
                      disabled={!canEdit}
                      onChange={(v) => set(k, 'start', v)}
                      onUseCurrent={
                        ready
                          ? () => set(k, 'start', secStr(videoRef.current!.currentTime * 1000))
                          : undefined
                      }
                    />
                  </td>
                  <td>
                    <TimeField
                      label={t('admin.markersPanel.endLabel', { kind: k })}
                      value={rows[k].end}
                      disabled={!canEdit}
                      onChange={(v) => set(k, 'end', v)}
                      onUseCurrent={
                        ready
                          ? () => set(k, 'end', secStr(videoRef.current!.currentTime * 1000))
                          : undefined
                      }
                    />
                    {problems[k] && (
                      <p role="alert" className="adm-art-error">
                        {problems[k]}
                      </p>
                    )}
                  </td>
                  <td>{s !== null && e !== null && e > s ? fmtClock(e - s) : ''}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="ss-meta">{t('admin.markersPanel.hint')}</p>
      <Banner error={error} notice={notice} />
      {canEdit ? (
        <div className="adm-actions">
          <Button onClick={save} loading={busy} disabled={bad || !dirty}>
            {t('admin.markersPanel.save')}
          </Button>
          <Button
            variant="ghost"
            disabled={!dirty || busy}
            onClick={() => {
              setRows(rowsFrom(loaded.data ?? []));
              setDirty(false);
            }}
          >
            {t('admin.markersPanel.revert')}
          </Button>
        </div>
      ) : (
        <p className="ss-na">{t('admin.markersPanel.readOnly')}</p>
      )}
    </div>
  );
}

function TimeField({
  label,
  value,
  onChange,
  onUseCurrent,
  disabled,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  onUseCurrent: (() => void) | undefined;
  disabled: boolean;
}) {
  const t = useT();
  return (
    <div className="adm-cin-time">
      <TextInput
        label={<span className="adm-vh">{label}</span>}
        type="number"
        min={0}
        step={0.1}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
      {onUseCurrent && !disabled && (
        <Button
          size="sm"
          variant="ghost"
          aria-label={t('admin.markersPanel.useCurrent', { label })}
          onClick={onUseCurrent}
        >
          {t('admin.markersPanel.now')}
        </Button>
      )}
    </div>
  );
}

/** Lazy preview through the player's engine; says so plainly when the asset cannot be played yet. */
function Preview({
  assetId,
  videoRef,
  onDuration,
  onTime,
  onReady,
}: {
  assetId: string | null;
  videoRef: React.MutableRefObject<HTMLVideoElement | null>;
  onDuration: (s: number) => void;
  onTime: (s: number) => void;
  onReady: (ready: boolean) => void;
}) {
  const t = useT();
  const [state, setState] = useState<'idle' | 'loading' | 'ready' | 'failed'>('idle');
  const [message, setMessage] = useState<string | null>(null);
  const engineRef = useRef<{ destroy: () => void } | null>(null);
  useEffect(() => () => engineRef.current?.destroy(), []);

  if (!assetId) return <p className="ss-na">{t('admin.markersPanel.preview.noVideo')}</p>;

  const load = async () => {
    setState('loading');
    setMessage(null);
    try {
      const [{ getAssetPlayback }, { createEngine }] = await Promise.all([
        import('../../cinema/player/api'),
        import('../../cinema/player/engine'),
      ]);
      const playback = await getAssetPlayback(assetId);
      const video = videoRef.current;
      if (!video) throw new Error('no video element');
      const engine = await createEngine(video);
      engineRef.current = engine;
      engine.load(playback.master_url);
      if (playback.duration_secs) onDuration(playback.duration_secs);
      setState('ready');
      onReady(true);
    } catch {
      setState('failed');
      setMessage(t('admin.markersPanel.preview.failed'));
    }
  };

  return (
    <div className="adm-cin-preview">
      <video
        ref={videoRef}
        controls={state === 'ready'}
        preload="metadata"
        className={state === 'ready' ? 'adm-cin-video' : 'adm-vh'}
        aria-label={t('admin.markersPanel.preview.label')}
        onLoadedMetadata={(e) => onDuration(e.currentTarget.duration || 0)}
        onTimeUpdate={(e) => onTime(e.currentTarget.currentTime)}
      />
      {state !== 'ready' && (
        <Button
          variant="secondary"
          size="sm"
          loading={state === 'loading'}
          onClick={() => void load()}
        >
          {t('admin.markersPanel.preview.load')}
        </Button>
      )}
      {message && (
        <p role="status" className="ss-meta">
          {message}
        </p>
      )}
    </div>
  );
}
