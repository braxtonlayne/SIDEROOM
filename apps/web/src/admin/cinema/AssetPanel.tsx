// Video assets of a title (CIN-ADM-001 asset linking, CIN-ADM-002 transcode status per asset and re-run).
import { Badge, Button, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
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
  const t = detail.title;
  const episodes = [...detail.episodes].sort(
    (a, b) => a.season_number - b.season_number || a.number - b.number,
  );
  return (
    <Section title="Video assets">
      <p className="ss-desc">
        Link the uploaded video a film or episode plays. Assets come from the ingest pipeline; paste
        the asset id here. Unlinking the last playable video unpublishes the title or episode.
      </p>
      {(t.kind === 'movie' || episodes.length === 0) && (
        <AssetRow
          label={t.title}
          assetId={t.asset_id ?? null}
          onLink={async (id) => {
            await api.linkTitleAsset(t.id, id);
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
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const [value, setValue] = useState(assetId ?? '');
  const [open, setOpen] = useState(false);
  const { busy, error, notice, run } = useRun();
  const dirty = value.trim() !== (assetId ?? '');
  return (
    <div className="ss-card adm-cin-asset" role="group" aria-label={`Video for ${label}`}>
      <div className="ss-row">
        <div className="ss-grow">
          <div className="ss-name">{label}</div>
          <div className="ss-meta">
            {assetId ? (
              <>
                Asset <span className="adm-mono">{assetId}</span>
              </>
            ) : (
              <Badge variant="warning">no video linked</Badge>
            )}
          </div>
        </div>
        {assetId && (
          <Button
            size="sm"
            variant="secondary"
            aria-expanded={open}
            aria-label={`${open ? 'Hide' : 'Show'} transcode status for ${label}`}
            onClick={() => setOpen((o) => !o)}
          >
            Transcode status
          </Button>
        )}
      </div>
      {canEdit && (
        <form
          className="adm-actions"
          aria-label={`Link video for ${label}`}
          onSubmit={(e) => {
            e.preventDefault();
            void run(
              () => onLink(value.trim() || null).then(() => true),
              value.trim() ? 'Linked.' : 'Unlinked.',
            );
          }}
        >
          <TextInput
            label={<span className="adm-vh">{`Asset id for ${label}`}</span>}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="Asset id"
          />
          <Button type="submit" size="sm" loading={busy} disabled={!dirty}>
            {value.trim() ? 'Link video' : 'Unlink video'}
          </Button>
        </form>
      )}
      <Banner error={error} notice={notice} />
      {open && assetId && <AssetTranscodeStatus assetId={assetId} />}
    </div>
  );
}

export function NotAvailableYet({ what }: { what: string }) {
  return (
    <Notice variant="info" role="status" title="Not available yet">
      {what} needs the transcoding admin API, which this server does not provide yet. Nothing is
      shown rather than made up.
    </Notice>
  );
}

/** Status and jobs of one asset, with a re-transcode action for MANAGE_TRANSCODE. */
export function AssetTranscodeStatus({ assetId }: { assetId: string }) {
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
        what="transcode status"
      />
    );
  if (unavailable) return <NotAvailableYet what="Transcode status" />;
  if (!res.data) return <p className="ss-loading">Loading status…</p>;
  const a = res.data;
  return (
    <div className="adm-cin-tx" aria-label="Transcode status">
      <dl className="adm-kv">
        <dt>State</dt>
        <dd>
          <StateBadge state={a.state} />
        </dd>
        {a.profile_version != null && (
          <>
            <dt>Profile version</dt>
            <dd>{a.profile_version}</dd>
          </>
        )}
      </dl>
      {a.jobs.length === 0 ? (
        <p className="ss-na">No transcode jobs for this asset.</p>
      ) : (
        <ul className="ss-list" aria-label="Transcode jobs">
          {a.jobs.map((j) => (
            <li key={j.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {j.kind} <StateBadge state={j.state} />
                  {j.percent != null && j.state === 'running' ? ` ${Math.round(j.percent)}%` : ''}
                </div>
                <div className="ss-meta">
                  attempt {j.attempt} · {fmtMs(j.created_at_ms)}
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
          Re-run transcode
        </Button>
      ) : (
        <p className="ss-meta">Re-running a transcode takes the Manage transcode permission.</p>
      )}
      <ReasonDialog
        open={rerun}
        title="Re-run the transcode"
        description="Queues a fresh ladder job from the stored original. Playback keeps using the current renditions until the new ones are accepted."
        confirmLabel="Queue transcode"
        reasonOptional
        onCancel={() => setRerun(false)}
        onConfirm={async (reason) => {
          await tx.retranscodeAsset(assetId, reason);
          setRerun(false);
          setNotice('Transcode queued.');
          await res.reload();
        }}
      />
    </div>
  );
}
