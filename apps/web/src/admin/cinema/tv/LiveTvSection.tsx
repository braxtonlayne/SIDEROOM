// Admin > Cinema > Live TV (TV-ADM-001..005): channels, schedule rules, generated schedule with publish and
// simulate-now, manual override ("now control") and the what-aired report. Needs MANAGE_TV (Media Curator and up).
// Routes: docs/specs/cinema-live-tv.md section 8.5 (see tvApi.ts); a server without them gets "not available yet".
import { Badge, Button, ConfirmDialog, EmptyState, Notice, Tabs } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { SectionHeader, useAdminViewer } from '../../ui';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import { ChannelEditor } from './ChannelEditor';
import { AiredReport, NowControl, SchedulePanel } from './ChannelPanels';
import { GuidePanel } from './GuidePanel';
import * as tv from './tvApi';

const KIND_LABEL: Record<tv.AdminKind, string> = {
  rule_based: 'admin.liveTv.kind.ruleBased',
  manual: 'admin.liveTv.kind.manual',
  imported_guide: 'admin.liveTv.kind.importedGuide',
};

export function LiveTvSection() {
  const t = useT();
  const allowed = useAdminViewer().can('MANAGE_TV');
  const [open, setOpen] = useState<string | 'new' | null>(() =>
    typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('new') === '1'
      ? 'new'
      : null,
  );
  if (!allowed)
    return (
      <>
        <SectionHeader title={t('admin.liveTv.title')} />
        <EmptyState
          title={t('admin.liveTv.noAccess.title')}
          description={t('admin.liveTv.noAccess.description')}
        />
      </>
    );
  if (open)
    return <ChannelDetail id={open === 'new' ? null : open} onClose={() => setOpen(null)} />;
  return <ChannelList onOpen={setOpen} />;
}

/** A read-only row: is an off-air slate configured, and does it play. Failures just leave the row out. */
function SlateRow() {
  const t = useT();
  const res = useLoad<tv.AdminSlate | null>('tv-slate', async () => {
    try {
      return await tv.getSlate();
    } catch {
      return null;
    }
  });
  const s = res.data;
  if (!s) return null;
  return (
    <section className="ss-meta" aria-label={t('admin.liveTv.slate.title')} data-testid="tv-slate">
      <strong>{t('admin.liveTv.slate.title')}</strong>{' '}
      {!s.configured ? (
        <>
          {t('admin.liveTv.slate.none')} <code>sr-cli tv slate</code>
        </>
      ) : !s.playable ? (
        t('admin.liveTv.slate.notPlayable', { id: s.asset_id ?? '' })
      ) : s.duration_ms ? (
        t('admin.liveTv.slate.set', {
          id: s.asset_id ?? '',
          seconds: Math.round(s.duration_ms / 1000),
        })
      ) : (
        t('admin.liveTv.slate.setNoLength', { id: s.asset_id ?? '' })
      )}
      {s.configured && (
        <>
          {' '}
          {t('admin.liveTv.slate.remove')} <code>sr-cli tv slate --remove</code>{' '}
          {t('admin.liveTv.slate.orLimits')}{' '}
          <Link to="/admin/$section" params={{ section: 'limits' }}>
            {t('admin.liveTv.slate.limitsLink')}
          </Link>
          .
        </>
      )}
    </section>
  );
}

function ChannelList({ onOpen }: { onOpen: (id: string | 'new') => void }) {
  const t = useT();
  const [unavailable, setUnavailable] = useState(false);
  const res = useLoad<{ items: tv.AdminChannel[] } | null>('tv-channels', async () => {
    try {
      return await tv.listChannels();
    } catch (e) {
      if (tv.isUnavailable(e)) {
        setUnavailable(true);
        return null;
      }
      throw e;
    }
  });
  return (
    <>
      <SectionHeader
        title={t('admin.liveTv.title')}
        description={t('admin.liveTv.description')}
        actions={
          <Button disabled={unavailable} onClick={() => onOpen('new')}>
            {t('admin.liveTv.new')}
          </Button>
        }
      />
      {unavailable && (
        <Notice variant="info" role="status" title={t('admin.liveTv.unavailable.title')}>
          {t('admin.liveTv.unavailable.body')}
        </Notice>
      )}
      {res.error && !res.data && !unavailable && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what={t('admin.liveTv.what')}
        />
      )}
      {!res.data && !res.error && !unavailable && (
        <p className="ss-loading">{t('admin.liveTv.loading')}</p>
      )}
      {res.data && res.data.items.length === 0 && (
        <EmptyState
          title={t('admin.liveTv.empty.title')}
          description={t('admin.liveTv.empty.description')}
        />
      )}
      {!unavailable && <SlateRow />}
      {res.data && res.data.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.liveTv.list')}>
          {[...res.data.items]
            .sort((a, b) => a.number - b.number)
            .map((c) => (
              <li key={c.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    <span className="adm-mono">{c.number}</span> {c.name}{' '}
                    <Badge>{t(KIND_LABEL[c.kind])}</Badge>{' '}
                    {!c.enabled && <Badge variant="warning">{t('admin.liveTv.disabled')}</Badge>}{' '}
                    {c.age_restricted && <Badge variant="danger">{t('admin.liveTv.age18')}</Badge>}
                  </div>
                  <div className="ss-meta">
                    {c.call_sign} · {c.timezone}
                    {c.era_label ? ` · ${c.era_label}` : ''}
                  </div>
                </div>
                <Button size="sm" variant="secondary" onClick={() => onOpen(c.id)}>
                  {t('admin.liveTv.open', { name: c.name })}
                </Button>
              </li>
            ))}
        </ul>
      )}
    </>
  );
}

function ChannelDetail({ id, onClose }: { id: string | null; onClose: () => void }) {
  const t = useT();
  const res = useLoad<{ items: tv.AdminChannel[] }>('tv-channels-detail', tv.listChannels);
  const [created, setCreated] = useState<tv.AdminChannel | null>(null);
  const [remove, setRemove] = useState(false);
  const del = useRun();
  const channel = created ?? res.data?.items.find((c) => c.id === id);
  const saved = (c: tv.AdminChannel) => {
    setCreated(c);
    void res.reload();
  };

  return (
    <>
      <SectionHeader
        title={
          id || created
            ? (channel?.name ?? t('admin.liveTv.detail.channel'))
            : t('admin.liveTv.new')
        }
        description={
          channel
            ? t('admin.liveTv.detail.description', {
                n: channel.number,
                callSign: channel.call_sign,
              })
            : undefined
        }
        actions={
          <>
            <Button variant="secondary" onClick={onClose}>
              {t('admin.liveTv.detail.back')}
            </Button>
            {channel && (
              <Button variant="danger" onClick={() => setRemove(true)}>
                {t('admin.liveTv.detail.delete')}
              </Button>
            )}
          </>
        }
      />
      <Banner error={del.error} notice={null} />
      {id && res.error && !res.data && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what={t('admin.liveTv.detail.what')}
        />
      )}
      {id && !channel && !res.error && (
        <p className="ss-loading">{t('admin.liveTv.detail.loading')}</p>
      )}
      {id && res.data && !channel && (
        <EmptyState
          title={t('admin.liveTv.detail.notFound.title')}
          description={t('admin.liveTv.detail.notFound.description')}
        />
      )}
      {(!id || channel) && (
        <Tabs
          label={t('admin.liveTv.detail.tabs')}
          items={[
            {
              id: 'details',
              label: t('admin.liveTv.detail.tab.details'),
              content: <ChannelEditor key={id ?? 'new'} channel={channel} onSaved={saved} />,
            },
            ...(channel?.kind === 'imported_guide'
              ? [
                  {
                    id: 'guide',
                    label: t('admin.liveTv.detail.tab.guide'),
                    content: <GuidePanel channel={channel} />,
                  },
                ]
              : []),
            ...(channel
              ? [
                  {
                    id: 'schedule',
                    label: t('admin.liveTv.detail.tab.schedule'),
                    content: <SchedulePanel channel={channel} />,
                  },
                  {
                    id: 'now',
                    label: t('admin.liveTv.detail.tab.now'),
                    content: <NowControl channel={channel} onKindChanged={saved} />,
                  },
                  {
                    id: 'aired',
                    label: t('admin.liveTv.detail.tab.aired'),
                    content: <AiredReport channel={channel} />,
                  },
                ]
              : []),
          ]}
        />
      )}
      <ConfirmDialog
        open={remove}
        danger
        title={t('admin.liveTv.detail.deleteTitle', {
          name: channel?.name ?? t('admin.liveTv.detail.channelWord'),
        })}
        description={t('admin.liveTv.detail.deleteDescription')}
        confirmLabel={t('admin.liveTv.detail.delete')}
        requireText={channel?.name}
        loading={del.busy}
        onCancel={() => setRemove(false)}
        onConfirm={() => {
          if (!channel) return;
          void del
            .run(async () => {
              await tv.deleteChannel(channel.id);
              return true;
            })
            .then((ok) => {
              setRemove(false);
              if (ok) onClose();
            });
        }}
      />
    </>
  );
}
