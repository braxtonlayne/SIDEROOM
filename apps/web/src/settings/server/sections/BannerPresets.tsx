// LNG-SSET-006: ten preset gradient banners. The API only stores images, so a preset is drawn on a canvas
// and uploaded exactly like a custom banner (a PNG data URI), which keeps it working with no backend change.
import { useT, type MessageKey } from '../../../i18n';

export interface BannerPreset {
  id: string;
  label: MessageKey;
  from: string;
  to: string;
}

export const BANNER_PRESETS: BannerPreset[] = [
  { id: 'dark', label: 'settings.bannerPresets.dark', from: '#2b2d31', to: '#0f1012' },
  { id: 'pink', label: 'settings.bannerPresets.pink', from: '#ff7eb6', to: '#c2185b' },
  { id: 'red', label: 'settings.bannerPresets.red', from: '#ff6b6b', to: '#a4161a' },
  { id: 'orange', label: 'settings.bannerPresets.orange', from: '#ffb347', to: '#d9480f' },
  { id: 'yellow', label: 'settings.bannerPresets.yellow', from: '#ffe066', to: '#e0a100' },
  { id: 'purple', label: 'settings.bannerPresets.purple', from: '#b197fc', to: '#5f3dc4' },
  { id: 'blue', label: 'settings.bannerPresets.blue', from: '#74c0fc', to: '#1864ab' },
  { id: 'teal', label: 'settings.bannerPresets.teal', from: '#63e6be', to: '#0b7285' },
  { id: 'green', label: 'settings.bannerPresets.green', from: '#8ce99a', to: '#2b8a3e' },
  { id: 'black', label: 'settings.bannerPresets.black', from: '#1a1a1a', to: '#000000' },
];

export const PRESET_WIDTH = 960;
export const PRESET_HEIGHT = 540;

/** Draw the preset as a PNG data URI; `null` when the browser has no canvas. */
export function renderPreset(p: BannerPreset): string | null {
  try {
    const canvas = document.createElement('canvas');
    canvas.width = PRESET_WIDTH;
    canvas.height = PRESET_HEIGHT;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const g = ctx.createLinearGradient(0, 0, PRESET_WIDTH, PRESET_HEIGHT);
    g.addColorStop(0, p.from);
    g.addColorStop(1, p.to);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, PRESET_WIDTH, PRESET_HEIGHT);
    const uri = canvas.toDataURL('image/png');
    return uri.startsWith('data:image/png') ? uri : null;
  } catch {
    return null;
  }
}

export function BannerPresets({
  disabled,
  onPick,
  onUnsupported,
}: {
  disabled?: boolean;
  onPick: (dataUri: string) => void;
  onUnsupported: () => void;
}) {
  const t = useT();
  return (
    <div role="group" aria-label={t('settings.bannerPresets.group')} className="ss-presets">
      {BANNER_PRESETS.map((p) => (
        <button
          key={p.id}
          type="button"
          className="ss-preset"
          disabled={disabled}
          title={t(p.label)}
          aria-label={t('settings.bannerPresets.use', { name: t(p.label) })}
          style={{ backgroundImage: `linear-gradient(135deg, ${p.from}, ${p.to})` }}
          onClick={() => {
            const uri = renderPreset(p);
            if (uri) onPick(uri);
            else onUnsupported();
          }}
        />
      ))}
    </div>
  );
}
