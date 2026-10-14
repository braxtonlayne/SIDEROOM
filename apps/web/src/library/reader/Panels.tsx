import { Button, Drawer, SegmentedControl, Slider, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import type { Bookmark } from '../progress';
import {
  useReaderPrefsStore,
  type FitMode,
  type ReaderBg,
  type ReaderMode,
  type ReaderPrefs,
} from '../prefs';

export interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  seriesId: string;
  prefs: ReaderPrefs;
}

/** Reader settings (LIB-RDR-009). Changes apply to this series; "Use for all series" makes them the default. */
export function SettingsPanel({ open, onClose, seriesId, prefs }: SettingsPanelProps) {
  const t = useT();
  const set = useReaderPrefsStore((s) => s.set);
  const clearOverride = useReaderPrefsStore((s) => s.clearOverride);
  const hasOverride = useReaderPrefsStore((s) => !!s.series[seriesId]);
  const [everywhere, setEverywhere] = useState(false);
  const change = (patch: Partial<ReaderPrefs>) => set(patch, { seriesId, override: !everywhere });
  return (
    <Drawer open={open} onClose={onClose} title={t('library.panels.settings.title')} side="right">
      <div className="lib-settings">
        <SegmentedControl
          label={t('library.panels.settings.readingMode')}
          value={prefs.mode}
          onValueChange={(v) => change({ mode: v as ReaderMode })}
          options={[
            { value: 'paged', label: t('library.panels.settings.single') },
            { value: 'double', label: t('library.panels.settings.double') },
            { value: 'strip', label: t('library.panels.settings.strip') },
          ]}
        />
        <SegmentedControl
          label={t('library.panels.settings.direction')}
          value={prefs.direction}
          onValueChange={(v) => change({ direction: v as 'rtl' | 'ltr' })}
          options={[
            { value: 'rtl', label: t('library.panels.settings.rtl') },
            { value: 'ltr', label: t('library.panels.settings.ltr') },
          ]}
          disabled={prefs.mode === 'strip'}
        />
        <SegmentedControl
          label={t('library.panels.settings.fit')}
          value={prefs.fit}
          onValueChange={(v) => change({ fit: v as FitMode })}
          options={[
            { value: 'smart', label: t('library.panels.settings.fitSmart') },
            { value: 'width', label: t('library.panels.settings.fitWidth') },
            { value: 'height', label: t('library.panels.settings.fitHeight') },
            { value: 'original', label: t('library.panels.settings.fitOriginal') },
          ]}
          disabled={prefs.mode === 'strip'}
        />
        <SegmentedControl
          label={t('library.panels.settings.background')}
          value={prefs.background}
          onValueChange={(v) => change({ background: v as ReaderBg })}
          options={[
            { value: 'black', label: t('library.panels.settings.bgBlack') },
            { value: 'grey', label: t('library.panels.settings.bgGrey') },
            { value: 'white', label: t('library.panels.settings.bgWhite') },
            { value: 'sepia', label: t('library.panels.settings.bgSepia') },
          ]}
        />
        <Slider
          label={t('library.panels.settings.gap')}
          min={0}
          max={48}
          step={4}
          value={prefs.gap}
          showValue
          formatValue={(v) => (v === 0 ? t('library.panels.settings.gapless') : `${v}px`)}
          onValueChange={(gap) => change({ gap })}
          disabled={prefs.mode !== 'strip'}
        />
        <Slider
          label={t('library.panels.settings.brightness')}
          min={30}
          max={100}
          step={5}
          value={prefs.brightness}
          showValue
          formatValue={(v) => `${v}%`}
          onValueChange={(brightness) => change({ brightness })}
        />
        <Switch
          label={t('library.panels.settings.smoothing')}
          checked={prefs.smoothing}
          onChange={(e) => change({ smoothing: e.target.checked })}
        />
        <Switch
          label={t('library.panels.settings.tapZones')}
          description={t('library.panels.settings.tapZonesHint')}
          checked={prefs.tapZones}
          onChange={(e) => change({ tapZones: e.target.checked })}
        />
        <Switch
          label={t('library.panels.settings.coverOffset')}
          checked={prefs.coverOffset}
          onChange={(e) => change({ coverOffset: e.target.checked })}
          disabled={prefs.mode !== 'double'}
        />
        <Switch
          label={t('library.panels.settings.crop')}
          description={t('library.panels.settings.cropHint')}
          checked={false}
          disabled
          onChange={() => undefined}
        />
        <Switch
          label={t('library.panels.settings.everywhere')}
          description={t('library.panels.settings.everywhereHint')}
          checked={everywhere}
          onChange={(e) => setEverywhere(e.target.checked)}
        />
        {hasOverride && (
          <Button variant="secondary" size="sm" onClick={() => clearOverride(seriesId)}>
            {t('library.panels.settings.reset')}
          </Button>
        )}
      </div>
    </Drawer>
  );
}

export interface BookmarksPanelProps {
  open: boolean;
  onClose: () => void;
  bookmarks: Bookmark[];
  currentPage: number;
  onAdd: (note: string) => void;
  onRemove: (id: string) => void;
  onJump: (pageIndex: number) => void;
}

/** Bookmarks (LIB-RDR-013): add one at the current page with an optional note, jump, remove. */
export function BookmarksPanel({
  open,
  onClose,
  bookmarks,
  currentPage,
  onAdd,
  onRemove,
  onJump,
}: BookmarksPanelProps) {
  const t = useT();
  const [note, setNote] = useState('');
  return (
    <Drawer open={open} onClose={onClose} title={t('library.panels.bookmarks.title')} side="right">
      <form
        className="lib-bookmark-form"
        onSubmit={(e) => {
          e.preventDefault();
          onAdd(note.trim());
          setNote('');
        }}
      >
        <TextInput
          label={t('library.panels.bookmarks.noteLabel', { n: currentPage + 1 })}
          value={note}
          maxLength={200}
          onChange={(e) => setNote(e.target.value)}
        />
        <Button type="submit">{t('library.panels.bookmarks.add', { n: currentPage + 1 })}</Button>
      </form>
      {bookmarks.length === 0 ? (
        <p className="lib-muted">{t('library.panels.bookmarks.empty')}</p>
      ) : (
        <ul className="lib-bookmark-list">
          {[...bookmarks]
            .sort((a, b) => a.pageIndex - b.pageIndex)
            .map((b) => (
              <li key={b.id}>
                <button
                  type="button"
                  className="lib-linklike"
                  onClick={() => {
                    onJump(b.pageIndex);
                    onClose();
                  }}
                >
                  {t('library.panels.bookmarks.page', { n: b.pageIndex + 1 })}
                  {b.note ? t('library.panels.bookmarks.noteSuffix', { note: b.note }) : ''}
                </button>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={t('library.panels.bookmarks.removeLabel', { n: b.pageIndex + 1 })}
                  onClick={() => onRemove(b.id)}
                >
                  {t('library.panels.bookmarks.remove')}
                </Button>
              </li>
            ))}
        </ul>
      )}
    </Drawer>
  );
}
