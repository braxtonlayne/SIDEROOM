// Programme detail (TV-UI-006): synopsis, next airings, Tune in (when on now), Remind me (when upcoming) and a
// link to the Cinema title page. A floating panel anchored to the cell on desktop, a modal on a phone.
import { Modal, useFloating, usePortalTarget } from '@sideroom/ui';
import { useMutation } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../../i18n';
import { AgeBadge } from '../ui';
import { isNotAvailable, setReminder, useBlockInfo } from './api';
import {
  formatDuration,
  isNow,
  kindYear,
  runtimeMinutes,
  timeLabel,
  titleHrefOf,
  watchChannelHref,
} from './model';
import type { TvBlock, TvChannel } from './types';

export interface ProgramPopoverProps {
  block: TvBlock;
  channel: TvChannel;
  /** The cell that opened it (desktop). Null renders the modal form. */
  anchor: HTMLElement | null;
  nowMs: number;
  tz: string | null;
  /** All blocks of the guide, to list the next airings of the same programme. */
  allBlocks: Record<string, TvBlock[]>;
  channels: TvChannel[];
  onClose: () => void;
}

/** Later airings of the same title (by title id, else by name) on any channel. */
export function nextAirings(
  block: TvBlock,
  allBlocks: Record<string, TvBlock[]>,
  nowMs: number,
  limit = 4,
): TvBlock[] {
  const out: TvBlock[] = [];
  for (const list of Object.values(allBlocks))
    for (const b of list) {
      if (b.id === block.id || b.end_ms <= nowMs || b.kind === 'filler') continue;
      const same = block.title_id ? b.title_id === block.title_id : b.title === block.title;
      if (same) out.push(b);
    }
  return out.sort((a, b) => a.start_ms - b.start_ms).slice(0, limit);
}

function Body({
  block: scheduled,
  channel,
  nowMs,
  tz,
  allBlocks,
  channels,
  titleId,
}: Omit<ProgramPopoverProps, 'anchor' | 'onClose'> & { titleId: string }): ReactNode {
  const t = useT();
  // Synopsis and rating come from the catalog (the schedule block carries neither).
  const block = useBlockInfo(scheduled) ?? scheduled;
  const live = isNow(block, nowMs);
  const future = block.start_ms > nowMs;
  const href = titleHrefOf(block);
  const airings = useMemo(() => nextAirings(block, allBlocks, nowMs), [block, allBlocks, nowMs]);
  const byId = useMemo(() => new Map(channels.map((c) => [c.id, c])), [channels]);
  const [reminded, setReminded] = useState(false);
  const remind = useMutation({
    mutationFn: () => setReminder(block.id, !reminded),
    onSuccess: () => setReminded((v) => !v),
  });
  const min = block.runtime_min ?? runtimeMinutes(block);
  return (
    <>
      <p className="tv-pop-kicker">
        {t('cinema.popover.channel', { number: channel.number, callSign: channel.call_sign })}
      </p>
      <h2 id={titleId} className="tv-pop-title">
        {block.title}
      </h2>
      {block.episode_label && <p className="tv-pop-ep">{block.episode_label}</p>}
      <p className="tv-pop-meta">
        <span className="tv-mono">
          {live ? t('cinema.popover.nowPrefix') : ''}
          {t('cinema.guide.timeRange', {
            start: timeLabel(block.start_ms, tz),
            end: timeLabel(block.end_ms, tz),
          })}
        </span>
        <span>
          {kindYear(block)} · {formatDuration(min)}
        </span>
        {block.content_rating && <span className="tv-rating">{block.content_rating}</span>}
        {block.age_restricted && <AgeBadge className="tv-age" />}
      </p>
      <p className="tv-pop-syn">{block.synopsis || t('cinema.popover.noDescription')}</p>
      {airings.length > 0 && (
        <section aria-label={t('cinema.popover.nextAirings')}>
          <h3 className="tv-pop-sub">{t('cinema.popover.nextAirings')}</h3>
          <ul className="tv-pop-airings">
            {airings.map((b) => (
              <li key={b.id}>
                <span className="tv-mono">
                  {new Intl.DateTimeFormat(undefined, { weekday: 'short' }).format(b.start_ms)}{' '}
                  {timeLabel(b.start_ms, tz)}
                </span>
                <span>
                  {byId.get(b.channel_id)
                    ? t('cinema.popover.channelN', { n: byId.get(b.channel_id)!.number })
                    : t('cinema.popover.anotherChannel')}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
      <div className="tv-pop-actions">
        {live && (
          <a className="tv-btn tv-btn-primary" href={watchChannelHref(channel.id)}>
            {t('cinema.popover.tuneIn')}
          </a>
        )}
        {future && (
          <button
            type="button"
            className="tv-btn"
            aria-pressed={reminded}
            disabled={remind.isPending || (remind.isError && isNotAvailable(remind.error))}
            onClick={() => remind.mutate()}
          >
            {reminded ? t('cinema.popover.reminderSet') : t('cinema.popover.remindMe')}
          </button>
        )}
        {href && (
          <a className="tv-btn" href={href}>
            {t('cinema.popover.goToTitle')}
          </a>
        )}
      </div>
      {remind.isError && (
        <p role="alert" className="tv-pop-err">
          {isNotAvailable(remind.error)
            ? t('cinema.popover.remindersUnavailable')
            : t('cinema.popover.reminderError')}
        </p>
      )}
    </>
  );
}

export function ProgramPopover(props: ProgramPopoverProps) {
  const { anchor, onClose } = props;
  const portal = usePortalTarget();
  const panel = useRef<HTMLDivElement | null>(null);
  const floating = useFloating({ open: !!anchor, anchor, placement: 'bottom-start', offset: 6 });
  const titleId = `tv-pop-${props.block.id}`;

  useEffect(() => {
    if (!anchor) return;
    panel.current?.focus();
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!panel.current?.contains(t) && !anchor.contains(t)) onClose();
    };
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
        anchor.focus();
      }
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [anchor, onClose]);

  if (!anchor)
    return (
      <Modal open onClose={onClose} title={props.block.title} size="sm">
        <div className="tv-pop-modal">
          <Body {...props} titleId={`${titleId}-m`} />
        </div>
      </Modal>
    );
  if (!portal) return null;
  return createPortal(
    <div
      ref={(el) => {
        panel.current = el;
        floating.ref(el);
      }}
      role="dialog"
      aria-labelledby={titleId}
      tabIndex={-1}
      className="tv-pop"
      style={floating.style}
    >
      <Body {...props} titleId={titleId} />
    </div>,
    portal,
  );
}
