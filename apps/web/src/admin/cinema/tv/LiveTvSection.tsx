// Admin > Cinema > Live TV (TV-ADM-001..005): channels, schedule rules, generated schedule with publish and
// simulate-now, manual override ("now control") and the what-aired report. Needs MANAGE_TV (Media Curator and up).
// Routes: docs/specs/cinema-live-tv.md section 8.5 (see tvApi.ts); a server without them gets "not available yet".
import { Badge, Button, ConfirmDialog, EmptyState, Notice, Tabs } from '@sideroom/ui';
import { useState } from 'react';
import { SectionHeader, useAdminViewer } from '../../ui';
import { Banner, LoadProblem, useLoad, useRun } from '../hooks';
import { ChannelEditor } from './ChannelEditor';
import { AiredReport, NowControl, SchedulePanel } from './ChannelPanels';
import * as tv from './tvApi';

const KIND_LABEL: Record<tv.AdminKind, string> = {
  rule_based: 'rule based',
  manual: 'manual',
  imported_guide: 'imported guide',
};

export function LiveTvSection() {
  const allowed = useAdminViewer().can('MANAGE_TV');
  const [open, setOpen] = useState<string | 'new' | null>(() =>
    typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('new') === '1'
      ? 'new'
      : null,
  );
  if (!allowed)
    return (
      <>
        <SectionHeader title="Live TV" />
        <EmptyState
          title="You do not have access"
          description="Live TV channels take the Manage TV permission (Media Curator)."
        />
      </>
    );
  if (open)
    return <ChannelDetail id={open === 'new' ? null : open} onClose={() => setOpen(null)} />;
  return <ChannelList onOpen={setOpen} />;
}

function ChannelList({ onOpen }: { onOpen: (id: string | 'new') => void }) {
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
        title="Live TV"
        description="Always-on channels built from the library. Each follows a wall-clock schedule, so everyone tuned in sees the same programme."
        actions={
          <Button disabled={unavailable} onClick={() => onOpen('new')}>
            New channel
          </Button>
        }
      />
      {unavailable && (
        <Notice variant="info" role="status" title="Not available yet">
          This server does not provide the Live TV admin API yet. Nothing is shown rather than made
          up.
        </Notice>
      )}
      {res.error && !res.data && !unavailable && (
        <LoadProblem
          error={res.error}
          status={res.status}
          onRetry={() => void res.reload()}
          what="Live TV"
        />
      )}
      {!res.data && !res.error && !unavailable && <p className="ss-loading">Loading channels…</p>}
      {res.data && res.data.items.length === 0 && (
        <EmptyState title="No channels" description="Create the first channel to fill the guide." />
      )}
      {res.data && res.data.items.length > 0 && (
        <ul className="ss-list" aria-label="Channels">
          {[...res.data.items]
            .sort((a, b) => a.number - b.number)
            .map((c) => (
              <li key={c.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    <span className="adm-mono">{c.number}</span> {c.name}{' '}
                    <Badge>{KIND_LABEL[c.kind]}</Badge>{' '}
                    {!c.enabled && <Badge variant="warning">disabled</Badge>}{' '}
                    {c.age_restricted && <Badge variant="danger">18+</Badge>}
                  </div>
                  <div className="ss-meta">
                    {c.call_sign} · {c.timezone}
                    {c.era_label ? ` · ${c.era_label}` : ''}
                  </div>
                </div>
                <Button size="sm" variant="secondary" onClick={() => onOpen(c.id)}>
                  Open {c.name}
                </Button>
              </li>
            ))}
        </ul>
      )}
    </>
  );
}

function ChannelDetail({ id, onClose }: { id: string | null; onClose: () => void }) {
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
        title={id || created ? (channel?.name ?? 'Channel') : 'New channel'}
        description={channel ? `Channel ${channel.number} · ${channel.call_sign}` : undefined}
        actions={
          <>
            <Button variant="secondary" onClick={onClose}>
              Back to channels
            </Button>
            {channel && (
              <Button variant="danger" onClick={() => setRemove(true)}>
                Delete channel
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
          what="this channel"
        />
      )}
      {id && !channel && !res.error && <p className="ss-loading">Loading the channel…</p>}
      {id && res.data && !channel && (
        <EmptyState title="Channel not found" description="It may have been deleted." />
      )}
      {(!id || channel) && (
        <Tabs
          label="Channel"
          items={[
            {
              id: 'details',
              label: 'Details and rules',
              content: <ChannelEditor key={id ?? 'new'} channel={channel} onSaved={saved} />,
            },
            ...(channel
              ? [
                  {
                    id: 'schedule',
                    label: 'Schedule',
                    content: <SchedulePanel channel={channel} />,
                  },
                  { id: 'now', label: 'Now control', content: <NowControl channel={channel} /> },
                  { id: 'aired', label: 'What aired', content: <AiredReport channel={channel} /> },
                ]
              : []),
          ]}
        />
      )}
      <ConfirmDialog
        open={remove}
        danger
        title={`Delete ${channel?.name ?? 'channel'}`}
        description="Viewers watching it lose the signal. Its schedule and aired history are removed. Type the channel name to confirm."
        confirmLabel="Delete channel"
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
