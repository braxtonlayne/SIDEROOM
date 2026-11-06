// ARC-UI-010: in-house cover art for the Arcade cards. Inline SVG only: no image files, no downloads, no third-party
// art. Each painting is a fixed palette (a box cover, not a theme surface), so it reads the same in Lounge Light and
// the dark theme; the card around it uses the theme tokens. Decorative: hidden from screen readers.
//
// ARC-ART-001 (D-481): every painting is a scene with one subject, a ground, one warm key light from the upper left
// (a lit side, a shadow side, a cast shadow to the lower right) and depth. The subject sits in the viewBox box
// x 120-200, y 20-100: a desktop row card shows only about x 120-200 of the 320 x 180 painting, a phone thumbnail
// about x 95-225, and a hero card keeps its text over the lower left. No filters, no animation.
import { useId, type ReactNode } from 'react';

export type ArtKind =
  | 'landio'
  | 'arena3d'
  | 'holdem'
  | 'whiteboard'
  | 'tabletop'
  | 'sketch'
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
  'sketch',
  'emulator',
  'featured',
  'cartridge',
  'workshop',
]);

/**
 * A stable hue (0..359) for a title, so a community game keeps its colours between visits. D-493: the olive band
 * 45..75 is folded onto gold (36..45), which reads as a curtain or a warm blueprint at the scenes' lightness.
 */
export function hueOf(seed: string): number {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  const r = Math.abs(h) % 360;
  return r >= 45 && r <= 75 ? 36 + Math.round(((r - 45) * 9) / 30) : r;
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
        <radialGradient id={vignette.id} cx="0.5" cy="0.42" r="0.72">
          <stop offset="0.3" stopColor="#0b0805" stopOpacity="0" />
          <stop offset="1" stopColor="#0b0805" stopOpacity="0.95" />
        </radialGradient>
        <radialGradient id={lamp.id} cx="0.22" cy="0.12" r="0.6">
          <stop offset="0" stopColor="#ffcf8a" stopOpacity="0.24" />
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

/** A soft ellipse with no edge: a cast shadow (black, the default) or a pool of light (a warm colour). */
function Soft({
  cx,
  cy,
  rx,
  ry,
  o = 0.65,
  c = '#000',
}: {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  o?: number;
  c?: string;
}) {
  const g = useSvgId()('soft');
  return (
    <>
      <radialGradient id={g.id}>
        <stop offset="0" stopColor={c} stopOpacity={o} />
        <stop offset="1" stopColor={c} stopOpacity="0" />
      </radialGradient>
      <ellipse cx={cx} cy={cy} rx={rx} ry={ry} fill={g.url} />
    </>
  );
}

/** The darker band behind the subject, so a lit subject stands off its background. */
function Band({ o = 0.4 }: { o?: number }) {
  const g = useSvgId()('band');
  return (
    <>
      <linearGradient id={g.id}>
        <stop offset="0" stopOpacity="0" />
        <stop offset="0.5" stopOpacity={o} />
        <stop offset="1" stopOpacity="0" />
      </linearGradient>
      <rect x="70" width="180" height="180" fill={g.url} />
    </>
  );
}

/** A warm cone of light from a lamp at (x, y) onto the span x1..x2 at height y2. */
function Beam({
  x,
  y,
  x1,
  x2,
  y2,
  o = 0.2,
}: Record<'x' | 'y' | 'x1' | 'x2' | 'y2', number> & { o?: number }) {
  const g = useSvgId()('beam');
  return (
    <>
      <linearGradient id={g.id} x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stopColor="#ffdca8" stopOpacity={o * 1.6} />
        <stop offset="1" stopColor="#ffdca8" stopOpacity={o * 0.4} />
      </linearGradient>
      <path d={`M${x} ${y}L${x2} ${y2}H${x1}z`} fill={g.url} />
    </>
  );
}

function LandIo() {
  const id = useSvgId();
  const bg = id('bg');
  const grid = id('grid');
  return (
    <Frame art="landio">
      <defs>
        <linearGradient id={bg.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#0f766e" />
          <stop offset="1" stopColor="#0b3b38" />
        </linearGradient>
        <pattern id={grid.id} width="16" height="16" patternUnits="userSpaceOnUse">
          <path d="M16 0H0V16" fill="none" stroke="#ffffff" strokeOpacity="0.08" />
        </pattern>
      </defs>
      <rect width="320" height="180" fill={bg.url} />
      <rect width="320" height="180" fill={grid.url} />
      <Band o={0.35} />
      {/* Rival territories at the edges, each a slab with a shadowed front edge. */}
      <path d="M16 24h72v40H56v32H16z" fill="#0284c7" />
      <path d="M16 96h40v5H16zM56 64h32v5H56z" fill="#075985" />
      <path d="M232 112h72v52h-88v-24h16z" fill="#d97706" />
      <path d="M216 164h88v5h-88z" fill="#92400e" />
      <path d="M240 16h56v36h-56z" fill="#7c3aed" />
      <path d="M240 52h56v5h-56z" fill="#4c1d95" />
      {/* The player's home: a raised slab in the middle, lit from the upper left. */}
      <Soft cx={172} cy={104} rx={46} ry={16} o={0.55} />
      <path d="M128 36h64v52h-24v16h-40z" fill="#fb7185" />
      <path d="M128 104h40v8h-40zM168 88h24v8h-24z" fill="#9f1239" />
      <path d="M192 40l4-2v52l-4 -2z" fill="#881337" />
      <path d="M128 36h64v4h-60v64h-4z" fill="#fecdd3" opacity="0.8" />
      <path d="M140 48h28v14h-28z" fill="#fda4af" opacity="0.55" />
      {/* The trail out and back, and the player at its head. */}
      <path
        d="M196 56h16v56h-36"
        fill="none"
        stroke="#fb7185"
        strokeWidth="5"
        strokeLinejoin="round"
        strokeDasharray="9 5"
      />
      <Soft cx={176} cy={122} rx={12} ry={4} o={0.6} />
      <rect
        x="160"
        y="104"
        width="15"
        height="15"
        rx="3"
        fill="#fb7185"
        stroke="#fff"
        strokeWidth="3"
      />
      <path d="M240 60v20h-20" fill="none" stroke="#a78bfa" strokeWidth="5" strokeDasharray="9 5" />
      <rect
        x="208"
        y="73"
        width="13"
        height="13"
        rx="3"
        fill="#a78bfa"
        stroke="#fff"
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
  const body = id('body');
  return (
    <Frame art="arena3d">
      <defs>
        <linearGradient id={sky.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#140f33" />
          <stop offset="0.6" stopColor="#5b21b6" />
          <stop offset="1" stopColor="#db2777" />
        </linearGradient>
        <linearGradient id={floor.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2e2a78" />
          <stop offset="1" stopColor="#120f30" />
        </linearGradient>
        <linearGradient id={body.id}>
          <stop offset="0" stopColor="#a5f3fc" />
          <stop offset="0.45" stopColor="#22d3ee" />
          <stop offset="1" stopColor="#0e5566" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={sky.url} />
      <circle cx="64" cy="40" r="18" fill="#fde68a" opacity="0.9" />
      <Soft cx={64} cy={40} rx={60} ry={44} c="#fde68a" o={0.3} />
      <path d="M0 76L50 64L110 74L170 58L240 72L320 62V90H0z" fill="#3b0f78" opacity="0.8" />
      <path d="M0 86H320V180H0z" fill={floor.url} />
      <g stroke="#a78bfa" strokeOpacity="0.4" strokeWidth="1.2">
        <path d="M160 86L-100 180M160 86L20 180M160 86L110 180M160 86L210 180M160 86L300 180M160 86L420 180" />
        <path d="M0 96H320M0 110H320M0 130H320M0 158H320" />
      </g>
      <Band o={0.3} />
      {/* The crate in the middle, lit on its left face, and the player beside it. */}
      <Soft cx={168} cy={106} rx={48} ry={10} o={0.7} />
      <Cube x={144} y={44} s={27} top="#e9e3ff" left="#8b5cf6" right="#3b0764" />
      <path d="M144 44l27 13.5" stroke="#fff" strokeOpacity="0.6" strokeWidth="1.5" />
      <Soft cx={194} cy={112} rx={18} ry={4} o={0.7} />
      <rect x="176" y="72" width="16" height="38" rx="8" fill={body.url} />
      <circle cx="184" cy="64" r="9" fill={body.url} />
      <Soft cx={104} cy={136} rx={22} ry={5} o={0.6} />
      <Cube x={92} y={108} s={13} top="#fde68a" left="#d97706" right="#78350f" />
      <Soft cx={258} cy={140} rx={22} ry={5} o={0.6} />
      <Cube x={246} y={110} s={15} top="#f9a8d4" left="#db2777" right="#701a45" />
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
      <rect x="-22" y="-30" width="52" height="76" rx="7" fill="#000" opacity="0.4" />
      <rect x="-26" y="-38" width="52" height="76" rx="7" fill="#fbf8f1" stroke="#d6d0c4" />
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
      <Soft cx={x + 8} cy={y + 4} rx={26} ry={8} o={0.6} />
      {[0, 1, 2, 3].map((i) => (
        <g key={i}>
          <ellipse
            cx={x}
            cy={y - i * 6}
            rx="18"
            ry="7"
            fill={color}
            stroke="#0f172a"
            strokeOpacity="0.45"
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
        <radialGradient id={felt.id} cx="0.42" cy="0.35" r="0.75">
          <stop offset="0" stopColor="#1f9d55" />
          <stop offset="0.6" stopColor="#14632f" />
          <stop offset="1" stopColor="#0b3a1c" />
        </radialGradient>
      </defs>
      <rect width="320" height="180" fill="#24170e" />
      <ellipse
        cx="160"
        cy="104"
        rx="190"
        ry="112"
        fill={felt.url}
        stroke="#5a2a0c"
        strokeWidth="16"
      />
      <ellipse
        cx="160"
        cy="104"
        rx="150"
        ry="76"
        fill="none"
        stroke="#fef3c7"
        strokeOpacity="0.22"
        strokeWidth="2"
      />
      <Soft cx={150} cy={66} rx={90} ry={56} c="#ffe2a8" o={0.22} />
      <PlayingCard x={140} y={62} r={-12} suit={HEART} red />
      <PlayingCard x={180} y={58} r={10} suit={SPADE} red={false} />
      <Chips x={196} y={118} color="#f1ece2" />
      <Chips x={70} y={126} color="#dc2626" />
      <Chips x={262} y={128} color="#2563eb" />
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
        <linearGradient id={board.id} x1="0" y1="0" x2="1" y2="0.6">
          <stop offset="0" stopColor="#f1e7d4" />
          <stop offset="1" stopColor="#a4937a" />
        </linearGradient>
        <pattern id={dots.id} width="14" height="14" patternUnits="userSpaceOnUse">
          <circle cx="2" cy="2" r="1.1" fill="#94a3b8" fillOpacity="0.45" />
        </pattern>
      </defs>
      {/* The board hangs on a dark wall over its marker tray. */}
      <rect width="320" height="180" fill="#2a2018" />
      <rect x="34" y="18" width="260" height="112" fill="#000" opacity="0.45" />
      <rect
        x="28"
        y="12"
        width="260"
        height="112"
        rx="2"
        fill={board.url}
        stroke="#7b7f86"
        strokeWidth="4"
      />
      <rect x="30" y="14" width="256" height="108" fill={dots.url} />
      <path
        d="M44 104c16-40 36-56 56-32s30 14 40-10"
        fill="none"
        stroke="#2563eb"
        strokeWidth="6"
        strokeLinecap="round"
      />
      <ellipse
        cx="138"
        cy="50"
        rx="30"
        ry="19"
        fill="none"
        stroke="#e11d48"
        strokeWidth="5"
        transform="rotate(-8 138 50)"
      />
      <path
        d="M128 98h56m-12-10l12 10l-12 10"
        fill="none"
        stroke="#16a34a"
        strokeWidth="5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <g transform="translate(164 28) rotate(6)">
        <rect x="3" y="4" width="40" height="40" fill="#000" opacity="0.3" />
        <rect width="40" height="40" rx="2" fill="#fde047" />
        <path d="M0 0h40v6H0z" fill="#facc15" />
        <path
          d="M8 16h24M8 24h20M8 32h14"
          stroke="#a16207"
          strokeWidth="2.5"
          strokeLinecap="round"
        />
      </g>
      {/* The tray, and a marker on it throwing its shadow to the right. */}
      <path d="M48 124h224l-8 10H56z" fill="#6b7280" />
      <path d="M56 134h208v3H56z" fill="#374151" />
      <Soft cx={176} cy={150} rx={110} ry={14} o={0.5} />
      <rect x="132" y="118" width="56" height="8" rx="3" fill="#7c3aed" />
      <rect x="176" y="118" width="12" height="8" rx="2" fill="#4c1d95" />
    </Frame>
  );
}

function Sketch() {
  const id = useSvgId();
  const table = id('table');
  const screen = id('screen');
  return (
    <Frame art="sketch">
      <defs>
        <linearGradient id={table.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#3b2a1c" />
          <stop offset="1" stopColor="#1c130c" />
        </linearGradient>
        <linearGradient id={screen.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fffbf3" />
          <stop offset="1" stopColor="#d9ccb6" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={table.url} />
      <Band o={0.35} />
      <Soft cx={150} cy={70} rx={100} ry={70} c="#ffe2a8" o={0.18} />
      {/* The phone, drawing pad up, its shadow falling to the lower right. */}
      <g transform="translate(112 22) rotate(-6)">
        <rect x="8" y="10" width="96" height="150" rx="14" fill="#000" opacity="0.5" />
        <rect width="96" height="150" rx="14" fill="#121214" />
        <path d="M14 1h68" stroke="#4a4a52" strokeWidth="1.5" strokeLinecap="round" />
        <rect x="6" y="14" width="84" height="122" rx="6" fill={screen.url} />
        <rect x="40" y="6" width="16" height="3" rx="1.5" fill="#2a2a2e" />
        {/* A doodle: a long-necked animal, scribbled. */}
        <path
          d="M22 118c6-18 14-26 26-26c10 0 16 8 22 4M48 92c2-22 4-40 8-56c2-6 8-8 10-2"
          fill="none"
          stroke="#1d1d22"
          strokeWidth="3.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <circle cx="60" cy="34" r="2.2" fill="#1d1d22" />
        <path
          d="M30 100l-2 18M58 100l2 18"
          stroke="#1d1d22"
          strokeWidth="3"
          strokeLinecap="round"
        />
        <path
          d="M40 70l6 4M46 58l6 3M50 80l5 2"
          stroke="#e53935"
          strokeWidth="3"
          strokeLinecap="round"
        />
      </g>
      {/* The pencil. */}
      <g transform="translate(214 118) rotate(-38)">
        <rect x="4" y="8" width="64" height="8" rx="2" fill="#000" opacity="0.4" />
        <rect width="64" height="10" rx="2" fill="#f2b632" />
        <rect y="6" width="64" height="4" fill="#c98a12" />
        <rect x="54" width="10" height="10" rx="2" fill="#e88aa3" />
        <path d="M0 0L-12 5L0 10z" fill="#e7cfa4" />
        <path d="M-8 3.4L-12 5L-8 6.6z" fill="#1d1d22" />
      </g>
      {/* Guesses rising from the room. */}
      <g fill="#f6efe3" fillOpacity="0.92">
        <rect x="22" y="34" width="62" height="24" rx="12" />
        <path d="M36 56l-4 10l12-8z" />
        <rect x="30" y="80" width="46" height="22" rx="11" />
        <path d="M44 100l-4 9l11-7z" />
      </g>
      <path d="M38 46h30M42 91h18" stroke="#6b5a48" strokeWidth="4" strokeLinecap="round" />
      <g transform="translate(238 26)">
        <rect width="56" height="34" rx="9" fill="#e53935" />
        <text
          x="28"
          y="23"
          textAnchor="middle"
          fontSize="15"
          fontWeight="800"
          fill="#fff"
          fontFamily="system-ui, sans-serif"
        >
          S&amp;S
        </text>
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
          <stop offset="0" stopColor="#f3e3b5" />
          <stop offset="1" stopColor="#c08a2e" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={map.url} />
      <path d="M0 140C60 120 90 160 150 132S250 150 320 120V180H0z" fill="#6f9fd0" opacity="0.7" />
      <path d={hexes.join('')} fill="none" stroke="#7c3a0c" strokeOpacity="0.3" strokeWidth="1.2" />
      <Band o={0.3} />
      {/* Tokens on the map, each with its shadow. */}
      {(
        [
          [104, 112, '#dc2626'],
          [214, 110, '#2563eb'],
          [150, 140, '#16a34a'],
        ] as const
      ).map(([x, y, c]) => (
        <g key={c}>
          <Soft cx={x + 6} cy={y + 8} rx={16} ry={6} o={0.55} />
          <circle cx={x} cy={y} r="10" fill={c} stroke="#fff" strokeWidth="3" />
        </g>
      ))}
      {/* The d20 in the middle: lit facets to the upper left, shadow facets to the lower right. */}
      <Soft cx={176} cy={96} rx={44} ry={12} o={0.7} />
      <g transform="translate(160 56)">
        <path d="M0 -36L31 -18V18L0 36L-31 18V-18z" fill="#6d28d9" />
        <path d="M0 -36L19 8H-19z" fill="#c4b5fd" />
        <path d="M-31 -18L0 -36L-19 8zM-31 -18V18L-19 8z" fill="#8b5cf6" />
        <path d="M31 -18V18L19 8zM19 8L0 36L31 18z" fill="#3b0f78" />
        <path
          d="M0 -36L19 8H-19zM-31 -18L-19 8M31 -18L19 8M-19 8L0 36L19 8M-31 18L-19 8M31 18L19 8"
          fill="none"
          stroke="#ede9fe"
          strokeOpacity="0.7"
          strokeWidth="1.5"
          strokeLinejoin="round"
        />
      </g>
    </Frame>
  );
}

function Emulator() {
  const id = useSvgId();
  const bg = id('bg');
  const scan = id('scan');
  const shell = id('shell');
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
          <stop offset="1" stopColor="#0b1020" />
        </linearGradient>
        <linearGradient id={shell.id}>
          <stop offset="0" stopColor="#f3f4f6" />
          <stop offset="0.7" stopColor="#cbd2dc" />
          <stop offset="1" stopColor="#8d96a5" />
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
      <Band o={0.45} />
      <Soft cx={150} cy={60} rx={90} ry={60} c="#c7d2fe" o={0.16} />
      <g transform="translate(100 14)">
        <rect x="10" y="10" width="120" height="156" rx="14" fill="#000" opacity="0.5" />
        <rect width="120" height="156" rx="14" fill={shell.url} />
        <rect x="0" y="140" width="120" height="16" rx="8" fill="#9aa3b2" />
        <path d="M14 1.5h92" stroke="#fff" strokeWidth="1.5" strokeLinecap="round" />
        <rect x="14" y="14" width="92" height="70" rx="6" fill="#0f172a" />
        <rect x="20" y="20" width="80" height="58" rx="2" fill="#14532d" />
        <g fill="#86efac" transform="translate(38 31)">
          {px.map(([x, y]) => (
            <rect key={`${x}-${y}`} x={x! * 4} y={y! * 4} width="4" height="4" />
          ))}
        </g>
        <path d="M22 26h40l-24 30H22z" fill="#fff" opacity="0.08" />
        <path d="M26 110h10v-10h10v10h10v10H46v10H36v-10H26z" fill="#1e293b" />
        <path d="M24 108h10v-10h10v10h10v10H44v10H34v-10H24z" fill="#334155" />
        <circle cx="88" cy="118" r="8" fill="#000" opacity="0.3" />
        <circle cx="86" cy="116" r="8" fill="#e11d48" />
        <circle cx="104" cy="106" r="8" fill="#000" opacity="0.3" />
        <circle cx="102" cy="104" r="8" fill="#7c3aed" />
      </g>
      <rect width="320" height="180" fill={scan.url} />
    </Frame>
  );
}

/** Vertical folds of cloth: a repeating light-dark stripe of the given width. */
function Folds({ id, w, lit, dark }: { id: string; w: number; lit: string; dark: string }) {
  return (
    <linearGradient id={id} gradientUnits="userSpaceOnUse" x2={w} spreadMethod="repeat">
      <stop offset="0" stopColor={dark} />
      <stop offset="0.45" stopColor={lit} />
      <stop offset="1" stopColor={dark} />
    </linearGradient>
  );
}

/** `featured`: a stage, its curtain in the game's hue, one spotlight on an arcade cabinet. */
function Stage({ h }: { h: number }) {
  const id = useSvgId();
  const back = id('back');
  const drape = id('drape');
  const boards = id('boards');
  const cab = id('cab');
  const scalloped = Array.from({ length: 8 }, () => 'q-20 14 -40 0').join('');
  return (
    <>
      <defs>
        <Folds id={back.id} w={16} lit={`hsl(${h} 52% 34%)`} dark={`hsl(${h} 58% 15%)`} />
        <Folds id={drape.id} w={22} lit={`hsl(${h} 50% 26%)`} dark={`hsl(${h} 60% 9%)`} />
        <linearGradient id={boards.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#5b3a22" />
          <stop offset="1" stopColor="#1e120a" />
        </linearGradient>
        <linearGradient id={cab.id}>
          <stop offset="0" stopColor="#6e4f3a" />
          <stop offset="1" stopColor="#2a1c14" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill="#0f0a08" />
      <rect width="320" height="122" fill={back.url} />
      {/* The boards, a stage lip and a few planks running back. */}
      <rect y="118" width="320" height="62" fill={boards.url} />
      <path d="M0 118h320v3H0z" fill="#7a5233" />
      <path
        d="M160 100L-40 180M160 100L60 180M160 100L130 180M160 100L190 180M160 100L260 180M160 100L360 180M0 140H320M0 162H320"
        stroke="#1a0f08"
        strokeOpacity="0.55"
      />
      {/* The spotlight from the upper left: its cone, the lit curtain behind, its pool on the boards. */}
      <Soft cx={162} cy={70} rx={64} ry={58} c="#ffd79a" o={0.32} />
      <Beam x={54} y={-6} x1={112} x2={214} y2={130} o={0.14} />
      <Soft cx={164} cy={128} rx={66} ry={13} c="#ffd79a" o={0.6} />
      {/* The cabinet: a lit front, a shadowed side, the marquee and screen glowing. */}
      <Soft cx={194} cy={130} rx={38} ry={6} o={0.75} />
      <path d="M182 30l8 4v94l-8 2z" fill="#0d0907" />
      <path d="M138 30h44v100h-44z" fill={cab.url} />
      <path d="M138 30h3v100h-3z" fill="#7a5a44" />
      <rect x="142" y="34" width="36" height="12" rx="1" fill={`hsl(${h} 85% 70%)`} />
      <path d="M146 40h28" stroke={`hsl(${h} 70% 30%)`} strokeWidth="3" strokeLinecap="round" />
      <rect x="143" y="52" width="34" height="28" rx="2" fill="#081014" />
      <rect x="146" y="55" width="28" height="22" fill={`hsl(${h} 70% 46%)`} opacity="0.85" />
      <path
        d="M150 70h6v-6h6v6h6M166 60h4v4h-4z"
        stroke="#fff"
        strokeOpacity="0.8"
        strokeWidth="2"
        fill="none"
      />
      <path d="M138 86h44l6 10h-56z" fill="#4a3428" />
      <circle cx="152" cy="91" r="2.5" fill="#f43f5e" />
      <circle cx="166" cy="91" r="2.5" fill="#facc15" />
      <path d="M144 100h32" stroke="#000" strokeOpacity="0.5" strokeWidth="2" />
      {/* The house curtain, drawn back to both sides and tied, and the valance across the top. */}
      <path d="M0 0h98c-8 30-26 52-30 66c-6 22 6 44 2 56H0z" fill={drape.url} />
      <path d="M320 0h-98c8 30 26 52 30 66c6 22-6 44-2 56h68z" fill={drape.url} />
      <ellipse cx="68" cy="72" rx="7" ry="4" fill="#c99a4a" />
      <ellipse cx="252" cy="72" rx="7" ry="4" fill="#7a5a2c" />
      <path d={`M320 0V16${scalloped}V0z`} fill={drape.url} />
      <path
        d={`M320 16${scalloped}`}
        fill="none"
        stroke="#c99a4a"
        strokeOpacity="0.7"
        strokeWidth="1.5"
      />
    </>
  );
}

/** `cartridge`: a cartridge standing on a table under a desk lamp, its shell and label in the game's hue. */
function CartridgeScene({ h }: { h: number }) {
  const id = useSvgId();
  const wall = id('wall');
  const table = id('table');
  const shell = id('shell');
  const label = id('label');
  return (
    <>
      <defs>
        <linearGradient id={wall.id} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#2a1d14" />
          <stop offset="1" stopColor="#120c08" />
        </linearGradient>
        <linearGradient id={table.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#6b4428" />
          <stop offset="1" stopColor="#24150b" />
        </linearGradient>
        <linearGradient id={shell.id}>
          <stop offset="0" stopColor={`hsl(${h} 18% 74%)`} />
          <stop offset="1" stopColor={`hsl(${h} 16% 50%)`} />
        </linearGradient>
        <linearGradient id={label.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={`hsl(${h} 70% 62%)`} />
          <stop offset="1" stopColor={`hsl(${h} 62% 30%)`} />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill={wall.url} />
      <Band o={0.3} />
      <rect y="106" width="320" height="74" fill={table.url} />
      <path d="M0 106h320" stroke="#000" strokeOpacity="0.5" strokeWidth="2" />
      <path
        d="M0 122c80 4 160-4 320 2M0 142c100 6 200-6 320 0M0 164c90 4 210-4 320 0"
        fill="none"
        stroke="#1a0e06"
        strokeOpacity="0.45"
      />
      {/* The desk lamp to the left, its light on the cartridge and the table. */}
      <Soft cx={60} cy={116} rx={30} ry={6} o={0.6} />
      <path d="M38 112h40l-4-6H42z" fill="#1c1c1f" />
      <path
        d="M58 108L44 58L96 34"
        fill="none"
        stroke="#2b2b30"
        strokeWidth="4"
        strokeLinejoin="round"
      />
      <circle cx="44" cy="58" r="4" fill="#3a3a40" />
      <path d="M84 22l30 14l-10 18l-28-8z" fill="#2f3a2f" />
      <path d="M84 22l30 14l-2 4l-30-12z" fill="#5d6e5a" />
      <Soft cx={108} cy={50} rx={14} ry={10} c="#fff3c4" o={0.9} />
      <Beam x={106} y={50} x1={110} x2={232} y2={112} o={0.16} />
      <Soft cx={168} cy={112} rx={74} ry={14} c="#ffd79a" o={0.55} />
      {/* The cartridge: a lit front, a shadowed right side, grip ridges, the label facing out. Scaled about its
          foot so the whole shell stays inside the phone list banner (about y 37-143). */}
      <g transform="translate(160 112) scale(0.84) translate(-160 -112)">
        <Soft cx={210} cy={112} rx={36} ry={6} o={0.75} />
        <path d="M190 30l8-4v80l-8 6z" fill={`hsl(${h} 14% 30%)`} />
        <path d="M134 34q0-4 4-4h52v82h-56z" fill={shell.url} />
        <path d="M138 30h52l8-4h-52z" fill={`hsl(${h} 16% 82%)`} />
        <path d="M140 37h44M140 41h44" stroke={`hsl(${h} 14% 40%)`} strokeOpacity="0.6" />
        <rect x="140" y="48" width="44" height="44" rx="2" fill={label.url} />
        <circle cx="172" cy="60" r="6" fill={`hsl(${h} 90% 86%)`} />
        <path d="M140 84l12-14l10 8l8-6l14 12v8h-44z" fill={`hsl(${h} 60% 18%)`} />
        <path
          d="M145 54h16"
          stroke="#fff"
          strokeOpacity="0.8"
          strokeWidth="2.5"
          strokeLinecap="round"
        />
        <path d="M144 100h36v8h-36z" fill={`hsl(${h} 12% 36%)`} />
        <path d="M134 34h2v78h-2z" fill="#fff" opacity="0.35" />
      </g>
      {/* A second cartridge lying in the foreground. */}
      <Soft cx={262} cy={152} rx={34} ry={7} o={0.6} />
      <path d="M232 136h52l-6 12h-52z" fill="#6a5c4e" />
      <path d="M226 148h52v4h-52zM278 148l6-12v4l-6 12z" fill="#2e2620" />
      <path d="M240 139h22l-2 5h-22z" fill="#8a6d4a" />
    </>
  );
}

/** `workshop`: a drafting desk under a lamp, the blueprint on it in the game's hue. */
function Drafting({ h }: { h: number }) {
  const id = useSvgId();
  const sheet = id('sheet');
  const desk = id('desk');
  const ink = `hsl(${h} 75% 88%)`;
  // Ruled lines on the tilted sheet: its left edge runs (110,36)->(94,112), its right (210,36)->(226,112).
  const ruled = [50, 64, 78, 92, 104]
    .map((y) => {
      const t = (y - 36) / 76;
      return `M${110 - 16 * t} ${y}H${210 + 16 * t}`;
    })
    .join('');
  return (
    <>
      <defs>
        <linearGradient id={sheet.id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor={`hsl(${h} 60% 42%)`} />
          <stop offset="1" stopColor={`hsl(${h} 62% 20%)`} />
        </linearGradient>
        <linearGradient id={desk.id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#5a3a22" />
          <stop offset="1" stopColor="#1e120a" />
        </linearGradient>
      </defs>
      <rect width="320" height="180" fill="#1a120c" />
      <path d="M0 0h320v12H0z" fill="#000" opacity="0.3" />
      <Band o={0.3} />
      {/* A sheet pinned to the wall, then the floor, the table's legs and their shadow. */}
      <rect x="246" y="26" width="38" height="48" fill={`hsl(${h} 35% 62%)`} opacity="0.5" />
      <path d="M252 38h24M252 46h20M252 54h24" stroke="#1a120c" strokeOpacity="0.5" />
      <circle cx="265" cy="30" r="2" fill="#e11d48" />
      <rect y="150" width="320" height="30" fill={desk.url} />
      <Soft cx={174} cy={170} rx={90} ry={9} o={0.75} />
      <path d="M102 124l-10 50M218 124l10 50M97 150h126" stroke="#3e2716" strokeWidth="5" />
      <path d="M102 124l-10 50" stroke="#8a5f3b" strokeWidth="1.5" />
      {/* The tilted drafting board and its edge. */}
      <path d="M102 28h116l20 92H82z" fill="#7a5233" />
      <path d="M82 120h156v6H82z" fill="#3e2716" />
      <path d="M102 28h116l2 4H101z" fill="#b48558" />
      {/* The blueprint: ruled lines and a cabinet drawn in plan. */}
      <path d="M110 36h100l16 76H94z" fill={sheet.url} />
      <path d={ruled} stroke={ink} strokeOpacity="0.28" />
      <path
        d="M140 44h34v52h-34zM144 50h26v18h-26zM140 76h34M150 84h14"
        fill="none"
        stroke={ink}
        strokeWidth="1.6"
      />
      <circle cx="194" cy="58" r="9" fill="none" stroke={ink} strokeWidth="1.4" />
      <path d="M186 58h16M194 50v16" stroke={ink} strokeOpacity="0.6" />
      {/* The T-square across the sheet, a set square, a pencil. */}
      <path d="M88 96h150v6H87z" fill="#e8d2a6" />
      <path d="M88 102h150v2H87z" fill="#000" opacity="0.4" />
      <path
        d="M200 66l26 28h-26z"
        fill="#dbeafe"
        opacity="0.35"
        stroke="#eff6ff"
        strokeOpacity="0.6"
      />
      <path d="M122 88l34-14l2 4l-34 14z" fill="#f2b632" />
      <path d="M122 88l-6 5l8-1z" fill="#e7cfa4" />
      {/* The lamp clamped above, its light pooled on the sheet. */}
      <path d="M20 0L58 18" stroke="#4a4a52" strokeWidth="4" />
      <path d="M50 6l32 8l-6 18l-30-10z" fill="#4f463d" />
      <Soft cx={78} cy={26} rx={14} ry={9} c="#fff3c4" o={0.9} />
      <Beam x={76} y={26} x1={100} x2={232} y2={112} o={0.14} />
      <Soft cx={162} cy={74} rx={70} ry={40} c="#ffd79a" o={0.4} />
    </>
  );
}

/** The generic paintings, coloured by a hue from the game's title. */
function Generic({ kind, seed }: { kind: 'featured' | 'cartridge' | 'workshop'; seed: string }) {
  const h = hueOf(seed);
  return (
    <Frame art={kind}>
      {kind === 'featured' && <Stage h={h} />}
      {kind === 'cartridge' && <CartridgeScene h={h} />}
      {kind === 'workshop' && <Drafting h={h} />}
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
    case 'sketch':
      return <Sketch />;
    case 'emulator':
      return <Emulator />;
    default:
      return <Generic kind={k} seed={seed || k} />;
  }
}
