import { Badge, Button, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import {
  disableForMe,
  enableForMe,
  listPackages,
  myEnabled,
  problemLines,
  type Enablement,
  type PackageView,
} from './api';
import { safeTokens, themeOf, useStoreThemes } from './themes';
import { useLoad } from './useLoad';
import './packages.css';

const SECTION_LABEL: Record<string, MessageKey> = {
  all: 'settings.themeStore.sectionAll',
  lounge: 'settings.themeStore.sectionLounge',
  cinema: 'settings.themeStore.sectionCinema',
  library: 'settings.themeStore.sectionLibrary',
  entrance: 'settings.themeStore.sectionEntrance',
};
const SECTIONS = Object.keys(SECTION_LABEL);

function Swatch({ tokens }: { tokens: Record<string, string> }) {
  const t = safeTokens(tokens);
  const colors = [t['--ui-bg'], t['--ui-bg-raised'], t['--ui-text'], t['--ui-accent']].filter(
    (c): c is string => !!c,
  );
  return (
    <div className="pkg-swatch" aria-hidden="true">
      {(colors.length ? colors : ['var(--ui-bg-overlay)']).map((c, i) => (
        <span key={i} style={{ background: c }} />
      ))}
    </div>
  );
}

/**
 * The Theme Store (EXT-PKG-005), in User Settings > Appearance: browse approved themes, try one on the whole
 * interface before keeping it, and apply it everywhere or to one section. A theme only changes design tokens;
 * it never runs code (D-063).
 */
export function ThemeStore() {
  const t = useT();
  const [query, setQuery] = useState('');
  const [where, setWhere] = useState<string>('all');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const list = useLoad<PackageView[]>(`themes:${query}`, () => listPackages('theme', query));
  const enabled = useLoad<Enablement[]>('themes:enabled', myEnabled);
  const preview = useStoreThemes((s) => s.preview);
  const setPreview = useStoreThemes((s) => s.setPreview);
  const setThemes = useStoreThemes((s) => s.setThemes);

  // Leaving the page ends a preview.
  useEffect(() => () => setPreview(null), [setPreview]);

  const themes = (enabled.data ?? []).filter((e) => e.kind === 'theme');
  const run = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await fn();
      const next = await myEnabled();
      enabled.set(next);
      setThemes(next);
    } catch (e) {
      setError(problemLines(e).join(' ') || t('settings.themeStore.failed'));
    } finally {
      setBusy(null);
    }
  };
  const apply = (p: { id: string }, section: string) => {
    setPreview(null);
    return run(`apply:${p.id}`, () => enableForMe(p.id, section));
  };
  const sectionLabel = (s: string) => t(SECTION_LABEL[s] ?? SECTION_LABEL.all!);

  return (
    <div>
      {preview && (
        <div className="pkg-banner" role="status">
          <span>{t('settings.themeStore.previewing', { name: preview.name })}</span>
          <span className="pkg-actions">
            <Button
              size="sm"
              variant="primary"
              disabled={busy !== null}
              onClick={() => void apply({ id: preview.packageId }, where)}
            >
              {t('settings.themeStore.applyPreview')}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setPreview(null)}>
              {t('settings.themeStore.stopPreview')}
            </Button>
          </span>
        </div>
      )}
      {error && (
        <Notice variant="danger" title={t('settings.themeStore.failed')}>
          {error}
        </Notice>
      )}

      {themes.length > 0 && (
        <>
          <h3 className="us-subhead">{t('settings.themeStore.inUse')}</h3>
          <ul className="pkg-list" aria-label={t('settings.themeStore.inUse')}>
            {themes.map((e) => (
              <li key={e.package_id} className="pkg-row">
                <span>
                  <strong>{e.name}</strong> <span className="us-muted">{e.version}</span>{' '}
                  <Badge>{sectionLabel(e.section)}</Badge>
                  {e.update_available && (
                    <>
                      {' '}
                      <Badge variant="accent">{t('settings.themeStore.updateAvailable')}</Badge>
                    </>
                  )}
                </span>
                <span className="pkg-actions">
                  {e.update_available && (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy !== null}
                      onClick={() => void apply({ id: e.package_id }, e.section)}
                    >
                      {t('settings.themeStore.update')}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy !== null}
                    onClick={() =>
                      void run(`off:${e.package_id}`, () => disableForMe(e.package_id))
                    }
                  >
                    {t('settings.themeStore.remove', { name: e.name })}
                  </Button>
                </span>
              </li>
            ))}
          </ul>
        </>
      )}

      <div className="pkg-toolbar">
        <TextInput
          label={t('settings.themeStore.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="pkg-where">
          <label className="us-muted" id="pkg-where-label">
            {t('settings.themeStore.applyTo')}
          </label>
          <Select
            aria-labelledby="pkg-where-label"
            options={SECTIONS.map((s) => ({ value: s, label: sectionLabel(s) }))}
            value={where}
            onValueChange={setWhere}
          />
        </div>
      </div>

      {list.loading && !list.data ? (
        <p className="us-muted" role="status">
          {t('settings.themeStore.loading')}
        </p>
      ) : list.error ? (
        <Notice variant="danger" title={t('settings.themeStore.loadFailed')}>
          {list.error}
        </Notice>
      ) : list.data && list.data.length > 0 ? (
        <ul className="pkg-grid" aria-label={t('settings.themeStore.listLabel')}>
          {list.data.map((p) => {
            const theme = themeOf({ ...p, payload: p.payload });
            const active = themes.some((e) => e.package_id === p.id);
            return (
              <li key={p.id} className="pkg-card">
                <Swatch tokens={theme?.tokens ?? {}} />
                <h3>{p.name}</h3>
                <p className="pkg-meta">
                  <span>{t('settings.themeStore.by', { author: p.author_name })}</span>
                  <Badge>
                    {theme?.scheme === 'light'
                      ? t('settings.themeStore.light')
                      : t('settings.themeStore.dark')}
                  </Badge>
                  <span>{p.version}</span>
                </p>
                {p.description && <p>{p.description}</p>}
                <div className="pkg-actions">
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('settings.themeStore.previewLabel', { name: p.name })}
                    disabled={!theme}
                    onClick={() => setPreview(theme)}
                  >
                    {t('settings.themeStore.preview')}
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    aria-label={t('settings.themeStore.applyLabel', { name: p.name })}
                    disabled={busy !== null || active}
                    onClick={() => void apply(p, where)}
                  >
                    {active ? t('settings.themeStore.applied') : t('settings.themeStore.apply')}
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      ) : (
        <EmptyState
          title={t('settings.themeStore.emptyTitle')}
          description={t('settings.themeStore.emptyBody')}
        />
      )}
    </div>
  );
}
