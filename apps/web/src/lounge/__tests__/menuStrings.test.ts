// SEC-AUD-040 (audit theme 11): the channel, server, user and voice context menus speak through t(), never
// through English literals (a person on another locale saw every one of these menus in English).
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { MUTE_DURATIONS } from '../shell/notifSettings';

const FILES = [
  '../shell/useChannelMenu.ts',
  '../shell/useGuildMenu.ts',
  '../shell/userMenu.ts',
  '../voice/useVoiceMenu.ts',
];

describe('context menu labels', () => {
  it.each(FILES)('%s has no literal English label or error', (rel) => {
    const src = readFileSync(resolve(__dirname, rel), 'utf8');
    expect(src.match(/label:\s*(['"`])[A-Za-z$][^'"`]*\1/g) ?? []).toEqual([]);
    expect(src.match(/onError\??\.?\(\s*['"`][A-Z]/g) ?? []).toEqual([]);
    expect(src.match(/:\s*'The voice change was refused\.'/g) ?? []).toEqual([]);
  });

  it('mute durations come from the catalog', () => {
    expect(MUTE_DURATIONS.map((d) => d.label)).toEqual([
      'For 15 Minutes',
      'For 1 Hour',
      'For 3 Hours',
      'For 8 Hours',
      'For 24 Hours',
      'Until I turn it back on',
    ]);
    const src = readFileSync(resolve(__dirname, '../shell/notifSettings.ts'), 'utf8');
    expect(src).not.toMatch(/label:\s*'For /);
  });
});
