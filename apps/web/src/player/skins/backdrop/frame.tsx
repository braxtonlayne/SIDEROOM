/**
 * MUS-SKN-001: the backdrop skin kind. A backdrop skin is painted artwork (brass plates, tubes, reels, chrome,
 * glass, fake knobs) with only the controls the player really has laid over it, the way the old desktop players'
 * skins worked. The rules every backdrop skin follows:
 *
 * - Artwork is `aria-hidden` (`<Art>`), never focusable and never clickable (`pointer-events: none`).
 * - Live controls are the shared ones from `../controls` (or ones built on the same store actions), so they keep
 *   their labels, keyboard focus and the focus ring.
 * - Decorative motion follows the player: `data-motion="on"` only while a track plays and reduced motion is off,
 *   and the CSS animates only under that attribute (and never under `prefers-reduced-motion`).
 * - The audio level drives the art through custom properties on the frame (`--bd-level`, `--bd-low`,
 *   `--bd-high`, `--bd-b0` to `--bd-b15`), written once per animation frame from the player's analyser, so
 *   meters, needles and spectra are plain CSS and nothing re-renders per frame.
 * - Each skin is drawn for one colour scheme: the frame wears a fixed Lounge theme (Onyx for the dark skins,
 *   Light for the bright ones) with the member's appearance settings, and builds its palette from those `--ui-*`
 *   tokens with `color-mix`, so the mood holds whatever the member's own theme is and contrast stays checked.
 */
import { ThemeProvider, useTheme, type ThemeContextValue } from '@sideroom/ui';
import { useEffect, useRef, type ReactNode } from 'react';
import { useT } from '../../../i18n';
import { useReducedMotionPref } from '../../../settings/user/gifAutoplay';
import { activeGraph } from '../../engine';
import { usePlayerStore } from '../../store';
import type { PlayerSkin } from '../../types';
import './backdrop.css';

export const SPECTRUM_BANDS = 16;

/** The appearance of the surrounding dock theme, when there is one (tests render skins bare). */
function useOuterAppearance(): ThemeContextValue['appearance'] | undefined {
  try {
    return useTheme().appearance;
  } catch {
    return undefined;
  }
}

/** Is decorative motion allowed right now: a track is playing and the member has not asked for less motion. */
export function useSkinMotion(): boolean {
  const reduced = useReducedMotionPref();
  const playing = usePlayerStore((s) => s.status === 'playing');
  return playing && !reduced;
}

/**
 * Levels from the analyser, 0..1: the overall level, a low band and a high band, and `SPECTRUM_BANDS` bands for
 * a spectrum. The analyser has 32 bins (fftSize 64, `graph.ts`); the spectrum spreads them over the bands.
 */
export function readBands(data: Uint8Array): {
  level: number;
  low: number;
  high: number;
  bands: number[];
} {
  const n = data.length;
  if (n === 0) return { level: 0, low: 0, high: 0, bands: Array(SPECTRUM_BANDS).fill(0) };
  const avg = (from: number, to: number) => {
    let sum = 0;
    let k = 0;
    for (let i = Math.max(0, from); i < Math.min(n, to); i++, k++) sum += data[i]!;
    return k ? sum / k / 255 : 0;
  };
  const boost = (v: number) => Math.min(1, v * 2.2);
  const bands: number[] = [];
  const per = n / SPECTRUM_BANDS;
  for (let b = 0; b < SPECTRUM_BANDS; b++) {
    // Each band also takes the loudest bin it covers, so a pure tone still lights its bar.
    const from = Math.floor(b * per);
    const to = Math.max(from + 1, Math.floor((b + 1) * per));
    let peak = 0;
    for (let i = from; i < to && i < n; i++) peak = Math.max(peak, data[i]!);
    bands.push(Math.min(1, (peak / 255) * 1.15));
  }
  return {
    level: boost(avg(0, n)),
    low: boost(avg(0, Math.ceil(n / 8))),
    high: boost(avg(Math.floor(n / 3), n)),
    bands,
  };
}

/** Write the level properties on `el` every frame while `on`; all zero when off. */
function useLevelProps(el: React.RefObject<HTMLElement | null>, on: boolean) {
  useEffect(() => {
    const node = el.current;
    if (!node) return;
    const write = (v: ReturnType<typeof readBands>) => {
      node.style.setProperty('--bd-level', v.level.toFixed(3));
      node.style.setProperty('--bd-low', v.low.toFixed(3));
      node.style.setProperty('--bd-high', v.high.toFixed(3));
      v.bands.forEach((b, i) => node.style.setProperty(`--bd-b${i}`, b.toFixed(3)));
    };
    const rest = readBands(new Uint8Array(0));
    if (!on) {
      write(rest);
      return;
    }
    let raf = 0;
    let scratch: Uint8Array | null = null;
    const tick = () => {
      const g = activeGraph();
      if (g) {
        scratch ??= new Uint8Array(g.analyser.frequencyBinCount);
        g.analyser.getByteFrequencyData(scratch);
        write(readBands(scratch));
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      write(rest);
    };
  }, [el, on]);
}

export interface BackdropFrameProps {
  skin: PlayerSkin;
  /** The Lounge token set the artwork is drawn for. */
  scheme: 'dark' | 'light';
  /** Full-frame artwork behind everything (wood sides, screws, hazard stripes, glass glare). */
  art?: ReactNode;
  className?: string;
  children: ReactNode;
}

export function BackdropFrame({ skin, scheme, art, className, children }: BackdropFrameProps) {
  const t = useT();
  const motion = useSkinMotion();
  const status = usePlayerStore((s) => s.status);
  const ref = useRef<HTMLElement>(null);
  useLevelProps(ref, motion);
  const appearance = useOuterAppearance();
  return (
    <ThemeProvider
      section="lounge"
      perSection={{ lounge: scheme === 'dark' ? 'lounge-onyx' : 'lounge-light' }}
      {...(appearance ? { appearance } : {})}
      allowServerTheming={false}
      style={{ background: 'transparent', display: 'contents' }}
      className="sr-ply-bd-theme"
    >
      <section
        ref={ref}
        className={`sr-ply sr-ply-bd sr-ply-${skin}${className ? ` ${className}` : ''}`}
        aria-label={t('player.region')}
        data-skin={skin}
        data-kind="backdrop"
        data-motion={motion ? 'on' : 'off'}
        data-status={status}
      >
        {art && <Art className="sr-ply-bd-backdrop">{art}</Art>}
        {children}
      </section>
    </ThemeProvider>
  );
}

/** A piece of decorative artwork: hidden from assistive tech, never interactive. */
export function Art({
  className,
  children,
  style,
}: {
  className?: string;
  children?: ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <div
      className={`sr-ply-bd-art${className ? ` ${className}` : ''}`}
      aria-hidden="true"
      style={style}
    >
      {children}
    </div>
  );
}

/** Bars that follow the spectrum bands (`--bd-b<i>`); decorative, so hidden from assistive tech. */
export function SpectrumBars({
  className,
  bands = SPECTRUM_BANDS,
}: {
  className?: string;
  bands?: number;
}) {
  return (
    <div
      className={`sr-ply-bd-spectrum${className ? ` ${className}` : ''}`}
      aria-hidden="true"
      data-testid="skin-spectrum"
    >
      {Array.from({ length: bands }, (_, i) => (
        <span
          key={i}
          style={
            {
              '--v': `var(--bd-b${Math.floor((i * SPECTRUM_BANDS) / bands)}, 0)`,
            } as React.CSSProperties
          }
        />
      ))}
    </div>
  );
}

/**
 * A segmented LED level meter: segment `i` of `n` lights when the level (`--bd-level`, or the property named by
 * `source`) passes `i / n`. The last segments are the hot ones.
 */
export function LedMeter({
  className,
  segments = 14,
  source = '--bd-level',
}: {
  className?: string;
  segments?: number;
  source?: '--bd-level' | '--bd-low' | '--bd-high';
}) {
  return (
    <div
      className={`sr-ply-bd-leds${className ? ` ${className}` : ''}`}
      aria-hidden="true"
      data-testid="skin-meter"
      style={{ '--src': `var(${source}, 0)`, '--n': segments } as React.CSSProperties}
    >
      {Array.from({ length: segments }, (_, i) => (
        <span
          key={i}
          data-zone={i >= segments - 2 ? 'hot' : i >= segments - 5 ? 'warm' : 'cool'}
          style={{ '--i': i } as React.CSSProperties}
        />
      ))}
    </div>
  );
}
