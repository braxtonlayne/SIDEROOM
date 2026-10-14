// Player glyphs the shared icon set does not have. Same drawing rules as @sideroom/ui Icon: 24 box, stroke
// follows currentColor, decorative (aria-hidden); the button around them carries the accessible name.
import type { ReactNode } from 'react';

function G({ children, size = 22 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export const Back10 = () => (
  <G>
    <path d="M4 12a8 8 0 1 0 2.6-5.9" />
    <path d="M4 4v4h4" />
    <text x="12" y="15.2" fontSize="7.5" textAnchor="middle" fill="currentColor" stroke="none">
      10
    </text>
  </G>
);
export const Fwd10 = () => (
  <G>
    <path d="M20 12a8 8 0 1 1-2.6-5.9" />
    <path d="M20 4v4h-4" />
    <text x="12" y="15.2" fontSize="7.5" textAnchor="middle" fill="currentColor" stroke="none">
      10
    </text>
  </G>
);
export const Captions = () => (
  <G>
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M10 10.5a2 2 0 1 0 0 3M17 10.5a2 2 0 1 0 0 3" />
  </G>
);
export const AudioIcon = () => (
  <G>
    <path d="M4 10v4M8 7v10M12 4v16M16 8v8M20 11v2" />
  </G>
);
export const Gauge = () => (
  <G>
    <path d="M4 18a9 9 0 1 1 16 0" />
    <path d="M12 14l4-5" />
  </G>
);
export const Cog = () => (
  <G>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" />
  </G>
);
export const Fullscreen = () => (
  <G>
    <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
  </G>
);
export const ExitFullscreen = () => (
  <G>
    <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />
  </G>
);
export const Pip = () => (
  <G>
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <rect x="12" y="11" width="7" height="5" rx="1" fill="currentColor" />
  </G>
);
export const Cast = () => (
  <G>
    <path d="M3 8V6a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4" />
    <path d="M3 12a8 8 0 0 1 8 8M3 16a4 4 0 0 1 4 4M3 20h.01" />
  </G>
);
export const Expand = () => (
  <G size={18}>
    <path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7" />
  </G>
);
export const SkipNext = () => (
  <G>
    <path d="M6 5l9 7-9 7z" fill="currentColor" />
    <path d="M18 5v14" />
  </G>
);
export const PlayGlyph = ({ size = 22 }: { size?: number }) => (
  <G size={size}>
    <path d="M7 4.5v15l12-7.5z" fill="currentColor" />
  </G>
);
export const PauseGlyph = ({ size = 22 }: { size?: number }) => (
  <G size={size}>
    <path d="M8 5v14M16 5v14" strokeWidth={3} />
  </G>
);
export const VolumeGlyph = ({ muted, level }: { muted: boolean; level: number }) => (
  <G>
    <path d="M4 10v4h4l5 4V6l-5 4z" fill="currentColor" />
    {muted ? (
      <path d="M17 9l5 6M22 9l-5 6" />
    ) : (
      <>
        {level > 0 && <path d="M16.5 9.5a3.5 3.5 0 0 1 0 5" />}
        {level > 0.5 && <path d="M19 7a7 7 0 0 1 0 10" />}
      </>
    )}
  </G>
);
