// UTL-AUD-001: the tool entry. utl-shell owns tools/registry.ts; its entry for the audio tool imports this.
import { lazy } from 'react';

export const audioTool = {
  id: 'audio',
  nameKey: 'tools.audio.name',
  descriptionKey: 'tools.audio.description',
  /** Task phrases for the search corpus (utl-search appends them to corpus.json). */
  phrases: [
    'trim audio',
    'cut a sound',
    'normalize volume',
    'make audio louder',
    'fade in and out',
    'audio to wav',
    'audio to opus',
    'shorten a voice message',
  ],
  component: lazy(() => import('./AudioTool').then((m) => ({ default: m.AudioTool }))),
};

export type { AudioLimits, AudioToolProps } from './AudioTool';
