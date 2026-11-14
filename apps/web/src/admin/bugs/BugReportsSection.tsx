import { Badge, Button, ConfirmDialog, EmptyState, Notice, Select } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../../api/client';
import {
  closeBugReport,
  getBugReport,
  getBugScreenshot,
  listBugReports,
} from '../../bugreport/api';
import type { BugReportDetail, BugReportRow, BugStatus } from '../../bugreport/contract';
import { setBugOpenCount } from '../../bugreport/openState';
import { formatNumber, useT } from '../../i18n';
import { Async, Loading, SectionHeader, fmtMs, useAdminViewer, useResource } from '../ui';
import './bugs.css';

/**
 * Admin > Bug reports (SEC-AUD-168, D-650): the list behind `GET /admin/bug-reports` (free), one report behind
 * `GET /admin/bug-reports/{id}` (an audited read), the picture only when a person presses Show screenshot (a second
 * audited read), and Close report, which deletes the picture. Any appointed staff member, like crash reports; the
 * server decides, the section list only hides the entry from everyone else.
 *
 * The list pages (SEC-AUD-267: "Load more" under it asks for the reports after the last one shown), and a report is
 * read defensively (SEC-AUD-266: every part of the debug bundle can be missing from a row stored before the server
 * wrote them all, and a report that cannot be opened would also hide the Close button that lives on its page).
 */
export function BugReportsSection() {
  const [openId, setOpenId] = useState<string | null>(null);
  return openId ? (
    <BugDetailView id={openId} onBack={() => setOpenId(null)} />
  ) : (
    <BugList onOpen={setOpenId} />
  );
}

function BugList({ onOpen }: { onOpen: (id: string) => void }) {
  const t = useT();
  const viewer = useAdminViewer();
  const [status, setStatus] = useState<BugStatus>('open');
  const res = useResource(`admin:bug-reports:${status}`, () => listBugReports(status));
  useEffect(() => {
    if (res.data) setBugOpenCount(res.data.open_count);
  }, [res.data]);
  // The pages after the first, appended under it. A new first page (another status, Refresh) starts the list again.
  const [more, setMore] = useState<{ rows: BugReportRow[]; hasMore: boolean }>({
    rows: [],
    hasMore: false,
  });
  const [moreBusy, setMoreBusy] = useState(false);
  const [moreError, setMoreError] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    generation.current += 1;
    setMore({ rows: [], hasMore: false });
    setMoreBusy(false);
    setMoreError(false);
  }, [res.data, status]);
  const loadMore = useCallback(
    async (lastId: string) => {
      const mine = generation.current;
      setMoreBusy(true);
      setMoreError(false);
      try {
        const page = await listBugReports(status, 50, lastId);
        if (mine !== generation.current) return;
        setMore((m) => ({ rows: [...m.rows, ...page.reports], hasMore: page.has_more === true }));
      } catch {
        if (mine === generation.current) setMoreError(true);
      } finally {
        if (mine === generation.current) setMoreBusy(false);
      }
    },
    [status],
  );
  return (
    <>
      <SectionHeader
        title={t('admin.bugs.title')}
        description={t('admin.bugs.description')}
        actions={
          <Button variant="secondary" onClick={() => void res.reload()} loading={res.loading}>
            {t('admin.bugs.refresh')}
          </Button>
        }
      />
      <Notice variant="info" className="ss-status">
        {t('admin.bugs.auditNotice')}
      </Notice>
      {res.data?.beta_over_100 && (
        <Notice variant="warning" className="ss-status">
          {t('admin.bugs.beta100Notice')}{' '}
          {viewer.can('MANAGE_FEATURES') && (
            <Link to="/admin/$section" params={{ section: 'features' }}>
              {t('admin.bugs.beta100Link')}
            </Link>
          )}
        </Notice>
      )}
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.bugs.filterStatus')}
          options={[
            { value: 'open', label: t('admin.bugs.statusOpen') },
            { value: 'closed', label: t('admin.bugs.statusClosed') },
          ]}
          value={status}
          onValueChange={(v) => setStatus(v === 'closed' ? 'closed' : 'open')}
        />
      </div>
      <Async res={res} label={t('admin.bugs.loading')}>
        {(list) => {
          const rows = [...list.reports, ...more.rows];
          const hasMore = more.rows.length > 0 ? more.hasMore : list.has_more === true;
          const lastId = rows[rows.length - 1]?.id;
          return rows.length === 0 ? (
            <EmptyState
              title={t('admin.bugs.emptyTitle')}
              description={t('admin.bugs.emptyDescription')}
            />
          ) : (
            <>
              <div className="adm-table-wrap">
                <table className="adm-table adm-bug-table" aria-label={t('admin.bugs.tableLabel')}>
                  <thead>
                    <tr>
                      <th scope="col">{t('admin.bugs.colWhen')}</th>
                      <th scope="col">{t('admin.bugs.colRoute')}</th>
                      <th scope="col">{t('admin.bugs.colBuild')}</th>
                      <th scope="col">{t('admin.bugs.colReporter')}</th>
                      <th scope="col">{t('admin.bugs.colPicture')}</th>
                      <th scope="col">{t('admin.bugs.colStatus')}</th>
                      <th scope="col">
                        <span className="adm-vh">{t('admin.bugs.colActions')}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((r) => (
                      <tr key={r.id}>
                        <td>{fmtMs(r.created_ms)}</td>
                        <td className="adm-mono">{r.route}</td>
                        <td className="adm-mono">{r.commit.slice(0, 12)}</td>
                        <td>{r.reporter.username}</td>
                        <td>{r.has_screenshot ? t('admin.bugs.yes') : t('admin.bugs.no')}</td>
                        <td>
                          <Badge variant={r.status === 'open' ? 'warning' : 'neutral'}>
                            {r.status === 'open'
                              ? t('admin.bugs.statusOpen')
                              : t('admin.bugs.statusClosed')}
                          </Badge>
                        </td>
                        <td>
                          <Button
                            size="sm"
                            variant="secondary"
                            aria-label={t('admin.bugs.openLabel', {
                              who: r.reporter.username,
                              when: fmtMs(r.created_ms),
                            })}
                            onClick={() => onOpen(r.id)}
                          >
                            {t('admin.bugs.open')}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {moreError && (
                <Notice variant="danger" role="alert">
                  {t('admin.bugs.loadMoreFailed')}
                </Notice>
              )}
              {hasMore && lastId && (
                <p className="adm-bug-more">
                  <Button
                    variant="secondary"
                    loading={moreBusy}
                    onClick={() => void loadMore(lastId)}
                  >
                    {t('admin.bugs.loadMore')}
                  </Button>
                </p>
              )}
            </>
          );
        }}
      </Async>
    </>
  );
}

function BugDetailView({ id, onBack }: { id: string; onBack: () => void }) {
  const t = useT();
  const res = useResource(`admin:bug-report:${id}`, () => getBugReport(id));
  const back = (
    <Button variant="secondary" onClick={onBack}>
      {t('admin.bugs.back')}
    </Button>
  );
  if (res.data === undefined) {
    return (
      <>
        <SectionHeader title={t('admin.bugs.title')} actions={back} />
        {res.error ? (
          <Async res={res}>{() => null}</Async>
        ) : (
          <Loading label={t('admin.bugs.loadingDetail')} />
        )}
      </>
    );
  }
  return <BugDetailBody report={res.data} back={back} onClosed={() => void res.reload()} />;
}

const yesNo = (t: ReturnType<typeof useT>, v: boolean | undefined) =>
  v ? t('admin.bugs.yes') : t('admin.bugs.no');

function Facts({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="adm-kv">
      {rows.map(([k, v]) => (
        <div key={k} style={{ display: 'contents' }}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function BugDetailBody({
  report,
  back,
  onClosed,
}: {
  report: BugReportDetail;
  back: ReactNode;
  onClosed: () => void;
}) {
  const t = useT();
  const b = report.bundle;
  // Every part of the bundle can be missing from a row stored before SEC-AUD-266: read each one with a guard.
  const unknown = t('admin.bugs.unknown');
  const crashes = b.crashes ?? [];
  const failed = b.failed_requests ?? [];
  const caps = Object.entries(b.caps ?? {})
    .filter(([k, v]) => k !== 'engine' && typeof v === 'boolean')
    .sort(([a], [c]) => a.localeCompare(c));
  const [shot, setShot] = useState<string | null>(null);
  const [shotBusy, setShotBusy] = useState(false);
  const [shotError, setShotError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [closing, setClosing] = useState(false);
  const [closeError, setCloseError] = useState<string | null>(null);
  const urlRef = useRef<string | null>(null);

  const hide = () => {
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    urlRef.current = null;
    setShot(null);
  };
  // A blob URL never outlives the page that showed it.
  useEffect(
    () => () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = null;
    },
    [],
  );

  const show = async () => {
    setShotBusy(true);
    setShotError(null);
    try {
      const blob = await getBugScreenshot(report.id);
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = URL.createObjectURL(blob);
      setShot(urlRef.current);
    } catch (e) {
      setShotError(
        e instanceof ApiError && e.status === 404
          ? t('admin.bugs.shotGone')
          : t('admin.bugs.shotFailed'),
      );
    } finally {
      setShotBusy(false);
    }
  };

  const doClose = async () => {
    setClosing(true);
    setCloseError(null);
    try {
      await closeBugReport(report.id);
      hide();
      setConfirm(false);
      onClosed();
    } catch {
      setCloseError(t('admin.bugs.closeFailed'));
    } finally {
      setClosing(false);
    }
  };

  const open = report.status === 'open';
  return (
    <>
      <SectionHeader
        title={t('admin.bugs.detailTitle', { who: report.reporter.username })}
        description={fmtMs(report.created_ms)}
        actions={
          <>
            {back}
            {open && (
              <Button variant="danger" onClick={() => setConfirm(true)}>
                {t('admin.bugs.close')}
              </Button>
            )}
          </>
        }
      />
      <Notice variant="info" className="ss-status" role="status">
        {t('admin.bugs.detailAudit')}
      </Notice>
      {closeError && (
        <Notice variant="danger" role="alert">
          {closeError}
        </Notice>
      )}

      <section className="ss-card" aria-labelledby="adm-bug-desc">
        <h2 id="adm-bug-desc">{t('admin.bugs.descriptionTitle')}</h2>
        <p className="adm-bug-desc">{report.description}</p>
      </section>

      <section className="ss-card" aria-labelledby="adm-bug-facts">
        <h2 id="adm-bug-facts">{t('admin.bugs.factsTitle')}</h2>
        <Facts
          rows={[
            [
              t('admin.bugs.colStatus'),
              open ? t('admin.bugs.statusOpen') : t('admin.bugs.statusClosed'),
            ],
            [t('admin.bugs.colReporter'), report.reporter.username],
            [t('admin.bugs.colRoute'), <span className="adm-mono">{report.route}</span>],
            [t('admin.bugs.colBuild'), <span className="adm-mono">{report.commit}</span>],
            [t('admin.bugs.sentAt'), typeof b.at === 'number' ? fmtMs(b.at) : unknown],
            ...(report.closed_ms
              ? ([
                  [
                    t('admin.bugs.closedAt'),
                    `${fmtMs(report.closed_ms)}${report.closed_by ? ` (${report.closed_by.username})` : ''}`,
                  ],
                ] as [string, ReactNode][])
              : []),
          ]}
        />
      </section>

      <section className="ss-card" aria-labelledby="adm-bug-debug">
        <h2 id="adm-bug-debug">{t('admin.bugs.debugTitle')}</h2>
        <Facts
          rows={[
            [
              t('admin.bugs.viewport'),
              // The server writes 0 by 0 for a viewport the body did not carry.
              b.viewport && b.viewport.w > 0 && b.viewport.h > 0
                ? `${b.viewport.w} × ${b.viewport.h} @${b.viewport.dpr}`
                : unknown,
            ],
            [t('admin.bugs.engine'), b.caps?.engine ?? unknown],
            [t('admin.bugs.locale'), b.locale ?? unknown],
            [t('admin.bugs.theme'), b.theme ?? unknown],
            [
              t('admin.bugs.online'),
              b.online === undefined || b.online === null ? unknown : yesNo(t, b.online),
            ],
            [
              t('admin.bugs.gateway'),
              b.gateway === undefined || b.gateway.status === 'unknown'
                ? unknown
                : [
                    b.gateway.status,
                    b.gateway.reconnects !== undefined
                      ? t('admin.bugs.reconnects', { n: b.gateway.reconnects })
                      : '',
                    b.gateway.heartbeat_ms !== undefined ? `${b.gateway.heartbeat_ms} ms` : '',
                  ]
                    .filter(Boolean)
                    .join(', '),
            ],
            [t('admin.bugs.sw'), b.sw ?? unknown],
            [t('admin.bugs.voice'), b.voice === undefined ? unknown : yesNo(t, b.voice.connected)],
            [
              t('admin.bugs.crashes'),
              crashes.length === 0 ? (
                t('admin.bugs.none')
              ) : (
                <span className="adm-mono">{crashes.map((c) => c.slice(0, 10)).join(', ')}</span>
              ),
            ],
          ]}
        />
        <h3>{t('admin.bugs.capsTitle')}</h3>
        {caps.length === 0 ? (
          <p className="ss-meta">{t('admin.bugs.none')}</p>
        ) : (
          <ul className="adm-bug-caps">
            {caps.map(([name, on]) => (
              <li key={name}>
                <Badge variant={on ? 'success' : 'neutral'}>
                  {on
                    ? t('admin.bugs.capOn', { name: name.replace(/_/g, ' ') })
                    : t('admin.bugs.capOff', { name: name.replace(/_/g, ' ') })}
                </Badge>
              </li>
            ))}
          </ul>
        )}
        <h3>{t('admin.bugs.failedTitle')}</h3>
        {failed.length === 0 ? (
          <p className="ss-meta">{t('admin.bugs.none')}</p>
        ) : (
          <div className="adm-table-wrap">
            <table className="adm-table" aria-label={t('admin.bugs.failedLabel')}>
              <thead>
                <tr>
                  <th scope="col">{t('admin.bugs.colMethod')}</th>
                  <th scope="col">{t('admin.bugs.colPath')}</th>
                  <th scope="col">{t('admin.bugs.colStatus')}</th>
                  <th scope="col">{t('admin.bugs.colMs')}</th>
                  <th scope="col">{t('admin.bugs.colAgo')}</th>
                </tr>
              </thead>
              <tbody>
                {failed.map((f, i) => (
                  <tr key={i}>
                    <td>{f.method}</td>
                    <td className="adm-mono">{f.path}</td>
                    <td>{f.status}</td>
                    <td>{formatNumber(f.ms)}</td>
                    <td>{formatNumber(f.ago_s)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="ss-card" aria-labelledby="adm-bug-shot">
        <h2 id="adm-bug-shot">{t('admin.bugs.shotTitle')}</h2>
        {!report.has_screenshot ? (
          <p className="ss-meta">{t('admin.bugs.noShot')}</p>
        ) : shot ? (
          <>
            <img className="adm-bug-shot" src={shot} alt={t('admin.bugs.shotAlt')} />
            <p>
              <Button variant="secondary" size="sm" onClick={hide}>
                {t('admin.bugs.hideShot')}
              </Button>
            </p>
          </>
        ) : (
          <>
            <p className="ss-sub">{t('admin.bugs.shotAudit')}</p>
            <Button variant="secondary" loading={shotBusy} onClick={() => void show()}>
              {t('admin.bugs.showShot')}
            </Button>
            {shotError && (
              <Notice variant="danger" role="alert">
                {shotError}
              </Notice>
            )}
          </>
        )}
      </section>

      <ConfirmDialog
        open={confirm}
        danger
        title={t('admin.bugs.closeTitle')}
        description={
          report.has_screenshot ? t('admin.bugs.closeBodyShot') : t('admin.bugs.closeBody')
        }
        confirmLabel={t('admin.bugs.close')}
        loading={closing}
        onConfirm={() => void doClose()}
        onCancel={() => setConfirm(false)}
      />
    </>
  );
}
