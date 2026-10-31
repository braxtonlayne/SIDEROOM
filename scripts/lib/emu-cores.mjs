// Emulator core pins (ACT-EMU-003, roadmap-p27 section 19, Q23): the by-name allow-list of cores and the checks that
// tie deploy/emu-cores.lock, deploy/EMULATOR.md and THIRD_PARTY_NOTICES.md together. Pure functions, no I/O, so
// scripts/check-licenses.mjs, scripts/check-ops-files.mjs and their tests share one definition.

// The cores are not npm packages: they are prebuilt files fetched at install time. They are allowed BY NAME with their
// licence, so a fourth, fifth or tenth core cannot arrive unnoticed. The rejected cores (Snes9x, Genesis Plus GX,
// PicoDrive, FBNeo, any arcade core, any non-commercial core) are deliberately not in this map.
export const EMU_CORES = new Map([
  ['fceumm', { licence: 'GPL-2.0', system: 'NES', notice: 'FCEUmm' }],
  ['mgba', { licence: 'MPL-2.0', system: 'GBA', notice: 'mGBA' }],
  ['pcsx_rearmed', { licence: 'GPL-2.0', system: 'PS1', notice: 'PCSX-ReARMed' }],
  // Wanted but not obtainable from the EmulatorJS 4.2.3 build set (deploy/EMULATOR.md): adding them later is a
  // pin-list change only.
  ['supafaust', { licence: 'GPL-2.0', system: 'SNES', notice: 'Beetle Supafaust' }],
  ['blastem', { licence: 'GPL-3.0', system: 'Genesis', notice: 'BlastEm' }],
]);

// The one non-core file in the pin list (unpacks the 7z core archives in the browser).
export const EMU_HELPERS = new Set(['extract7z.js']);

/** Parse deploy/emu-cores.lock into rows. */
export function parseLock(text) {
  return text
    .split('\n')
    .filter((l) => l.trim() && !l.startsWith('#'))
    .map((l) => {
      const [dest, url, sha256, bytes, licence] = l.split('\t');
      return { dest, url, sha256, bytes: Number(bytes), licence };
    });
}

/** Every pinned file belongs to an allowed core with the licence we claim, and the notices carry its text. */
export function coreProblems(lockText, noticesText) {
  const bad = [];
  const seen = new Set();
  for (const row of parseLock(lockText)) {
    if (EMU_HELPERS.has(row.dest)) continue;
    const core = row.dest.split('/')[0];
    const allowed = EMU_CORES.get(core);
    if (!allowed) {
      bad.push(`emu-cores.lock: ${row.dest} is not an allowed emulator core`);
      continue;
    }
    if (row.licence !== allowed.licence)
      bad.push(`emu-cores.lock: ${core} is ${allowed.licence}, the lock says ${row.licence}`);
    seen.add(core);
  }
  if (seen.size && !noticesText.includes('## Emulator cores'))
    bad.push('THIRD_PARTY_NOTICES.md has no "Emulator cores" section');
  for (const core of seen) {
    const { notice, licence } = EMU_CORES.get(core);
    const text = licence === 'MPL-2.0' ? 'Mozilla Public License' : 'GNU General Public License';
    if (!noticesText.includes(notice) || !noticesText.includes(text))
      bad.push(`THIRD_PARTY_NOTICES.md has no ${licence} notice for ${notice}`);
  }
  return bad;
}

/** The markdown table of deploy/EMULATOR.md must list exactly the lock's files, URLs, sizes and digests. */
export function tableProblems(lockText, emulatorMd) {
  const bad = [];
  const rows = parseLock(lockText);
  for (const r of rows) {
    const line = emulatorMd
      .split('\n')
      .find((l) => l.includes(`\`${r.dest}\``) && l.includes(r.sha256));
    if (!line) {
      bad.push(`deploy/EMULATOR.md has no table row for ${r.dest} with digest ${r.sha256}`);
      continue;
    }
    if (!line.includes(r.url))
      bad.push(`deploy/EMULATOR.md: ${r.dest} has another URL than the lock`);
    if (!line.includes(String(r.bytes)))
      bad.push(`deploy/EMULATOR.md: ${r.dest} has another size than the lock`);
  }
  // No digest in the doc that is not in the lock (a stale row).
  const known = new Set(rows.map((r) => r.sha256));
  for (const m of emulatorMd.matchAll(/`([0-9a-f]{64})`/g))
    if (!known.has(m[1]))
      bad.push(`deploy/EMULATOR.md lists digest ${m[1]} which is not in the lock`);
  return bad;
}
