import { Button, ConfirmDialog, Notice, SegmentedControl, Select, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import * as cinema from '../../../cinema/my/api';
import {
  readPlayerDefaults,
  writePlayerDefaults,
  type PlayerDefaults,
} from '../../../cinema/player/defaults';
import { useT } from '../../../i18n';
import {
  DEFAULT_READER_PREFS,
  useReaderPrefsStore,
  type ReaderPrefs,
} from '../../../library/prefs';
import { errorText, useResource } from '../api';
import { Card, Loaded } from '../parts';

/** Subtitle languages offered as a default (the player matches on the track's language code). */
const SUBTITLE_LANGS = ['en', 'es', 'fr', 'de', 'it', 'pt', 'ja', 'ko', 'zh', 'ru'];

/**
 * Watch history (LNG-USET-035): what Cinema remembers you watched, with Clear all and the server-side pause.
 * There is no route that removes a single item, so the list is read-only apart from Clear all.
 */
export function CinemaHistoryCard() {
  const t = useT();
  const history = useResource(() => cinema.getHistory(20));
  const [paused, setPaused] = useState<boolean | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    cinema
      .getHistorySettings()
      .then((s) => live && setPaused(s.paused))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  const togglePause = async (next: boolean) => {
    setError(null);
    try {
      await cinema.setHistoryPaused(next);
      setPaused(next);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const clear = async () => {
    setBusy(true);
    setError(null);
    try {
      await cinema.clearHistory();
      setConfirm(false);
      history.reload();
    } catch (e) {
      setError(errorText(e));
      setConfirm(false);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-watch-history"
      title={t('settings.activityHistory.watchTitle')}
      description={t('settings.activityHistory.watchDesc')}
      actions={
        <Button
          variant="danger"
          size="sm"
          disabled={!history.data || history.data.items.length === 0}
          onClick={() => setConfirm(true)}
        >
          {t('settings.activityHistory.clearAll')}
        </Button>
      }
    >
      {paused !== null && (
        <Switch
          label={t('settings.activityHistory.pauseWatch')}
          description={t('settings.activityHistory.pauseWatchDesc')}
          checked={paused}
          onChange={(e) => void togglePause(e.target.checked)}
        />
      )}
      <Loaded res={history} what={t('settings.activityHistory.watchWhat')}>
        {(page) =>
          page.items.length === 0 ? (
            <p className="us-muted">{t('settings.activityHistory.watchEmpty')}</p>
          ) : (
            <ul className="us-list">
              {page.items.map((h) => (
                <li key={`${h.title.id}:${h.episode?.id ?? ''}`}>
                  <div className="us-row-main">
                    <strong>{h.title.title}</strong>
                    {h.episode && (
                      <p className="us-muted">
                        {t('settings.activityHistory.episode', {
                          number: h.episode.number,
                          title: h.episode.title ?? '',
                        })}
                      </p>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      {error && (
        <Notice variant="danger" role="alert">
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={confirm}
        danger
        loading={busy}
        title={t('settings.activityHistory.clearTitle')}
        description={t('settings.activityHistory.clearDesc')}
        confirmLabel={t('settings.activityHistory.clearAll')}
        onConfirm={() => void clear()}
        onCancel={() => setConfirm(false)}
      />
    </Card>
  );
}

/** Cinema player defaults: autoplay next episode, data saver and the subtitle language. */
export function CinemaDefaultsCard() {
  const t = useT();
  const [d, setD] = useState<PlayerDefaults>(readPlayerDefaults);
  const set = (patch: Partial<PlayerDefaults>) => setD(writePlayerDefaults(patch));
  return (
    <Card
      id="us-cinema-defaults"
      title={t('settings.activityHistory.cinemaTitle')}
      description={t('settings.activityHistory.cinemaDesc')}
    >
      <div className="us-stack">
        <Switch
          label={t('settings.activityHistory.autoplayNext')}
          checked={d.autoplayNext}
          onChange={(e) => set({ autoplayNext: e.target.checked })}
        />
        <Switch
          label={t('settings.activityHistory.dataSaver')}
          description={t('settings.activityHistory.dataSaverDesc')}
          checked={d.dataSaver}
          onChange={(e) => set({ dataSaver: e.target.checked })}
        />
        <label className="us-muted" id="us-sub-lang-label">
          {t('settings.activityHistory.subtitleLang')}
        </label>
        <Select
          aria-labelledby="us-sub-lang-label"
          value={d.subtitleLang ?? 'default'}
          options={[
            { value: 'default', label: t('settings.activityHistory.subtitleDefault') },
            ...SUBTITLE_LANGS.map((code) => ({ value: code, label: code })),
          ]}
          onValueChange={(v) => set({ subtitleLang: v === 'default' ? null : v })}
        />
        <p className="us-muted">{t('settings.activityHistory.cinemaMissing')}</p>
      </div>
    </Card>
  );
}

/** Library reader defaults, the same record the reader's own settings panel edits as "all series". */
export function LibraryDefaultsCard() {
  const t = useT();
  const global = useReaderPrefsStore((s) => s.global);
  const store = useReaderPrefsStore.getState();
  const p: ReaderPrefs = { ...DEFAULT_READER_PREFS, ...global };
  const set = (patch: Partial<ReaderPrefs>) => store.set(patch, { seriesId: '', override: false });
  return (
    <Card
      id="us-library-defaults"
      title={t('settings.activityHistory.libraryTitle')}
      description={t('settings.activityHistory.libraryDesc')}
    >
      <div className="us-stack">
        <SegmentedControl
          label={t('settings.activityHistory.readingMode')}
          value={p.mode}
          onValueChange={(v) => set({ mode: v as ReaderPrefs['mode'] })}
          options={[
            { value: 'paged', label: t('settings.activityHistory.modePaged') },
            { value: 'double', label: t('settings.activityHistory.modeDouble') },
            { value: 'strip', label: t('settings.activityHistory.modeStrip') },
          ]}
        />
        <SegmentedControl
          label={t('settings.activityHistory.readingDirection')}
          value={p.direction}
          onValueChange={(v) => set({ direction: v as ReaderPrefs['direction'] })}
          options={[
            { value: 'rtl', label: t('settings.activityHistory.dirRtl') },
            { value: 'ltr', label: t('settings.activityHistory.dirLtr') },
          ]}
        />
        <SegmentedControl
          label={t('settings.activityHistory.readerBackground')}
          value={p.background}
          onValueChange={(v) => set({ background: v as ReaderPrefs['background'] })}
          options={[
            { value: 'black', label: t('settings.activityHistory.bgBlack') },
            { value: 'grey', label: t('settings.activityHistory.bgGrey') },
            { value: 'white', label: t('settings.activityHistory.bgWhite') },
            { value: 'sepia', label: t('settings.activityHistory.bgSepia') },
          ]}
        />
        <p className="us-muted">{t('settings.activityHistory.libraryMissing')}</p>
      </div>
    </Card>
  );
}
