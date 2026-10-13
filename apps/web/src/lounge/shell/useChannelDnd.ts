import { useRef, useState, type DragEvent } from 'react';
import type { Channel } from '../data';
import { applyChannelMoves } from './channelActions';
import { planDrop } from './channelMoves';

export type ChannelDropMark = { id: string; zone: 'before' | 'after' | 'into' } | null;

/** Drag and drop reordering and re-parenting for the channel list; only for people who manage channels. */
export function useChannelDnd(
  guildId: string,
  channels: Channel[],
  enabled: boolean,
  onError: (message: string) => void,
) {
  const drag = useRef<string | null>(null);
  const [mark, setMark] = useState<ChannelDropMark>(null);
  const [dragging, setDragging] = useState<string | null>(null);

  const props = (id: string) => {
    if (!enabled) return {};
    const target = channels.find((c) => c.id === id);
    const zoneOf = (e: DragEvent): 'before' | 'after' | 'into' => {
      const r = e.currentTarget.getBoundingClientRect();
      const ratio = r.height ? (e.clientY - r.top) / r.height : 0.5;
      return target?.type === 'category' && ratio > 0.25 && ratio < 0.75
        ? 'into'
        : ratio < 0.5
          ? 'before'
          : 'after';
    };
    return {
      draggable: true,
      onDragStart: (e: DragEvent) => {
        e.stopPropagation();
        drag.current = id;
        setDragging(id);
        e.dataTransfer.effectAllowed = 'move';
        try {
          e.dataTransfer.setData('application/x-sideroom-channel', id);
        } catch {
          /* the ref carries the state */
        }
      },
      onDragEnd: () => {
        drag.current = null;
        setDragging(null);
        setMark(null);
      },
      onDragOver: (e: DragEvent) => {
        const d = drag.current;
        if (!d || d === id) return;
        const zone = zoneOf(e);
        if (!planDrop(channels, d, id, zone)) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = 'move';
        setMark({ id, zone });
      },
      onDragLeave: () => setMark((m) => (m?.id === id ? null : m)),
      onDrop: (e: DragEvent) => {
        const d = drag.current;
        drag.current = null;
        setDragging(null);
        setMark(null);
        if (!d || d === id) return;
        const moves = planDrop(channels, d, id, zoneOf(e));
        if (!moves) return;
        e.preventDefault();
        e.stopPropagation();
        applyChannelMoves(guildId, moves).catch(() =>
          onError('Could not move that channel. You may not have permission.'),
        );
      },
    };
  };
  return { props, mark, dragging };
}
