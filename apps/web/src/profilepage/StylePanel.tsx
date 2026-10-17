// The style panel of the profile editor (EXT-PRF-005): palette, background pattern or banner, a font from the
// approved list, cursor set, sparkle overlay and block borders. Every choice is from a closed list or a
// `#rrggbb` colour, and text must stay readable on the palette (the server enforces 4.5:1 as well).
import { Button, Select } from '@sideroom/ui';
import { useRef } from 'react';
import { useT, type MessageKey } from '../i18n';
import {
  BORDERS,
  CURSORS,
  FONTS,
  OVERLAYS,
  PATTERNS,
  effectiveBackground,
  type Background,
  type Palette,
  type ProfileMedia,
  type Style,
} from './types';

const HEX = /^#[0-9a-fA-F]{6}$/;

function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
}

/** WCAG contrast ratio of two `#rrggbb` colours, or null when either is not one. */
export function contrast(a: string, b: string): number | null {
  if (!HEX.test(a) || !HEX.test(b)) return null;
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** The palette entries whose text contrast is too low, for the warning under the colours. */
export function lowContrast(p: Palette): ('background' | 'panel')[] {
  return (['background', 'panel'] as const).filter((k) => {
    const c = contrast(p.text, p[k]);
    return c !== null && c < 4.5;
  });
}

const FONT_LABEL: Record<string, MessageKey> = {
  inter: 'lounge.profileEditor.fontInter',
  newsreader: 'lounge.profileEditor.fontNewsreader',
  'hanken-grotesk': 'lounge.profileEditor.fontHanken',
  'plex-mono': 'lounge.profileEditor.fontPlex',
};
const CURSOR_LABEL: Record<string, MessageKey> = {
  default: 'lounge.profileEditor.cursorDefault',
  pointer: 'lounge.profileEditor.cursorPointer',
  crosshair: 'lounge.profileEditor.cursorCrosshair',
  cell: 'lounge.profileEditor.cursorCell',
  help: 'lounge.profileEditor.cursorHelp',
};
const OVERLAY_LABEL: Record<string, MessageKey> = {
  none: 'lounge.profileEditor.overlayNone',
  sparkle: 'lounge.profileEditor.overlaySparkle',
  stars: 'lounge.profileEditor.overlayStars',
  hearts: 'lounge.profileEditor.overlayHearts',
  snow: 'lounge.profileEditor.overlaySnow',
};
const BORDER_LABEL: Record<string, MessageKey> = {
  none: 'lounge.profileEditor.borderNone',
  solid: 'lounge.profileEditor.borderSolid',
  dashed: 'lounge.profileEditor.borderDashed',
  dotted: 'lounge.profileEditor.borderDotted',
  double: 'lounge.profileEditor.borderDouble',
  groove: 'lounge.profileEditor.borderGroove',
};
const PATTERN_LABEL: Record<string, MessageKey> = {
  none: 'lounge.profileEditor.patternNone',
  dots: 'lounge.profileEditor.patternDots',
  grid: 'lounge.profileEditor.patternGrid',
  stripes: 'lounge.profileEditor.patternStripes',
  checks: 'lounge.profileEditor.patternChecks',
  waves: 'lounge.profileEditor.patternWaves',
};

const COLOR_KEYS: { key: keyof Palette; label: MessageKey }[] = [
  { key: 'background', label: 'lounge.profileEditor.colorBackground' },
  { key: 'panel', label: 'lounge.profileEditor.colorPanel' },
  { key: 'text', label: 'lounge.profileEditor.colorText' },
  { key: 'accent', label: 'lounge.profileEditor.colorAccent' },
];

export function StylePanel({
  style,
  onChange,
  media = [],
  onUploadBackground,
  onRemoveMedia,
  uploading = false,
}: {
  style: Style;
  onChange: (s: Style) => void;
  /** The person's uploaded pictures (K-17). */
  media?: ProfileMedia[];
  /** Upload a background picture; resolves to the stored picture, or null when it failed. */
  onUploadBackground?: (file: File) => Promise<ProfileMedia | null>;
  /** Delete an uploaded picture (and take it out of the saved document on the server). */
  onRemoveMedia?: (id: string) => void;
  uploading?: boolean;
}) {
  const t = useT();
  const fileInput = useRef<HTMLInputElement>(null);
  const set = (patch: Partial<Style>) => onChange({ ...style, ...patch });
  const bg = effectiveBackground(style);
  const picture = media.find((m) => m.kind === 'background' && m.id === bg.image_id);
  const uploaded = media.find((m) => m.kind === 'background');
  /** Set the background and keep the older `pattern` / `banner_background` pair in step for older readers. */
  const setBackground = (b: Background) =>
    set({ background: b, pattern: b.pattern ?? 'none', banner_background: b.kind === 'banner' });
  const setColor = (key: keyof Palette, value: string) =>
    set({ palette: { ...style.palette, [key]: value } });
  const low = lowContrast(style.palette);
  const choices = (list: readonly string[], labels: Record<string, MessageKey>) =>
    list.map((v) => ({ value: v, label: labels[v] ? t(labels[v]) : v }));

  return (
    <div className="pp-fields">
      <fieldset className="pp-colors">
        <legend>{t('lounge.profileEditor.palette')}</legend>
        {COLOR_KEYS.map(({ key, label }) => (
          <label key={key} className="pp-color">
            <span>{t(label)}</span>
            <input
              type="color"
              aria-label={t('lounge.profileEditor.pickColor', { name: t(label) })}
              value={HEX.test(style.palette[key]) ? style.palette[key] : '#000000'}
              onChange={(e) => setColor(key, e.target.value)}
            />
            <input
              type="text"
              aria-label={t('lounge.profileEditor.hexColor', { name: t(label) })}
              value={style.palette[key]}
              maxLength={7}
              size={8}
              spellCheck={false}
              aria-invalid={!HEX.test(style.palette[key])}
              onChange={(e) => setColor(key, e.target.value)}
            />
          </label>
        ))}
      </fieldset>
      {low.length > 0 && (
        <p role="alert" className="us-muted">
          {t('lounge.profileEditor.lowContrast')}
        </p>
      )}
      <Select
        aria-label={t('lounge.profileEditor.backgroundKind')}
        options={[
          { value: 'pattern', label: t('lounge.profileEditor.backgroundPattern') },
          { value: 'banner', label: t('lounge.profileEditor.backgroundBanner') },
          ...(uploaded
            ? [{ value: 'image', label: t('lounge.profileEditor.backgroundImage') }]
            : []),
        ]}
        value={bg.kind === 'image' && !picture ? 'pattern' : bg.kind}
        onValueChange={(v) => {
          const pattern = bg.pattern ?? 'none';
          if (v === 'image' && uploaded)
            setBackground({ kind: 'image', pattern, image_id: uploaded.id });
          else if (v === 'banner') setBackground({ kind: 'banner', pattern });
          else setBackground({ kind: 'pattern', pattern });
        }}
      />
      <Select
        aria-label={t('lounge.profileEditor.pattern')}
        options={choices(PATTERNS, PATTERN_LABEL)}
        value={bg.pattern ?? 'none'}
        onValueChange={(v) => setBackground({ ...bg, pattern: v })}
      />
      {onUploadBackground && (
        <>
          <input
            ref={fileInput}
            type="file"
            hidden
            accept="image/png,image/jpeg,image/gif,image/webp"
            aria-label={t('lounge.profileEditor.backgroundUploadFile')}
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = '';
              if (!f) return;
              void onUploadBackground(f).then((m) => {
                if (m)
                  setBackground({ kind: 'image', pattern: bg.pattern ?? 'none', image_id: m.id });
              });
            }}
          />
          <Button
            variant="secondary"
            loading={uploading}
            onClick={() => fileInput.current?.click()}
          >
            {t('lounge.profileEditor.backgroundUpload')}
          </Button>
        </>
      )}
      {picture && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            onRemoveMedia?.(picture.id);
            setBackground({ kind: 'pattern', pattern: bg.pattern ?? 'none' });
          }}
        >
          {t('lounge.profileEditor.backgroundRemove')}
        </Button>
      )}
      <Select
        aria-label={t('lounge.profileEditor.font')}
        options={choices(FONTS, FONT_LABEL)}
        value={style.font}
        onValueChange={(v) => set({ font: v })}
      />
      <Select
        aria-label={t('lounge.profileEditor.cursor')}
        options={choices(CURSORS, CURSOR_LABEL)}
        value={style.cursor}
        onValueChange={(v) => set({ cursor: v })}
      />
      <Select
        aria-label={t('lounge.profileEditor.overlay')}
        options={choices(OVERLAYS, OVERLAY_LABEL)}
        value={style.overlay}
        onValueChange={(v) => set({ overlay: v })}
      />
      <Select
        aria-label={t('lounge.profileEditor.border')}
        options={choices(BORDERS, BORDER_LABEL)}
        value={style.border}
        onValueChange={(v) => set({ border: v })}
      />
    </div>
  );
}
