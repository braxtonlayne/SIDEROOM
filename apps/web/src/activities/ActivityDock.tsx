// The window of the Activity you are in (J-15, D-248). One iframe, never remounted: full size over the slot
// of the channel view that owns the instance, and a small draggable window with "Return" and "Leave" when the
// person is anywhere else in the Lounge. Mounted once in the Lounge shell.
import { Button, Icon } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useCurrentUser } from '../lounge/data';
import { useLoungeGoto } from '../lounge/nav';
import { onDispatch } from '../store/events';
import { useStore } from '../store/store';
import { httpActivityApi, type ActivityApi } from './api';
import { ActivityFrame } from './ActivityFrame';
import { useActivitySession, type SlotRect } from './session';
import type { ActivityInstance, InstanceUpdateEvent } from './types';
import './activities.css';

const DOCK_W = 320;
const DOCK_H = 180;
const MARGIN = 12;

interface Pos {
  x: number;
  y: number;
}

function clamp(p: Pos, vw: number, vh: number): Pos {
  return {
    x: Math.min(Math.max(0, p.x), Math.max(0, vw - DOCK_W)),
    y: Math.min(Math.max(0, p.y), Math.max(0, vh - DOCK_H - 40)),
  };
}

/** The window itself, without routing or stores, so layout changes can be tested on one element. */
export function ActivityWindow({
  instance,
  channelName,
  rect,
  onReturn,
  onLeave,
}: {
  instance: ActivityInstance;
  channelName: string;
  /** The slot to cover, or `null` to dock. */
  rect: SlotRect | null;
  onReturn: () => void;
  onLeave: () => void;
}) {
  const t = useT();
  const [pos, setPos] = useState<Pos | null>(null);
  const drag = useRef<{ dx: number; dy: number } | null>(null);

  const origin = (): Pos =>
    pos ?? {
      x: window.innerWidth - DOCK_W - MARGIN,
      y: window.innerHeight - DOCK_H - 40 - MARGIN,
    };

  // The same element changes its box between the two modes; the iframe inside never moves in the tree.
  // Viewport coordinates are physical, so this positioning is too (RTL included).
  const style =
    rect === null
      ? {
          left: 0,
          top: 0,
          transform: `translate(${origin().x}px, ${origin().y}px)`,
        }
      : {
          left: rect.left,
          top: rect.top,
          width: rect.width,
          height: rect.height,
        };

  return (
    <div
      className="act-window"
      data-mode={rect === null ? 'docked' : 'full'}
      style={style}
      role="region"
      aria-label={t('activities.window.label', { name: instance.application_name })}
    >
      <div
        className="act-window-bar"
        hidden={rect !== null}
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest('button')) return;
          const o = origin();
          drag.current = { dx: e.clientX - o.x, dy: e.clientY - o.y };
          e.currentTarget.setPointerCapture?.(e.pointerId);
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (d) {
            setPos(
              clamp(
                { x: e.clientX - d.dx, y: e.clientY - d.dy },
                window.innerWidth,
                window.innerHeight,
              ),
            );
          }
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
      >
        <Icon name="sparkle" size={16} />
        <span className="act-window-title">
          <strong>{instance.application_name}</strong>
          <span className="act-muted"> · {channelName}</span>
        </span>
        <Button size="sm" variant="ghost" onClick={onReturn}>
          {t('activities.window.return')}
        </Button>
        <Button size="sm" variant="ghost" onClick={onLeave}>
          {t('activities.window.leave')}
        </Button>
      </div>
      <ActivityFrame instance={instance} channelName={channelName} />
    </div>
  );
}

/** Mounted in the Lounge shell: shows the window of the Activity you joined, wherever you are. */
export function ActivityDock({ api = httpActivityApi }: { api?: ActivityApi }) {
  const me = useCurrentUser();
  const goto = useLoungeGoto();
  const instance = useActivitySession((s) => s.instance);
  const slot = useActivitySession((s) => s.slot);
  const adopt = useActivitySession((s) => s.adopt);
  const clear = useActivitySession((s) => s.clear);
  const channelName = useStore((s) =>
    instance ? (s.channels[instance.channel_id]?.name ?? '') : '',
  );

  // Updates arrive for every channel you can see; this one tracks the instance you are in, from anywhere.
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event !== 'SR_ACTIVITY_INSTANCE_UPDATE') return;
        const e = data as InstanceUpdateEvent;
        adopt(e.channel_id, e.instance, me.id);
      }),
    [adopt, me.id],
  );

  if (!instance) return null;
  const rect = slot && slot.channelId === instance.channel_id ? slot.rect : null;
  return (
    <ActivityWindow
      instance={instance}
      channelName={channelName}
      rect={rect}
      onReturn={() => goto(instance.guild_id, instance.channel_id)}
      onLeave={() => {
        void api.leave(instance.id).catch(() => undefined);
        clear();
      }}
    />
  );
}
