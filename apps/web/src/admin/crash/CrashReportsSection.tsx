import {
  Badge,
  Button,
  EmptyState,
  Notice,
  Select,
  TextInput,
  type BadgeVariant,
} from '@sideroom/ui';
import { useMemo, useState, type ReactNode } from 'react';
import { formatNumber, type MessageKey, useT } from '../../i18n';
import { Async, Loading, SectionHeader, fmtMs, useResource } from '../ui';
import * as api from './api';
import './crash.css';

/**
 * Admin > Crash reports (OPS-CRASH-002): the groups behind `GET /admin/crash-reports`, and one report with its replay
 * buffer behind `GET /admin/crash-reports/{id}`. The list is free; opening a report is the audited read the API
 * already records, so a report is only fetched when a person presses Open. Staff-only exactly like the API (any
 * appointed staff member); the server decides, the section list only hides the entry from everyone else.
 */

const KIND_LABEL: Record<string, MessageKey> = {
  error: 'admin.crash.kindError',
  unhandled_rejection: 'admin.crash.kindRejection',
  react_boundary: 'admin.crash.kindRender',
  wasm_panic: 'admin.crash.kindWasm',
  webgpu_panic: 'admin.crash.kindWebgpu',
};
const KIND_VARIANT: Record<string, BadgeVariant> = {
  error: 'danger',
  unhandled_rejection: 'warning',
  react_boundary: 'danger',
  wasm_panic: 'accent',
  webgpu_panic: 'accent',
};

type Sort = 'recent' | 'count';

/** The first characters of a fingerprint: enough to tell groups apart in a list. */
const shortFp = (fp: string) => fp.slice(0, 10);

export function CrashReportsSection() {
  const [openId, setOpenId] = useState<string | null>(null);
  return openId ? (
    <CrashDetailView id={openId} onBack={() => setOpenId(null)} />
  ) : (
    <CrashList onOpen={setOpenId} />
  );
}

function useKindLabel() {
  const t = useT();
  return (kind: string) => {
    const key = KIND_LABEL[kind];
    return key ? t(key) : kind.replace(/_/g, ' ');
  };
}

function KindBadge({ kind }: { kind: string }) {
  const label = useKindLabel();
  return <Badge variant={KIND_VARIANT[kind] ?? 'neutral'}>{label(kind)}</Badge>;
}

function CrashList({ onOpen }: { onOpen: (id: string) => void }) {
  const t = useT();
  const res = useResource('admin:crash-reports', api.listCrashReports);
  const [q, setQ] = useState('');
  const [kind, setKind] = useState('');
  const [sort, setSort] = useState<Sort>('recent');

  const kindOptions = [
    { value: '', label: t('admin.crash.kindAll') },
    { value: 'error', label: t('admin.crash.kindError') },
    { value: 'unhandled_rejection', label: t('admin.crash.kindRejection') },
    { value: 'react_boundary', label: t('admin.crash.kindRender') },
    { value: 'wasm_panic', label: t('admin.crash.kindWasm') },
    { value: 'webgpu_panic', label: t('admin.crash.kindWebgpu') },
  ];
  const sortOptions = [
    { value: 'recent', label: t('admin.crash.sortRecent') },
    { value: 'count', label: t('admin.crash.sortCount') },
  ];

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = (res.data ?? []).filter(
      (r) =>
        (!kind || r.kind === kind) &&
        (!needle ||
          [r.fingerprint, r.commit, r.route, r.kind, ...(r.top_frames ?? [])].some((s) =>
            s.toLowerCase().includes(needle),
          )),
    );
    return [...list].sort((a, b) =>
      sort === 'count'
        ? b.count - a.count || b.last_seen_ms - a.last_seen_ms
        : b.last_seen_ms - a.last_seen_ms,
    );
  }, [res.data, q, kind, sort]);

  const showFrames = (res.data ?? []).some((r) => (r.top_frames?.length ?? 0) > 0);
  const filtered = q.trim() !== '' || kind !== '';

  return (
    <>
      <SectionHeader
        title={t('admin.crash.title')}
        description={t('admin.crash.description')}
        actions={
          <Button variant="secondary" onClick={() => void res.reload()} loading={res.loading}>
            {t('admin.crash.refresh')}
          </Button>
        }
      />
      <Notice variant="info" className="ss-status">
        {t('admin.crash.auditNotice')}
      </Notice>
      <Async res={res} label={t('admin.crash.loading')}>
        {(all) =>
          all.length === 0 ? (
            <EmptyState
              title={t('admin.crash.emptyTitle')}
              description={t('admin.crash.emptyDescription')}
            />
          ) : (
            <>
              <div className="ss-toolbar adm-crash-toolbar">
                <div className="ss-search">
                  <TextInput
                    label={t('admin.crash.searchLabel')}
                    type="search"
                    value={q}
                    placeholder={t('admin.crash.searchPlaceholder')}
                    onChange={(e) => setQ(e.target.value)}
                  />
                </div>
                <Select
                  aria-label={t('admin.crash.filterByKind')}
                  options={kindOptions}
                  value={kind}
                  onValueChange={setKind}
                />
                <Select
                  aria-label={t('admin.crash.sortBy')}
                  options={sortOptions}
                  value={sort}
                  onValueChange={(v) => setSort(v === 'count' ? 'count' : 'recent')}
                />
              </div>
              <p className="ss-meta" role="status">
                {t('admin.crash.shown', {
                  shown: formatNumber(rows.length),
                  total: formatNumber(all.length),
                })}
                {all.length >= api.LIST_LIMIT &&
                  ` ${t('admin.crash.truncated', { count: formatNumber(api.LIST_LIMIT) })}`}
              </p>
              {rows.length === 0 ? (
                <EmptyState
                  title={t('admin.crash.noMatchTitle')}
                  description={t('admin.crash.noMatchDescription')}
                  {...(filtered
                    ? {
                        action: (
                          <Button
                            variant="secondary"
                            onClick={() => {
                              setQ('');
                              setKind('');
                            }}
                          >
                            {t('admin.crash.clearFilters')}
                          </Button>
                        ),
                      }
                    : {})}
                />
              ) : (
                <div className="adm-table-wrap">
                  <table
                    className="adm-table adm-crash-table"
                    aria-label={t('admin.crash.tableLabel')}
                  >
                    <thead>
                      <tr>
                        <th scope="col">{t('admin.crash.colFingerprint')}</th>
                        <th scope="col">{t('admin.crash.colCount')}</th>
                        <th scope="col">{t('admin.crash.colFirstSeen')}</th>
                        <th scope="col">{t('admin.crash.colLastSeen')}</th>
                        <th scope="col">{t('admin.crash.colCommit')}</th>
                        <th scope="col">{t('admin.crash.colRoute')}</th>
                        {showFrames && <th scope="col">{t('admin.crash.colFrames')}</th>}
                        <th scope="col">
                          <span className="adm-vh">{t('admin.crash.colActions')}</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((r) => (
                        <tr key={r.id}>
                          <td>
                            <div className="adm-mono">{shortFp(r.fingerprint)}</div>
                            <KindBadge kind={r.kind} />
                          </td>
                          <td>{formatNumber(r.count)}</td>
                          <td>{fmtMs(r.first_seen_ms)}</td>
                          <td>{fmtMs(r.last_seen_ms)}</td>
                          <td className="adm-mono">{r.commit.slice(0, 12)}</td>
                          <td className="adm-mono">{r.route}</td>
                          {showFrames && (
                            <td className="adm-mono">
                              {(r.top_frames ?? []).map((f, i) => (
                                <div key={i}>{f}</div>
                              ))}
                            </td>
                          )}
                          <td>
                            <Button
                              size="sm"
                              variant="secondary"
                              aria-label={t('admin.crash.openLabel', {
                                fingerprint: shortFp(r.fingerprint),
                              })}
                              onClick={() => onOpen(r.id)}
                            >
                              {t('admin.crash.open')}
                            </Button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          )
        }
      </Async>
    </>
  );
}

function CrashDetailView({ id, onBack }: { id: string; onBack: () => void }) {
  const t = useT();
  const res = useResource(`admin:crash-report:${id}`, () => api.getCrashReport(id));
  const back = (
    <Button variant="secondary" onClick={onBack}>
      {t('admin.crash.back')}
    </Button>
  );
  if (res.data === undefined) {
    return (
      <>
        <SectionHeader title={t('admin.crash.title')} actions={back} />
        {res.error ? (
          <Async res={res}>{() => null}</Async>
        ) : (
          <Loading label={t('admin.crash.loadingDetail')} />
        )}
      </>
    );
  }
  return <CrashDetailBody report={res.data} back={back} />;
}

function CrashDetailBody({ report, back }: { report: api.CrashDetail; back: ReactNode }) {
  const t = useT();
  const label = useKindLabel();
  const frames = report.card?.frames ?? [];
  const caps = Object.entries(report.card?.caps ?? {}).sort(([a], [b]) => a.localeCompare(b));
  const replay = Array.isArray(report.replay) ? report.replay : [];
  return (
    <>
      <SectionHeader
        title={t('admin.crash.detailTitle', { fingerprint: shortFp(report.fingerprint) })}
        description={label(report.kind)}
        actions={back}
      />
      <Notice variant="info" className="ss-status" role="status">
        {t('admin.crash.detailAudit')}
      </Notice>

      <section className="ss-card" aria-labelledby="adm-crash-card">
        <h2 id="adm-crash-card">{t('admin.crash.cardTitle')}</h2>
        <dl className="adm-kv">
          <dt>{t('admin.crash.fullFingerprint')}</dt>
          <dd className="adm-mono">{report.fingerprint}</dd>
          <dt>{t('admin.crash.colKind')}</dt>
          <dd>
            <KindBadge kind={report.kind} />
          </dd>
          <dt>{t('admin.crash.colCount')}</dt>
          <dd>{formatNumber(report.count)}</dd>
          <dt>{t('admin.crash.colFirstSeen')}</dt>
          <dd>{fmtMs(report.first_seen_ms)}</dd>
          <dt>{t('admin.crash.colLastSeen')}</dt>
          <dd>{fmtMs(report.last_seen_ms)}</dd>
          <dt>{t('admin.crash.colCommit')}</dt>
          <dd className="adm-mono">{report.commit}</dd>
          <dt>{t('admin.crash.colRoute')}</dt>
          <dd className="adm-mono">{report.route}</dd>
          {report.card?.engine && (
            <>
              <dt>{t('admin.crash.engine')}</dt>
              <dd>{report.card.engine}</dd>
            </>
          )}
          {typeof report.card?.at === 'number' && (
            <>
              <dt>{t('admin.crash.reportedAt')}</dt>
              <dd>{fmtMs(report.card.at)}</dd>
            </>
          )}
        </dl>
      </section>

      <section className="ss-card" aria-labelledby="adm-crash-frames">
        <h2 id="adm-crash-frames">{t('admin.crash.framesTitle')}</h2>
        {frames.length === 0 ? (
          <p className="ss-meta">{t('admin.crash.noFrames')}</p>
        ) : (
          <div className="adm-table-wrap">
            <table className="adm-table" aria-label={t('admin.crash.framesLabel')}>
              <thead>
                <tr>
                  <th scope="col">#</th>
                  <th scope="col">{t('admin.crash.colFunction')}</th>
                  <th scope="col">{t('admin.crash.colFile')}</th>
                  <th scope="col">{t('admin.crash.colLine')}</th>
                  <th scope="col">{t('admin.crash.colColumn')}</th>
                </tr>
              </thead>
              <tbody>
                {frames.map((f, i) => (
                  <tr key={i}>
                    <td>{i + 1}</td>
                    <td className="adm-mono">{f.fn ?? '—'}</td>
                    <td className="adm-mono">{f.file}</td>
                    <td>{f.line}</td>
                    <td>{f.col}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="ss-card" aria-labelledby="adm-crash-caps">
        <h2 id="adm-crash-caps">{t('admin.crash.capsTitle')}</h2>
        {caps.length === 0 ? (
          <p className="ss-meta">{t('admin.crash.noCaps')}</p>
        ) : (
          <ul className="adm-crash-caps">
            {caps.map(([name, on]) => (
              <li key={name}>
                <Badge variant={on ? 'success' : 'neutral'}>
                  {on
                    ? t('admin.crash.capOn', { name: name.replace(/_/g, ' ') })
                    : t('admin.crash.capOff', { name: name.replace(/_/g, ' ') })}
                </Badge>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="ss-card" aria-labelledby="adm-crash-replay">
        <h2 id="adm-crash-replay">{t('admin.crash.replayTitle')}</h2>
        <p className="ss-sub">{t('admin.crash.replayDescription')}</p>
        {replay.length === 0 ? (
          <p className="ss-meta">{t('admin.crash.noReplay')}</p>
        ) : (
          <div className="adm-table-wrap">
            <table className="adm-table" aria-label={t('admin.crash.replayLabel')}>
              <thead>
                <tr>
                  <th scope="col">{t('admin.crash.colTick')}</th>
                  <th scope="col">{t('admin.crash.colTime')}</th>
                  <th scope="col">{t('admin.crash.colSource')}</th>
                  <th scope="col">{t('admin.crash.colInput')}</th>
                </tr>
              </thead>
              <tbody>
                {replay.map((tick, i) => (
                  <tr key={i}>
                    <td>{i + 1}</td>
                    <td>{formatNumber(tick.t)}</td>
                    <td>{tick.src}</td>
                    <td className="adm-mono">{tick.input}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
