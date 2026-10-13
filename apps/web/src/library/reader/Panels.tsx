import { Button, Drawer, SegmentedControl, Slider, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
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
  const set = useReaderPrefsStore((s) => s.set);
  const clearOverride = useReaderPrefsStore((s) => s.clearOverride);
  const hasOverride = useReaderPrefsStore((s) => !!s.series[seriesId]);
  const [everywhere, setEverywhere] = useState(false);
  const change = (patch: Partial<ReaderPrefs>) => set(patch, { seriesId, override: !everywhere });
  return (
    <Drawer open={open} onClose={onClose} title="Reader settings" side="right">
      <div className="lib-settings">
        <SegmentedControl
          label="Reading mode"
          value={prefs.mode}
          onValueChange={(v) => change({ mode: v as ReaderMode })}
          options={[
            { value: 'paged', label: 'Single page' },
            { value: 'double', label: 'Double page' },
            { value: 'strip', label: 'Long strip' },
          ]}
        />
        <SegmentedControl
          label="Direction"
          value={prefs.direction}
          onValueChange={(v) => change({ direction: v as 'rtl' | 'ltr' })}
          options={[
            { value: 'rtl', label: 'Right to left' },
            { value: 'ltr', label: 'Left to right' },
          ]}
          disabled={prefs.mode === 'strip'}
        />
        <SegmentedControl
          label="Fit"
          value={prefs.fit}
          onValueChange={(v) => change({ fit: v as FitMode })}
          options={[
            { value: 'smart', label: 'Smart' },
            { value: 'width', label: 'Width' },
            { value: 'height', label: 'Height' },
            { value: 'original', label: 'Original' },
          ]}
          disabled={prefs.mode === 'strip'}
        />
        <SegmentedControl
          label="Background"
          value={prefs.background}
          onValueChange={(v) => change({ background: v as ReaderBg })}
          options={[
            { value: 'black', label: 'Black' },
            { value: 'grey', label: 'Grey' },
            { value: 'white', label: 'White' },
            { value: 'sepia', label: 'Sepia' },
          ]}
        />
        <Slider
          label="Page gap (long strip)"
          min={0}
          max={48}
          step={4}
          value={prefs.gap}
          showValue
          formatValue={(v) => (v === 0 ? 'Gapless' : `${v}px`)}
          onValueChange={(gap) => change({ gap })}
          disabled={prefs.mode !== 'strip'}
        />
        <Slider
          label="Brightness"
          min={30}
          max={100}
          step={5}
          value={prefs.brightness}
          showValue
          formatValue={(v) => `${v}%`}
          onValueChange={(brightness) => change({ brightness })}
        />
        <Switch
          label="Image smoothing"
          checked={prefs.smoothing}
          onChange={(e) => change({ smoothing: e.target.checked })}
        />
        <Switch
          label="Tap zones"
          description="Tap the left or right third of the page to turn it."
          checked={prefs.tapZones}
          onChange={(e) => change({ tapZones: e.target.checked })}
        />
        <Switch
          label="Show first page alone in double-page mode"
          checked={prefs.coverOffset}
          onChange={(e) => change({ coverOffset: e.target.checked })}
          disabled={prefs.mode !== 'double'}
        />
        <Switch
          label="Crop borders"
          description="Needs page border data, which arrives with the ingest work."
          checked={false}
          disabled
          onChange={() => undefined}
        />
        <Switch
          label="Use for all series"
          description="Off: changes apply to this series only."
          checked={everywhere}
          onChange={(e) => setEverywhere(e.target.checked)}
        />
        {hasOverride && (
          <Button variant="secondary" size="sm" onClick={() => clearOverride(seriesId)}>
            Reset this series to my defaults
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
  const [note, setNote] = useState('');
  return (
    <Drawer open={open} onClose={onClose} title="Bookmarks" side="right">
      <form
        className="lib-bookmark-form"
        onSubmit={(e) => {
          e.preventDefault();
          onAdd(note.trim());
          setNote('');
        }}
      >
        <TextInput
          label={`Note for page ${currentPage + 1} (optional)`}
          value={note}
          maxLength={200}
          onChange={(e) => setNote(e.target.value)}
        />
        <Button type="submit">Bookmark page {currentPage + 1}</Button>
      </form>
      {bookmarks.length === 0 ? (
        <p className="lib-muted">No bookmarks in this chapter yet. Press B to add one.</p>
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
                  Page {b.pageIndex + 1}
                  {b.note ? `: ${b.note}` : ''}
                </button>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={`Remove bookmark on page ${b.pageIndex + 1}`}
                  onClick={() => onRemove(b.id)}
                >
                  Remove
                </Button>
              </li>
            ))}
        </ul>
      )}
    </Drawer>
  );
}
