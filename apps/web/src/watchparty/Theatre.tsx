// The theatre (WP-FE-002): the shared player on the left, Chat / Queue / People on the right, the control bar
// below the picture. The <video> belongs to the Cinema player session; the theatre only attaches it to its stage
// and hands it to the mini player when the viewer leaves, so playback and the voice call carry on.
import { Badge, Button, Icon, Modal, Tabs } from '@sideroom/ui';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { playerSession } from '../cinema/player/session';
import { httpPartyApi } from './api';
import { ChatPanel } from './ChatPanel';
import { ControlBar } from './ControlBar';
import { GestureButton, ReactionsOverlay, ReadyCheck, WaitingBanner } from './Overlays';
import { PeoplePanel } from './PeoplePanel';
import { QueuePanel } from './QueuePanel';
import { partyController, useParty } from './runtime';
import { ShareDialog } from './ShareDialog';
import { clock } from './sync';
import './watchparty.css';

export function Theatre({ onExit }: { onExit: () => void }) {
  const s = playerSession();
  const c = partyController();
  const rootRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const party = useParty((x) => x.party);
  const notice = useParty((x) => x.notice);
  const drift = useParty((x) => x.driftMs);
  const [tab, setTab] = useState('chat');
  const [share, setShare] = useState(false);
  const [confirmEnd, setConfirmEnd] = useState(false);

  // The session owns the element: attach on mount, hand it to the mini player on the way out.
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    s.attach(stage);
    s.setMode('full');
    return () => {
      s.detach(stage);
      if (c.state.status === 'active') s.setMode('mini');
    };
  }, [s, c]);

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => c.clearNotice(), 4000);
    return () => clearTimeout(t);
  }, [notice, c]);

  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled;
  const fullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else
      void (rootRef.current?.closest('[data-theme]') as HTMLElement | null)?.requestFullscreen?.();
  }, []);

  if (!party) return null;
  const you = party.you;
  const media = party.media;
  const leave = async () => {
    await c.close({ leave: true });
    s.close();
    onExit();
  };
  const end = async () => {
    await httpPartyApi.end(party.id).catch(() => undefined);
    await c.close();
    s.close();
    onExit();
  };

  return (
    <div ref={rootRef} className="wp-root">
      <header className="wp-top">
        <Button
          variant="ghost"
          iconOnly
          aria-label="Leave the theatre (the party keeps playing in the mini player)"
          onClick={onExit}
        >
          <Icon name="close" size={20} />
        </Button>
        <div className="wp-titleblock">
          <h1 className="wp-title">{media.title}</h1>
          {media.episode_label && <p className="wp-sub">{media.episode_label}</p>}
        </div>
        <Badge variant="accent">{party.participants.length} watching</Badge>
        {media.age_restricted && <Badge variant="danger">18+</Badge>}
        <span className="wp-spacer" />
        <Button size="sm" variant="secondary" onClick={() => setShare(true)}>
          Invite
        </Button>
        {you?.can_manage && (
          <Button size="sm" variant="danger" onClick={() => setConfirmEnd(true)}>
            End party
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={() => void leave()}>
          Leave
        </Button>
      </header>

      <div className="wp-body">
        <section className="wp-stage-wrap" aria-label="Shared player">
          <div ref={stageRef} className="wp-stage" />
          <ReactionsOverlay />
          <WaitingBanner />
          <GestureButton />
          <ReadyCheck />
          {notice && (
            <p className="wp-notice" role="status">
              {notice}
            </p>
          )}
          <ControlBar
            onFullscreen={fullscreen}
            canFullscreen={canFullscreen}
            onShare={() => setShare(true)}
          />
          <p className="wp-sr-only" role="status">
            {party.playback.playing ? 'Playing' : party.playback.auto_paused ? 'Waiting' : 'Paused'}
            {` at ${clock(party.playback.expected_position_ms)}`}
          </p>
          <p className="wp-drift" aria-hidden="true" data-testid="drift">
            {Math.abs(drift) >= 250 ? `${drift > 0 ? '+' : ''}${drift} ms` : ''}
          </p>
        </section>

        <aside className="wp-panel" aria-label="Party panel">
          <Tabs
            label="Party panel"
            value={tab}
            onValueChange={setTab}
            items={[
              { id: 'chat', label: 'Chat', content: <ChatPanel /> },
              {
                id: 'queue',
                label: party.queue.length ? `Queue (${party.queue.length})` : 'Queue',
                content: <QueuePanel />,
              },
              { id: 'people', label: 'People', content: <PeoplePanel /> },
            ]}
          />
        </aside>
      </div>

      <ShareDialog open={share} onClose={() => setShare(false)} party={party} />
      <Modal
        open={confirmEnd}
        onClose={() => setConfirmEnd(false)}
        title="End the party?"
        description="Everyone is taken out of the party."
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmEnd(false)}>
              Keep watching
            </Button>
            <Button variant="danger" onClick={() => void end()}>
              End party
            </Button>
          </>
        }
      />
    </div>
  );
}
