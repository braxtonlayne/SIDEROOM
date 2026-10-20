// Writes the SVG sources of the "Side Room Words" starter sticker pack (D-289, CC0-1.0). One 320x320 rounded
// card per word in the `sideroom` palette of doc 10, set in the Entrance display face (Newsreader). No figures.
// Run: node scripts/make-sticker-svgs.mjs   then   node scripts/render-stickers.mjs   (or `just stickers`).
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = join(
  dirname(fileURLToPath(import.meta.url)),
  '../packages/starter-packs/side-room-words/src',
);

// Three colour variants from doc 10 section 2: olive card with cream type, gold card with ink type, cream card
// with ink type. Each has a hairline inset frame in a second palette colour.
const VARIANTS = [
  { bg: '#23251d', fg: '#ece6d6', frame: '#c2b08f' },
  { bg: '#c2b08f', fg: '#111110', frame: '#111110' },
  { bg: '#ece6d6', fg: '#161613', frame: '#6f6c60' },
];

// file name, lines, variant
const WORDS = [
  ['brb', ['brb'], 0],
  ['gg', ['gg'], 1],
  ['lol', ['lol'], 2],
  ['yes', ['yes'], 1],
  ['no', ['no'], 0],
  ['hype', ['hype'], 1],
  ['ok', ['ok'], 2],
  ['wow', ['wow'], 0],
  ['nice', ['nice'], 2],
  ['oof', ['oof'], 1],
  ['sad', ['sad'], 0],
  ['popcorn', ['popcorn'], 2],
  ['movie_night', ['movie', 'night'], 0],
  ['reading', ['reading'], 1],
  ['afk', ['afk'], 2],
  ['gn', ['gn'], 0],
];

mkdirSync(out, { recursive: true });
for (const [name, lines, v] of WORDS) {
  const c = VARIANTS[v];
  const longest = Math.max(...lines.map((l) => l.length));
  const size = Math.min(
    lines.length > 1 ? 104 : 170,
    Math.floor(236 / (Math.max(longest, 2) * 0.5)),
  );
  const lineHeight = size * 0.92;
  const first = 160 + size * 0.3 - ((lines.length - 1) * lineHeight) / 2;
  const text = lines
    .map(
      (l, i) =>
        `  <text x="160" y="${(first + i * lineHeight).toFixed(1)}" text-anchor="middle" font-family="Newsreader, 'Source Serif 4', Georgia, serif" font-size="${size}" font-weight="500" font-style="italic" fill="${c.fg}">${l}</text>`,
    )
    .join('\n');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="320" viewBox="0 0 320 320">
  <title>${lines.join(' ')}</title>
  <rect width="320" height="320" rx="44" fill="${c.bg}"/>
  <rect x="14" y="14" width="292" height="292" rx="32" fill="none" stroke="${c.frame}" stroke-opacity="0.55" stroke-width="2"/>
${text}
</svg>
`;
  writeFileSync(join(out, `${name}.svg`), svg);
}
console.log(`wrote ${WORDS.length} SVG files to ${out}`);
