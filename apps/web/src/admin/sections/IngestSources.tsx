// Admin > Ingest sources (ADM-CNT-005, REV-UP-005/006): watched folders and storage imports of
// Library files. A dry run shows the structure the importer detected before anything is imported;
// a scan creates draft submissions for the source's staff owner. The naming tester is the same one
// the submitters see.
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState } from 'react';
import { useT } from '../../i18n';
import { NamingGuide } from '../../review/NamingGuide';
import * as api from '../../review/apiP13';
import { ActionStatus, Async, errorText, useAction, useResource } from '../ui';
import '../../review/review.css';

// The naming tester uses react-query; the admin shell has no provider of its own.
const namingClient = new QueryClient();

function Report({ r }: { r: api.ScanReport }) {
  const t = useT();
  return (
    <section className="rv-panel" aria-label={t('review.ingest.report')}>
      <h3>{t('review.ingest.report')}</h3>
      <p>
        {t('review.ingest.reportSummary', {
          files: r.files_seen,
          imported: r.imported,
          waiting: r.waiting,
          failed: r.failed,
        })}
      </p>
      {r.series.map((s) => (
        <div key={s.title}>
          <h4>
            {t('review.ingest.series', { title: s.title, count: s.chapters.length })}
            {s.existing_series && <Badge variant="neutral">{t('review.ingest.addsTo')}</Badge>}
          </h4>
          <ul className="rv-changes">
            {s.chapters.map((c) => (
              <li key={c.rel_path}>
                <strong>{c.rel_path}</strong>
                <span>
                  {c.status}
                  {c.note ? `: ${c.note}` : ''}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ))}
      {r.unplaced.length > 0 && (
        <>
          <h4>{t('review.ingest.unplaced')}</h4>
          <ul className="rv-changes">
            {r.unplaced.map((c) => (
              <li key={c.rel_path}>
                <strong>{c.rel_path}</strong>
                <span>{c.note}</span>
              </li>
            ))}
          </ul>
        </>
      )}
      {r.problems.length > 0 && (
        <ul>
          {r.problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
    </section>
  );
}

function NewSource({ onCreated }: { onCreated: () => void }) {
  const t = useT();
  const action = useAction();
  const [label, setLabel] = useState('');
  const [kind, setKind] = useState<api.IngestSource['kind']>('watched_folder');
  const [backend, setBackend] = useState<api.IngestSource['backend']>('local');
  const [location, setLocation] = useState('');
  const [owner, setOwner] = useState('');
  return (
    <form
      className="rv-stack"
      aria-label={t('review.ingest.add')}
      onSubmit={(e) => {
        e.preventDefault();
        void action
          .run(
            () => api.createIngestSource({ label, kind, backend, location, owner_id: owner }),
            t('review.ingest.saved'),
          )
          .then((s) => {
            if (s) {
              setLabel('');
              setLocation('');
              onCreated();
            }
          });
      }}
    >
      <TextInput
        label={t('review.ingest.label')}
        value={label}
        onChange={(e) => setLabel(e.target.value)}
      />
      <Select
        aria-label={t('review.ingest.kind')}
        options={[
          { value: 'watched_folder', label: t('review.ingest.kindWatched') },
          { value: 'storage_import', label: t('review.ingest.kindImport') },
        ]}
        value={kind}
        onValueChange={(v) => setKind(v as api.IngestSource['kind'])}
      />
      <Select
        aria-label={t('review.ingest.backend')}
        options={[
          { value: 'local', label: t('review.ingest.backendLocal') },
          { value: 'storage', label: t('review.ingest.backendStorage') },
        ]}
        value={backend}
        onValueChange={(v) => setBackend(v as api.IngestSource['backend'])}
      />
      <TextInput
        label={t('review.ingest.location')}
        value={location}
        onChange={(e) => setLocation(e.target.value)}
      />
      <TextInput
        label={t('review.ingest.owner')}
        value={owner}
        onChange={(e) => setOwner(e.target.value)}
      />
      <div className="rv-row">
        <Button
          type="submit"
          loading={action.busy}
          disabled={!label.trim() || !location.trim() || !owner.trim()}
        >
          {t('review.ingest.add')}
        </Button>
      </div>
      <ActionStatus error={action.error} done={action.done} />
    </form>
  );
}

export function IngestSourcesSection() {
  const t = useT();
  const res = useResource('ingest-sources', api.ingestSources);
  const action = useAction();
  const [report, setReport] = useState<api.ScanReport | null>(null);
  const [removing, setRemoving] = useState<api.IngestSource | null>(null);
  const run = async (fn: () => Promise<api.ScanReport>) => {
    const r = await action.run(fn, t('review.ingest.saved'));
    if (r) {
      setReport(r);
      void res.reload();
    }
  };
  return (
    <>
      <h2>{t('review.ingest.title')}</h2>
      <p className="ss-desc">{t('review.ingest.intro')}</p>
      <Async res={res} label={t('review.ingest.loading')}>
        {(d: { sources: api.IngestSource[] }) =>
          d.sources.length === 0 ? (
            <EmptyState
              title={t('review.ingest.empty')}
              description={t('review.ingest.emptyDescription')}
            />
          ) : (
            <div className="rv-table-wrap">
              <table className="rv-table" aria-label={t('review.ingest.table')}>
                <thead>
                  <tr>
                    <th scope="col">{t('review.ingest.label')}</th>
                    <th scope="col">{t('review.ingest.kind')}</th>
                    <th scope="col">{t('review.ingest.location')}</th>
                    <th scope="col">{t('review.ingest.owner')}</th>
                    <th scope="col">{t('review.ingest.enabled')}</th>
                    <th scope="col">{t('review.ingest.lastScan')}</th>
                    <th scope="col" />
                  </tr>
                </thead>
                <tbody>
                  {d.sources.map((s) => (
                    <tr key={s.id}>
                      <th scope="row">{s.label}</th>
                      <td>
                        {s.kind === 'watched_folder'
                          ? t('review.ingest.kindWatched')
                          : t('review.ingest.kindImport')}
                      </td>
                      <td className="adm-mono">{s.location}</td>
                      <td className="adm-mono">{s.owner_id}</td>
                      <td>
                        <input
                          type="checkbox"
                          checked={s.enabled}
                          aria-label={`${t('review.ingest.enabled')}: ${s.label}`}
                          onChange={(e) =>
                            void action
                              .run(
                                () => api.patchIngestSource(s.id, { enabled: e.target.checked }),
                                t('review.ingest.saved'),
                              )
                              .then(() => res.reload())
                          }
                        />
                      </td>
                      <td>
                        {s.last_scan_at_ms
                          ? new Date(s.last_scan_at_ms).toLocaleString()
                          : t('review.ingest.never')}
                        {s.last_error && <div className="rv-muted">{s.last_error}</div>}
                      </td>
                      <td>
                        <div className="rv-row">
                          <Button
                            size="sm"
                            variant="secondary"
                            aria-label={`${t('review.ingest.dryRun')}: ${s.label}`}
                            onClick={() => void run(() => api.dryRunIngestSource(s.id))}
                          >
                            {t('review.ingest.dryRun')}
                          </Button>
                          <Button
                            size="sm"
                            aria-label={`${t('review.ingest.scan')}: ${s.label}`}
                            onClick={() => void run(() => api.scanIngestSource(s.id))}
                          >
                            {t('review.ingest.scan')}
                          </Button>
                          <Button
                            size="sm"
                            variant="danger"
                            aria-label={`${t('review.ingest.remove')}: ${s.label}`}
                            onClick={() => setRemoving(s)}
                          >
                            {t('review.ingest.remove')}
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </Async>
      <ActionStatus error={action.error} done={action.done} />
      {report && <Report r={report} />}
      <NewSource onCreated={() => void res.reload()} />
      <QueryClientProvider client={namingClient}>
        <NamingGuide section="library" />
      </QueryClientProvider>
      <ConfirmDialog
        open={removing !== null}
        title={t('review.ingest.remove')}
        description={t('review.ingest.removeConfirm', { label: removing?.label ?? '' })}
        confirmLabel={t('review.ingest.remove')}
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const s = removing;
          setRemoving(null);
          if (s)
            void action
              .run(() => api.deleteIngestSource(s.id), t('review.ingest.removed'))
              .then(() => res.reload())
              .catch((e) => action.setError(errorText(e, '')));
        }}
      />
    </>
  );
}
