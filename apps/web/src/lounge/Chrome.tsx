import { Link } from '@tanstack/react-router';
import { Icon, Tooltip } from '@sideroom/ui';
import { useState } from 'react';
import { ComingSoon } from './components/ComingSoon';
import { useLoungeUi } from './store';

/** Thin global section bar above everything (spec 1). Collapsible via the store setting. */
export function SectionBar() {
  const show = useLoungeUi((s) => s.showSectionBar);
  const voice = useLoungeUi((s) => s.voice);
  if (!show) return null;
  return (
    <header className="lng-sectionbar" aria-label="Sections">
      <span className="lng-sectionbar-brand">SIDE ROOM</span>
      <span aria-hidden="true">/</span>
      <nav aria-label="Sections" className="lng-sectionbar-nav">
        <Link to="/channels" className="lng-sectionbar-link" aria-current="page">
          The lounge
        </Link>
        <Link to="/cinema" className="lng-sectionbar-link">
          The cinema
        </Link>
        <Link to="/library" className="lng-sectionbar-link">
          The library
        </Link>
      </nav>
      {voice && (
        <span className="lng-sectionbar-voice" role="status">
          <Icon name="volume" size={16} /> Voice connected: {voice.name}
        </span>
      )}
    </header>
  );
}

/** Desktop title strip: history buttons, window title, inbox, help. */
export function TitleStrip({ title }: { title: string }) {
  const showSectionBar = useLoungeUi((s) => s.showSectionBar);
  const setShow = useLoungeUi((s) => s.setShowSectionBar);
  const [coming, setComing] = useState<string | null>(null);
  return (
    <div className="lng-titlestrip">
      <div className="lng-titlestrip-nav">
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Back"
          onClick={() => window.history.back()}
        >
          <Icon name="arrow-left" size={16} />
        </button>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Forward"
          onClick={() => window.history.forward()}
        >
          <Icon name="arrow-right" size={16} />
        </button>
      </div>
      <div className="lng-titlestrip-title">{title}</div>
      <div className="lng-titlestrip-tools">
        <Tooltip content={showSectionBar ? 'Hide section bar' : 'Show section bar'}>
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={showSectionBar ? 'Hide section bar' : 'Show section bar'}
            aria-pressed={!showSectionBar}
            onClick={() => setShow(!showSectionBar)}
          >
            <Icon name="menu" size={16} />
          </button>
        </Tooltip>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Inbox"
          onClick={() => setComing('Inbox')}
        >
          <Icon name="bell" size={16} />
        </button>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Help"
          onClick={() => setComing('Help')}
        >
          <Icon name="info" size={16} />
        </button>
      </div>
      <ComingSoon open={coming !== null} onClose={() => setComing(null)} title={coming ?? ''}>
        <p>This arrives with the notifications and help inventory items.</p>
      </ComingSoon>
    </div>
  );
}
