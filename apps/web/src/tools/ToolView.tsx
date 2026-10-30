import { Component, lazy, Suspense, type ComponentType, type ReactNode } from 'react';
import { useT } from '../i18n';
import type { ToolEntry, ToolHost, ToolProps } from './registry';

const cache = new Map<string, ComponentType<ToolProps>>();

function componentFor(entry: ToolEntry): ComponentType<ToolProps> {
  let c = cache.get(entry.id);
  if (!c) {
    c = lazy(entry.load);
    cache.set(entry.id, c);
  }
  return c;
}

/** A tool that fails to load or to render must not take the drawer or the page with it. */
class ToolBoundary extends Component<
  { children: ReactNode; message: string },
  { failed: boolean }
> {
  override state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  override render() {
    return this.state.failed ? (
      <p role="alert" className="sr-tl-muted">
        {this.props.message}
      </p>
    ) : (
      this.props.children
    );
  }
}

/** Renders one tool in the given host. The same component serves the drawer, a gadget and the rail panel. */
export function ToolView({
  entry,
  host,
  onClose,
}: {
  entry: ToolEntry;
  host: ToolHost;
  onClose?: () => void;
}) {
  const t = useT();
  const C = componentFor(entry);
  return (
    <ToolBoundary key={entry.id} message={t('tools.error.load')}>
      <Suspense fallback={<p className="sr-tl-muted">{t('tools.loading')}</p>}>
        <C host={host} {...(onClose ? { onClose } : {})} />
      </Suspense>
    </ToolBoundary>
  );
}
