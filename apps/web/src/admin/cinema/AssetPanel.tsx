// Video assets of a title (CIN-ADM-001 asset linking, CIN-ADM-002 transcode status per asset and re-run).
import { Badge, Button, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { StateBadge, fmtMs, useAdminViewer } from '../ui';
import * as api from './api';
import { Banner, LoadProblem, Section, useLoad, useRun } from './hooks';
import { ReasonDialog } from '../ui';
import * as tx from './transcodeApi';

interface Props {
  detail: api.AdminTitleDetail;
  reload: () => Promise<void>;
}

export function AssetPanel({ detail, reload }: Props) {
  const t = useT();
  const ttl = detail.title;
  const episodes = [...detail.episodes].sort(
    (a, b) => a.season_number - b.season_number || a.number - b.number,
  );
  return (
    <Section title={t('admin.assetPanel.title')}>
      <p className="ss-desc">{t('admin.assetPanel.intro')}</p>
      {(ttl.kind === 'movie' || episodes.length === 0) && (
        <AssetRow
          label={ttl.title}
          assetId={ttl.asset_id ?? null}
          onLink={async (id) => {
            await api.linkTitleAsset(ttl.id, id);
            await reload();
          }}
        />
      )}
      {episodes.map((e) => (
        <AssetRow
          key={e.id}
          label={`S${e.season_number} E${e.number}${e.title ? ` · ${e.title}` : ''}`}
          assetId={e.asset_id ?? null}
          onLink={async (id) => {
            await api.linkEpisodeAsset(e.id, id);
            await reload();
          }}
        />
      ))}
    </Section>
  );
}

function AssetRow({
  label,
  assetId,
  onLink,
}: {
  label: string;
  assetId: string | null;
  onLink: (id: string | null) => Promise<void>;
}) {
  const t = useT();
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const [value, setValue] = useState(assetId ?? '');
  const [open, setOpen] = useState(false);
  const { busy, error, notice, run } = useRun();
  const dirty = value.trim() !== (assetId ?? '');
  return (
    <div
      className="ss-card adm-cin-asset"
      role="group"
      aria-label={t('admin.assetPanel.videoFor', { label })}
    >
      <div className="ss-row">
        <div className="ss-grow">
          <div className="ss-name">{label}</div>
          <div className="ss-meta">
            {assetId ? (
              <>
                {t('admin.assetPanel.asset')} <span className="adm-mono">{assetId}</span>
              </>
            ) : (
              <Badge variant="warning">{t('admin.assetPanel.noVideo')}</Badge>
            )}
          </div>
        </div>
        {assetId && (
          <Button
            size="sm"
            variant="secondary"
            aria-expanded={open}
            aria-label={
              open
                ? t('admin.assetPanel.hideStatus', { label })
                : t('admin.assetPanel.showStatus', { label })
            }
            onClick={() => setOpen((o) => !o)}
          >
            {t('admin.assetPanel.transcodeStatus')}
          </Button>
        )}
      </div>
      {canEdit && (
        <form
          className="adm-actions"
          aria-label={t('admin.assetPanel.linkFor', { label })}
          onSubmit={(e) => {
            e.preventDefault();
            void run(
              () => onLink(value.trim() || null).then(() => true),
              value.trim() ? t('admin.assetPanel.linked') : t('admin.assetPanel.unlinked'),
            );
          }}
        >
          <TextInput
            label={<span className="adm-vh">{t('admin.assetPanel.assetIdFor', { label })}</span>}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={t('admin.assetPanel.assetId')}
          />
          <Button type="submit" size="sm" loading={busy} disabled={!dirty}>
            {value.trim() ? t('admin.assetPanel.linkVideo') : t('admin.assetPanel.unlinkVideo')}
          </Button>
        </form>
      )}
      <Banner error={error} notice={notice} />
      {open && assetId && <AssetTranscodeStatus assetId={assetId} />}
    </div>
  );
}

export function NotAvailableYet({ what }: { what: string }) {
  const t = useT();
  return (
    <Notice variant="info" role="status" title={t('admin.assetPanel.notAvailable.title')}>
      {t('admin.assetPanel.notAvailable.body', { what })}
    </Notice>
  );
}

/** Status and jobs of one asset, with a re-transcode action for MANAGE_TRANSCODE. */
export function AssetTranscodeStatus({ assetId }: { assetId: string }) {
  const t = useT();
  const canRun = useAdminViewer().can('MANAGE_TRANSCODE');
  const [unavailable, setUnavailable] = useState(false);
  const res = useLoad<tx.AssetTranscode | null>(`asset-tx:${assetId}`, async () => {
    try {
      return await tx.getAssetTranscode(assetId);
    } catch (e) {
      if (tx.isUnavailable(e)) {
        setUnavailable(true);
        return null;
      }
      throw e;
    }
  });
  const [rerun, setRerun] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.assetPanel.whatStatus')}
      />
    );
  if (unavailable) return <NotAvailableYet what={t('admin.assetPanel.transcodeStatus')} />;
  if (!res.data) return <p className="ss-loading">{t('admin.assetPanel.loadingStatus')}</p>;
  const a = res.data;
  return (
    <div className="adm-cin-tx" aria-label={t('admin.assetPanel.transcodeStatus')}>
      <dl className="adm-kv">
        <dt>{t('admin.assetPanel.state')}</dt>
        <dd>
          <StateBadge state={a.state} />
        </dd>
        {a.profile_version != null && (
          <>
            <dt>{t('admin.assetPanel.profileVersion')}</dt>
            <dd>{a.profile_version}</dd>
          </>
        )}
      </dl>
      {a.jobs.length === 0 ? (
        <p className="ss-na">{t('admin.assetPanel.noJobs')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.assetPanel.jobs')}>
          {a.jobs.map((j) => (
            <li key={j.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {j.kind} <StateBadge state={j.state} />
                  {j.percent != null && j.state === 'running' ? ` ${Math.round(j.percent)}%` : ''}
                </div>
                <div className="ss-meta">
                  {t('admin.assetPanel.attempt', { n: j.attempt })} · {fmtMs(j.created_at_ms)}
                  {j.error ? ` · ${j.error}` : ''}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      {canRun ? (
        <Button variant="secondary" onClick={() => setRerun(true)}>
          {t('admin.assetPanel.rerun')}
        </Button>
      ) : (
        <p className="ss-meta">{t('admin.assetPanel.rerunNeedsPerm')}</p>
      )}
      <ReasonDialog
        open={rerun}
        title={t('admin.assetPanel.rerunDialog.title')}
        description={t('admin.assetPanel.rerunDialog.description')}
        confirmLabel={t('admin.assetPanel.rerunDialog.confirm')}
        reasonOptional
        onCancel={() => setRerun(false)}
        onConfirm={async (reason) => {
          await tx.retranscodeAsset(assetId, reason);
          setRerun(false);
          setNotice(t('admin.assetPanel.queued'));
          await res.reload();
        }}
      />
    </div>
  );
}
