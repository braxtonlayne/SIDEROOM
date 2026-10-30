import { Icon, Modal } from '@sideroom/ui';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { useRovingTabindex } from '../../a11y/useRovingTabindex';
import { useT } from '../../i18n';
import { GotoList, useGotoResults } from '../../lounge/QuickSwitcher';
import { TOOLS, getTool } from '../registry';
import { MAX_GADGETS } from '../gadgets/layout';
import { useToolsStore } from '../gadgets/store';
import { ToolView } from '../ToolView';
import { closeDrawer, useDrawerRequest, type DrawerTab } from './openState';
import { searchTools } from './toolSearch';

/**
 * UTL-SHL-001: the utility drawer. Mod+K opens it on the "Go to" tab (the former quick switcher, unchanged); `>`
 * as the first character, or the "Tools" tab, lists the tools. A tool opens inside the drawer, where it can be
 * popped out into a floating gadget or pinned to the side rail. Escape closes the drawer only: gadgets and pins
 * are not part of it and stay.
 */
export function Drawer() {
  const t = useT();
  const req = useDrawerRequest();
  const listId = useId();
  const tabsRef = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<DrawerTab>('goto');
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const [viewId, setViewId] = useState<string | null>(null);

  const layout = useToolsStore((s) => s.layout);
  const limitHit = useToolsStore((s) => s.limitHit);
  const { popOut, pin, unpin, clearLimit } = useToolsStore.getState();

  useRovingTabindex(tabsRef, { selector: '[role="tab"]', orientation: 'horizontal' });

  useEffect(() => {
    if (!req.open) return;
    setTab(req.tab);
    setQuery('');
    setActive(0);
    setViewId(null);
    clearLimit();
  }, [req.open, req.nonce, req.tab, clearLimit]);

  const goto = useGotoResults(query, closeDrawer);
  const toolRows = useMemo(
    () =>
      searchTools(
        TOOLS.map((entry) => ({
          entry,
          name: t(entry.nameKey),
          description: t(entry.descriptionKey),
        })),
        query,
      ),
    [query, t],
  );
  const count = tab === 'goto' ? goto.results.length : toolRows.length;
  const clamped = Math.min(active, Math.max(0, count - 1));
  const view = viewId ? getTool(viewId) : undefined;

  const onChange = (value: string) => {
    setViewId(null);
    setActive(0);
    if (tab === 'goto' && value.startsWith('>')) {
      // `>` switches to the tools: the rest of the line is the tool query.
      setTab('tools');
      setQuery(value.slice(1).trimStart());
      return;
    }
    setQuery(value);
  };
  const openTool = (id: string) => {
    setViewId(id);
    clearLimit();
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (view) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActive((clamped + 1) % Math.max(1, count));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((clamped - 1 + count) % Math.max(1, count));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (tab === 'goto') goto.choose(goto.results[clamped]);
      else {
        const row = toolRows[clamped];
        if (!row) return;
        // Shift+Enter pops the tool straight out.
        if (e.shiftKey && row.entry.floatable !== false) {
          if (popOut(row.entry.id)) closeDrawer();
        } else openTool(row.entry.id);
      }
    }
  };
  const selectTab = (next: DrawerTab) => {
    setTab(next);
    setActive(0);
    setViewId(null);
  };

  const pinned = view ? layout.pins.includes(view.id) : false;
  const floating = view ? layout.gadgets.some((g) => g.tool === view.id) : false;

  return (
    <Modal
      open={req.open}
      onClose={closeDrawer}
      title={t('tools.drawer.title')}
      size="md"
      className="sr-tl-drawer"
    >
      <div ref={tabsRef} role="tablist" aria-label={t('tools.drawer.tabs')} className="sr-tl-tabs">
        {(['goto', 'tools'] as const).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`${listId}-tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`${listId}-panel`}
            className="sr-tl-tab"
            onClick={() => selectTab(id)}
          >
            {id === 'goto' ? t('tools.drawer.tabGoto') : t('tools.drawer.tabTools')}
          </button>
        ))}
      </div>
      <input
        data-autofocus
        className="lng-switcher-input sr-tl-input"
        role="combobox"
        aria-expanded={!view}
        aria-controls={listId}
        aria-activedescendant={!view && count > 0 ? `${listId}-${clamped}` : undefined}
        aria-label={
          tab === 'goto' ? t('lounge.quickSwitcher.searchLabel') : t('tools.drawer.searchTools')
        }
        placeholder={
          tab === 'goto' ? t('tools.drawer.placeholderGoto') : t('tools.drawer.placeholderTools')
        }
        value={query}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
      />
      <div
        role="tabpanel"
        id={`${listId}-panel`}
        aria-labelledby={`${listId}-tab-${tab}`}
        className="sr-tl-panel"
      >
        {view ? (
          <section className="sr-tl-view" aria-label={t(view.nameKey)}>
            <header className="sr-tl-view-head">
              <button type="button" className="sr-tl-btn" onClick={() => setViewId(null)}>
                {t('tools.drawer.back')}
              </button>
              <h3 className="sr-tl-view-title">{t(view.nameKey)}</h3>
              {view.floatable !== false && (
                <>
                  <button
                    type="button"
                    className="sr-tl-btn"
                    disabled={!floating && layout.gadgets.length >= MAX_GADGETS}
                    onClick={() => {
                      if (popOut(view.id)) closeDrawer();
                    }}
                  >
                    <Icon name="maximize" size={16} />
                    {t('tools.drawer.popOut')}
                  </button>
                  <button
                    type="button"
                    className="sr-tl-btn"
                    aria-pressed={pinned}
                    onClick={() => (pinned ? unpin(view.id) : pin(view.id))}
                  >
                    <Icon name="pin" size={16} />
                    {pinned ? t('tools.drawer.unpin') : t('tools.drawer.pin')}
                  </button>
                </>
              )}
            </header>
            {limitHit && (
              <p role="status" className="sr-tl-muted">
                {t('tools.drawer.limit', { max: MAX_GADGETS })}
              </p>
            )}
            <div className="sr-tl-view-body">
              <ToolView entry={view} host="drawer" onClose={() => setViewId(null)} />
            </div>
          </section>
        ) : tab === 'goto' ? (
          <>
            <GotoList
              id={listId}
              results={goto.results}
              active={clamped}
              onActive={setActive}
              onChoose={goto.choose}
            />
            <p className="lng-switcher-tip">
              {goto.hasPrefix || query
                ? t('lounge.quickSwitcher.tipActive')
                : t('tools.drawer.tipGoto')}
            </p>
          </>
        ) : (
          <>
            <ul
              id={listId}
              role="listbox"
              aria-label={t('tools.drawer.toolResults')}
              className="lng-switcher-list"
            >
              {toolRows.length === 0 && (
                <li className="sr-tl-muted sr-tl-pad" role="presentation">
                  {t('tools.drawer.noTools')}
                </li>
              )}
              {toolRows.map((row, i) => (
                <li
                  key={row.entry.id}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === clamped}
                  className="lng-switcher-item"
                  data-active={i === clamped || undefined}
                  data-tool={row.entry.id}
                  onMouseMove={() => setActive(i)}
                  onClick={() => openTool(row.entry.id)}
                >
                  <Icon name={row.entry.icon} size={20} />
                  <span className="lng-switcher-label">{row.name}</span>
                  <span className="sr-tl-muted">{row.description}</span>
                </li>
              ))}
            </ul>
            <p className="lng-switcher-tip">{t('tools.drawer.tipTools')}</p>
          </>
        )}
      </div>
    </Modal>
  );
}
