// ARC-UI-010: in-house cover art for the Arcade cards. Inline SVG only: no image files, no downloads, no third-party
// art. Each painting is a fixed palette (a box cover, not a theme surface), so it reads the same in Lounge Light and
// the dark theme; the card around it uses the theme tokens. Decorative: hidden from screen readers.
import { useId, type ReactNode } from 'react';

export type ArtKind =
  | 'landio'
  | 'arena3d'
  | 'holdem'
  | 'whiteboard'
  | 'tabletop'
  | 'emulator'
  | 'featured'
  | 'cartridge'
  | 'workshop';

const KNOWN = new Set<string>([
  'landio',
  'arena3d',
  'holdem',
  'whiteboard',
  'tabletop',
  'emulator',
  'featured',
  'cartridge',
  'workshop',
]);

/** A stable hue (0..359) for a title, so a community game keeps its colours between visits. */
export function hueOf(seed: string): number {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h) % 360;
}

/** The art to paint for a kind; an unknown kind (a new built-in without a painting yet) gets the generic one. */
export function artKind(kind: string): ArtKind {
  return (KNOWN.has(kind) ? kind : 'featured') as ArtKind;
}

/**
 * Every painting is lit the same way (the owner's concept, ARC-UI-010): a warm low-light grade over the scene, a
 * soft lamp glow from the upper left, and a vignette that darkens the edges, so the covers read as one set.
 */
function Frame({ children, art }: { children: ReactNode; art: string }) {
  const id = useSvgId();
  const vignette = id('vig');
  const lamp = id('lamp');
  return (
    <svg
      className="arc-art-svg"
      viewBox="0 0 320 180"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      focusable="false"
      data-art={art}
    >
      <defs>
        <radialGradient id={vignette.id} cx="0.5" cy="0.45" r="0.75">
          <stop offset="0.35" stopColor="#0b0805" stopOpacity="0" />
          <stop offset="1" stopColor="#0b0805" stopOpacity="0.9" />
        </radialGradient>
        <radialGradient id={lamp.id} cx="0.22" cy="0.15" r="0.6">
          <stop offset="0" stopColor="#ffcf8a" stopOpacity="0.22" />
          <stop offset="1" stopColor="#ffcf8a" stopOpacity="0" />
        </radialGradient>
      </defs>
      {children}
      <rect
        width="320"
        height="180"
        fill="#2a1a0b"
        opacity="0.32"
        style={{ mixBlendMode: 'multiply' }}
      />
      <rect width="320" height="180" fill={lamp.url} />
      <rect width="320" height="180" fill={vignette.url} />
    </svg>
  );
}

function useSvgId(): (name: string) => { id: string; url: string } {
  const raw = useId().replace(/[^a-zA-Z0-9_-]/g, '');
  return (name) => ({ id: `arc${raw}${name}`, url: `url(#arc${raw}${name})` });
}

function LandIo() {
  const id = useSvgId();
  const bg = id('bg');
  const grid = id('grid');
  return (
    <Frame art="landio">
      <defs>
        <linearGradient id={bg.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#0d9488" />
          <stop offset="1" stopColor="#134e4a" />
        </linearGradient>
        <pattern id={grid.id} width="16" height="16" patternUnits="userSpaceOnUse">
          <path d="M16 0H0V16" fill="none" stroke="#ffffff" strokeOpacity="0.09" />
        </pattern>
      </defs>
      <rect width="320" height="180" fill={bg.url} />
      <rect width="320" height="180" fill={grid.url} />
      <path d="M32 32h80v48h-32v32H32z" fill="#fb7185" />
      <path d="M48 48h48v16H48z" fill="#fda4af" opacity="0.5" />
      <path d="M208 96h80v64h-96v-32h16z" fill="#fbbf24" />
      <path d="M224 112h48v32h-48z" fill="#fde68a" opacity="0.45" />
      <path d="M192 16h64v48h-64z" fill="#38bdf8" />
      <path
        d="M112 64h48v48h48v-16"
        fill="none"
        stroke="#fb7185"
        strokeWidth="6"
        strokeLinejoin="round"
        strokeDasharray="10 6"
      />
      <rect
        x="200"
        y="88"
        width="16"
        height="16"
        rx="3"
        fill="#fb7185"
        stroke="#ffffff"
        strokeWidth="3"
      />
      <path
        d="M256 64v24h-32"
        fill="none"
        stroke="#38bdf8"
        strokeWidth="6"
        strokeDasharray="10 6"
      />
      <rect
        x="216"
        y="80"
        width="16"
        height="16"
        rx="3"
        fill="#38bdf8"
        stroke="#ffffff"
        strokeWidth="3"
      />
      <rect
        x="152"
        y="136"
        width="16"
        height="16"
        rx="3"
        fill="#fbbf24"
        stroke="#ffffff"
        strokeWidth="3"
      />
    </Frame>
  );
}

interface CubeProps {
  x: number;
  y: number;
  s: number;
  top: string;
  left: string;
  right: string;
}

/** An isometric cube with its top at (x, y). */
function Cube({ x, y, s, top, left, right }: CubeProps) {
  const h = s / 2;
  return (
    <g>
      <path d={`M${x} ${y}l${s} ${h}l-${s} ${h}l-${s} -${h}z`} fill={top} />
      <path d={`M${x - s} ${y + h}l${s} ${h}v${s}l-${s} -${h}z`} fill={left} />
      <path d={`M${x + s} ${y + h}l-${s} ${h}v${s}l${s} -${h}z`} fill={right} />
    </g>
  );
}

function Arena3d() {
  const id = useSvgId();
  const sky = id('sky');
  const floor = id('floor');
  return (
    <Frame art="arena3d">
      <defs>
        <linearGradient id={sky.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#1e1b4b" />
          <stop offset="0.55" stopColor="#6d28d9" />
          <stop offset="1" stopColor="#f472b6" />
        </linearGradient>
        <linearGradient id={floor.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#312e81" />
          <stop offset="1" stopColor="#1e1b4b" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={sky.url} />
      <circle cx="250" cy="70" r="26" fill="#fde68a" opacity="0.85" />
      <path d="M0 100L60 84L120 96L190 78L260 92L320 82V180H0z" fill="#4c1d95" opacity="0.7" />
      <path d="M0 112H320V180H0z" fill={floor.url} />
      <g stroke="#a78bfa" strokeOpacity="0.45" strokeWidth="1.5">
        <path d="M160 112L-60 180M160 112L40 180M160 112L120 180M160 112L200 180M160 112L280 180M160 112L380 180" />
        <path d="M0 124H320M0 140H320M0 162H320" />
      </g>
      <Cube x={92} y={98} s={22} top="#c4b5fd" left="#7c3aed" right="#5b21b6" />
      <Cube x={228} y={112} s={16} top="#f9a8d4" left="#db2777" right="#9d174d" />
      <Cube x={128} y={124} s={12} top="#fde68a" left="#d97706" right="#92400e" />
      <rect x="176" y="104" width="16" height="30" rx="8" fill="#22d3ee" />
      <circle cx="184" cy="98" r="8" fill="#67e8f9" />
      <ellipse cx="184" cy="136" rx="12" ry="3" fill="#1e1b4b" opacity="0.6" />
    </Frame>
  );
}

const HEART = 'M0 4c0-5 7-6 8 0c1-6 8-5 8 0c0 6-8 10-8 12c0-2-8-6-8-12z';
const SPADE = 'M8 0c0 4-8 7-8 12c0 4 5 5 7 2l-2 5h6l-2-5c2 3 7 2 7-2c0-5-8-8-8-12z';

function PlayingCard({
  x,
  y,
  r,
  suit,
  red,
}: {
  x: number;
  y: number;
  r: number;
  suit: string;
  red: boolean;
}) {
  return (
    <g transform={`translate(${x} ${y}) rotate(${r})`}>
      <rect x="-26" y="-38" width="52" height="76" rx="7" fill="#ffffff" stroke="#e5e7eb" />
      <path
        d={suit}
        transform="translate(-14 -16) scale(1.75)"
        fill={red ? '#dc2626' : '#111827'}
      />
      <path d={suit} transform="translate(-21 -32) scale(0.6)" fill={red ? '#dc2626' : '#111827'} />
    </g>
  );
}

function Chips({ x, y, color }: { x: number; y: number; color: string }) {
  return (
    <g>
      {[0, 1, 2, 3].map((i) => (
        <g key={i}>
          <ellipse
            cx={x}
            cy={y - i * 6}
            rx="18"
            ry="7"
            fill={color}
            stroke="#0f172a"
            strokeOpacity="0.35"
          />
          <ellipse
            cx={x}
            cy={y - i * 6}
            rx="11"
            ry="4"
            fill="none"
            stroke="#ffffff"
            strokeOpacity="0.8"
            strokeDasharray="4 3"
          />
        </g>
      ))}
    </g>
  );
}

function Holdem() {
  const id = useSvgId();
  const felt = id('felt');
  return (
    <Frame art="holdem">
      <defs>
        <radialGradient id={felt.id} cx="0.5" cy="0.5" r="0.75">
          <stop offset="0" stopColor="#16a34a" />
          <stop offset="1" stopColor="#14532d" />
        </radialGradient>
      </defs>
      <rect width="320" height="180" fill="#3f2a1a" />
      <ellipse
        cx="160"
        cy="96"
        rx="190"
        ry="110"
        fill={felt.url}
        stroke="#78350f"
        strokeWidth="14"
      />
      <ellipse
        cx="160"
        cy="96"
        rx="150"
        ry="74"
        fill="none"
        stroke="#fef3c7"
        strokeOpacity="0.25"
        strokeWidth="2"
      />
      <PlayingCard x={136} y={92} r={-12} suit={HEART} red />
      <PlayingCard x={176} y={88} r={10} suit={SPADE} red={false} />
      <Chips x={70} y={120} color="#dc2626" />
      <Chips x={252} y={124} color="#2563eb" />
      <Chips x={224} y={140} color="#f8fafc" />
    </Frame>
  );
}

function Whiteboard() {
  const id = useSvgId();
  const board = id('board');
  const dots = id('dots');
  return (
    <Frame art="whiteboard">
      <defs>
        <linearGradient id={board.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#e8dcc6" />
          <stop offset="1" stopColor="#b9a888" />
        </linearGradient>
        <pattern id={dots.id} width="14" height="14" patternUnits="userSpaceOnUse">
          <circle cx="2" cy="2" r="1.2" fill="#94a3b8" fillOpacity="0.45" />
        </pattern>
      </defs>
      <rect width="320" height="180" fill={board.url} />
      <rect width="320" height="180" fill={dots.url} />
      <path
        d="M36 120c20-50 46-70 70-40s40 20 56-14"
        fill="none"
        stroke="#2563eb"
        strokeWidth="7"
        strokeLinecap="round"
      />
      <ellipse
        cx="200"
        cy="56"
        rx="34"
        ry="22"
        fill="none"
        stroke="#e11d48"
        strokeWidth="6"
        transform="rotate(-8 200 56)"
      />
      <path
        d="M150 140h70m-14-12l14 12l-14 12"
        fill="none"
        stroke="#16a34a"
        strokeWidth="6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <g transform="translate(246 96) rotate(6)">
        <rect width="54" height="54" rx="3" fill="#fde047" />
        <path
          d="M10 16h34M10 27h28M10 38h20"
          stroke="#a16207"
          strokeWidth="3"
          strokeLinecap="round"
        />
      </g>
      <g transform="translate(66 52) rotate(-30)">
        <rect width="70" height="14" rx="4" fill="#7c3aed" />
        <rect x="56" width="14" height="14" rx="3" fill="#4c1d95" />
        <path d="M0 2L-12 7L0 12z" fill="#1f2937" />
      </g>
    </Frame>
  );
}

function hexPath(cx: number, cy: number, r: number): string {
  const pts = Array.from({ length: 6 }, (_, i) => {
    const a = (Math.PI / 3) * i + Math.PI / 6;
    return `${(cx + r * Math.cos(a)).toFixed(1)} ${(cy + r * Math.sin(a)).toFixed(1)}`;
  });
  return `M${pts.join('L')}z`;
}

function Tabletop() {
  const id = useSvgId();
  const map = id('map');
  const fog = id('fog');
  const hexes: string[] = [];
  const r = 18;
  const w = Math.sqrt(3) * r;
  for (let row = 0; row < 7; row++)
    for (let col = 0; col < 11; col++)
      hexes.push(hexPath(col * w + (row % 2 ? w / 2 : 0), row * r * 1.5, r));
  return (
    <Frame art="tabletop">
      <defs>
        <linearGradient id={map.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fef3c7" />
          <stop offset="1" stopColor="#fcd34d" />
        </linearGradient>
        <linearGradient id={fog.id} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0.55" stopColor="#1c1917" stopOpacity="0" />
          <stop offset="0.8" stopColor="#1c1917" stopOpacity="0.55" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={map.url} />
      <path d="M0 140C60 120 90 160 150 132S250 150 320 120V180H0z" fill="#93c5fd" opacity="0.7" />
      <path d={hexes.join('')} fill="none" stroke="#92400e" strokeOpacity="0.3" strokeWidth="1.2" />
      <path d={hexPath(4 * w + w / 2, 3 * r * 1.5, r)} fill="#b45309" fillOpacity="0.3" />
      <circle cx={2 * w} cy={2 * r * 1.5} r="11" fill="#dc2626" stroke="#ffffff" strokeWidth="3" />
      <circle
        cx={4 * w + w / 2}
        cy={3 * r * 1.5}
        r="11"
        fill="#2563eb"
        stroke="#ffffff"
        strokeWidth="3"
      />
      <circle cx={3 * w} cy={4 * r * 1.5} r="11" fill="#16a34a" stroke="#ffffff" strokeWidth="3" />
      <rect width="320" height="180" fill={fog.url} />
      <g transform="translate(246 70)">
        <path d="M0 -34L30 -17V17L0 34L-30 17V-17z" fill="#7c3aed" />
        <path
          d="M0 -34L18 8H-18zM-30 -17L-18 8M30 -17L18 8M-18 8L0 34L18 8M-30 17L-18 8M30 17L18 8"
          fill="none"
          stroke="#ddd6fe"
          strokeWidth="2"
          strokeLinejoin="round"
        />
        <path d="M0 -34L18 8H-18z" fill="#a78bfa" />
      </g>
    </Frame>
  );
}

function Emulator() {
  const id = useSvgId();
  const bg = id('bg');
  const scan = id('scan');
  const px = [
    [2, 0],
    [8, 0],
    [3, 1],
    [7, 1],
    [2, 2],
    [3, 2],
    [4, 2],
    [5, 2],
    [6, 2],
    [7, 2],
    [8, 2],
    [1, 3],
    [2, 3],
    [4, 3],
    [5, 3],
    [6, 3],
    [8, 3],
    [9, 3],
    [0, 4],
    [1, 4],
    [2, 4],
    [3, 4],
    [4, 4],
    [5, 4],
    [6, 4],
    [7, 4],
    [8, 4],
    [9, 4],
    [10, 4],
    [0, 5],
    [2, 5],
    [8, 5],
    [10, 5],
    [3, 6],
    [7, 6],
  ];
  return (
    <Frame art="emulator">
      <defs>
        <linearGradient id={bg.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#1e1b4b" />
          <stop offset="1" stopColor="#0f172a" />
        </linearGradient>
        <pattern id={scan.id} width="4" height="4" patternUnits="userSpaceOnUse">
          <rect width="4" height="1" fill="#ffffff" fillOpacity="0.05" />
        </pattern>
      </defs>
      <rect width="320" height="180" fill={bg.url} />
      {[
        [30, 30],
        [60, 140],
        [280, 40],
        [300, 150],
        [250, 20],
        [20, 100],
        [290, 100],
      ].map(([x, y]) => (
        <rect key={`${x}-${y}`} x={x} y={y} width="4" height="4" fill="#fde68a" opacity="0.8" />
      ))}
      <g transform="translate(100 14)">
        <rect width="120" height="156" rx="14" fill="#e5e7eb" />
        <rect x="0" y="140" width="120" height="16" rx="8" fill="#cbd5e1" />
        <rect x="14" y="14" width="92" height="70" rx="6" fill="#0f172a" />
        <rect x="20" y="20" width="80" height="58" rx="2" fill="#14532d" />
        <g fill="#86efac" transform="translate(38 31)">
          {px.map(([x, y]) => (
            <rect key={`${x}-${y}`} x={x! * 4} y={y! * 4} width="4" height="4" />
          ))}
        </g>
        <path d="M24 108h10v-10h10v10h10v10H44v10H34v-10H24z" fill="#334155" />
        <circle cx="86" cy="116" r="8" fill="#e11d48" />
        <circle cx="102" cy="104" r="8" fill="#7c3aed" />
      </g>
      <rect width="320" height="180" fill={scan.url} />
    </Frame>
  );
}

/** The generic paintings, coloured by a hue from the game's title. */
function Generic({ kind, seed }: { kind: 'featured' | 'cartridge' | 'workshop'; seed: string }) {
  const id = useSvgId();
  const bg = id('bg');
  const grid = id('grid');
  const h = hueOf(seed);
  const c1 = `hsl(${h} 62% 44%)`;
  const c2 = `hsl(${(h + 48) % 360} 58% 26%)`;
  const light = `hsl(${h} 90% 86%)`;
  return (
    <Frame art={kind}>
      <defs>
        <linearGradient id={bg.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor={c1} />
          <stop offset="1" stopColor={c2} />
        </linearGradient>
        <pattern id={grid.id} width="20" height="20" patternUnits="userSpaceOnUse">
          <path d="M20 0H0V20" fill="none" stroke="#ffffff" strokeOpacity="0.12" />
        </pattern>
      </defs>
      <rect width="320" height="180" fill={bg.url} />
      {kind === 'featured' && (
        <g>
          <circle
            cx="160"
            cy="90"
            r="70"
            fill="none"
            stroke={light}
            strokeOpacity="0.25"
            strokeWidth="14"
          />
          <circle
            cx="160"
            cy="90"
            r="40"
            fill="none"
            stroke={light}
            strokeOpacity="0.35"
            strokeWidth="10"
          />
          <path d="M160 52L170 80L198 90L170 100L160 128L150 100L122 90L150 80z" fill={light} />
          <circle cx="60" cy="40" r="5" fill={light} opacity="0.7" />
          <circle cx="262" cy="140" r="7" fill={light} opacity="0.6" />
          <circle cx="270" cy="38" r="3" fill={light} opacity="0.8" />
        </g>
      )}
      {kind === 'cartridge' && (
        <g>
          <rect width="320" height="180" fill={grid.url} />
          <g transform="translate(110 22)">
            <path d="M0 8a8 8 0 0 1 8-8h84a8 8 0 0 1 8 8v128H0z" fill={light} />
            <rect x="12" y="16" width="76" height="62" rx="4" fill={c2} />
            <path
              d="M24 34h52M24 46h40M24 58h46"
              stroke={light}
              strokeWidth="5"
              strokeLinecap="round"
              opacity="0.8"
            />
            <path
              d="M14 98h72M14 108h72M14 118h72"
              stroke={c2}
              strokeOpacity="0.35"
              strokeWidth="4"
            />
          </g>
        </g>
      )}
      {kind === 'workshop' && (
        <g>
          <rect width="320" height="180" fill={grid.url} />
          <g fill="none" stroke={light} strokeWidth="3" strokeDasharray="8 6">
            <rect x="104" y="40" width="112" height="100" rx="10" />
            <circle cx="160" cy="90" r="28" />
          </g>
          <path d="M40 150h80" stroke={light} strokeWidth="6" strokeLinecap="round" />
          <path
            d="M48 150v-10M64 150v-6M80 150v-10M96 150v-6M112 150v-10"
            stroke={light}
            strokeWidth="3"
          />
          <path d="M240 40l40 40-12 12-40-40z" fill={light} opacity="0.8" />
          <path d="M228 52l-8 8 12 4z" fill={light} />
        </g>
      )}
    </Frame>
  );
}

/** The cover art for one card. `seed` colours the generic paintings (use the game's title). */
export function GameArt({ kind, seed = '' }: { kind: string; seed?: string }) {
  const k = artKind(kind);
  switch (k) {
    case 'landio':
      return <LandIo />;
    case 'arena3d':
      return <Arena3d />;
    case 'holdem':
      return <Holdem />;
    case 'whiteboard':
      return <Whiteboard />;
    case 'tabletop':
      return <Tabletop />;
    case 'emulator':
      return <Emulator />;
    default:
      return <Generic kind={k} seed={seed || k} />;
  }
}
