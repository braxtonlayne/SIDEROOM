import type { SVGProps } from 'react';

const base = (p: SVGProps<SVGSVGElement>) => ({
  width: 20,
  height: 20,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.4,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  'aria-hidden': true,
  focusable: false,
  ...p,
});

export const DoorIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}>
    <path d="M6 21V4h12v17M3 21h18M9 8v13M15 8v13" />
  </svg>
);
export const KeyIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}>
    <circle cx="8" cy="15" r="4" />
    <path d="M11 12l8-8M16 7l2 2M14 9l2 2" />
  </svg>
);
export const LockIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}>
    <rect x="5" y="11" width="14" height="9" rx="1" />
    <path d="M8 11V8a4 4 0 018 0v3" />
  </svg>
);
export const DiscordIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base({ fill: 'currentColor', stroke: 'none', ...p })}>
    <path d="M19.3 5.3A17 17 0 0015 4l-.2.4a15 15 0 013.8 1.9 14 14 0 00-13.2 0A15 15 0 019.2 4.4L9 4a17 17 0 00-4.3 1.3C2 9.300 1.300 13.200 1.700 17a17 17 0 005.200 2.600l1.100-1.800a11 11 0 01-1.700-.8l.4-.3a12 12 0 0010.600 0l.4.300a11 11 0 01-1.700.800l1.100 1.800a17 17 0 005.200-2.600c.5-4.400-.8-8.300-3-11.700zM8.700 14.800c-1 0-1.800-.9-1.800-2s.8-2 1.800-2 1.800.9 1.800 2-.8 2-1.800 2zm6.600 0c-1 0-1.800-.9-1.800-2s.8-2 1.800-2 1.800.9 1.800 2-.8 2-1.800 2z" />
  </svg>
);
