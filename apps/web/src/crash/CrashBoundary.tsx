import { Component, type ReactNode } from 'react';
import { useT } from '../i18n';
import { reportRenderError } from './install';

function Fallback() {
  const t = useT();
  return (
    <div role="alert" style={{ padding: '2rem', maxInlineSize: '40rem', marginInline: 'auto' }}>
      <h1>{t('crash.boundary.title')}</h1>
      <p>{t('crash.boundary.body')}</p>
      <button type="button" onClick={() => window.location.reload()}>
        {t('crash.boundary.reload')}
      </button>
    </div>
  );
}

interface State {
  failed: boolean;
}

/**
 * The last React error boundary (OPS-CRASH-001): a render failure anywhere under it is reported as a crash card
 * (allow-listed frames only, never the message) and replaced by a plain page with a reload button.
 */
export class CrashBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: unknown): void {
    void reportRenderError(error);
  }

  render(): ReactNode {
    return this.state.failed ? <Fallback /> : this.props.children;
  }
}
