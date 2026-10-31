import { Link } from '@tanstack/react-router';
import { Icon, Tooltip } from '@sideroom/ui';
import { useT } from '../i18n';
import { carryCall } from '../voice/carry';
import { useVoiceSession } from '../voice/session';
import { HelpButton } from './header/HelpButton';
import { InboxButton } from './inbox/InboxButton';
import { useNavVisibility } from '../storage/navModes';
import { useLoungeUi } from './store';

/** Thin global section bar above everything (spec 1). Collapsible via the store setting. */
export function SectionBar() {
  const t = useT();
  const show = useLoungeUi((s) => s.showSectionBar);
  const nav = useNavVisibility();
  const voiceName = useVoiceSession((s) => (s.status === 'connected' ? s.channelName : null));
  if (!show) return null;
  return (
    <header className="lng-sectionbar" aria-label={t('lounge.chrome.sections')}>
      <span className="lng-sectionbar-brand">{t('lounge.chrome.brand')}</span>
      <span aria-hidden="true">/</span>
      <nav aria-label={t('lounge.chrome.sections')} className="lng-sectionbar-nav">
        <Link to="/channels" className="lng-sectionbar-link" aria-current="page">
          {t('lounge.chrome.lounge')}
        </Link>
        {nav.cinema && (
          <Link to="/cinema" className="lng-sectionbar-link">
            {t('lounge.chrome.cinema')}
          </Link>
        )}
        {nav.library && (
          <Link to="/library" className="lng-sectionbar-link">
            {t('lounge.chrome.library')}
          </Link>
        )}
        {/* The emulator (ACT-EMU-001). In-app, and the call is carried along (voice/carry.ts), so a game can be hosted. */}
        <Link to="/games" className="lng-sectionbar-link" onClick={carryCall}>
          {t('lounge.chrome.games')}
        </Link>
      </nav>
      {voiceName && (
        <span className="lng-sectionbar-voice" role="status">
          <Icon name="volume" size={16} /> {t('lounge.chrome.voiceConnected', { name: voiceName })}
        </span>
      )}
    </header>
  );
}

/** Desktop title strip: history buttons, window title, inbox, help. */
export function TitleStrip({ title }: { title: string }) {
  const t = useT();
  const showSectionBar = useLoungeUi((s) => s.showSectionBar);
  const setShow = useLoungeUi((s) => s.setShowSectionBar);
  return (
    <div className="lng-titlestrip" role="region" aria-label={t('a11y.landmark.titleStrip')}>
      <div className="lng-titlestrip-nav">
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.chrome.back')}
          onClick={() => window.history.back()}
        >
          <Icon name="arrow-left" size={16} />
        </button>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.chrome.forward')}
          onClick={() => window.history.forward()}
        >
          <Icon name="arrow-right" size={16} />
        </button>
      </div>
      <div className="lng-titlestrip-title">{title}</div>
      <div className="lng-titlestrip-tools">
        <Tooltip
          content={
            showSectionBar ? t('lounge.chrome.hideSectionBar') : t('lounge.chrome.showSectionBar')
          }
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={
              showSectionBar ? t('lounge.chrome.hideSectionBar') : t('lounge.chrome.showSectionBar')
            }
            aria-pressed={!showSectionBar}
            onClick={() => setShow(!showSectionBar)}
          >
            <Icon name="menu" size={16} />
          </button>
        </Tooltip>
        <InboxButton iconSize={16} />
        <HelpButton iconSize={16} />
      </div>
    </div>
  );
}
