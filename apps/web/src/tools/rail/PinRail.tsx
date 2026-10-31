import { Icon } from '@sideroom/ui';
import { useRef, useState } from 'react';
import { useRovingTabindex } from '../../a11y/useRovingTabindex';
import { useT } from '../../i18n';
import { MAX_GADGETS } from '../gadgets/layout';
import { useToolsStore } from '../gadgets/store';
import { useClientFlags } from '../../moderation/reports';
import { toolAllowed } from '../flags';
import { getTool } from '../registry';
import { ToolView } from '../ToolView';

/**
 * UTL-SHL-001: the side rail. Pinned tools get a button on a slim strip at the inline-end edge (one Tab stop,
 * arrow keys move between the buttons). A button opens the tool in a docked panel beside the strip; the panel can
 * pop the tool out into a gadget or unpin it. The rail renders nothing while nothing is pinned.
 */
export function PinRail() {
  const t = useT();
  const pins = useToolsStore((s) => s.layout.pins);
  const flags = useClientFlags((s) => s.flags);
  const gadgets = useToolsStore((s) => s.layout.gadgets);
  const railRef = useRef<HTMLDivElement>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  useRovingTabindex(railRef, { selector: 'button[data-pin]', orientation: 'vertical' });

  const entries = pins.flatMap((id) => {
    const e = getTool(id);
    return e && toolAllowed(flags, e.id) ? [e] : [];
  });
  if (entries.length === 0) return null;
  const open = entries.find((e) => e.id === openId);
  const floating = open ? gadgets.some((g) => g.tool === open.id) : false;

  return (
    <div className="sr-tl-rail-wrap">
      {open && (
        <section className="sr-tl-rail-panel" aria-label={t(open.nameKey)} data-tool={open.id}>
          <header className="sr-tl-gadget-head sr-tl-rail-head">
            <Icon name={open.icon} size={16} />
            <span className="sr-tl-gadget-title">{t(open.nameKey)}</span>
            <button
              type="button"
              className="sr-tl-icon-btn"
              aria-label={t('tools.drawer.popOut')}
              disabled={!floating && gadgets.length >= MAX_GADGETS}
              onClick={() => {
                if (useToolsStore.getState().popOut(open.id)) setOpenId(null);
              }}
            >
              <Icon name="maximize" size={16} />
            </button>
            <button
              type="button"
              className="sr-tl-icon-btn"
              aria-label={t('tools.drawer.unpin')}
              onClick={() => {
                useToolsStore.getState().unpin(open.id);
                setOpenId(null);
              }}
            >
              <Icon name="pin" size={16} />
            </button>
            <button
              type="button"
              className="sr-tl-icon-btn"
              aria-label={t('tools.rail.closePanel')}
              onClick={() => setOpenId(null)}
            >
              <Icon name="close" size={16} />
            </button>
          </header>
          <div className="sr-tl-gadget-body">
            <ToolView entry={open} host="rail" onClose={() => setOpenId(null)} />
          </div>
        </section>
      )}
      <div
        ref={railRef}
        className="sr-tl-rail"
        role="toolbar"
        aria-orientation="vertical"
        aria-label={t('tools.rail.label')}
      >
        {entries.map((e) => (
          <button
            key={e.id}
            type="button"
            data-pin={e.id}
            className="sr-tl-rail-btn"
            aria-label={t(e.nameKey)}
            aria-pressed={openId === e.id}
            title={t(e.nameKey)}
            onClick={() => setOpenId(openId === e.id ? null : e.id)}
          >
            <Icon name={e.icon} size={20} />
          </button>
        ))}
      </div>
    </div>
  );
}
